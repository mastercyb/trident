//! Stack-Aware Transformer Decoder.
//!
//! 6-layer decoder with self-attention + cross-attention to GNN node
//! embeddings. Stack depth and type state are injected as additional
//! input features at each step.

use burn::config::Config;
use burn::module::Module;
use burn::nn::attention::{MhaInput, MultiHeadAttention, MultiHeadAttentionConfig};
use burn::nn::{Embedding, EmbeddingConfig, LayerNorm, LayerNormConfig, Linear, LinearConfig};
use burn::prelude::*;

use super::vocab::VOCAB_SIZE;

// ─── Configuration ────────────────────────────────────────────────

/// Decoder configuration.
#[derive(Config, Debug)]
pub struct DecoderConfig {
    /// Model dimension.
    #[config(default = 256)]
    pub d_model: usize,
    /// Number of decoder layers.
    #[config(default = 6)]
    pub num_layers: usize,
    /// Number of attention heads.
    #[config(default = 8)]
    pub n_heads: usize,
    /// FFN inner dimension (4x d_model).
    #[config(default = 1024)]
    pub d_ff: usize,
    /// Maximum sequence length.
    #[config(default = 256)]
    pub max_seq: usize,
    /// Maximum stack depth for depth embedding.
    #[config(default = 65)]
    pub max_stack_depth: usize,
    /// Stack type window size.
    #[config(default = 8)]
    pub type_window: usize,
    /// Dropout rate.
    #[config(default = 0.1)]
    pub dropout: f64,
}

// ─── Decoder Layer ────────────────────────────────────────────────

/// Single decoder layer: self-attn → cross-attn → FFN.
#[derive(Module, Debug)]
pub struct DecoderLayer<B: Backend> {
    self_attn: MultiHeadAttention<B>,
    cross_attn: MultiHeadAttention<B>,
    ffn1: Linear<B>,
    ffn2: Linear<B>,
    norm1: LayerNorm<B>,
    norm2: LayerNorm<B>,
    norm3: LayerNorm<B>,
}

/// Initialize a decoder layer.
fn init_decoder_layer<B: Backend>(
    d_model: usize,
    n_heads: usize,
    d_ff: usize,
    dropout: f64,
    device: &B::Device,
) -> DecoderLayer<B> {
    DecoderLayer {
        self_attn: MultiHeadAttentionConfig::new(d_model, n_heads)
            .with_dropout(dropout)
            .init(device),
        cross_attn: MultiHeadAttentionConfig::new(d_model, n_heads)
            .with_dropout(dropout)
            .init(device),
        ffn1: LinearConfig::new(d_model, d_ff).init(device),
        ffn2: LinearConfig::new(d_ff, d_model).init(device),
        norm1: LayerNormConfig::new(d_model).init(device),
        norm2: LayerNormConfig::new(d_model).init(device),
        norm3: LayerNormConfig::new(d_model).init(device),
    }
}

impl<B: Backend> DecoderLayer<B> {
    /// Forward pass for one decoder layer.
    ///
    /// - `x`: [batch, seq, d_model] — decoder input
    /// - `memory`: [batch, N, d_model] — encoder (GNN) output
    /// - `causal_mask`: [batch, seq, seq] — causal attention mask (Bool)
    pub fn forward(
        &self,
        x: Tensor<B, 3>,
        memory: Tensor<B, 3>,
        causal_mask: Option<Tensor<B, 3, Bool>>,
    ) -> Tensor<B, 3> {
        // Self-attention + residual + norm
        let mut self_attn_input = MhaInput::self_attn(x.clone());
        if let Some(mask) = causal_mask {
            self_attn_input = self_attn_input.mask_attn(mask);
        }
        let self_attn_out = self.self_attn.forward(self_attn_input).context;
        let x = self.norm1.forward(x + self_attn_out);

        // Cross-attention to encoder memory + residual + norm
        let cross_input = MhaInput::new(x.clone(), memory.clone(), memory);
        let cross_out = self.cross_attn.forward(cross_input).context;
        let x = self.norm2.forward(x + cross_out);

        // FFN: Linear → GELU → Linear + residual + norm
        let ffn_out = self
            .ffn2
            .forward(burn::tensor::activation::gelu(self.ffn1.forward(x.clone())));
        self.norm3.forward(x + ffn_out)
    }
}

// ─── Stack-Aware Decoder ──────────────────────────────────────────

/// Stack-Aware Transformer Decoder.
///
/// At each step, the input is:
///   token_emb(prev_token) + pos_emb(t) + depth_emb(stack_depth) + type_proj(type_state)
///
/// The decoder attends to GNN node embeddings via cross-attention.
#[derive(Module, Debug)]
pub struct StackAwareDecoder<B: Backend> {
    /// Token embedding: VOCAB_SIZE → d_model
    token_embed: Embedding<B>,
    /// Positional embedding: max_seq → d_model
    pos_embed: Embedding<B>,
    /// Stack depth embedding: max_stack_depth → d_depth (32)
    depth_embed: Embedding<B>,
    /// Stack type projection: 3*type_window → d_type (32)
    type_proj: Linear<B>,
    /// Input projection: d_model + d_depth + d_type → d_model
    input_proj: Linear<B>,
    /// Decoder layers
    layers: Vec<DecoderLayer<B>>,
    /// Final layer norm
    final_norm: LayerNorm<B>,
    /// Output projection: d_model → VOCAB_SIZE
    output_proj: Linear<B>,
}

