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
        neural: model,
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
    neural: Option<NeuralModel>,
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

        let neural = match &self.neural {
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
pub fn decode_output(codes: &[u64]) -> Vec<String> {
    // TASM instruction vocabulary (simplified)
    const VOCAB: &[&str] = &[
        "",                  // 0: end of sequence
        "push 0",            // 1
        "push 1",            // 2
        "push -1",           // 3
        "pop 1",             // 4
        "pop 2",             // 5
        "pop 3",             // 6
        "pop 4",             // 7
        "pop 5",             // 8
        "dup 0",             // 9
        "dup 1",             // 10
        "dup 2",             // 11
        "dup 3",             // 12
        "dup 4",             // 13
        "dup 5",             // 14
        "swap 1",            // 15
        "swap 2",            // 16
        "swap 3",            // 17
        "swap 4",            // 18
        "swap 5",            // 19
        "add",               // 20
        "mul",               // 21
        "eq",                // 22
        "lt",                // 23
        "and",               // 24
        "xor",               // 25
        "invert",            // 26
        "split",             // 27
        "div_mod",           // 28
        "pow",               // 29
        "hash",              // 30
        "assert",            // 31
        "read_io 1",         // 32
        "write_io 1",        // 33
        "read_mem 1",        // 34
        "write_mem 1",       // 35
        "divine 1",          // 36
        "sponge_init",       // 37
        "sponge_absorb",     // 38
        "sponge_squeeze",    // 39
        "nop",               // 40
        "skiz",              // 41
        "return",            // 42
        "halt",              // 43
        "read_io 5",         // 44
        "write_io 5",        // 45
        "read_mem 5",        // 46
        "write_mem 5",       // 47
        "divine 5",          // 48
        "pop_count",         // 49
        "log_2_floor",       // 50
        "merkle_step",       // 51
        "sponge_absorb_mem", // 52
        "xb_mul",            // 53
        "x_invert",          // 54
        "xx_dot_step",       // 55
        "xb_dot_step",       // 56
        "assert_vector",     // 57
        "dup 6",             // 58
        "dup 7",             // 59
        "swap 6",            // 60
        "swap 7",            // 61
        "swap 8",            // 62
        "swap 9",            // 63
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
