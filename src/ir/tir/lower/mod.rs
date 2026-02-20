//! StackLowering: consumes `Vec<TIROp>` and produces target assembly text.
//!
//! Each target implements `StackLowering` to control instruction selection
//! and control-flow structure. The speculative lowering wraps the classical
//! path with an optional neural optimizer.

#[cfg(test)]
mod tests;
mod triton;

use super::encode;
use super::neural::model::NeuralModel;
use super::neural::report::{BlockDecision, DecisionReason, OptimizerReport, Winner};
use super::neural::weights::OptimizerStatus;
use super::TIROp;
use crate::cost::scorer;

pub use triton::TritonLowering;

/// Lowers IR operations into target assembly lines.
pub trait StackLowering {
    /// Convert a sequence of IR operations into assembly text lines.
    fn lower(&self, ops: &[TIROp]) -> Vec<String>;
}

/// Create a stack lowering backend for the given target name.
pub fn create_stack_lowering(_target: &str) -> Box<dyn StackLowering> {
    Box::new(TritonLowering::new())
}

/// Create a speculative stack lowering with an optional neural model.
pub fn create_speculative_lowering(
    _target: &str,
    model: Option<NeuralModel>,
    meta_generation: u64,
    meta_hash: String,
    meta_status: OptimizerStatus,
) -> SpeculativeLowering {
    SpeculativeLowering {
        classical: TritonLowering::new(),
        neural: std::cell::RefCell::new(model),
        report: std::cell::RefCell::new(OptimizerReport {
            status: meta_status,
            generation: meta_generation,
            weight_hash: meta_hash,
            decisions: Vec::new(),
            total_neural_cost: 0,
            total_classical_cost: 0,
        }),
    }
}

/// Speculative lowering: classical path always runs, neural path is pure upside.
pub struct SpeculativeLowering {
    classical: TritonLowering,
    neural: std::cell::RefCell<Option<NeuralModel>>,
    report: std::cell::RefCell<OptimizerReport>,
}

impl SpeculativeLowering {
    /// Get the accumulated optimizer report.
    pub fn report(&self) -> OptimizerReport {
        self.report.borrow().clone()
    }
}

impl StackLowering for SpeculativeLowering {
    fn lower(&self, ops: &[TIROp]) -> Vec<String> {
        // Classical path always runs
        let baseline = self.classical.lower(ops);

        let mut neural_ref = self.neural.borrow_mut();
        let neural = match neural_ref.as_mut() {
            Some(model) => model,
            None => return baseline,
        };

        // Encode TIR blocks for neural inference
        let blocks = encode::encode_blocks(ops);
        if blocks.is_empty() {
            return baseline;
        }

        let baseline_profile = scorer::profile_tasm_str(&baseline.join("\n"));
        let baseline_cost = baseline_profile.cost();

        let mut report = self.report.borrow_mut();
        report.total_classical_cost += baseline_cost;

        // Try neural path on each block
        let mut any_neural_win = false;
        for block in &blocks {
            let output_codes = neural.forward(block);

            if output_codes.is_empty() {
                report.decisions.push(BlockDecision {
                    block_id: block.block_id(),
                    winner: Winner::Classical,
                    winner_cost: baseline_cost,
                    loser_cost: baseline_cost,
                    reason: DecisionReason::NoCandidate,
                });
                continue;
            }

            // Decode output codes to TASM instructions
            let candidate_lines = decode_output(&output_codes);
            if candidate_lines.is_empty() {
                report.decisions.push(BlockDecision {
                    block_id: block.block_id(),
                    winner: Winner::Classical,
                    winner_cost: baseline_cost,
                    loser_cost: baseline_cost,
                    reason: DecisionReason::NoCandidate,
                });
                continue;
            }

            let candidate_profile = scorer::profile_tasm(
                &candidate_lines
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>(),
            );
            let candidate_cost = candidate_profile.cost();

            if candidate_cost < baseline_cost {
                // Determine the reason
                let reason = if candidate_profile.is_cliff_jump(&baseline_profile) {
                    DecisionReason::CliffJump
                } else if candidate_profile.is_table_rebalance(&baseline_profile) {
                    DecisionReason::TableRebalance
                } else {
                    DecisionReason::StackScheduling
                };

                report.decisions.push(BlockDecision {
                    block_id: block.block_id(),
                    winner: Winner::Neural,
                    winner_cost: candidate_cost,
                    loser_cost: baseline_cost,
                    reason,
                });
                any_neural_win = true;
            } else {
                report.decisions.push(BlockDecision {
                    block_id: block.block_id(),
                    winner: Winner::Classical,
                    winner_cost: baseline_cost,
                    loser_cost: baseline_cost,
                    reason: DecisionReason::NeuralWorse(candidate_cost),
                });
            }
        }

        // For now, always return classical output.
        // Neural output substitution requires full semantic equivalence verification,
        // which will be wired in when the verify pipeline is connected.
        // The report still tracks what WOULD have happened.
        report.total_neural_cost += if any_neural_win {
            // Sum up the best cost per block
            report.decisions.iter().map(|d| d.winner_cost).sum::<u64>()
        } else {
            baseline_cost
        };

        baseline
    }
}

