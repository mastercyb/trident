//! End-to-end latency benchmark for neural compiler v2.
//!
//! Measures each stage of the v2 inference pipeline:
//! 1. TIR parsing + graph construction
//! 2. GNN encoder (CPU, single graph)
//! 3. Transformer decoder (beam search, K=32)
//! 4. Candidate validation (parallel)
//! 5. Total end-to-end
//!
//! Target: P90 <= 200ms per function (design doc section 8).

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use trident::ir::tir::TIROp;
use trident::neural::data::tir_graph::TirGraph;
use trident::neural::inference::beam::{beam_search, BeamConfig};
use trident::neural::inference::execute::validate_and_rank;
use trident::neural::model::composite::NeuralCompilerConfig;
use trident::neural::model::vocab::Vocab;
use trident::neural::training::supervised::{graph_to_edges, graph_to_features};

use burn::backend::NdArray;

type B = NdArray;

/// Build a synthetic TIR sequence of given size for benchmarking.
fn synthetic_tir(n: usize) -> Vec<TIROp> {
    let mut ops = Vec::with_capacity(n);
    for i in 0..n {
        match i % 5 {
            0 => ops.push(TIROp::Push(i as u64)),
            1 => ops.push(TIROp::Add),
            2 => ops.push(TIROp::Mul),
            3 => ops.push(TIROp::Dup(0)),
            4 => ops.push(TIROp::Swap(1)),
            _ => unreachable!(),
        }
    }
    ops
}

/// Benchmark: TIR -> TirGraph construction.
fn bench_graph_build(c: &mut Criterion) {
    let ops_50 = synthetic_tir(50);
    let ops_100 = synthetic_tir(100);

    let mut group = c.benchmark_group("graph_build");
    group.bench_function("50_ops", |b| {
        b.iter(|| TirGraph::from_tir_ops(black_box(&ops_50)))
    });
    group.bench_function("100_ops", |b| {
        b.iter(|| TirGraph::from_tir_ops(black_box(&ops_100)))
    });
    group.finish();
}

/// Benchmark: GNN encoder forward pass (CPU, single graph).
fn bench_gnn_encoder(c: &mut Criterion) {
    let device = Default::default();
    let config = NeuralCompilerConfig {
        d_model: 256,
        d_edge: 32,
        gnn_layers: 4,
        decoder_layers: 6,
        n_heads: 8,
        d_ff: 1024,
        max_seq: 256,
        dropout: 0.0,
    };
    let model = config.init::<B>(&device);

    let ops = synthetic_tir(100);
    let graph = TirGraph::from_tir_ops(&ops);
    let node_features = graph_to_features::<B>(&graph, &device);
    let (edge_src, edge_dst, edge_types) = graph_to_edges::<B>(&graph, &device);

    c.bench_function("gnn_encoder_100_nodes", |b| {
        b.iter(|| {
            model.encoder.forward(
                black_box(node_features.clone()),
                black_box(edge_src.clone()),
                black_box(edge_dst.clone()),
                black_box(edge_types.clone()),
            )
        })
    });
}

/// Benchmark: full beam search (encoder + decoder + grammar masks).
fn bench_beam_search(c: &mut Criterion) {
    let device = Default::default();
    // Use smaller model for benchmark feasibility
    let config = NeuralCompilerConfig {
        d_model: 64,
        d_edge: 16,
        gnn_layers: 2,
        decoder_layers: 2,
        n_heads: 4,
        d_ff: 128,
        max_seq: 64,
        dropout: 0.0,
    };
    let model = config.init::<B>(&device);

    let ops = synthetic_tir(20);
    let graph = TirGraph::from_tir_ops(&ops);
    let node_features = graph_to_features::<B>(&graph, &device);
    let (edge_src, edge_dst, edge_types) = graph_to_edges::<B>(&graph, &device);

    let beam_config = BeamConfig {
        k: 8, // Reduced K for benchmark speed
        max_steps: 16,
    };

    c.bench_function("beam_search_k8_steps16", |b| {
        b.iter(|| {
            beam_search(
                black_box(&model.encoder),
                black_box(&model.decoder),
                black_box(node_features.clone()),
                black_box(edge_src.clone()),
                black_box(edge_dst.clone()),
                black_box(edge_types.clone()),
                black_box(&beam_config),
                0,
                &device,
            )
        })
    });
}

/// Benchmark: candidate validation (parallel via rayon).
fn bench_validation(c: &mut Criterion) {
    let vocab = Vocab::new();
    let baseline: Vec<String> = vec![
        "push 1".into(),
        "push 2".into(),
        "add".into(),
        "push 3".into(),
        "mul".into(),
    ];

    // 32 candidates, each a few tokens
    let candidates: Vec<Vec<u32>> = (0..32)
        .map(|i| {
            vec![
                (i % 10 + 1) as u32, // various push/dup/swap tokens
                (i % 5 + 1) as u32,
            ]
        })
        .collect();

    c.bench_function("validate_32_candidates", |b| {
        b.iter(|| {
            validate_and_rank(
                black_box(&candidates),
                black_box(&vocab),
                black_box(&baseline),
                42,
            )
        })
    });
}

/// Benchmark: full end-to-end pipeline (graph + encode + beam + validate).
fn bench_end_to_end(c: &mut Criterion) {
    let device = Default::default();
    let config = NeuralCompilerConfig {
        d_model: 64,
        d_edge: 16,
        gnn_layers: 2,
        decoder_layers: 2,
        n_heads: 4,
        d_ff: 128,
        max_seq: 64,
        dropout: 0.0,
    };
    let model = config.init::<B>(&device);
    let vocab = Vocab::new();

    let ops = synthetic_tir(20);
    let baseline: Vec<String> = vec!["push 1".into(), "push 2".into(), "add".into()];

    let beam_config = BeamConfig {
        k: 8,
        max_steps: 16,
    };

    c.bench_function("end_to_end_20ops_k8", |b| {
        b.iter(|| {
            // 1. Graph build
            let graph = TirGraph::from_tir_ops(black_box(&ops));

            // 2. Feature extraction
            let node_features = graph_to_features::<B>(&graph, &device);
            let (edge_src, edge_dst, edge_types) = graph_to_edges::<B>(&graph, &device);

            // 3. Beam search (encoder + decoder)
            let result = beam_search(
                &model.encoder,
                &model.decoder,
                node_features,
                edge_src,
                edge_dst,
                edge_types,
                &beam_config,
                0,
                &device,
            );

            // 4. Validate + rank
            let _ranked = validate_and_rank(&result.sequences, &vocab, &baseline, 42);
        })
    });
}

criterion_group!(
    benches,
    bench_graph_build,
    bench_gnn_encoder,
    bench_beam_search,
    bench_validation,
    bench_end_to_end,
);
criterion_main!(benches);
