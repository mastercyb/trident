//! Optimizer decision report for CLI display.
//!
//! Shows per-block decisions (neural vs classical), scores, reasons,
//! and convergence status.

/// Convergence status of the optimizer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OptimizerStatus {
    Improving,
    Plateaued,
    Converged,
}

impl std::fmt::Display for OptimizerStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OptimizerStatus::Improving => write!(f, "improving"),
            OptimizerStatus::Plateaued => write!(f, "plateaued"),
            OptimizerStatus::Converged => write!(f, "converged"),
        }
    }
}

/// Why a particular path was chosen.
#[derive(Clone, Debug)]
pub enum DecisionReason {
    /// Neural found a score below a power-of-2 boundary.
    CliffJump,
    /// Neural reduced max table by rebalancing across tables.
    TableRebalance,
    /// Neural found better stack arrangement (fewer ops).
    StackScheduling,
    /// Candidate TASM not semantically equivalent.
    NeuralFailedVerify,
    /// Candidate verified but score >= baseline.
    NeuralWorse(u64),
    /// Model produced empty or unparseable output.
    NoCandidate,
    /// Inference exceeded time budget.
    NeuralTimeout,
}

impl std::fmt::Display for DecisionReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecisionReason::CliffJump => write!(f, "cliff jump"),
            DecisionReason::TableRebalance => write!(f, "table rebalance"),
            DecisionReason::StackScheduling => write!(f, "stack scheduling"),
            DecisionReason::NeuralFailedVerify => write!(f, "neural failed verify"),
            DecisionReason::NeuralWorse(score) => write!(f, "neural worse ({})", score),
            DecisionReason::NoCandidate => write!(f, "no neural candidate"),
            DecisionReason::NeuralTimeout => write!(f, "neural timeout"),
        }
    }
}

/// Which path won for a block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Winner {
    Neural,
    Classical,
}

impl std::fmt::Display for Winner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Winner::Neural => write!(f, "neural"),
            Winner::Classical => write!(f, "classical"),
        }
    }
}

/// Per-block optimization decision.
#[derive(Clone, Debug)]
pub struct BlockDecision {
    pub block_id: String,
    pub winner: Winner,
    pub winner_cost: u64,
    pub loser_cost: u64,
    pub reason: DecisionReason,
}

impl BlockDecision {
    pub fn improvement_pct(&self) -> f64 {
        if self.loser_cost == 0 {
            return 0.0;
        }
        let change = self.winner_cost as f64 - self.loser_cost as f64;
        (change / self.loser_cost as f64) * 100.0
    }
}

/// Full compilation report with optimizer decisions.
#[derive(Clone, Debug)]
pub struct OptimizerReport {
    pub status: OptimizerStatus,
    pub generation: u64,
    pub weight_hash: String,
    pub decisions: Vec<BlockDecision>,
    pub total_neural_cost: u64,
    pub total_classical_cost: u64,
}

impl OptimizerReport {
    /// Create an empty report (no neural optimizer active).
    pub fn empty() -> Self {
        Self {
            status: OptimizerStatus::Improving,
            generation: 0,
            weight_hash: String::new(),
            decisions: Vec::new(),
            total_neural_cost: 0,
            total_classical_cost: 0,
        }
    }

    /// Number of blocks where neural won.
    pub fn neural_wins(&self) -> usize {
        self.decisions
            .iter()
            .filter(|d| d.winner == Winner::Neural)
            .count()
    }

    /// Total savings percentage.
    pub fn total_improvement_pct(&self) -> f64 {
        if self.total_classical_cost == 0 {
            return 0.0;
        }
        (1.0 - self.total_neural_cost as f64 / self.total_classical_cost as f64) * 100.0
    }

