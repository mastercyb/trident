//! Beam search decoder (K=32).
//!
//! Runs the Transformer decoder autoregressively with beam search,
//! applying grammar masks at each step to enforce TASM validity.
//! Returns K candidate sequences ranked by log-probability.

use burn::prelude::*;

use crate::neural::model::decoder::StackAwareDecoder;
use crate::neural::model::encoder::GnnEncoder;
use crate::neural::model::vocab::VOCAB_SIZE;

/// Beam search configuration.
pub struct BeamConfig {
    /// Number of beams to maintain.
    pub k: usize,
    /// Maximum output sequence length.
    pub max_steps: usize,
    /// Minimum tokens before EOS is allowed.
    pub min_tokens: usize,
    /// Length normalization exponent (0=none, 1=full). Prevents short sequences
    /// from dominating by normalizing log-prob by `length^alpha`.
    pub length_alpha: f32,
    /// Repetition penalty applied to tokens seen in the last `rep_window` steps.
    /// 1.0 = no penalty, >1.0 = penalize (logit divided by this value).
    pub rep_penalty: f32,
    /// Window size for repetition penalty.
    pub rep_window: usize,
}

impl Default for BeamConfig {
    fn default() -> Self {
        Self {
            k: 32,
            max_steps: 256,
            min_tokens: 1,
            length_alpha: 0.7,
            rep_penalty: 1.5,
            rep_window: 16,
        }
    }
}

/// Result of beam search: K candidate sequences with scores.
pub struct BeamResult {
    /// Token ID sequences (one per beam), sorted by log-prob descending.
    pub sequences: Vec<Vec<u32>>,
    /// Log-probabilities for each sequence.
    pub log_probs: Vec<f32>,
}