/// Decode neural output codes to TASM instruction strings.
/// Each code maps to a basic TASM instruction.
///
/// Every entry must be in the verifier's ALLOWED list. Side-effect ops
/// (split, assert, write_io, divine, halt, assert_vector) are included —
/// the verifier handles them via side-channel comparison. Ops that remain
/// remapped (hash, read_io, read_mem, etc.) use dummy values the verifier
/// can't meaningfully compare. The vocab size stays 64 to match the model
/// architecture and GPU shader.
pub fn decode_output(codes: &[u64]) -> Vec<String> {
    const VOCAB: &[&str] = &[
        "",              // 0: end of sequence
        "push 0",        // 1
        "push 1",        // 2
        "push -1",       // 3
        "pop 1",         // 4
        "pop 2",         // 5
        "pop 3",         // 6
        "pop 4",         // 7
        "pop 5",         // 8
        "dup 0",         // 9
        "dup 1",         // 10
        "dup 2",         // 11
        "dup 3",         // 12
        "dup 4",         // 13
        "dup 5",         // 14
        "swap 1",        // 15
        "swap 2",        // 16
        "swap 3",        // 17
        "swap 4",        // 18
        "swap 5",        // 19
        "add",           // 20
        "mul",           // 21
        "eq",            // 22
        "lt",            // 23
        "and",           // 24
        "xor",           // 25
        "div_mod",       // 26  (was: invert)
        "split",         // 27
        "pop_count",     // 28
        "log_2_floor",   // 29
        "nop",           // 30  (was: hash)
        "assert",        // 31
        "dup 9",         // 32  (was: read_io 1)
        "write_io 1",    // 33
        "dup 11",        // 34  (was: read_mem 1)
        "dup 12",        // 35  (was: write_mem 1)
        "divine 1",      // 36
        "dup 14",        // 37  (was: sponge_init)
        "dup 15",        // 38  (was: sponge_absorb)
        "swap 10",       // 39  (was: sponge_squeeze)
        "swap 11",       // 40  (was: nop at 40)
        "swap 12",       // 41  (was: skiz)
        "swap 13",       // 42  (was: return)
        "halt",          // 43
        "swap 15",       // 44  (was: read_io 5)
        "write_io 5",    // 45
        "pick 2",        // 46  (was: read_mem 5)
        "pick 3",        // 47  (was: write_mem 5)
        "divine 5",      // 48
        "pick 5",        // 49  (was: pop_count, moved to 28)
        "place 1",       // 50  (was: log_2_floor, moved to 29)
        "place 2",       // 51  (was: merkle_step)
        "place 3",       // 52  (was: sponge_absorb_mem)
        "place 4",       // 53  (was: xb_mul)
        "place 5",       // 54  (was: x_invert)
        "push 2",        // 55  (was: xx_dot_step)
        "push 3",        // 56  (was: xb_dot_step)
        "assert_vector", // 57
        "dup 6",         // 58
        "dup 7",         // 59
        "swap 6",        // 60
        "swap 7",        // 61
        "swap 8",        // 62
        "swap 9",        // 63
    ];

    let mut out = Vec::new();
    for &code in codes {
        let idx = code as usize;
        if idx == 0 || idx >= VOCAB.len() {
            break;
        }
        out.push(VOCAB[idx].to_string());
    }
    out
}