    /// Format the report for CLI display.
    pub fn format_report(&self) -> String {
        let mut out = String::new();

        // Header
        out.push_str(&format!(
            "Neural optimizer: gen {}, {} ({:.1}% vs baseline)\n",
            self.generation,
            self.status,
            self.total_improvement_pct(),
        ));
        if !self.weight_hash.is_empty() {
            let short_hash = &self.weight_hash[..self.weight_hash.len().min(12)];
            out.push_str(&format!(
                "  weights: {} | score: {} | baseline: {}\n",
                short_hash, self.total_neural_cost, self.total_classical_cost,
            ));
        }
        out.push('\n');

        // Per-block decisions
        for (i, d) in self.decisions.iter().enumerate() {
            let cmp = if d.winner == Winner::Neural { "<" } else { "=" };
            out.push_str(&format!(
                "  Block {} ({})  {:>10}  {} {} {}  {:>+6.1}%  {}\n",
                i + 1,
                d.block_id,
                d.winner,
                d.winner_cost,
                cmp,
                d.loser_cost,
                d.improvement_pct(),
                d.reason,
            ));
        }

        // Summary
        let total = self.decisions.len();
        let wins = self.neural_wins();
        out.push_str(&format!(
            "\n  Summary: {}/{} blocks improved by neural path\n",
            wins, total,
        ));
        out.push_str(&format!(
            "  Total cost: {} (classical: {}, saved: {:.1}%)\n",
            self.total_neural_cost,
            self.total_classical_cost,
            self.total_improvement_pct(),
        ));

        out
    }

    /// Format training progress for --train mode.
    pub fn format_training(
        gen_start: u64,
        gen_end: u64,
        duration_us: u64,
        score_before: u64,
        score_after: u64,
        status: &OptimizerStatus,
    ) -> String {
        let improvement = if score_before > 0 {
            (1.0 - score_after as f64 / score_before as f64) * 100.0
        } else {
            0.0
        };
        format!(
            "Neural optimizer: training gen {} -> {} ({} generations, {:.1}ms)\n\
             \x20 best: {} -> {} ({:.1}% improvement)\n\
             \x20 status: {}\n",
            gen_start,
            gen_end,
            gen_end - gen_start,
            duration_us as f64 / 1000.0,
            score_before,
            score_after,
            improvement,
            status,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_report() {
        let r = OptimizerReport::empty();
        assert_eq!(r.neural_wins(), 0);
        let text = r.format_report();
        assert!(text.contains("Neural optimizer"));
        assert!(text.contains("0/0"));
    }

    #[test]
    fn report_with_decisions() {
        let r = OptimizerReport {
            status: OptimizerStatus::Improving,
            generation: 100,
            weight_hash: "abc123def456".to_string(),
            decisions: vec![
                BlockDecision {
                    block_id: "main:0..14".to_string(),
                    winner: Winner::Neural,
                    winner_cost: 1024,
                    loser_cost: 2048,
                    reason: DecisionReason::CliffJump,
                },
                BlockDecision {
                    block_id: "main:15..28".to_string(),
                    winner: Winner::Classical,
                    winner_cost: 512,
                    loser_cost: 512,
                    reason: DecisionReason::NeuralFailedVerify,
                },
            ],
            total_neural_cost: 1536,
            total_classical_cost: 2560,
        };
        assert_eq!(r.neural_wins(), 1);
        let text = r.format_report();
        assert!(text.contains("cliff jump"));
        assert!(text.contains("neural failed verify"));
        assert!(text.contains("1/2 blocks"));
        assert!(text.contains("gen 100"));
    }

    #[test]
    fn improvement_percentage() {
        let d = BlockDecision {
            block_id: "test:0..5".to_string(),
            winner: Winner::Neural,
            winner_cost: 1024,
            loser_cost: 2048,
            reason: DecisionReason::CliffJump,
        };
        assert!((d.improvement_pct() - (-50.0)).abs() < 0.1);
    }

    #[test]
    fn training_format() {
        let text = OptimizerReport::format_training(
            100,
            150,
            2300,
            8192,
            8064,
            &OptimizerStatus::Improving,
        );
        assert!(text.contains("100 -> 150"));
        assert!(text.contains("50 generations"));
        assert!(text.contains("improving"));
    }
}
