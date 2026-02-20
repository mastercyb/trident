//! Stage 3: Online learning with replay buffer and regression guard.
//!
//! Micro-finetunes the model on new build results as they accumulate.
//! Includes a regression guard that prevents activating checkpoints
//! that perform worse than the current production model.

use crate::neural::data::replay::{BuildResult, ReplayBuffer};

/// Online learning configuration.
pub struct OnlineConfig {
    /// Minimum new results before triggering micro-finetune.
    pub min_new_results: usize,
    /// Maximum time between finetunes (seconds).
    pub max_interval_secs: u64,
    /// GFlowNet gradient steps per micro-finetune.
    pub gradient_steps: usize,
    /// Fraction of historical samples to mix in (prevents forgetting).
    pub historical_fraction: f32,
    /// Maximum validity delta (pp) before rejecting a checkpoint.
    pub max_validity_regression: f32,
    /// Minimum valid non-fallback results for Phase B activation.
    pub phase_b_threshold: usize,
}

impl Default for OnlineConfig {
    fn default() -> Self {
        Self {
            min_new_results: 50,
            max_interval_secs: 86400, // 24h
            gradient_steps: 200,
            historical_fraction: 0.10,
            max_validity_regression: 2.0,
            phase_b_threshold: 100,
        }
    }
}

/// Phase B metrics (binding after phase_b_threshold reached).
pub struct PhaseBMetrics {
    /// Fraction of outputs that pass stack verification.
    pub validity_rate: f32,
    /// Fraction of valid outputs that beat compiler.
    pub improvement_rate: f32,
    /// Median cycle reduction as fraction.
    pub median_cycle_reduction: f32,
    /// P90 inference latency in milliseconds.
    pub p90_latency_ms: f32,
    /// Fraction of outputs that fell back to compiler.
    pub fallback_rate: f32,
}

impl PhaseBMetrics {
    /// Check if all Phase B targets are met.
    pub fn all_targets_met(&self) -> bool {
        self.validity_rate >= 0.95
            && self.improvement_rate >= 0.60
            && self.median_cycle_reduction >= 0.10
            && self.p90_latency_ms <= 200.0
            && self.fallback_rate <= 0.05
    }
}

/// Compute Phase B metrics from the replay buffer.
pub fn compute_phase_b_metrics(buffer: &ReplayBuffer) -> PhaseBMetrics {
    let entries = buffer.sample(buffer.len());
    let total = entries.len() as f32;

    if total == 0.0 {
        return PhaseBMetrics {
            validity_rate: 0.0,
            improvement_rate: 0.0,
            median_cycle_reduction: 0.0,
            p90_latency_ms: 0.0,
            fallback_rate: 0.0,
        };
    }

    let valid_count = entries.iter().filter(|r| r.valid).count();
    let fallback_count = entries.iter().filter(|r| r.fallback_used).count();

    let validity_rate = valid_count as f32 / total;
    let fallback_rate = fallback_count as f32 / total;

    // Improvement rate: fraction of valid that beat compiler
    let valid_entries: Vec<&&BuildResult> = entries.iter().filter(|r| r.valid).collect();
    let improved_count = valid_entries
        .iter()
        .filter(|r| {
            r.clock_cycles
                .map(|c| c < r.compiler_cycles)
                .unwrap_or(false)
        })
        .count();
    let improvement_rate = if valid_entries.is_empty() {
        0.0
    } else {
        improved_count as f32 / valid_entries.len() as f32
    };

    // Median cycle reduction
    let mut reductions: Vec<f32> = valid_entries
        .iter()
        .filter_map(|r| {
            r.clock_cycles.map(|c| {
                if r.compiler_cycles == 0 {
                    0.0
                } else {
                    (r.compiler_cycles as f32 - c as f32) / r.compiler_cycles as f32
                }
            })
        })
        .collect();
    reductions.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_cycle_reduction = if reductions.is_empty() {
        0.0
    } else {
        reductions[reductions.len() / 2]
    };

    PhaseBMetrics {
        validity_rate,
        improvement_rate,
        median_cycle_reduction,
        p90_latency_ms: 0.0, // Measured externally
        fallback_rate,
    }
}

