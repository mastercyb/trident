//! Data augmentation for neural compiler training.
//!
//! Two families of augmentations:
//!
//! 1. **Structural** (TIR-side): reorder independent ops, insert dead code.
//!    These change the graph topology while preserving semantics.
//!
//! 2. **Output-space** (TASM-side): swap adjacent independent instructions,
//!    apply equivalent substitutions. Validated via stack_verifier.
//!
//! Target: 50 seed pairs → 5,000-10,000 augmented pairs.

use crate::cost::stack_verifier;
use crate::neural::data::pairs::TrainingPair;
use crate::neural::data::tir_graph::TirGraph;
use crate::neural::model::vocab::Vocab;

/// Configuration for augmentation pipeline.
pub struct AugmentConfig {
    /// Number of TIR reordering variants per seed pair.
    pub tir_reorder_variants: usize,
    /// Number of TASM random-walk variants per seed pair.
    pub tasm_walk_variants: usize,
    /// Max swap attempts per random walk.
    pub max_swap_attempts: usize,
    /// Random seed for reproducibility.
    pub seed: u64,
}

impl Default for AugmentConfig {
    fn default() -> Self {
        Self {
            tir_reorder_variants: 10,
            tasm_walk_variants: 50,
            max_swap_attempts: 20,
            seed: 0xDEAD_BEEF_A097,
        }
    }
}

/// Augment a set of training pairs using both structural and output-space methods.
///
/// Returns the original pairs plus all augmented variants.
pub fn augment_pairs(
    pairs: &[TrainingPair],
    vocab: &Vocab,
    config: &AugmentConfig,
) -> Vec<TrainingPair> {
    let mut result = Vec::with_capacity(
        pairs.len() * (1 + config.tir_reorder_variants + config.tasm_walk_variants),
    );
    let mut rng = Xorshift64::new(config.seed);

    for (pair_idx, pair) in pairs.iter().enumerate() {
        // Keep original
        result.push(TrainingPair {
            graph: pair.graph.clone(),
            target_tokens: pair.target_tokens.clone(),
            source_id: pair.source_id.clone(),
            baseline_cost: pair.baseline_cost,
        });

        // Decode target tokens back to TASM lines for TASM-side augmentation
        let tasm_lines: Vec<String> = pair
            .target_tokens
            .iter()
            .filter(|&&t| t != 0) // skip EOS
            .filter_map(|&t| vocab.decode(t).map(|s| s.to_string()))
            .collect();

        // Output-space augmentation: random walk on TASM
        for variant in 0..config.tasm_walk_variants {
            if let Some(augmented_tasm) =
                random_walk_tasm(&tasm_lines, config.max_swap_attempts, &mut rng)
            {
                let tokens = vocab.encode_sequence(&augmented_tasm);
                if tokens.len() > 1 {
                    result.push(TrainingPair {
                        graph: pair.graph.clone(),
                        target_tokens: tokens,
                        source_id: format!("{}:walk{}", pair.source_id, variant),
                        baseline_cost: pair.baseline_cost,
                    });
                }
            }
        }

        // Equivalent substitutions on TASM
        let sub_variants = equivalent_substitutions(&tasm_lines);
        for (sub_idx, sub_tasm) in sub_variants.into_iter().enumerate() {
            let tokens = vocab.encode_sequence(&sub_tasm);
            if tokens.len() > 1 {
                result.push(TrainingPair {
                    graph: pair.graph.clone(),
                    target_tokens: tokens,
                    source_id: format!("{}:sub{}", pair.source_id, sub_idx),
                    baseline_cost: pair.baseline_cost,
                });
            }
        }

        // TIR-side augmentation: dead code insertion
        for variant in 0..config.tir_reorder_variants {
            let augmented_tir = insert_dead_code(&pair.graph, &mut rng);
            result.push(TrainingPair {
                graph: augmented_tir,
                target_tokens: pair.target_tokens.clone(),
                source_id: format!("{}:dead{}", pair.source_id, variant),
                baseline_cost: pair.baseline_cost,
            });
        }

        if (pair_idx + 1) % 10 == 0 {
            eprintln!(
                "  augmented {}/{} seed pairs ({} total)",
                pair_idx + 1,
                pairs.len(),
                result.len()
            );
        }
    }

    eprintln!(
        "  augmentation: {} seeds → {} pairs ({:.1}x)",
        pairs.len(),
        result.len(),
        result.len() as f64 / pairs.len().max(1) as f64,
    );

    result
}

