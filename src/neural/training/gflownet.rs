//! Stage 2: GFlowNet training with Trajectory Balance loss.
//!
//! After supervised pre-training, the model is fine-tuned using GFlowNets
//! to explore the space of valid TASM programs. The reward signal comes
//! from actual clock cycle improvements over the compiler baseline.

use burn::prelude::*;

use crate::neural::model::composite::NeuralCompilerV2;
use crate::neural::model::grammar::StackStateMachine;
use crate::neural::model::vocab::{Vocab, VOCAB_SIZE};
use crate::neural::training::supervised::{graph_to_edges, graph_to_features};

/// GFlowNet training configuration.
pub struct GFlowNetConfig {
    /// Initial temperature for sampling.
    pub tau_start: f32,
    /// Final temperature.
    pub tau_end: f32,
    /// Total steps over which temperature anneals.
    pub anneal_steps: usize,
    /// Maximum sequence length for sampling.
    pub max_seq_len: usize,
    /// Epsilon floor for reward (prevents log(0)).
    pub reward_epsilon: f32,
    /// Steps of partial credit shaping before switching to pure reward.
    pub shaping_steps: usize,
    /// Validity threshold to disable shaping.
    pub shaping_validity_threshold: f32,
}

impl Default for GFlowNetConfig {
    fn default() -> Self {
        Self {
            tau_start: 2.0,
            tau_end: 0.5,
            anneal_steps: 10_000,
            max_seq_len: 256,
            reward_epsilon: 1e-3,
            shaping_steps: 1000,
            shaping_validity_threshold: 0.7,
        }
    }
}

/// Reward for a generated TASM sequence.
///
/// R(tasm) = epsilon                                          if !valid
///         = 1 + max(0, (compiler_cycles - model_cycles)      if valid
///                      / compiler_cycles)
pub fn compute_reward(
    valid: bool,
    model_cycles: Option<u64>,
    compiler_cycles: u64,
    epsilon: f32,
) -> f32 {
    if !valid || model_cycles.is_none() {
        return epsilon;
    }
    let mc = model_cycles.unwrap() as f32;
    let cc = compiler_cycles as f32;
    if cc <= 0.0 {
        return 1.0;
    }
    1.0 + ((cc - mc) / cc).max(0.0)
}

/// Partial credit shaped reward for early training.
///
/// R_shaped = epsilon + (k / total_length) * validity_bonus
/// where k = step of first stack violation.
pub fn compute_shaped_reward(
    first_violation_step: Option<usize>,
    total_length: usize,
    epsilon: f32,
) -> f32 {
    match first_violation_step {
        None => {
            // No violation — full validity bonus
            epsilon + 1.0
        }
        Some(k) => {
            if total_length == 0 {
                return epsilon;
            }
            epsilon + (k as f32 / total_length as f32)
        }
    }
}

/// Temperature at a given training step (linear annealing).
pub fn temperature_at_step(step: usize, config: &GFlowNetConfig) -> f32 {
    if step >= config.anneal_steps {
        return config.tau_end;
    }
    let progress = step as f32 / config.anneal_steps as f32;
    config.tau_start + (config.tau_end - config.tau_start) * progress
}

