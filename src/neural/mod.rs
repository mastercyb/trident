//! Neural compiler v2: GNN encoder + Transformer decoder.
//!
//! Replaces the v1 MLP evolutionary model with a ~13M parameter
//! architecture trained via supervised learning + GFlowNets.
//!
//! # Public API
//!
//! ```ignore
//! use trident::neural;
//! let result = neural::compile(&tir_ops, &baseline_tasm)?;
//! ```

pub mod checkpoint;
pub mod data;
pub mod inference;
pub mod model;
pub mod training;

use burn::backend::Wgpu;

use crate::ir::tir::TIROp;
use data::tir_graph::TirGraph;
use inference::beam::{beam_search, BeamConfig};
use inference::execute::validate_and_rank;
use model::vocab::Vocab;
use training::supervised::{graph_to_edges, graph_to_features};

/// Result of neural compilation.
pub struct CompileResult {
    /// Optimized TASM instructions.
    pub tasm_lines: Vec<String>,
    /// Table cost (clock cycles) of the result.
    pub cost: u64,
    /// How many beam candidates were valid.
    pub valid_count: usize,
    /// Total beam candidates evaluated.
    pub total_count: usize,
    /// Whether this is a neural result (true) or fallback (false).
    pub neural: bool,
}

/// Compile TIR ops to optimized TASM using the neural model.
///
/// Loads the production checkpoint, runs beam search (K=32, max_steps=256),
/// validates candidates against baseline TASM, and returns the cheapest valid one.
///
/// Falls back to `baseline_tasm` if no valid candidate is found or if
/// no trained checkpoint exists.
pub fn compile(tir_ops: &[TIROp], baseline_tasm: &[String]) -> Result<CompileResult, String> {
    let device = burn::backend::wgpu::WgpuDevice::default();
    compile_with_device::<Wgpu>(tir_ops, baseline_tasm, &device)
}

/// Compile TIR ops with a specific burn backend device.
pub fn compile_with_device<B: burn::prelude::Backend>(
    tir_ops: &[TIROp],
    baseline_tasm: &[String],
    device: &B::Device,
) -> Result<CompileResult, String> {
    let vocab = Vocab::new();

    // Build graph from TIR
    let graph = TirGraph::from_tir_ops(tir_ops);
    if graph.nodes.is_empty() {
        return Ok(fallback_result(baseline_tasm));
    }

    // Load production checkpoint
    let config = model::composite::NeuralCompilerConfig::new();
    let model = config.init::<B>(device);
    let model =
        match checkpoint::load_checkpoint(model, checkpoint::CheckpointTag::Production, device) {
            Ok(Some(loaded)) => loaded,
            Ok(None) => {
                // No checkpoint — try stage1_best as fallback
                let model2 = config.init::<B>(device);
                match checkpoint::load_checkpoint(
                    model2,
                    checkpoint::CheckpointTag::Stage1Best,
                    device,
                ) {
                    Ok(Some(loaded)) => loaded,
                    _ => return Ok(fallback_result(baseline_tasm)),
                }
            }
            Err(_) => return Ok(fallback_result(baseline_tasm)),
        };

    // Encode graph
    let node_features = graph_to_features::<B>(&graph, device);
    let (edge_src, edge_dst, edge_types) = graph_to_edges::<B>(&graph, device);

    // Beam search
    let beam_config = BeamConfig::default(); // K=32, max_steps=256
    let beam_result = beam_search(
        &model.encoder,
        &model.decoder,
        node_features,
        edge_src,
        edge_dst,
        edge_types,
        &beam_config,
        0, // must match training initial_stack_depth
        device,
    );

    // Validate and rank
    match validate_and_rank(&beam_result.sequences, &vocab, baseline_tasm, 0) {
        Some(ranked) => Ok(CompileResult {
            tasm_lines: ranked.tasm_lines,
            cost: ranked.cost,
            valid_count: ranked.valid_count,
            total_count: ranked.total_count,
            neural: true,
        }),
        None => Ok(fallback_result(baseline_tasm)),
    }
}

fn fallback_result(baseline_tasm: &[String]) -> CompileResult {
    use crate::cost::scorer::profile_tasm;
    let refs: Vec<&str> = baseline_tasm.iter().map(|s| s.as_str()).collect();
    let cost = profile_tasm(&refs).cost();
    CompileResult {
        tasm_lines: baseline_tasm.to_vec(),
        cost,
        valid_count: 0,
        total_count: 0,
        neural: false,
    }
}