// ─── TASM Random Walk ─────────────────────────────────────────────

/// Apply random adjacent swaps to TASM, keeping only valid variants.
///
/// Strategy: try swapping adjacent instructions. If the result passes
/// `verify_equivalent()` on multiple random inputs, accept the swap.
fn random_walk_tasm(
    tasm: &[String],
    max_attempts: usize,
    rng: &mut Xorshift64,
) -> Option<Vec<String>> {
    if tasm.len() < 2 {
        return None;
    }

    let mut current = tasm.to_vec();
    let mut changed = false;

    for _ in 0..max_attempts {
        let i = (rng.next() % (current.len() - 1) as u64) as usize;

        // Skip swaps that would reorder dependent instructions
        if instructions_are_independent(&current[i], &current[i + 1]) {
            current.swap(i, i + 1);

            // Verify equivalence on 3 random seeds
            let valid = (0..3u64).all(|trial| {
                let seed = rng.next() ^ trial.wrapping_mul(0x9E3779B97F4A7C15);
                stack_verifier::verify_equivalent(tasm, &current, seed)
            });

            if valid {
                changed = true;
            } else {
                // Revert
                current.swap(i, i + 1);
            }
        }
    }

    if changed {
        Some(current)
    } else {
        None
    }
}

/// Check if two TASM instructions are likely independent (can be reordered).
///
/// Conservative: returns true only for pure stack ops that don't depend
/// on each other's outputs (both push to different stack positions).
fn instructions_are_independent(a: &str, b: &str) -> bool {
    let a_parts: Vec<&str> = a.split_whitespace().collect();
    let b_parts: Vec<&str> = b.split_whitespace().collect();

    if a_parts.is_empty() || b_parts.is_empty() {
        return false;
    }

    let a_op = a_parts[0];
    let b_op = b_parts[0];

    // Two push instructions are always independent
    if a_op == "push" && b_op == "push" {
        return true;
    }

    // Commutative binary ops followed by another commutative op
    // Actually, this is tricky. Be very conservative:
    // Only allow swapping two instructions that both only push (no pops).
    let a_pure_push = matches!(a_op, "push" | "divine" | "read_io");
    let b_pure_push = matches!(b_op, "push" | "divine" | "read_io");

    if a_pure_push && b_pure_push {
        return true;
    }

    // Two nops
    if a_op == "nop" || b_op == "nop" {
        return true;
    }

    false
}

// ─── Equivalent Substitutions ─────────────────────────────────────

/// Apply pattern-based equivalent substitutions to TASM.
///
/// Returns all valid single-substitution variants.
fn equivalent_substitutions(tasm: &[String]) -> Vec<Vec<String>> {
    let mut variants = Vec::new();

    for i in 0..tasm.len() {
        // Single-instruction substitutions
        match tasm[i].as_str() {
            "nop" => {
                // nop → (remove)
                let mut v = tasm.to_vec();
                v.remove(i);
                if verify_substitution(tasm, &v) {
                    variants.push(v);
                }
            }
            "push 0" if i + 1 < tasm.len() && tasm[i + 1] == "add" => {
                // push 0; add → (remove both — identity)
                let mut v = tasm.to_vec();
                v.remove(i + 1);
                v.remove(i);
                if verify_substitution(tasm, &v) {
                    variants.push(v);
                }
            }
            "push 1" if i + 1 < tasm.len() && tasm[i + 1] == "mul" => {
                // push 1; mul → (remove both — identity)
                let mut v = tasm.to_vec();
                v.remove(i + 1);
                v.remove(i);
                if verify_substitution(tasm, &v) {
                    variants.push(v);
                }
            }
            "dup 0" if i + 1 < tasm.len() && tasm[i + 1] == "pop 1" => {
                // dup 0; pop 1 → (remove both — noop)
                let mut v = tasm.to_vec();
                v.remove(i + 1);
                v.remove(i);
                if verify_substitution(tasm, &v) {
                    variants.push(v);
                }
            }
            "swap 1" if i + 1 < tasm.len() && tasm[i + 1] == "swap 1" => {
                // swap 1; swap 1 → (remove both — identity)
                let mut v = tasm.to_vec();
                v.remove(i + 1);
                v.remove(i);
                if verify_substitution(tasm, &v) {
                    variants.push(v);
                }
            }
            _ => {}
        }

        // Expansion substitutions (make longer but equivalent)
        if tasm[i] == "add" && i >= 1 {
            // add → swap 1; add (commutativity — same result)
            let mut v = tasm.to_vec();
            v.insert(i, "swap 1".to_string());
            if verify_substitution(tasm, &v) {
                variants.push(v);
            }
        }

        if tasm[i] == "mul" && i >= 1 {
            // mul → swap 1; mul (commutativity — same result)
            let mut v = tasm.to_vec();
            v.insert(i, "swap 1".to_string());
            if verify_substitution(tasm, &v) {
                variants.push(v);
            }
        }
    }

    variants
}

