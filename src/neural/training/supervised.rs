//! Stage 1: Supervised pre-training with cross-entropy loss.
//!
//! Teacher forcing with grammar mask penalties. Trains the composite
//! model (GNN encoder + Transformer decoder) on (TirGraph, TASM) pairs.

use burn::grad_clipping::GradientClippingConfig;
use burn::optim::{AdamWConfig, GradientsParams, Optimizer};
use burn::prelude::*;
use burn::tensor::activation;

use crate::neural::data::pairs::TrainingPair;
use crate::neural::data::tir_graph::NODE_FEATURE_DIM;
use crate::neural::model::composite::NeuralCompilerV2;
use crate::neural::model::grammar::precompute_sequence_state;
use crate::neural::model::vocab::VOCAB_SIZE;

/// Supervised training configuration.
pub struct SupervisedConfig {
    /// Initial learning rate.
    pub lr: f64,
    /// Minimum learning rate (cosine decay target).
    pub lr_min: f64,
    /// Weight decay.
    pub weight_decay: f64,
    /// Gradient clipping norm.
    pub grad_clip: f32,
    /// Maximum epochs.
    pub max_epochs: usize,
    /// Early stopping patience (epochs without improvement).
    pub patience: usize,
}

impl Default for SupervisedConfig {
    fn default() -> Self {
        Self {
            lr: 3e-4,
            lr_min: 1e-5,
            weight_decay: 0.01,
            grad_clip: 1.0,
            max_epochs: 100,
            patience: 3,
        }
    }
}

/// Cosine annealing learning rate: lr_min + 0.5*(lr - lr_min)*(1 + cos(pi*t/T))
pub fn cosine_lr(config: &SupervisedConfig, epoch: usize, total_epochs: usize) -> f64 {
    if total_epochs <= 1 {
        return config.lr;
    }
    let t = epoch as f64 / total_epochs as f64;
    config.lr_min + 0.5 * (config.lr - config.lr_min) * (1.0 + (std::f64::consts::PI * t).cos())
}

/// Result of one training epoch.
pub struct EpochResult {
    /// Average cross-entropy loss over all pairs.
    pub avg_loss: f32,
    /// Number of training pairs processed.
    pub num_pairs: usize,
}

/// Train one epoch of supervised learning on the given pairs.
///
/// Uses teacher forcing: at each step, the ground-truth previous token
/// is provided as input. Grammar masks are applied as logit penalties.
///
/// Returns the model with updated weights and the epoch result.
pub fn train_epoch<B: burn::tensor::backend::AutodiffBackend>(
    model: NeuralCompilerV2<B>,
    pairs: &[TrainingPair],
    optimizer: &mut impl Optimizer<NeuralCompilerV2<B>, B>,
    lr: f64,
    device: &B::Device,
) -> (NeuralCompilerV2<B>, EpochResult) {
    let mut total_loss = 0.0f32;
    let mut model = model;

    for pair in pairs {
        // 1. Prepare graph inputs
        let node_features = graph_to_features::<B>(&pair.graph, device);
        let (edge_src, edge_dst, edge_types) = graph_to_edges::<B>(&pair.graph, device);

        // 2. Encode graph
        let (node_emb, _global) =
            model
                .encoder
                .forward(node_features, edge_src, edge_dst, edge_types);
        // node_emb: [N, d_model] → expand to [1, N, d_model] for batch=1
        let d_model = node_emb.dims()[1];
        let num_nodes = node_emb.dims()[0];
        let memory = node_emb.unsqueeze_dim::<3>(0);

        // 3. Prepare decoder inputs (teacher forcing)
        // Truncate to max_seq=256 to fit position embedding table
        const MAX_SEQ: usize = 256;
        let tokens = if pair.target_tokens.len() > MAX_SEQ {
            &pair.target_tokens[..MAX_SEQ]
        } else {
            &pair.target_tokens
        };
        let seq_len = tokens.len();
        if seq_len < 2 {
            continue; // Need at least input + one target
        }

        // Input tokens: [0, t0, t1, ..., t_{n-2}] (shifted right, prepend EOS=0)
        let mut input_tokens = vec![0i32]; // Start with EOS
        for &t in &tokens[..seq_len - 1] {
            input_tokens.push(t as i32);
        }
        let token_ids =
            Tensor::<B, 2, Int>::from_data(TensorData::new(input_tokens, [1, seq_len]), device);

        // Positions: [0, 1, 2, ...]
        let positions = Tensor::<B, 2, Int>::from_data(
            TensorData::new((0..seq_len as i32).collect::<Vec<_>>(), [1, seq_len]),
            device,
        );

        // Precompute grammar state for the (truncated) target sequence
        let state = precompute_sequence_state(tokens, 0);

        let stack_depths = Tensor::<B, 2, Int>::from_data(
            TensorData::new(
                state
                    .depths
                    .iter()
                    .map(|&d| (d as i32).min(64))
                    .collect::<Vec<_>>(),
                [1, seq_len],
            ),
            device,
        );

        let type_data: Vec<f32> = state.type_states.into_iter().flatten().collect();
        let type_states =
            Tensor::<B, 3>::from_data(TensorData::new(type_data, [1, seq_len, 24]), device);

        // 4. Forward pass
        let memory_expanded = memory.expand([1, num_nodes, d_model]);
        let logits = model.decoder.forward(
            token_ids,
            positions,
            stack_depths,
            type_states,
            memory_expanded,
        );
        // logits: [1, seq_len, VOCAB_SIZE]

        // 5. Apply grammar mask penalties
        let mask_data: Vec<f32> = state.masks.into_iter().flatten().collect();
        let grammar_mask =
            Tensor::<B, 3>::from_data(TensorData::new(mask_data, [1, seq_len, VOCAB_SIZE]), device);
        let masked_logits = logits + grammar_mask;

        // 6. Cross-entropy loss
        // Target: [1, seq_len]
        let targets = Tensor::<B, 2, Int>::from_data(
            TensorData::new(
                tokens.iter().map(|&t| t as i32).collect::<Vec<_>>(),
                [1, seq_len],
            ),
            device,
        );

        let loss = cross_entropy_loss(masked_logits, targets);
        let loss_val: f32 = loss.clone().into_data().to_vec::<f32>().unwrap()[0];
        total_loss += loss_val;

        // 7. Backward pass + optimizer step
        let grads = loss.backward();
        let grads = GradientsParams::from_grads(grads, &model);
        model = optimizer.step(lr, model, grads);
    }

    let avg_loss = if pairs.is_empty() {
        0.0
    } else {
        total_loss / pairs.len() as f32
    };

    (
        model,
        EpochResult {
            avg_loss,
            num_pairs: pairs.len(),
        },
    )
}

