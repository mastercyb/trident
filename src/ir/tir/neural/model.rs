//! The 78K-parameter encoder-decoder neural model.
//!
//! Encoder: 2-layer self-attention with DAG-aware masking, dim 64.
//! Decoder: autoregressive MLP producing TASM instruction sequences.
//! All arithmetic in fixed-point Goldilocks via fused dot products.

use crate::field::fixed::{self, Fixed, RawAccum};
use crate::field::goldilocks::Goldilocks;
use crate::field::PrimeField;
use crate::ir::tir::encode::{TIRBlock, WORDS_PER_NODE};
#[cfg(test)]
use crate::ir::tir::encode::{CONTEXT_SIZE, MAX_NODES};

/// Model hyperparameters.
pub const DIM: usize = 64;
pub const HEADS: usize = 2;
pub const LAYERS: usize = 2;
pub const FFN_HIDDEN: usize = 64;
pub const MAX_OUTPUT: usize = 16;
pub const HEAD_DIM: usize = DIM / HEADS;

/// Encoder per-layer weights:
///   QKV: 3 * 64 * 64 = 12,288
///   out_proj: 64 * 64 = 4,096
///   ffn1: 64 * 64 = 4,096
///   ffn2: 64 * 64 = 4,096
///   ln_scale + ln_bias: 128
///   Total: 24,704 per layer, 49,408 for 2 layers
///
/// Decoder:
///   hidden: (64+64) * 64 = 8,192
///   hidden_bias: 64
///   output: 64 * 64 = 4,096
///   output_bias: 64
///   Total: 12,416
///
/// Input projection: WORDS_PER_NODE * DIM = 4 * 64 = 256
///
/// Grand total: 49,408 + 12,416 + 256 = 62,080
pub const PARAM_COUNT: usize = 49_408 + 12_416;

/// Pre-allocated scratch buffers to avoid per-forward-pass allocations.
pub struct Scratch {
    embeddings: Vec<Fixed>,
    q: Vec<Fixed>,
    k: Vec<Fixed>,
    v: Vec<Fixed>,
    scores: Vec<Fixed>,
    exp_scores: Vec<Fixed>,
    attn_result: Vec<Fixed>,
    projected: Vec<Fixed>,
    ffn_output: Vec<Fixed>,
    ffn_hidden: Vec<Fixed>,
    decoder_hidden: Vec<Fixed>,
    decoder_out: Vec<Fixed>,
}

impl Scratch {
    fn new() -> Self {
        use crate::ir::tir::encode::MAX_NODES;
        Self {
            embeddings: vec![Fixed::ZERO; MAX_NODES * DIM],
            q: vec![Fixed::ZERO; MAX_NODES * HEAD_DIM],
            k: vec![Fixed::ZERO; MAX_NODES * HEAD_DIM],
            v: vec![Fixed::ZERO; MAX_NODES * HEAD_DIM],
            scores: vec![Fixed::ZERO; MAX_NODES * MAX_NODES],
            exp_scores: vec![Fixed::ZERO; MAX_NODES],
            attn_result: vec![Fixed::ZERO; MAX_NODES * DIM],
            projected: vec![Fixed::ZERO; MAX_NODES * DIM],
            ffn_output: vec![Fixed::ZERO; MAX_NODES * DIM],
            ffn_hidden: vec![Fixed::ZERO; FFN_HIDDEN],
            decoder_hidden: vec![Fixed::ZERO; FFN_HIDDEN],
            decoder_out: vec![Fixed::ZERO; DIM],
        }
    }

    fn zero_range(buf: &mut [Fixed], len: usize) {
        for x in buf[..len].iter_mut() {
            *x = Fixed::ZERO;
        }
    }
}

/// The neural optimizer model.
pub struct NeuralModel {
    pub encoder: [EncoderLayer; LAYERS],
    pub decoder: Decoder,
    pub input_proj: Vec<Fixed>,
    scratch: Scratch,
}

