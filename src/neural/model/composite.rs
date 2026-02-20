//! Composite neural compiler model: GNN encoder + Transformer decoder.
//!
//! Wraps the encoder and decoder into a single `Module` that can be
//! saved/loaded as a unit.

use burn::config::Config;
use burn::module::Module;
use burn::prelude::*;

use super::decoder::{DecoderConfig, StackAwareDecoder};
use super::encoder::{GnnEncoder, GnnEncoderConfig};
use super::vocab::VOCAB_SIZE;

/// Configuration for the composite neural compiler model.
#[derive(Config, Debug)]
pub struct NeuralCompilerConfig {
    /// Model dimension (shared between encoder and decoder).
    #[config(default = 256)]
    pub d_model: usize,
    /// Edge embedding dimension for GNN.
    #[config(default = 32)]
    pub d_edge: usize,
    /// Number of GNN layers.
    #[config(default = 4)]
    pub gnn_layers: usize,
    /// Number of decoder layers.
    #[config(default = 6)]
    pub decoder_layers: usize,
    /// Number of attention heads.
    #[config(default = 8)]
    pub n_heads: usize,
    /// FFN inner dimension.
    #[config(default = 1024)]
    pub d_ff: usize,
    /// Maximum output sequence length.
    #[config(default = 256)]
    pub max_seq: usize,
    /// Dropout rate (0 for inference).
    #[config(default = 0.1)]
    pub dropout: f64,
}

/// Composite model: GNN encoder + Transformer decoder.
#[derive(Module, Debug)]
pub struct NeuralCompilerV2<B: Backend> {
    pub encoder: GnnEncoder<B>,
    pub decoder: StackAwareDecoder<B>,
}

impl NeuralCompilerConfig {
    /// Initialize the composite model.
    pub fn init<B: Backend>(&self, device: &B::Device) -> NeuralCompilerV2<B> {
        let encoder = GnnEncoderConfig::new()
            .with_d_model(self.d_model)
            .with_d_edge(self.d_edge)
            .with_num_layers(self.gnn_layers)
            .init(device);

        let decoder = DecoderConfig {
            d_model: self.d_model,
            num_layers: self.decoder_layers,
            n_heads: self.n_heads,
            d_ff: self.d_ff,
            max_seq: self.max_seq,
            max_stack_depth: 65,
            type_window: 8,
            dropout: self.dropout,
        }
        .init(device);

        NeuralCompilerV2 { encoder, decoder }
    }

    /// Parameter count estimate.
    pub fn param_estimate(&self) -> usize {
        // GNN: node_proj(59*d) + edge_embed(3*d_e) + layers*(3*d*d + d_e*d + d + d*d + d*d)
        // Decoder: token_embed(V*d) + pos_embed(S*d) + depth_embed(65*32) + type(24*32)
        //        + proj(d+64)*d + layers*(3*d*d*3 + d*4d + 4d*d) + out(d*V)
        let d = self.d_model;
        let gnn_per_layer = 4 * d * d + self.d_edge * d;
        let gnn = 59 * d + 3 * self.d_edge + self.gnn_layers * gnn_per_layer + 2 * d * d;
        let dec_per_layer = 3 * (d * d * 4) + d * self.d_ff + self.d_ff * d;
        let dec = VOCAB_SIZE * d
            + self.max_seq * d
            + 65 * 32
            + 24 * 32
            + (d + 64) * d
            + self.decoder_layers * dec_per_layer
            + d * VOCAB_SIZE;
        gnn + dec
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use burn::backend::NdArray;

    type B = NdArray;

    #[test]
    fn composite_model_initializes() {
        let device = Default::default();
        let config = NeuralCompilerConfig {
            d_model: 32,
            d_edge: 8,
            gnn_layers: 1,
            decoder_layers: 1,
            n_heads: 4,
            d_ff: 64,
            max_seq: 32,
            dropout: 0.0,
        };
        let _model = config.init::<B>(&device);
    }

    #[test]
    fn param_estimate_reasonable() {
        let config = NeuralCompilerConfig::new();
        let params = config.param_estimate();
        // Should be in the ~10-15M range for default config
        assert!(params > 5_000_000, "too few params: {}", params);
        assert!(params < 50_000_000, "too many params: {}", params);
    }
}