/// Sample a sequence from the model using temperature-scaled logits.
///
/// Returns (token_sequence, log_forward_prob, first_violation_step).
pub fn sample_sequence<B: Backend>(
    model: &NeuralCompilerV2<B>,
    graph: &crate::neural::data::tir_graph::TirGraph,
    tau: f32,
    config: &GFlowNetConfig,
    device: &B::Device,
) -> (Vec<u32>, f32, Option<usize>) {
    let node_features = graph_to_features::<B>(graph, device);
    let (edge_src, edge_dst, edge_types) = graph_to_edges::<B>(graph, device);

    // Encode graph
    let (node_emb, _global) = model
        .encoder
        .forward(node_features, edge_src, edge_dst, edge_types);
    let d_model = node_emb.dims()[1];
    let num_nodes = node_emb.dims()[0];
    let memory = node_emb.unsqueeze_dim::<3>(0); // [1, N, d_model]

    let initial_depth = 0i32; // must match training
    let mut tokens = Vec::new();
    let mut log_pf = 0.0f32;
    let mut first_violation: Option<usize> = None;

    for step in 0..config.max_seq_len {
        let cur_len = step + 1;

        // Build full sequence: [EOS=0, tok_0, ..., tok_{step-1}]
        let mut token_data = vec![0i32]; // EOS start
        for &t in &tokens {
            token_data.push(t as i32);
        }
        let pos_data: Vec<i32> = (0..cur_len as i32).collect();

        // Stack depths: replay state machine
        let mut depth_data = Vec::with_capacity(cur_len);
        let mut sm_replay = StackStateMachine::new(initial_depth);
        depth_data.push(sm_replay.depth_for_embedding(65) as i32);
        for &t in &tokens {
            sm_replay.step(t);
            depth_data.push(sm_replay.depth_for_embedding(65) as i32);
        }

        // Type states: replay
        let mut type_data = Vec::with_capacity(cur_len * 24);
        let mut sm_replay2 = StackStateMachine::new(initial_depth);
        type_data.extend(sm_replay2.type_encoding());
        for &t in &tokens {
            sm_replay2.step(t);
            type_data.extend(sm_replay2.type_encoding());
        }

        let token_ids =
            Tensor::<B, 2, Int>::from_data(TensorData::new(token_data, [1, cur_len]), device);
        let positions =
            Tensor::<B, 2, Int>::from_data(TensorData::new(pos_data, [1, cur_len]), device);
        let stack_depths =
            Tensor::<B, 2, Int>::from_data(TensorData::new(depth_data, [1, cur_len]), device);
        let type_states =
            Tensor::<B, 3>::from_data(TensorData::new(type_data, [1, cur_len, 24]), device);
        let memory_expanded = memory.clone().expand([1, num_nodes, d_model]);

        // Forward pass: [1, cur_len, VOCAB_SIZE]
        let logits = model.decoder.forward(
            token_ids,
            positions,
            stack_depths,
            type_states,
            memory_expanded,
        );
        // Extract logits at the last position: [VOCAB_SIZE]
        let logits_1d = logits
            .slice([0..1, step..step + 1, 0..VOCAB_SIZE])
            .squeeze_dim::<2>(0)
            .squeeze_dim::<1>(0);
        let logits_data: Vec<f32> = logits_1d.to_data().to_vec().unwrap();

        // Temperature-scaled softmax — no grammar mask (matches training).
        // Track violations via separate StackStateMachine for shaped reward.
        let scaled: Vec<f32> = logits_data.iter().map(|&l| l / tau).collect();
        let max_l = scaled.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let probs: Vec<f32> = scaled.iter().map(|&l| (l - max_l).exp()).collect();
        let sum: f32 = probs.iter().sum();
        let probs: Vec<f32> = probs.iter().map(|&p| p / sum).collect();

        // Sample from categorical distribution
        let token = sample_categorical(&probs);

        // Track validity using a separate state machine (for shaped reward only)
        if first_violation.is_none() && token != 0 {
            let mut sm_check = StackStateMachine::new(initial_depth);
            for &t in &tokens {
                sm_check.step(t);
            }
            let mask = sm_check.valid_mask();
            if mask[token as usize] < -1e8 {
                first_violation = Some(step);
            }
        }

        // Accumulate log forward probability
        let prob = probs[token as usize].max(1e-10);
        log_pf += prob.ln();

        if token == 0 {
            break; // EOS
        }

        tokens.push(token);
    }

    (tokens, log_pf, first_violation)
}

/// Sample from a categorical distribution using inverse-CDF.
///
/// Uses thread-local xorshift64 PRNG — fast, no external deps, non-deterministic.
fn sample_categorical(probs: &[f32]) -> u32 {
    use std::cell::Cell;
    thread_local! {
        static RNG_STATE: Cell<u64> = Cell::new(
            // Seed from system time (nanos) to avoid deterministic argmax trap
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0xDEAD_BEEF_CAFE_1337) | 1
        );
    }

    let u: f32 = RNG_STATE.with(|s| {
        let mut x = s.get();
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        s.set(x);
        // Map to [0, 1)
        (x >> 40) as f32 / (1u64 << 24) as f32
    });

    // Inverse-CDF sampling
    let mut cumulative = 0.0f32;
    for (i, &p) in probs.iter().enumerate() {
        cumulative += p;
        if u < cumulative {
            return i as u32;
        }
    }
    // Fallback to last token (numerical edge case)
    (probs.len() - 1) as u32
}

/// Trajectory Balance loss.
///
/// L_TB = (log_Z + log_P_F - log_P_B - log_R)^2
///
/// Where:
/// - log_Z: learned log-partition function (trained jointly)
/// - log_P_F: sum of log P(token_t | history) from forward sampling
/// - log_P_B: uniform backward policy (constant)
/// - log_R: log(reward), clipped >= log(epsilon)
pub fn tb_loss<B: Backend>(
    log_pf: f32,
    log_pb: f32,
    log_r: f32,
    log_z: Tensor<B, 1>,
    device: &B::Device,
) -> Tensor<B, 1> {
    let pf_tensor = Tensor::<B, 1>::from_data(TensorData::new(vec![log_pf], [1]), device);
    let pb_tensor = Tensor::<B, 1>::from_data(TensorData::new(vec![log_pb], [1]), device);
    let r_tensor = Tensor::<B, 1>::from_data(TensorData::new(vec![log_r], [1]), device);

    let residual = log_z + pf_tensor - pb_tensor - r_tensor;
    residual.clone() * residual
}