impl DecoderConfig {
    /// Initialize the stack-aware decoder.
    pub fn init<B: Backend>(&self, device: &B::Device) -> StackAwareDecoder<B> {
        let d_depth = 32;
        let d_type = 32;

        let mut layers = Vec::with_capacity(self.num_layers);
        for _ in 0..self.num_layers {
            layers.push(init_decoder_layer(
                self.d_model,
                self.n_heads,
                self.d_ff,
                self.dropout,
                device,
            ));
        }

        StackAwareDecoder {
            token_embed: EmbeddingConfig::new(VOCAB_SIZE, self.d_model).init(device),
            pos_embed: EmbeddingConfig::new(self.max_seq, self.d_model).init(device),
            depth_embed: EmbeddingConfig::new(self.max_stack_depth, d_depth).init(device),
            type_proj: LinearConfig::new(3 * self.type_window, d_type).init(device),
            input_proj: LinearConfig::new(self.d_model + d_depth + d_type, self.d_model)
                .init(device),
            layers,
            final_norm: LayerNormConfig::new(self.d_model).init(device),
            output_proj: LinearConfig::new(self.d_model, VOCAB_SIZE).init(device),
        }
    }
}

impl<B: Backend> StackAwareDecoder<B> {
    /// Forward pass: teacher-forcing mode (full sequence at once).
    ///
    /// - `token_ids`: [batch, seq] — previous token IDs (shifted right)
    /// - `positions`: [batch, seq] — position indices
    /// - `stack_depths`: [batch, seq] — stack depth at each step
    /// - `type_states`: [batch, seq, 3*W] — stack type encoding at each step
    /// - `memory`: [batch, N, d_model] — GNN encoder output (node embeddings)
    ///
    /// Returns: [batch, seq, VOCAB_SIZE] — logits over vocabulary
    pub fn forward(
        &self,
        token_ids: Tensor<B, 2, Int>,
        positions: Tensor<B, 2, Int>,
        stack_depths: Tensor<B, 2, Int>,
        type_states: Tensor<B, 3>,
        memory: Tensor<B, 3>,
    ) -> Tensor<B, 3> {
        let [batch_size, seq_len, _] = type_states.dims();

        // Embed tokens + positions: [batch, seq, d_model]
        let tok_emb = self.token_embed.forward(token_ids); // [batch, seq, d_model]
        let pos_emb = self.pos_embed.forward(positions); // [batch, seq, d_model]
        let tok_pos = tok_emb + pos_emb;

        // Embed stack depth: [batch, seq] → [batch, seq, d_depth]
        let depth_emb = self.depth_embed.forward(stack_depths); // [batch, seq, d_depth]

        // Project type state: [batch, seq, 3*W] → [batch, seq, d_type]
        let type_emb = self.type_proj.forward(type_states); // [batch, seq, d_type]

        // Concatenate: [batch, seq, d_model + d_depth + d_type]
        let combined = Tensor::cat(vec![tok_pos, depth_emb, type_emb], 2);

        // Project to d_model: [batch, seq, d_model]
        let mut x = self.input_proj.forward(combined);

        // Causal mask: prevent attending to future positions
        let causal_mask = self.make_causal_mask(batch_size, seq_len, &x.device());

        // Decoder layers
        for layer in &self.layers {
            x = layer.forward(x, memory.clone(), Some(causal_mask.clone()));
        }

        // Final norm + output projection
        let x = self.final_norm.forward(x);
        self.output_proj.forward(x) // [batch, seq, VOCAB_SIZE]
    }

    /// Create a causal (lower-triangular) attention mask.
    /// True = masked (can't attend), False = allowed.
    fn make_causal_mask(
        &self,
        batch_size: usize,
        seq_len: usize,
        device: &B::Device,
    ) -> Tensor<B, 3, Bool> {
        // Create upper triangular matrix (True above diagonal = masked)
        let ones = Tensor::<B, 2>::ones([seq_len, seq_len], device);
        let mask_2d = ones.triu(1); // upper triangle with diagonal offset 1
        let mask_bool: Tensor<B, 2, Bool> = mask_2d.greater_elem(0.5);
        // Expand to [batch, seq, seq]
        mask_bool
            .unsqueeze_dim::<3>(0)
            .expand([batch_size, seq_len, seq_len])
    }
}

// ─── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use burn::backend::NdArray;

    type B = NdArray;

    #[test]
    fn decoder_forward_shape() {
        let device = Default::default();
        let config = DecoderConfig {
            d_model: 32,
            num_layers: 2,
            n_heads: 4,
            d_ff: 64,
            max_seq: 64,
            max_stack_depth: 65,
            type_window: 8,
            dropout: 0.0,
        };
        let decoder = config.init::<B>(&device);

        let batch = 2;
        let seq = 10;
        let num_encoder_nodes = 5;

        let token_ids = Tensor::<B, 2, Int>::zeros([batch, seq], &device);
        let positions = Tensor::<B, 2, Int>::zeros([batch, seq], &device);
        let stack_depths = Tensor::<B, 2, Int>::zeros([batch, seq], &device);
        let type_states = Tensor::<B, 3>::zeros([batch, seq, 24], &device); // 3*W=24
        let memory = Tensor::<B, 3>::zeros([batch, num_encoder_nodes, 32], &device);

        let logits: Tensor<B, 3> =
            decoder.forward(token_ids, positions, stack_depths, type_states, memory);

        assert_eq!(logits.dims(), [batch, seq, VOCAB_SIZE]);
    }

    #[test]
    fn causal_mask_shape() {
        let device = Default::default();
        let config = DecoderConfig {
            d_model: 32,
            num_layers: 1,
            n_heads: 4,
            d_ff: 64,
            max_seq: 64,
            max_stack_depth: 65,
            type_window: 8,
            dropout: 0.0,
        };
        let decoder = config.init::<B>(&device);

        let mask = decoder.make_causal_mask(2, 5, &device);
        assert_eq!(mask.dims(), [2, 5, 5]);
    }
}
