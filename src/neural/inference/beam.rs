//! Beam search decoder (K=32).
//!
//! Runs the Transformer decoder autoregressively with beam search,
//! applying grammar masks at each step to enforce TASM validity.
//! Returns K candidate sequences ranked by log-probability.

use burn::prelude::*;

use crate::neural::model::decoder::StackAwareDecoder;
use crate::neural::model::encoder::GnnEncoder;
use crate::neural::model::grammar::StackStateMachine;
use crate::neural::model::vocab::VOCAB_SIZE;

/// Beam search configuration.
pub struct BeamConfig {
    /// Number of beams to maintain.
    pub k: usize,
    /// Maximum output sequence length.
    pub max_steps: usize,
}

impl Default for BeamConfig {
    fn default() -> Self {
        Self {
            k: 32,
            max_steps: 256,
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
/// - `initial_stack_depth`: initial stack depth for grammar mask
///
/// Returns K candidate token sequences ranked by log-probability.
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
    let mut beam_state_machines: Vec<StackStateMachine> = (0..k)
        .map(|_| StackStateMachine::new(initial_stack_depth))
        .collect();

    // 3. Autoregressive decoding
    for step in 0..config.max_steps {
        // Check if all beams are finished
        if beam_finished.iter().all(|&f| f) {
            break;
        }

        // Build decoder inputs for all active beams
        let prev_tokens: Vec<u32> = (0..k)
            .map(|b| {
                if beam_finished[b] {
                    0 // EOS for finished beams
                } else {
                    beam_sequences[b].last().copied().unwrap_or(0)
                }
            })
            .collect();

        // Create input tensors: [K, 1] (single step)
        let token_ids = Tensor::<B, 2, Int>::from_data(
            TensorData::new(
                prev_tokens.iter().map(|&t| t as i32).collect::<Vec<_>>(),
                [k, 1],
            ),
            device,
        );
        let positions =
            Tensor::<B, 2, Int>::from_data(TensorData::new(vec![step as i32; k], [k, 1]), device);

        // Stack depths for each beam
        let depth_vals: Vec<i32> = beam_state_machines
            .iter()
            .map(|sm| sm.depth_for_embedding(65) as i32)
            .collect();
        let stack_depths =
            Tensor::<B, 2, Int>::from_data(TensorData::new(depth_vals, [k, 1]), device);

        // Type states for each beam: [K, 1, 24]
        let type_data: Vec<f32> = beam_state_machines
            .iter()
            .flat_map(|sm| sm.type_encoding())
            .collect();
        let type_states = Tensor::<B, 3>::from_data(TensorData::new(type_data, [k, 1, 24]), device);

        // Forward pass: [K, 1, VOCAB_SIZE]
        let logits = decoder.forward(
            token_ids,
            positions,
            stack_depths,
            type_states,
            memory.clone(),
        );

        // Extract logits for the single step: [K, VOCAB_SIZE]
        let logits_2d = logits.squeeze_dim::<2>(1);

        // Convert to CPU for beam management
        let logits_data = logits_2d.to_data();
        let logits_flat: Vec<f32> = logits_data.to_vec().unwrap();

        // 4. Apply grammar masks and find top-K candidates across all beams
        let mut candidates: Vec<(usize, u32, f32)> = Vec::new(); // (beam_idx, token, log_prob)

        for b in 0..k {
            if beam_finished[b] {
                // Finished beam: only EOS continuation with same score
                candidates.push((b, 0, beam_log_probs[b]));
                continue;
            }

            let mask = beam_state_machines[b].valid_mask();
            let beam_offset = b * VOCAB_SIZE;

            // Apply mask to logits and compute log-softmax
            let mut masked_logits = Vec::with_capacity(VOCAB_SIZE);
            for t in 0..VOCAB_SIZE {
                masked_logits.push(logits_flat[beam_offset + t] + mask[t]);
            }

            // Log-softmax for normalized scores
            let max_logit = masked_logits
                .iter()
                .copied()
                .fold(f32::NEG_INFINITY, f32::max);
            let log_sum_exp: f32 = masked_logits
                .iter()
                .map(|&l| (l - max_logit).exp())
                .sum::<f32>()
                .ln()
                + max_logit;

            for t in 0..VOCAB_SIZE {
                let log_prob = masked_logits[t] - log_sum_exp;
                if mask[t] > -1e8 {
                    // Only consider valid tokens
                    candidates.push((b, t as u32, beam_log_probs[b] + log_prob));
                }
            }
        }

        // Sort candidates by score (descending) and keep top K
        candidates.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
        candidates.truncate(k);

        // 5. Update beams
        let mut new_sequences: Vec<Vec<u32>> = Vec::with_capacity(k);
        let mut new_log_probs: Vec<f32> = Vec::with_capacity(k);
        let mut new_finished: Vec<bool> = Vec::with_capacity(k);
        let mut new_state_machines: Vec<StackStateMachine> = Vec::with_capacity(k);

        for &(src_beam, token, log_prob) in candidates.iter().take(k) {
            let mut seq = beam_sequences[src_beam].clone();
            let finished = beam_finished[src_beam] || token == 0;

            if !beam_finished[src_beam] && token != 0 {
                seq.push(token);
            }

            // Clone state machine and advance
            let mut sm = StackStateMachine::new(initial_stack_depth);
            // Replay sequence to rebuild state (simpler than cloning SM internals)
            for &t in &seq {
                sm.step(t);
            }

            new_sequences.push(seq);
            new_log_probs.push(log_prob);
            new_finished.push(finished);
            new_state_machines.push(sm);
        }

        // Pad if fewer than K candidates
        while new_sequences.len() < k {
            new_sequences.push(vec![]);
            new_log_probs.push(f32::NEG_INFINITY);
            new_finished.push(true);
            new_state_machines.push(StackStateMachine::new(initial_stack_depth));
        }

        beam_sequences = new_sequences;
        beam_log_probs = new_log_probs;
        beam_finished = new_finished;
        beam_state_machines = new_state_machines;
    }

    // Sort final results by log-prob
    let mut indexed: Vec<(usize, f32)> = beam_log_probs.iter().copied().enumerate().collect();
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