/// Check if micro-finetune should be triggered.
pub fn should_finetune(
    new_results_count: usize,
    last_finetune_timestamp: u64,
    current_timestamp: u64,
    config: &OnlineConfig,
) -> bool {
    if new_results_count >= config.min_new_results {
        return true;
    }
    if current_timestamp.saturating_sub(last_finetune_timestamp) >= config.max_interval_secs {
        return new_results_count > 0;
    }
    false
}

/// Regression guard: check if new checkpoint is safe to activate.
///
/// Returns true if the new validity rate is within acceptable range
/// of the current production model's validity rate.
pub fn regression_guard(current_validity: f32, new_validity: f32, max_regression_pp: f32) -> bool {
    // new_validity should not drop more than max_regression_pp percentage points
    new_validity >= current_validity - max_regression_pp / 100.0
}

/// Whether Phase B should be activated (enough valid results in buffer).
pub fn phase_b_active(buffer: &ReplayBuffer, config: &OnlineConfig) -> bool {
    buffer.valid_count() >= config.phase_b_threshold
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::neural::data::replay::BuildResult;

    fn make_result(valid: bool, cycles: Option<u64>, compiler: u64) -> BuildResult {
        BuildResult {
            tir_hash: [0u8; 32],
            generated_tasm: vec!["push 1".into()],
            valid,
            clock_cycles: cycles,
            compiler_cycles: compiler,
            fallback_used: false,
            timestamp: 0,
            model_version: 1,
        }
    }

    #[test]
    fn should_finetune_by_count() {
        let config = OnlineConfig::default();
        assert!(should_finetune(50, 0, 100, &config));
        assert!(!should_finetune(10, 0, 100, &config));
    }

    #[test]
    fn should_finetune_by_time() {
        let config = OnlineConfig::default();
        // 24h+ elapsed, some results
        assert!(should_finetune(5, 0, 86401, &config));
        // 24h+ elapsed, no results
        assert!(!should_finetune(0, 0, 86401, &config));
    }

    #[test]
    fn regression_guard_allows_improvement() {
        assert!(regression_guard(0.80, 0.85, 2.0));
    }

    #[test]
    fn regression_guard_allows_small_regression() {
        assert!(regression_guard(0.80, 0.79, 2.0));
    }

    #[test]
    fn regression_guard_rejects_large_regression() {
        assert!(!regression_guard(0.80, 0.75, 2.0));
    }

    #[test]
    fn phase_b_activation() {
        let config = OnlineConfig::default();
        let mut buf = ReplayBuffer::new(200);
        for _ in 0..99 {
            buf.push(make_result(true, Some(5), 10));
        }
        assert!(!phase_b_active(&buf, &config)); // 99 < 100

        buf.push(make_result(true, Some(5), 10));
        assert!(phase_b_active(&buf, &config)); // 100 >= 100
    }

    #[test]
    fn phase_b_metrics_computation() {
        let mut buf = ReplayBuffer::new(200);
        // 8 valid improvements
        for _ in 0..8 {
            buf.push(make_result(true, Some(5), 10)); // 50% reduction
        }
        // 2 valid no improvement
        for _ in 0..2 {
            buf.push(make_result(true, Some(10), 10)); // 0% reduction
        }

        let metrics = compute_phase_b_metrics(&buf);
        assert!((metrics.validity_rate - 1.0).abs() < 0.01); // all valid
        assert!((metrics.improvement_rate - 0.8).abs() < 0.01); // 8/10
        assert!(metrics.median_cycle_reduction > 0.0);
        assert!((metrics.fallback_rate).abs() < 0.01);
    }

    #[test]
    fn phase_b_all_targets() {
        let metrics = PhaseBMetrics {
            validity_rate: 0.96,
            improvement_rate: 0.65,
            median_cycle_reduction: 0.15,
            p90_latency_ms: 150.0,
            fallback_rate: 0.03,
        };
        assert!(metrics.all_targets_met());

        let bad = PhaseBMetrics {
            validity_rate: 0.90, // below 0.95
            ..metrics
        };
        assert!(!bad.all_targets_met());
    }
}