/// Verify that a substituted TASM sequence is equivalent to the original.
fn verify_substitution(original: &[String], candidate: &[String]) -> bool {
    // Test on 3 different random seeds
    (0..3).all(|seed| stack_verifier::verify_equivalent(original, candidate, seed * 7919 + 42))
}

// ─── Dead Code Insertion (TIR-side) ──────────────────────────────

/// Insert dead code nodes into a TirGraph.
///
/// Adds operations that don't affect the output: push+pop pairs,
/// dup+pop pairs, nop sequences. The model must learn to ignore these.
fn insert_dead_code(graph: &TirGraph, rng: &mut Xorshift64) -> TirGraph {
    use crate::neural::data::tir_graph::{EdgeKind, FieldType, OpKind, TirNode};

    let mut nodes = graph.nodes.clone();
    let mut edges = graph.edges.clone();

    // Number of dead code insertions: 1-3
    let num_insertions = 1 + (rng.next() % 3) as usize;

    for _ in 0..num_insertions {
        if nodes.is_empty() {
            break;
        }

        // Pick random insertion point
        let insert_at = (rng.next() % nodes.len() as u64) as usize;
        let dead_kind = rng.next() % 3;

        let dead_nodes: Vec<TirNode> = match dead_kind {
            0 => {
                // push + pop pair
                vec![
                    TirNode {
                        op: OpKind::Push,
                        field_type: FieldType::BFE,
                        immediate: Some(0),
                    },
                    TirNode {
                        op: OpKind::Pop,
                        field_type: FieldType::Unknown,
                        immediate: Some(1),
                    },
                ]
            }
            1 => {
                // dup 0 + pop 1 (if stack nonempty — conservative: always add push first)
                vec![
                    TirNode {
                        op: OpKind::Push,
                        field_type: FieldType::BFE,
                        immediate: Some(0),
                    },
                    TirNode {
                        op: OpKind::Dup,
                        field_type: FieldType::BFE,
                        immediate: Some(0),
                    },
                    TirNode {
                        op: OpKind::Pop,
                        field_type: FieldType::Unknown,
                        immediate: Some(2),
                    },
                ]
            }
            _ => {
                // Single nop-like: push 0; push 0; add; pop 1
                vec![
                    TirNode {
                        op: OpKind::Push,
                        field_type: FieldType::BFE,
                        immediate: Some(0),
                    },
                    TirNode {
                        op: OpKind::Push,
                        field_type: FieldType::BFE,
                        immediate: Some(0),
                    },
                    TirNode {
                        op: OpKind::Add,
                        field_type: FieldType::BFE,
                        immediate: None,
                    },
                    TirNode {
                        op: OpKind::Pop,
                        field_type: FieldType::Unknown,
                        immediate: Some(1),
                    },
                ]
            }
        };

        let num_dead = dead_nodes.len();

        // Shift all edge indices >= insert_at by num_dead
        for edge in edges.iter_mut() {
            if edge.0 >= insert_at {
                edge.0 += num_dead;
            }
            if edge.1 >= insert_at {
                edge.1 += num_dead;
            }
        }

        // Insert dead nodes
        let mut new_nodes = nodes[..insert_at].to_vec();
        new_nodes.extend(dead_nodes);
        new_nodes.extend_from_slice(&nodes[insert_at..]);
        nodes = new_nodes;

        // Add control flow edges within dead code
        for j in 0..num_dead.saturating_sub(1) {
            edges.push((insert_at + j, insert_at + j + 1, EdgeKind::ControlFlow));
        }

        // Add data dep edges within dead code (push→pop, push→dup, etc.)
        if num_dead >= 2 {
            edges.push((insert_at, insert_at + num_dead - 1, EdgeKind::DataDep));
        }

        // Connect to surrounding control flow
        if insert_at > 0 {
            edges.push((insert_at - 1, insert_at, EdgeKind::ControlFlow));
        }
        if insert_at + num_dead < nodes.len() {
            edges.push((
                insert_at + num_dead - 1,
                insert_at + num_dead,
                EdgeKind::ControlFlow,
            ));
        }
    }

    TirGraph { nodes, edges }
}