/// One encoder layer: self-attention + FFN + layer norm.
pub struct EncoderLayer {
    pub qkv: Vec<Fixed>,
    pub out_proj: Vec<Fixed>,
    pub ffn1: Vec<Fixed>,
    pub ffn2: Vec<Fixed>,
    pub ln_scale: Vec<Fixed>,
    pub ln_bias: Vec<Fixed>,
}

/// Autoregressive MLP decoder.
pub struct Decoder {
    pub hidden: Vec<Fixed>,
    pub hidden_bias: Vec<Fixed>,
    pub output: Vec<Fixed>,
    pub output_bias: Vec<Fixed>,
}

impl NeuralModel {
    pub fn zeros() -> Self {
        Self {
            encoder: [EncoderLayer::zeros(), EncoderLayer::zeros()],
            decoder: Decoder::zeros(),
            input_proj: vec![Fixed::ZERO; WORDS_PER_NODE * DIM],
            scratch: Scratch::new(),
        }
    }

    /// Run forward pass: TIR block -> TASM instruction codes.
    pub fn forward(&mut self, block: &TIRBlock) -> Vec<u64> {
        // Zero-weight fast path: if input projection is all zeros,
        // the model hasn't been trained yet — skip the expensive forward pass
        if self.input_proj.iter().all(|w| w.0 == Goldilocks(0)) {
            return Vec::new();
        }

        let seq_len = block.node_count.max(1);
        let s = &mut self.scratch;

        // 1. Project input nodes to DIM-dimensional embeddings (fused dot)
        Scratch::zero_range(&mut s.embeddings, seq_len * DIM);
        for n in 0..seq_len {
            let node_start = n * WORDS_PER_NODE;
            for d in 0..DIM {
                let mut acc = RawAccum::zero();
                for w in 0..WORDS_PER_NODE {
                    let weight = self.input_proj[w * DIM + d];
                    let input = if node_start + w < block.nodes.len() {
                        Fixed::from_raw(Goldilocks::from_u64(block.nodes[node_start + w]))
                    } else {
                        Fixed::ZERO
                    };
                    acc.add_prod(input, weight);
                }
                s.embeddings[n * DIM + d] = acc.finish();
            }
        }

        // 2. Encoder layers (operate on s.embeddings in-place)
        for layer in &self.encoder {
            layer.forward(s, seq_len);
        }

        // 3. Pool: mean across nodes
        let mut latent = [Fixed::ZERO; DIM];
        let n_inv = Fixed::from_f64(1.0 / seq_len as f64);
        for n in 0..seq_len {
            for d in 0..DIM {
                latent[d] = latent[d].add(s.embeddings[n * DIM + d]);
            }
        }
        for d in 0..DIM {
            latent[d] = latent[d].mul(n_inv);
        }

        // 4. Autoregressive decoding
        let mut output = Vec::with_capacity(MAX_OUTPUT);
        let mut prev_instr = [Fixed::ZERO; DIM];
        for _ in 0..MAX_OUTPUT {
            self.decoder.step(&latent, &prev_instr, s);
            let code = instr_to_code(&s.decoder_out[..DIM]);
            if code == 0 {
                break;
            }
            output.push(code);
            prev_instr.copy_from_slice(&s.decoder_out[..DIM]);
        }

        output
    }

    /// Flatten all weights into a single vector for evolutionary search.
    pub fn to_weight_vec(&self) -> Vec<Fixed> {
        let mut v = Vec::with_capacity(PARAM_COUNT + WORDS_PER_NODE * DIM);
        v.extend_from_slice(&self.input_proj);
        for layer in &self.encoder {
            v.extend_from_slice(&layer.qkv);
            v.extend_from_slice(&layer.out_proj);
            v.extend_from_slice(&layer.ffn1);
            v.extend_from_slice(&layer.ffn2);
            v.extend_from_slice(&layer.ln_scale);
            v.extend_from_slice(&layer.ln_bias);
        }
        v.extend_from_slice(&self.decoder.hidden);
        v.extend_from_slice(&self.decoder.hidden_bias);
        v.extend_from_slice(&self.decoder.output);
        v.extend_from_slice(&self.decoder.output_bias);
        v
    }

