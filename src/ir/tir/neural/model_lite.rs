//! Lite 10K-parameter MLP neural model.
//!
//! No attention layers. Flat input projection + hidden MLP + autoregressive decoder.
//! DIM=32, FFN=32, vocab=64 (same output format as original).
//! ~6x fewer params than the original transformer model.

use crate::field::fixed::{Fixed, RawAccum};
use crate::field::goldilocks::Goldilocks;
use crate::field::PrimeField;
use crate::ir::tir::encode::{TIRBlock, MAX_NODES, WORDS_PER_NODE};

/// Lite model hyperparameters.
pub const LITE_DIM: usize = 32;
pub const LITE_FFN: usize = 32;
pub const LITE_VOCAB: usize = 64;
pub const MAX_OUTPUT: usize = 16;
pub const INPUT_FLAT: usize = MAX_NODES * WORDS_PER_NODE; // 128

/// Total parameters: 10,400.
///   input_proj:     128 * 32 = 4,096
///   input_bias:     32
///   hidden_w:       32 * 32  = 1,024
///   hidden_bias:    32
///   dec_hidden:     96 * 32  = 3,072
///   dec_hidden_bias: 32
///   dec_output:     32 * 64  = 2,048
///   dec_output_bias: 64
pub const LITE_PARAM_COUNT: usize = 10_400;

struct LiteScratch {
    projected: Vec<Fixed>,
    hidden: Vec<Fixed>,
    dec_h: Vec<Fixed>,
    dec_out: Vec<Fixed>,
}

impl LiteScratch {
    fn new() -> Self {
        Self {
            projected: vec![Fixed::ZERO; LITE_DIM],
            hidden: vec![Fixed::ZERO; LITE_DIM],
            dec_h: vec![Fixed::ZERO; LITE_FFN],
            dec_out: vec![Fixed::ZERO; LITE_VOCAB],
        }
    }
}

pub struct NeuralModelLite {
    pub input_proj: Vec<Fixed>,      // [INPUT_FLAT * LITE_DIM]
    pub input_bias: Vec<Fixed>,      // [LITE_DIM]
    pub hidden_w: Vec<Fixed>,        // [LITE_DIM * LITE_DIM]
    pub hidden_bias: Vec<Fixed>,     // [LITE_DIM]
    pub dec_hidden: Vec<Fixed>,      // [(LITE_DIM + LITE_VOCAB) * LITE_FFN]
    pub dec_hidden_bias: Vec<Fixed>, // [LITE_FFN]
    pub dec_output: Vec<Fixed>,      // [LITE_FFN * LITE_VOCAB]
    pub dec_output_bias: Vec<Fixed>, // [LITE_VOCAB]
    scratch: LiteScratch,
}

impl NeuralModelLite {
    pub fn zeros() -> Self {
        Self {
            input_proj: vec![Fixed::ZERO; INPUT_FLAT * LITE_DIM],
            input_bias: vec![Fixed::ZERO; LITE_DIM],
            hidden_w: vec![Fixed::ZERO; LITE_DIM * LITE_DIM],
            hidden_bias: vec![Fixed::ZERO; LITE_DIM],
            dec_hidden: vec![Fixed::ZERO; (LITE_DIM + LITE_VOCAB) * LITE_FFN],
            dec_hidden_bias: vec![Fixed::ZERO; LITE_FFN],
            dec_output: vec![Fixed::ZERO; LITE_FFN * LITE_VOCAB],
            dec_output_bias: vec![Fixed::ZERO; LITE_VOCAB],
            scratch: LiteScratch::new(),
        }
    }