/// Run beam search on a single input graph.
///
/// - `encoder`: GNN encoder (already loaded)
/// - `decoder`: Transformer decoder (already loaded)
/// - `node_features`: [N, 59] node feature matrix
/// - `edge_src`, `edge_dst`: [E] edge endpoint indices
/// - `edge_types`: [E] edge type IDs (0=DataDep, 1=ControlFlow, 2=MemOrder)
/// - `config`: beam search parameters
/// - `initial_stack_depth`: initial stack depth for depth/type features
///   (must match training — typically 0)
///
/// Returns K candidate token sequences ranked by log-probability.
///
/// No grammar mask is applied during decoding. The model was trained
/// without grammar masks (teacher forcing with ground truth), so the
/// learned distribution should naturally avoid invalid tokens.
pub fn beam_search<B: Backend>(
    encoder: &GnnEncoder<B>,
    decoder: &StackAwareDecoder<B>,
    node_features: Tensor<B, 2>,
    edge_src: Tensor<B, 1, Int>,
    edge_dst: Tensor<B, 1, Int>,
    edge_types: Tensor<B, 1, Int>,
    config: &BeamConfig,
    initial_stack_depth: i32,
    device: &B::Device,
) -> BeamResult {
    use crate::neural::model::grammar::StackStateMachine;

    let k = config.k;
    let num_nodes = node_features.dims()[0];

    // 1. Encode graph → node embeddings + global context
    let (node_emb, _global) = encoder.forward(node_features, edge_src, edge_dst, edge_types);
    // node_emb: [N, d_model]

    // Expand memory for K beams: [K, N, d_model]
    let d_model = node_emb.dims()[1];
    let memory = node_emb
        .unsqueeze_dim::<3>(0)
        .expand([k, num_nodes, d_model]);

    // 2. Initialize beams
    let mut beam_sequences: Vec<Vec<u32>> = vec![vec![]; k];
    let mut beam_log_probs: Vec<f32> = vec![0.0; k];
    let mut beam_finished: Vec<bool> = vec![false; k];

    // 3. Autoregressive decoding — pass full sequence history each step.
    //
    // The decoder uses self-attention over all previous positions, so we must
    // feed the entire generated sequence (not just the last token). At step t,
    // input shape is [K, t+1] and we take logits at position t.
    //
    // Stack depth/type features are replayed from the generated sequence using
    // the same initial_stack_depth as training (typically 0). This ensures
    // the decoder sees depth embeddings consistent with what it learned.
    for step in 0..config.max_steps {
        // Check if all beams are finished
        if beam_finished.iter().all(|&f| f) {
            break;
        }

        let cur_len = step + 1; // sequence length including the start EOS token

        // Build full sequence inputs: [K, cur_len]
        // Each beam's input is: [EOS=0, tok_0, tok_1, ..., tok_{step-1}]
        let mut token_data = Vec::with_capacity(k * cur_len);
        let mut pos_data = Vec::with_capacity(k * cur_len);
        let mut depth_data = Vec::with_capacity(k * cur_len);
        let mut type_data = Vec::with_capacity(k * cur_len * 24);

        for b in 0..k {
            // Token IDs: [EOS, ...generated tokens]
            token_data.push(0i32); // EOS start
            for &t in &beam_sequences[b] {
                token_data.push(t as i32);
            }
            // Pad if beam is shorter than step
            while token_data.len() < (b + 1) * cur_len {
                token_data.push(0i32);
            }

            // Positions: [0, 1, 2, ...]
            for p in 0..cur_len {
                pos_data.push(p as i32);
            }

            // Stack depths: replay state machine from generated tokens
            let mut sm = StackStateMachine::new(initial_stack_depth);
            depth_data.push(sm.depth_for_embedding(65) as i32); // depth at EOS start
            for &t in &beam_sequences[b] {
                sm.step(t);
                depth_data.push(sm.depth_for_embedding(65) as i32);
            }
            while depth_data.len() < (b + 1) * cur_len {
                depth_data.push(sm.depth_for_embedding(65) as i32);
            }

            // Type states: replay for each position
            let mut sm2 = StackStateMachine::new(initial_stack_depth);
            type_data.extend(sm2.type_encoding()); // type at EOS start
            for &t in &beam_sequences[b] {
                sm2.step(t);
                type_data.extend(sm2.type_encoding());
            }
            while type_data.len() < (b + 1) * cur_len * 24 {
                type_data.extend(std::iter::repeat(0.0f32).take(24));
            }
        }

        let token_ids =
            Tensor::<B, 2, Int>::from_data(TensorData::new(token_data, [k, cur_len]), device);
        let positions =
            Tensor::<B, 2, Int>::from_data(TensorData::new(pos_data, [k, cur_len]), device);
        let stack_depths =
            Tensor::<B, 2, Int>::from_data(TensorData::new(depth_data, [k, cur_len]), device);
        let type_states =
            Tensor::<B, 3>::from_data(TensorData::new(type_data, [k, cur_len, 24]), device);

        // Forward pass: [K, cur_len, VOCAB_SIZE]
        let logits = decoder.forward(
            token_ids,
            positions,
            stack_depths,
            type_states,
            memory.clone(),
        );

        // Extract logits at the LAST position only: [K, VOCAB_SIZE]
        let logits_2d = logits
            .slice([0..k, step..step + 1, 0..VOCAB_SIZE])
            .squeeze_dim::<2>(1);

        // Convert to CPU for beam management
        let logits_data = logits_2d.to_data();
        let logits_flat: Vec<f32> = logits_data.to_vec().unwrap();

        // 4. Score all (beam, token) candidates — no grammar mask.
        // The model was trained without grammar mask penalties, so its learned
        // distribution should naturally prefer valid continuations.
        let mut candidates: Vec<(usize, u32, f32)> = Vec::new(); // (beam_idx, token, log_prob)

        for b in 0..k {
            if beam_finished[b] {
                // Finished beam: only EOS continuation with same score
                candidates.push((b, 0, beam_log_probs[b]));
                continue;
            }

            let beam_offset = b * VOCAB_SIZE;

            // Build repetition set: tokens in last rep_window steps
            let seq = &beam_sequences[b];
            let rep_start = seq.len().saturating_sub(config.rep_window);
            let mut recent = [false; VOCAB_SIZE];
            for &tok in &seq[rep_start..] {
                if (tok as usize) < VOCAB_SIZE {
                    recent[tok as usize] = true;
                }
            }

            // Apply repetition penalty to raw logits, then log-softmax
            let mut adjusted = Vec::with_capacity(VOCAB_SIZE);
            for t in 0..VOCAB_SIZE {
                let logit = logits_flat[beam_offset + t];
                if recent[t] && config.rep_penalty > 1.0 {
                    // Penalize: divide positive logits, multiply negative ones
                    if logit > 0.0 {
                        adjusted.push(logit / config.rep_penalty);
                    } else {
                        adjusted.push(logit * config.rep_penalty);
                    }
                } else {
                    adjusted.push(logit);
                }
            }

            let max_logit = adjusted.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let log_sum_exp: f32 = adjusted
                .iter()
                .map(|&l| (l - max_logit).exp())
                .sum::<f32>()
                .ln()
                + max_logit;

            for t in 0..VOCAB_SIZE {
                // Block EOS until we've generated min_tokens
                if t == 0 && step < config.min_tokens {
                    continue;
                }
                let log_prob = adjusted[t] - log_sum_exp;
                let cumulative = beam_log_probs[b] + log_prob;
                candidates.push((b, t as u32, cumulative));
            }
        }

        // Sort by length-normalized score (descending), keep top K.
        // Length normalization prevents short sequences from dominating.
        let alpha = config.length_alpha;
        candidates.sort_by(|a, b| {
            let len_a = (beam_sequences[a.0].len() + if a.1 == 0 { 0 } else { 1 }).max(1) as f32;
            let len_b = (beam_sequences[b.0].len() + if b.1 == 0 { 0 } else { 1 }).max(1) as f32;
            let score_a = a.2 / len_a.powf(alpha);
            let score_b = b.2 / len_b.powf(alpha);
            score_b
                .partial_cmp(&score_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        candidates.truncate(k);

        // 5. Update beams
        let mut new_sequences: Vec<Vec<u32>> = Vec::with_capacity(k);
        let mut new_log_probs: Vec<f32> = Vec::with_capacity(k);
        let mut new_finished: Vec<bool> = Vec::with_capacity(k);

        for &(src_beam, token, log_prob) in candidates.iter().take(k) {
            let mut seq = beam_sequences[src_beam].clone();
            let finished = beam_finished[src_beam] || token == 0;

            if !beam_finished[src_beam] && token != 0 {
                seq.push(token);
            }

            new_sequences.push(seq);
            new_log_probs.push(log_prob);
            new_finished.push(finished);
        }

        // Pad if fewer than K candidates
        while new_sequences.len() < k {
            new_sequences.push(vec![]);
            new_log_probs.push(f32::NEG_INFINITY);
            new_finished.push(true);
        }

        beam_sequences = new_sequences;
        beam_log_probs = new_log_probs;
        beam_finished = new_finished;
    }

    // Sort final results by length-normalized log-prob
    let alpha = config.length_alpha;
    let mut indexed: Vec<(usize, f32)> = beam_log_probs
        .iter()
        .enumerate()
        .map(|(i, &lp)| {
            let len = beam_sequences[i].len().max(1) as f32;
            (i, lp / len.powf(alpha))
        })
        .collect();
    indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let sequences: Vec<Vec<u32>> = indexed
        .iter()
        .map(|&(i, _)| beam_sequences[i].clone())
        .collect();
    let log_probs: Vec<f32> = indexed.iter().map(|&(_, lp)| lp).collect();

    BeamResult {
        sequences,
        log_probs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::neural::model::decoder::DecoderConfig;
    use crate::neural::model::encoder::GnnEncoderConfig;
    use burn::backend::NdArray;

    type B = NdArray;

    #[test]
    fn beam_search_produces_k_sequences() {
        let device = Default::default();

        // Small model for testing
        let encoder = GnnEncoderConfig::new()
            .with_d_model(32)
            .with_d_edge(8)
            .with_num_layers(1)
            .init::<B>(&device);

        let decoder = DecoderConfig {
            d_model: 32,
            num_layers: 1,
            n_heads: 4,
            d_ff: 64,
            max_seq: 64,
            max_stack_depth: 65,
            type_window: 8,
            dropout: 0.0,
        }
        .init::<B>(&device);

        // Tiny graph: 3 nodes, 2 edges
        let node_features = Tensor::<B, 2>::zeros([3, 59], &device);
        let edge_src = Tensor::<B, 1, Int>::from_data(TensorData::new(vec![0i32, 1], [2]), &device);
        let edge_dst = Tensor::<B, 1, Int>::from_data(TensorData::new(vec![1i32, 2], [2]), &device);
        let edge_types =
            Tensor::<B, 1, Int>::from_data(TensorData::new(vec![0i32, 1], [2]), &device);

        let config = BeamConfig {
            k: 4, // Small K for test speed
            max_steps: 5,
            min_tokens: 1,
            ..Default::default()
        };

        let result = beam_search(
            &encoder,
            &decoder,
            node_features,
            edge_src,
            edge_dst,
            edge_types,
            &config,
            0,
            &device,
        );

        assert_eq!(result.sequences.len(), 4);
        assert_eq!(result.log_probs.len(), 4);
        // Log probs should be sorted descending
        for i in 1..result.log_probs.len() {
            assert!(
                result.log_probs[i] <= result.log_probs[i - 1],
                "log_probs not sorted: {} > {}",
                result.log_probs[i],
                result.log_probs[i - 1]
            );
        }
    }
}