    /// Reconstruct model from a flat weight vector.
    pub fn from_weight_vec(w: &[Fixed]) -> Self {
        let mut i = 0;
        let mut take = |n: usize| -> Vec<Fixed> {
            let slice = w[i..i + n].to_vec();
            i += n;
            slice
        };

        let input_proj = take(WORDS_PER_NODE * DIM);

        let mut encoder = [EncoderLayer::zeros(), EncoderLayer::zeros()];
        for layer in &mut encoder {
            layer.qkv = take(3 * DIM * DIM);
            layer.out_proj = take(DIM * DIM);
            layer.ffn1 = take(DIM * FFN_HIDDEN);
            layer.ffn2 = take(FFN_HIDDEN * DIM);
            layer.ln_scale = take(DIM);
            layer.ln_bias = take(DIM);
        }

        let hidden = take(2 * DIM * FFN_HIDDEN);
        let hidden_bias = take(FFN_HIDDEN);
        let output = take(FFN_HIDDEN * DIM);
        let output_bias = take(DIM);
        let decoder = Decoder {
            hidden,
            hidden_bias,
            output,
            output_bias,
        };

        Self {
            encoder,
            decoder,
            input_proj,
            scratch: Scratch::new(),
        }
    }

    /// Total number of weights in the flat vector.
    pub fn weight_count(&self) -> usize {
        self.to_weight_vec().len()
    }
}

impl EncoderLayer {
    fn zeros() -> Self {
        Self {
            qkv: vec![Fixed::ZERO; 3 * DIM * DIM],
            out_proj: vec![Fixed::ZERO; DIM * DIM],
            ffn1: vec![Fixed::ZERO; DIM * FFN_HIDDEN],
            ffn2: vec![Fixed::ZERO; FFN_HIDDEN * DIM],
            ln_scale: vec![Fixed::ONE; DIM],
            ln_bias: vec![Fixed::ZERO; DIM],
        }
    }

    /// Self-attention + FFN forward pass on s.embeddings in-place.
    fn forward(&self, s: &mut Scratch, seq_len: usize) {
        // Multi-head self-attention (reads s.embeddings, writes s.projected)
        self.attention(s, seq_len);

        // Residual connection: embeddings += projected
        for i in 0..seq_len * DIM {
            s.embeddings[i] = s.embeddings[i].add(s.projected[i]);
        }

        // Layer norm + scale/bias
        for n in 0..seq_len {
            let start = n * DIM;
            let end = start + DIM;
            fixed::layer_norm(&mut s.embeddings[start..end]);
            for d in 0..DIM {
                s.embeddings[start + d] = s.embeddings[start + d]
                    .mul(self.ln_scale[d])
                    .add(self.ln_bias[d]);
            }
        }

        // FFN with residual (reads s.embeddings, writes s.ffn_output)
        self.ffn(s, seq_len);
        for i in 0..seq_len * DIM {
            s.embeddings[i] = s.embeddings[i].add(s.ffn_output[i]);
        }
    }