/// Cross-entropy loss between logits and targets.
/// logits: [batch, seq, vocab], targets: [batch, seq]
fn cross_entropy_loss<B: Backend>(
    logits: Tensor<B, 3>,
    targets: Tensor<B, 2, Int>,
) -> Tensor<B, 1> {
    let [batch, seq, vocab] = logits.dims();

    // Reshape to [batch*seq, vocab] for softmax
    let logits_flat = logits.reshape([batch * seq, vocab]);
    let targets_flat = targets.reshape([batch * seq]);

    // Log-softmax
    let log_probs = activation::log_softmax(logits_flat, 1);

    // Gather the log-prob of the target class
    let targets_2d: Tensor<B, 2, Int> = targets_flat.unsqueeze_dim::<2>(1);
    let selected = log_probs.gather(1, targets_2d); // [batch*seq, 1]

    // Negative mean
    selected.mean().neg().unsqueeze()
}

/// Convert TirGraph nodes to a feature tensor.
pub fn graph_to_features<B: Backend>(
    graph: &crate::neural::data::tir_graph::TirGraph,
    device: &B::Device,
) -> Tensor<B, 2> {
    let num_nodes = graph.nodes.len();
    let mut data = vec![0.0f32; num_nodes * NODE_FEATURE_DIM];
    for (i, node) in graph.nodes.iter().enumerate() {
        let fv = node.feature_vector();
        data[i * NODE_FEATURE_DIM..(i + 1) * NODE_FEATURE_DIM].copy_from_slice(&fv);
    }
    Tensor::from_data(TensorData::new(data, [num_nodes, NODE_FEATURE_DIM]), device)
}

/// Convert TirGraph edges to index tensors.
pub fn graph_to_edges<B: Backend>(
    graph: &crate::neural::data::tir_graph::TirGraph,
    device: &B::Device,
) -> (Tensor<B, 1, Int>, Tensor<B, 1, Int>, Tensor<B, 1, Int>) {
    let num_edges = graph.edges.len().max(1); // Need at least 1 edge for burn
    let mut src = vec![0i32; num_edges];
    let mut dst = vec![0i32; num_edges];
    let mut types = vec![0i32; num_edges];

    for (i, &(s, d, ref kind)) in graph.edges.iter().enumerate() {
        src[i] = s as i32;
        dst[i] = d as i32;
        types[i] = match kind {
            crate::neural::data::tir_graph::EdgeKind::DataDep => 0,
            crate::neural::data::tir_graph::EdgeKind::ControlFlow => 1,
            crate::neural::data::tir_graph::EdgeKind::MemOrder => 2,
        };
    }

    (
        Tensor::from_data(TensorData::new(src, [num_edges]), device),
        Tensor::from_data(TensorData::new(dst, [num_edges]), device),
        Tensor::from_data(TensorData::new(types, [num_edges]), device),
    )
}

/// Create an AdamW optimizer with gradient clipping.
pub fn create_optimizer<B: burn::tensor::backend::AutodiffBackend>(
    config: &SupervisedConfig,
) -> impl Optimizer<NeuralCompilerV2<B>, B> {
    AdamWConfig::new()
        .with_weight_decay(config.weight_decay as f32)
        .with_grad_clipping(Some(GradientClippingConfig::Norm(config.grad_clip)))
        .init()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::tir::TIROp;
    use crate::neural::data::pairs::extract_pairs;
    use crate::neural::model::composite::NeuralCompilerConfig;
    use crate::neural::model::vocab::Vocab;
    use burn::backend::Autodiff;
    use burn::backend::NdArray;

    type B = Autodiff<NdArray>;

    #[test]
    fn train_epoch_runs() {
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
        let model = config.init::<B>(&device);

        let vocab = Vocab::new();
        let blocks = vec![(
            vec![TIROp::Push(1), TIROp::Push(2), TIROp::Add],
            vec!["push 1".into(), "push 2".into(), "add".into()],
            "test:0..3".into(),
            3u64,
        )];
        let pairs = extract_pairs(&blocks, &vocab);

        let supervised_config = SupervisedConfig::default();
        let mut optimizer = create_optimizer::<B>(&supervised_config);

        let lr = supervised_config.lr;
        let (model, result) = train_epoch(model, &pairs, &mut optimizer, lr, &device);
        assert_eq!(result.num_pairs, 1);
        assert!(result.avg_loss > 0.0, "loss should be positive");
        assert!(result.avg_loss.is_finite(), "loss should be finite");

        // Train a second epoch — loss should change
        let (_model2, result2) = train_epoch(model, &pairs, &mut optimizer, lr, &device);
        assert!(result2.avg_loss.is_finite());
    }
}