// ─── PRNG ─────────────────────────────────────────────────────────

/// Simple xorshift64 PRNG for reproducible augmentation.
struct Xorshift64 {
    state: u64,
}

impl Xorshift64 {
    fn new(seed: u64) -> Self {
        Self {
            state: seed | 1, // ensure non-zero
        }
    }

    fn next(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }
}

// ─── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::tir::TIROp;
    use crate::neural::data::tir_graph::TirGraph;

    #[test]
    fn random_walk_preserves_equivalence() {
        let tasm = vec![
            "push 3".to_string(),
            "push 4".to_string(),
            "add".to_string(),
        ];
        let mut rng = Xorshift64::new(42);
        // Might or might not produce a variant (depends on RNG)
        let result = random_walk_tasm(&tasm, 10, &mut rng);
        if let Some(ref variant) = result {
            // Must be equivalent
            assert!(stack_verifier::verify_equivalent(&tasm, variant, 0));
        }
    }

    #[test]
    fn equivalent_substitutions_are_valid() {
        let tasm = vec!["push 0".to_string(), "add".to_string()];
        let variants = equivalent_substitutions(&tasm);
        for variant in &variants {
            assert!(
                stack_verifier::verify_equivalent(&tasm, variant, 42),
                "substitution not equivalent: {:?}",
                variant,
            );
        }
    }

    #[test]
    fn push_0_add_removed() {
        let tasm = vec![
            "push 5".to_string(),
            "push 0".to_string(),
            "add".to_string(),
        ];
        let variants = equivalent_substitutions(&tasm);
        // Should find the push 0; add → remove variant
        let has_shorter = variants.iter().any(|v| v.len() < tasm.len());
        assert!(has_shorter, "expected push 0; add to be removed");
    }

    #[test]
    fn dead_code_increases_graph_size() {
        let ops = vec![TIROp::Push(1), TIROp::Push(2), TIROp::Add];
        let graph = TirGraph::from_tir_ops(&ops);
        let original_size = graph.num_nodes();

        let mut rng = Xorshift64::new(42);
        let augmented = insert_dead_code(&graph, &mut rng);
        assert!(
            augmented.num_nodes() > original_size,
            "dead code should increase graph size",
        );
    }

    #[test]
    fn augment_pairs_multiplies_dataset() {
        let vocab = Vocab::new();
        let graph = TirGraph::from_tir_ops(&[TIROp::Push(1), TIROp::Push(2), TIROp::Add]);
        let tokens = vocab.encode_sequence(&[
            "push 1".to_string(),
            "push 2".to_string(),
            "add".to_string(),
        ]);

        let pairs = vec![TrainingPair {
            graph,
            target_tokens: tokens,
            source_id: "test:0".into(),
            baseline_cost: 3,
        }];

        let config = AugmentConfig {
            tir_reorder_variants: 2,
            tasm_walk_variants: 3,
            max_swap_attempts: 5,
            seed: 42,
        };

        let augmented = augment_pairs(&pairs, &vocab, &config);
        assert!(
            augmented.len() > 1,
            "augmentation should produce more than original",
        );
    }

    #[test]
    fn swap_1_swap_1_eliminated() {
        let tasm = vec![
            "push 1".to_string(),
            "push 2".to_string(),
            "swap 1".to_string(),
            "swap 1".to_string(),
            "add".to_string(),
        ];
        let variants = equivalent_substitutions(&tasm);
        let has_shorter = variants.iter().any(|v| v.len() < tasm.len());
        assert!(has_shorter, "swap 1; swap 1 should be eliminated");
    }
}