    /// Multi-head self-attention. Reads s.embeddings, writes s.projected.
    fn attention(&self, s: &mut Scratch, seq_len: usize) {
        Scratch::zero_range(&mut s.attn_result, seq_len * DIM);

        for h in 0..HEADS {
            let head_offset = h * HEAD_DIM;

            // QKV projection with fused dot
            Scratch::zero_range(&mut s.q, seq_len * HEAD_DIM);
            Scratch::zero_range(&mut s.k, seq_len * HEAD_DIM);
            Scratch::zero_range(&mut s.v, seq_len * HEAD_DIM);

            for n in 0..seq_len {
                for d in 0..HEAD_DIM {
                    let out_d = head_offset + d;
                    let mut q_acc = RawAccum::zero();
                    let mut k_acc = RawAccum::zero();
                    let mut v_acc = RawAccum::zero();
                    for j in 0..DIM {
                        let inp = s.embeddings[n * DIM + j];
                        q_acc.add_prod(inp, self.qkv[j * DIM + out_d]);
                        k_acc.add_prod(inp, self.qkv[DIM * DIM + j * DIM + out_d]);
                        v_acc.add_prod(inp, self.qkv[2 * DIM * DIM + j * DIM + out_d]);
                    }
                    s.q[n * HEAD_DIM + d] = q_acc.finish();
                    s.k[n * HEAD_DIM + d] = k_acc.finish();
                    s.v[n * HEAD_DIM + d] = v_acc.finish();
                }
            }

            // Attention scores: Q * K^T / sqrt(HEAD_DIM)
            let scale_inv = Fixed::from_f64(1.0 / (HEAD_DIM as f64).sqrt());
            for i in 0..seq_len {
                let mut max_score = Fixed::from_f64(-1000.0);

                for j in 0..seq_len {
                    let mut acc = RawAccum::zero();
                    for d in 0..HEAD_DIM {
                        acc.add_prod(s.q[i * HEAD_DIM + d], s.k[j * HEAD_DIM + d]);
                    }
                    let score = acc.finish().mul(scale_inv);
                    s.scores[i * seq_len + j] = score;
                    if score.to_f64() > max_score.to_f64() {
                        max_score = score;
                    }
                }

                // Softmax: exp(x - max) / sum, using 1+x+x²/2 approximation
                let mut exp_sum = Fixed::ZERO;
                for j in 0..seq_len {
                    let x = s.scores[i * seq_len + j].sub(max_score);
                    let x2 = x.mul(x);
                    let half = Fixed::from_f64(0.5);
                    let exp_x = Fixed::ONE.add(x).add(x2.mul(half));
                    let exp_x = if exp_x.to_f64() < 0.0 {
                        Fixed::from_f64(0.001)
                    } else {
                        exp_x
                    };
                    s.exp_scores[j] = exp_x;
                    exp_sum = exp_sum.add(exp_x);
                }
                let sum_inv = if exp_sum.to_f64().abs() > 1e-10 {
                    exp_sum.inv()
                } else {
                    Fixed::ONE
                };
                for j in 0..seq_len {
                    s.exp_scores[j] = s.exp_scores[j].mul(sum_inv);
                }

                // Weighted sum of values (fused)
                for d in 0..HEAD_DIM {
                    let mut acc = RawAccum::zero();
                    for j in 0..seq_len {
                        acc.add_prod(s.exp_scores[j], s.v[j * HEAD_DIM + d]);
                    }
                    let idx = i * DIM + head_offset + d;
                    s.attn_result[idx] = s.attn_result[idx].add(acc.finish());
                }
            }
        }

        // Output projection (fused)
        Scratch::zero_range(&mut s.projected, seq_len * DIM);
        for n in 0..seq_len {
            for d in 0..DIM {
                let mut acc = RawAccum::zero();
                for j in 0..DIM {
                    acc.add_prod(s.attn_result[n * DIM + j], self.out_proj[j * DIM + d]);
                }
                s.projected[n * DIM + d] = acc.finish();
            }
        }
    }

    /// Feed-forward network: DIM -> FFN_HIDDEN -> DIM with ReLU (fused dot).
    /// Reads s.embeddings, writes s.ffn_output.
    fn ffn(&self, s: &mut Scratch, seq_len: usize) {
        Scratch::zero_range(&mut s.ffn_output, seq_len * DIM);

        for n in 0..seq_len {
            // Layer 1: DIM -> FFN_HIDDEN + ReLU
            for h in 0..FFN_HIDDEN {
                let mut acc = RawAccum::zero();
                for d in 0..DIM {
                    acc.add_prod(s.embeddings[n * DIM + d], self.ffn1[d * FFN_HIDDEN + h]);
                }
                s.ffn_hidden[h] = acc.finish().relu();
            }

            // Layer 2: FFN_HIDDEN -> DIM
            for d in 0..DIM {
                let mut acc = RawAccum::zero();
                for h in 0..FFN_HIDDEN {
                    acc.add_prod(s.ffn_hidden[h], self.ffn2[h * DIM + d]);
                }
                s.ffn_output[n * DIM + d] = acc.finish();
            }
        }
    }
}

