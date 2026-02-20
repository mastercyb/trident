//! Training pair extraction: (TirGraph, TASM token sequence).
//!
//! Builds training pairs from compiled Trident source files.
//! Each pair maps a TIR basic block (as a graph) to its compiler-generated
//! TASM output (as a token sequence).

use crate::ir::tir::TIROp;
use crate::neural::data::tir_graph::TirGraph;
use crate::neural::model::vocab::Vocab;

/// A single training pair: graph input + token sequence target.
pub struct TrainingPair {
    /// TIR graph representation of the input block.
    pub graph: TirGraph,
    /// Target TASM as vocab token IDs (with EOS appended).
    pub target_tokens: Vec<u32>,
    /// Source identifier (e.g., "poseidon2:12..28").
    pub source_id: String,
    /// Compiler baseline cost for this block.
    pub baseline_cost: u64,
}

/// Extract training pairs from pre-compiled data.
///
/// Takes TIR ops and their corresponding compiler TASM output,
/// splits into basic blocks, and creates (graph, tokens) pairs.
///
/// - `tir_ops`: Full TIR op sequence for a function
/// - `tasm_lines`: Corresponding compiler-generated TASM lines per block
/// - `source_id`: Identifier for the source file/function
/// - `vocab`: Vocabulary for encoding TASM lines
pub fn extract_pairs(
    blocks: &[(Vec<TIROp>, Vec<String>, String, u64)],
    vocab: &Vocab,
) -> Vec<TrainingPair> {
    let mut pairs = Vec::new();

    for (tir_ops, tasm_lines, source_id, baseline_cost) in blocks {
        if tir_ops.is_empty() || tasm_lines.is_empty() {
            continue;
        }

        // Build graph from TIR ops
        let graph = TirGraph::from_tir_ops(tir_ops);
        if graph.nodes.is_empty() {
            continue;
        }

        // Encode TASM to token IDs
        let target_tokens = vocab.encode_sequence(tasm_lines);
        if target_tokens.len() <= 1 {
            // Only EOS — no actual content
            continue;
        }

        pairs.push(TrainingPair {
            graph,
            target_tokens,
            source_id: source_id.clone(),
            baseline_cost: *baseline_cost,
        });
    }

    pairs
}

/// Split training pairs into train and holdout sets.
/// Returns (train, holdout).
pub fn train_holdout_split(
    pairs: Vec<TrainingPair>,
    holdout_count: usize,
) -> (Vec<TrainingPair>, Vec<TrainingPair>) {
    if pairs.len() <= holdout_count {
        return (pairs, Vec::new());
    }
    let split_point = pairs.len() - holdout_count;
    let mut all = pairs;
    let holdout = all.split_off(split_point);
    (all, holdout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::tir::TIROp;

    #[test]
    fn extract_pairs_basic() {
        let vocab = Vocab::new();
        let blocks = vec![(
            vec![TIROp::Push(1), TIROp::Push(2), TIROp::Add],
            vec!["push 1".into(), "push 2".into(), "add".into()],
            "test:0..3".into(),
            3u64,
        )];
        let pairs = extract_pairs(&blocks, &vocab);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].source_id, "test:0..3");
        assert_eq!(pairs[0].baseline_cost, 3);
        // Target should end with EOS (0)
        assert_eq!(*pairs[0].target_tokens.last().unwrap(), 0);
    }

    #[test]
    fn extract_pairs_skips_empty() {
        let vocab = Vocab::new();
        let blocks: Vec<(Vec<TIROp>, Vec<String>, String, u64)> = vec![
            (vec![], vec!["push 1".into()], "empty_tir".into(), 0),
            (vec![TIROp::Push(1)], vec![], "empty_tasm".into(), 0),
        ];
        let pairs = extract_pairs(&blocks, &vocab);
        assert_eq!(pairs.len(), 0);
    }

    #[test]
    fn train_holdout_split_works() {
        let vocab = Vocab::new();
        let blocks: Vec<_> = (0..10)
            .map(|i| {
                (
                    vec![TIROp::Push(i as u64)],
                    vec![format!("push {}", i)],
                    format!("block:{}", i),
                    1u64,
                )
            })
            .collect();
        let pairs = extract_pairs(&blocks, &vocab);
        let (train, holdout) = train_holdout_split(pairs, 3);
        assert_eq!(train.len(), 7);
        assert_eq!(holdout.len(), 3);
    }
}