    pub fn forward(&mut self, block: &TIRBlock) -> Vec<u64> {
        // Zero-weight fast path
        if self.input_proj.iter().all(|w| w.0 == Goldilocks(0)) {
            return Vec::new();
        }

        let seq_len = block.node_count.max(1);
        let s = &mut self.scratch;

        // 1. Flatten + project: all nodes → [LITE_DIM] + bias + ReLU
        for d in 0..LITE_DIM {
            let mut acc = RawAccum::zero();
            acc.add_bias(self.input_bias[d]);
            for n in 0..seq_len {
                let node_start = n * WORDS_PER_NODE;
                for w in 0..WORDS_PER_NODE {
                    let input = Fixed::from_raw(Goldilocks::from_u64(block.nodes[node_start + w]));
                    acc.add_prod(
                        input,
                        self.input_proj[(n * WORDS_PER_NODE + w) * LITE_DIM + d],
                    );
                }
            }
            s.projected[d] = acc.finish().relu();
        }

        // 2. Hidden layer: [LITE_DIM] → [LITE_DIM] + bias + ReLU
        for d in 0..LITE_DIM {
            let mut acc = RawAccum::zero();
            acc.add_bias(self.hidden_bias[d]);
            for j in 0..LITE_DIM {
                acc.add_prod(s.projected[j], self.hidden_w[j * LITE_DIM + d]);
            }
            s.hidden[d] = acc.finish().relu();
        }

        // 3. Autoregressive decoder
        let mut output = Vec::with_capacity(MAX_OUTPUT);
        let mut prev_out = vec![Fixed::ZERO; LITE_VOCAB];

        for _ in 0..MAX_OUTPUT {
            // Decoder hidden: [LITE_DIM + LITE_VOCAB] → [LITE_FFN] + bias + ReLU
            for fh in 0..LITE_FFN {
                let mut acc = RawAccum::zero();
                acc.add_bias(self.dec_hidden_bias[fh]);
                for d in 0..LITE_DIM {
                    acc.add_prod(s.hidden[d], self.dec_hidden[d * LITE_FFN + fh]);
                }
                for d in 0..LITE_VOCAB {
                    acc.add_prod(prev_out[d], self.dec_hidden[(LITE_DIM + d) * LITE_FFN + fh]);
                }
                s.dec_h[fh] = acc.finish().relu();
            }

            // Decoder output: [LITE_FFN] → [LITE_VOCAB] + bias
            for d in 0..LITE_VOCAB {
                let mut acc = RawAccum::zero();
                acc.add_bias(self.dec_output_bias[d]);
                for fh in 0..LITE_FFN {
                    acc.add_prod(s.dec_h[fh], self.dec_output[fh * LITE_VOCAB + d]);
                }
                s.dec_out[d] = acc.finish();
            }

            // Argmax over LITE_VOCAB positions
            let mut best_val = s.dec_out[0].to_f64();
            let mut best_idx = 0u64;
            for (i, x) in s.dec_out.iter().enumerate().skip(1) {
                let v = x.to_f64();
                if v > best_val {
                    best_val = v;
                    best_idx = i as u64;
                }
            }

            if best_idx == 0 {
                break;
            }
            output.push(best_idx);
            prev_out.copy_from_slice(&s.dec_out);
        }

        output
    }

    pub fn to_weight_vec(&self) -> Vec<Fixed> {
        let mut v = Vec::with_capacity(LITE_PARAM_COUNT);
        v.extend_from_slice(&self.input_proj);
        v.extend_from_slice(&self.input_bias);
        v.extend_from_slice(&self.hidden_w);
        v.extend_from_slice(&self.hidden_bias);
        v.extend_from_slice(&self.dec_hidden);
        v.extend_from_slice(&self.dec_hidden_bias);
        v.extend_from_slice(&self.dec_output);
        v.extend_from_slice(&self.dec_output_bias);
        v
    }

    pub fn from_weight_vec(w: &[Fixed]) -> Self {
        let mut i = 0;
        let mut take = |n: usize| -> Vec<Fixed> {
            let slice = w[i..i + n].to_vec();
            i += n;
            slice
        };

        let input_proj = take(INPUT_FLAT * LITE_DIM);
        let input_bias = take(LITE_DIM);
        let hidden_w = take(LITE_DIM * LITE_DIM);
        let hidden_bias = take(LITE_DIM);
        let dec_hidden = take((LITE_DIM + LITE_VOCAB) * LITE_FFN);
        let dec_hidden_bias = take(LITE_FFN);
        let dec_output = take(LITE_FFN * LITE_VOCAB);
        let dec_output_bias = take(LITE_VOCAB);

        Self {
            input_proj,
            input_bias,
            hidden_w,
            hidden_bias,
            dec_hidden,
            dec_hidden_bias,
            dec_output,
            dec_output_bias,
            scratch: LiteScratch::new(),
        }
    }

    pub fn weight_count(&self) -> usize {
        LITE_PARAM_COUNT
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::tir::encode::CONTEXT_SIZE;

    #[test]
    fn zeros_model_produces_empty() {
        let mut model = NeuralModelLite::zeros();
        let block = TIRBlock {
            nodes: [0; MAX_NODES * WORDS_PER_NODE],
            context: [0; CONTEXT_SIZE],
            node_count: 3,
            fn_name: "test".into(),
            start_idx: 0,
            end_idx: 3,
        };
        assert!(model.forward(&block).is_empty());
    }

    #[test]
    fn weight_vec_roundtrip() {
        let model = NeuralModelLite::zeros();
        let weights = model.to_weight_vec();
        assert_eq!(weights.len(), LITE_PARAM_COUNT);
        let restored = NeuralModelLite::from_weight_vec(&weights);
        let w2 = restored.to_weight_vec();
        assert_eq!(weights, w2);
    }

    #[test]
    fn forward_deterministic() {
        let mut model =
            NeuralModelLite::from_weight_vec(&vec![Fixed::from_f64(0.01); LITE_PARAM_COUNT]);
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

    #[test]
    fn param_count_matches() {
        let model = NeuralModelLite::zeros();
        let vec = model.to_weight_vec();
        assert_eq!(vec.len(), LITE_PARAM_COUNT);
        assert_eq!(vec.len(), 10_400);
    }
}