/// Run one GFlowNet training step.
///
/// Samples a sequence, computes reward, and returns TB loss.
pub fn gflownet_step<B: burn::tensor::backend::AutodiffBackend>(
    model: &NeuralCompilerV2<B>,
    graph: &crate::neural::data::tir_graph::TirGraph,
    baseline_tasm: &[String],
    compiler_cycles: u64,
    log_z: Tensor<B, 1>,
    step: usize,
    config: &GFlowNetConfig,
    vocab: &Vocab,
    device: &B::Device,
) -> (Tensor<B, 1>, f32, bool) {
    let tau = temperature_at_step(step, config);

    // Sample sequence from model
    let (tokens, log_pf, first_violation) = sample_sequence(model, graph, tau, config, device);

    // Decode and validate
    let tasm_lines = vocab.decode_sequence(&tokens);
    let valid = if tasm_lines.is_empty() {
        false
    } else {
        crate::cost::stack_verifier::verify_equivalent(baseline_tasm, &tasm_lines, 42)
    };

    let model_cycles = if valid {
        let line_refs: Vec<&str> = tasm_lines.iter().map(|s| s.as_str()).collect();
        Some(crate::cost::scorer::profile_tasm(&line_refs).cost())
    } else {
        None
    };

    // Compute reward
    let reward = if step < config.shaping_steps
        && (step as f32 / config.shaping_steps as f32) < config.shaping_validity_threshold
    {
        compute_shaped_reward(first_violation, tokens.len(), config.reward_epsilon)
    } else {
        compute_reward(valid, model_cycles, compiler_cycles, config.reward_epsilon)
    };

    let log_r = reward.max(config.reward_epsilon).ln();
    let log_pb = 0.0; // Uniform backward policy

    let loss = tb_loss(log_pf, log_pb, log_r, log_z, device);

    (loss, reward, valid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reward_valid_improvement() {
        let r = compute_reward(true, Some(5), 10, 1e-3);
        assert!((r - 1.5).abs() < 0.01); // 1 + (10-5)/10 = 1.5
    }

    #[test]
    fn reward_valid_no_improvement() {
        let r = compute_reward(true, Some(10), 10, 1e-3);
        assert!((r - 1.0).abs() < 0.01); // 1 + max(0, 0) = 1.0
    }

    #[test]
    fn reward_invalid() {
        let r = compute_reward(false, None, 10, 1e-3);
        assert!((r - 1e-3).abs() < 1e-6);
    }

    #[test]
    fn shaped_reward_no_violation() {
        let r = compute_shaped_reward(None, 10, 1e-3);
        assert!((r - 1.001).abs() < 0.01);
    }

    #[test]
    fn shaped_reward_early_violation() {
        let r = compute_shaped_reward(Some(3), 10, 1e-3);
        // epsilon + 3/10 = 0.001 + 0.3 = 0.301
        assert!((r - 0.301).abs() < 0.01);
    }

    #[test]
    fn temperature_annealing() {
        let config = GFlowNetConfig {
            tau_start: 2.0,
            tau_end: 0.5,
            anneal_steps: 100,
            ..Default::default()
        };
        assert!((temperature_at_step(0, &config) - 2.0).abs() < 0.01);
        assert!((temperature_at_step(50, &config) - 1.25).abs() < 0.01);
        assert!((temperature_at_step(100, &config) - 0.5).abs() < 0.01);
        assert!((temperature_at_step(200, &config) - 0.5).abs() < 0.01);
    }

    #[test]
    fn tb_loss_zero_residual() {
        use burn::backend::NdArray;
        let device = Default::default();
        // When log_Z + log_PF - log_PB - log_R = 0, loss should be 0
        let log_z = Tensor::<NdArray, 1>::from_data(TensorData::new(vec![1.0f32], [1]), &device);
        let loss = tb_loss::<NdArray>(2.0, 1.0, 2.0, log_z, &device);
        // residual = 1 + 2 - 1 - 2 = 0
        let val: Vec<f32> = loss.to_data().to_vec().unwrap();
        assert!(val[0].abs() < 1e-6);
    }

    #[test]
    fn tb_loss_nonzero_residual() {
        use burn::backend::NdArray;
        let device = Default::default();
        let log_z = Tensor::<NdArray, 1>::from_data(TensorData::new(vec![0.0f32], [1]), &device);
        let loss = tb_loss::<NdArray>(1.0, 0.0, 0.5, log_z, &device);
        // residual = 0 + 1 - 0 - 0.5 = 0.5, loss = 0.25
        let val: Vec<f32> = loss.to_data().to_vec().unwrap();
        assert!((val[0] - 0.25).abs() < 1e-4);
    }
}