impl Decoder {
    fn zeros() -> Self {
        Self {
            hidden: vec![Fixed::ZERO; 2 * DIM * FFN_HIDDEN],
            hidden_bias: vec![Fixed::ZERO; FFN_HIDDEN],
            output: vec![Fixed::ZERO; FFN_HIDDEN * DIM],
            output_bias: vec![Fixed::ZERO; DIM],
        }
    }

    /// One decoding step: latent + prev -> next instruction embedding.
    /// Result written to s.decoder_out.
    fn step(&self, latent: &[Fixed; DIM], prev: &[Fixed; DIM], s: &mut Scratch) {
        // Hidden layer with ReLU (fused dot with bias)
        for h in 0..FFN_HIDDEN {
            let mut acc = RawAccum::zero();
            acc.add_bias(self.hidden_bias[h]);
            for d in 0..DIM {
                acc.add_prod(latent[d], self.hidden[d * FFN_HIDDEN + h]);
            }
            for d in 0..DIM {
                acc.add_prod(prev[d], self.hidden[(DIM + d) * FFN_HIDDEN + h]);
            }
            s.decoder_hidden[h] = acc.finish().relu();
        }

        // Output layer (fused dot with bias)
        for d in 0..DIM {
            let mut acc = RawAccum::zero();
            acc.add_bias(self.output_bias[d]);
            for h in 0..FFN_HIDDEN {
                acc.add_prod(s.decoder_hidden[h], self.output[h * DIM + d]);
            }
            s.decoder_out[d] = acc.finish();
        }
    }
}

/// Convert a DIM-dimensional output vector to a TASM instruction code.
/// Takes the argmax across positions.
fn instr_to_code(output: &[Fixed]) -> u64 {
    let mut best_val = output[0].to_f64();
    let mut best_idx = 0u64;
    for (i, x) in output.iter().enumerate().skip(1) {
        let v = x.to_f64();
        if v > best_val {
            best_val = v;
            best_idx = i as u64;
        }
    }
    best_idx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zeros_model_produces_empty() {
        let mut model = NeuralModel::zeros();
        let block = TIRBlock {
            nodes: [0; MAX_NODES * WORDS_PER_NODE],
            context: [0; CONTEXT_SIZE],
            node_count: 3,
            fn_name: "test".into(),
            start_idx: 0,
            end_idx: 3,
        };
        let output = model.forward(&block);
        // Zero weights -> fast path returns empty immediately
        assert!(output.is_empty());
    }

    #[test]
    fn weight_vec_roundtrip() {
        let model = NeuralModel::zeros();
        let weights = model.to_weight_vec();
        let restored = NeuralModel::from_weight_vec(&weights);
        let w2 = restored.to_weight_vec();
        assert_eq!(weights.len(), w2.len());
        for (a, b) in weights.iter().zip(w2.iter()) {
            assert_eq!(a, b);
        }
    }

    #[test]
    fn weight_count_reasonable() {
        let model = NeuralModel::zeros();
        let count = model.weight_count();
        // Should be around 62K (was 91K before shrink)
        assert!(count > 50_000, "too few weights: {}", count);
        assert!(count < 80_000, "too many weights: {}", count);
    }

    #[test]
    fn forward_deterministic() {
        let mut model = NeuralModel::from_weight_vec(&vec![
            Fixed::from_f64(0.01);
            NeuralModel::zeros().weight_count()
        ]);
        let block = TIRBlock {
            nodes: [0; MAX_NODES * WORDS_PER_NODE],
            context: [0; CONTEXT_SIZE],
            node_count: 2,
            fn_name: "test".into(),
            start_idx: 0,
            end_idx: 2,
        };
        let out1 = model.forward(&block);
        let out2 = model.forward(&block);
        assert_eq!(out1, out2);
    }
}
