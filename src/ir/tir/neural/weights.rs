//! Weight serialization and persistence.
//!
//! Weights stored as raw little-endian u64 values in neural/weights.bin.
//! Metadata (generation, scores, convergence) in neural/meta.toml.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use crate::field::fixed::Fixed;
use crate::field::goldilocks::Goldilocks;
use crate::field::poseidon2;
use crate::field::PrimeField;

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

/// Tracks convergence over generations.
pub struct ConvergenceTracker {
    scores: VecDeque<u64>,
    window: usize,
    threshold: f64,
    plateau_count: usize,
}

impl ConvergenceTracker {
    pub fn new() -> Self {
        Self {
            scores: VecDeque::new(),
            window: 50,
            threshold: 0.001,
            plateau_count: 0,
        }
    }

    /// Record a generation's best score and return the current status.
    pub fn record(&mut self, score: u64) -> OptimizerStatus {
        self.scores.push_back(score);
        if self.scores.len() > self.window {
            self.scores.pop_front();
        }

        if self.scores.len() < self.window {
            return OptimizerStatus::Improving;
        }

        // Check if improvement over the window exceeds threshold
        let oldest = *self.scores.front().unwrap();
        let newest = *self.scores.back().unwrap();

        let improved = if oldest > 0 {
            (oldest as f64 - newest as f64) / oldest as f64
        } else {
            0.0
        };

        if improved < self.threshold {
            self.plateau_count += 1;
            if self.plateau_count >= 3 {
                OptimizerStatus::Converged
            } else {
                OptimizerStatus::Plateaued
            }
        } else {
            self.plateau_count = 0;
            OptimizerStatus::Improving
        }
    }

    pub fn status(&self) -> OptimizerStatus {
        if self.scores.len() < self.window {
            return OptimizerStatus::Improving;
        }
        let oldest = *self.scores.front().unwrap();
        let newest = *self.scores.back().unwrap();
        let improved = if oldest > 0 {
            (oldest as f64 - newest as f64) / oldest as f64
        } else {
            0.0
        };
        if improved < self.threshold {
            if self.plateau_count >= 3 {
                OptimizerStatus::Converged
            } else {
                OptimizerStatus::Plateaued
            }
        } else {
            OptimizerStatus::Improving
        }
    }
}

/// Optimizer metadata persisted alongside weights.
#[derive(Clone, Debug)]
pub struct OptimizerMeta {
    pub generation: u64,
    pub weight_hash: String,
    pub best_score: u64,
    pub prev_score: u64,
    pub baseline_score: u64,
    pub status: OptimizerStatus,
}

impl OptimizerMeta {
    pub fn improvement_pct(&self) -> f64 {
        if self.baseline_score == 0 {
            return 0.0;
        }
        (1.0 - self.best_score as f64 / self.baseline_score as f64) * 100.0
    }
}

/// Serialize weights to a binary file (raw little-endian u64 values).
pub fn save_weights(weights: &[Fixed], path: &Path) -> std::io::Result<()> {
    let mut bytes = Vec::with_capacity(weights.len() * 8);
    for w in weights {
        bytes.extend_from_slice(&w.raw().to_u64().to_le_bytes());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, &bytes)
}

/// Load weights from a binary file.
pub fn load_weights(path: &Path) -> std::io::Result<Vec<Fixed>> {
    let bytes = std::fs::read(path)?;
    if bytes.len() % 8 != 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "weight file size not multiple of 8",
        ));
    }
    let mut weights = Vec::with_capacity(bytes.len() / 8);
    for chunk in bytes.chunks_exact(8) {
        let val = u64::from_le_bytes(chunk.try_into().unwrap());
        weights.push(Fixed::from_raw(Goldilocks::from_u64(val)));
    }
    Ok(weights)
}

/// Compute a content-addressable hash of weight vector using Poseidon2.
pub fn hash_weights(weights: &[Fixed]) -> String {
    let elements: Vec<Goldilocks> = weights.iter().take(256).map(|w| w.raw()).collect();
    let digest = poseidon2::hash_fields_goldilocks(&elements);
    format!("{:016x}{:016x}", digest[0].to_u64(), digest[1].to_u64())
}

/// Save optimizer metadata to TOML.
pub fn save_meta(meta: &OptimizerMeta, path: &Path) -> std::io::Result<()> {
    let content = format!(
        "[optimizer]\n\
         generation = {}\n\
         weight_hash = \"{}\"\n\
         best_score = {}\n\
         prev_score = {}\n\
         baseline_score = {}\n\
         improvement = \"{:.1}%\"\n\
         status = \"{}\"\n",
        meta.generation,
        meta.weight_hash,
        meta.best_score,
        meta.prev_score,
        meta.baseline_score,
        meta.improvement_pct(),
        meta.status,
    );
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

/// Load optimizer metadata from TOML (simple line parsing).
pub fn load_meta(path: &Path) -> std::io::Result<OptimizerMeta> {
    let content = std::fs::read_to_string(path)?;
    let mut generation = 0u64;
    let mut weight_hash = String::new();
    let mut best_score = 0u64;
    let mut prev_score = 0u64;
    let mut baseline_score = 0u64;
    let mut status = OptimizerStatus::Improving;

    for line in content.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("generation = ") {
            generation = val.parse().unwrap_or(0);
        } else if let Some(val) = line.strip_prefix("weight_hash = ") {
            weight_hash = val.trim_matches('"').to_string();
        } else if let Some(val) = line.strip_prefix("best_score = ") {
            best_score = val.parse().unwrap_or(0);
        } else if let Some(val) = line.strip_prefix("prev_score = ") {
            prev_score = val.parse().unwrap_or(0);
        } else if let Some(val) = line.strip_prefix("baseline_score = ") {
            baseline_score = val.parse().unwrap_or(0);
        } else if let Some(val) = line.strip_prefix("status = ") {
            let s = val.trim_matches('"');
            status = match s {
                "plateaued" => OptimizerStatus::Plateaued,
                "converged" => OptimizerStatus::Converged,
                _ => OptimizerStatus::Improving,
            };
        }
    }

    Ok(OptimizerMeta {
        generation,
        weight_hash,
        best_score,
        prev_score,
        baseline_score,
        status,
    })
}

/// User-local neural weights directory: ~/.trident/neural/
/// Training writes here. Takes priority over bundled weights.
fn local_neural_dir() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".trident").join("neural")
}

/// Bundled weights shipped with the compiler: data/neural/
fn bundled_neural_dir() -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest).join("data").join("neural")
}

/// Weights path for saving (always writes to user-local).
pub fn weights_path(_project_root: &Path) -> PathBuf {
    local_neural_dir().join("weights.bin")
}

/// Meta path for saving (always writes to user-local).
pub fn meta_path(_project_root: &Path) -> PathBuf {
    local_neural_dir().join("meta.toml")
}

/// Load weights: user-local first, then bundled.
pub fn load_best_weights() -> std::io::Result<Vec<Fixed>> {
    let local = local_neural_dir().join("weights.bin");
    if local.exists() {
        return load_weights(&local);
    }
    let bundled = bundled_neural_dir().join("weights.bin");
    load_weights(&bundled)
}

/// Load meta: user-local first, then bundled.
pub fn load_best_meta() -> std::io::Result<OptimizerMeta> {
    let local = local_neural_dir().join("meta.toml");
    if local.exists() {
        return load_meta(&local);
    }
    let bundled = bundled_neural_dir().join("meta.toml");
    load_meta(&bundled)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convergence_tracker_starts_improving() {
        let mut tracker = ConvergenceTracker::new();
        assert_eq!(tracker.record(1000), OptimizerStatus::Improving);
        assert_eq!(tracker.record(900), OptimizerStatus::Improving);
    }

    #[test]
    fn convergence_detects_plateau() {
        let mut tracker = ConvergenceTracker::new();
        tracker.window = 5;
        // Same score for entire window
        for _ in 0..5 {
            tracker.record(1000);
        }
        assert_eq!(tracker.status(), OptimizerStatus::Plateaued);
    }

    #[test]
    fn convergence_detects_converged() {
        let mut tracker = ConvergenceTracker::new();
        tracker.window = 3;
        // 3 consecutive plateau windows = converged
        for _ in 0..9 {
            tracker.record(1000);
        }
        assert_eq!(tracker.status(), OptimizerStatus::Converged);
    }

    #[test]
    fn weight_hash_deterministic() {
        let w = vec![Fixed::from_f64(0.5); 100];
        let h1 = hash_weights(&w);
        let h2 = hash_weights(&w);
        assert_eq!(h1, h2);
    }

    #[test]
    fn weight_hash_differs() {
        let w1 = vec![Fixed::from_f64(0.5); 100];
        let w2 = vec![Fixed::from_f64(0.6); 100];
        assert_ne!(hash_weights(&w1), hash_weights(&w2));
    }

    #[test]
    fn save_load_roundtrip() {
        let dir = std::env::temp_dir().join("trident_test_weights");
        let _ = std::fs::remove_dir_all(&dir);

        let weights = vec![Fixed::from_f64(0.5), Fixed::from_f64(-0.3), Fixed::ONE];
        let path = dir.join("test_weights.bin");
        save_weights(&weights, &path).unwrap();
        let loaded = load_weights(&path).unwrap();
        assert_eq!(weights.len(), loaded.len());
        for (a, b) in weights.iter().zip(loaded.iter()) {
            assert_eq!(a, b);
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn meta_roundtrip() {
        let dir = std::env::temp_dir().join("trident_test_meta");
        let _ = std::fs::remove_dir_all(&dir);

        let meta = OptimizerMeta {
            generation: 42,
            weight_hash: "abc123".to_string(),
            best_score: 8192,
            prev_score: 8704,
            baseline_score: 16384,
            status: OptimizerStatus::Improving,
        };
        let path = dir.join("meta.toml");
        save_meta(&meta, &path).unwrap();
        let loaded = load_meta(&path).unwrap();
        assert_eq!(loaded.generation, 42);
        assert_eq!(loaded.weight_hash, "abc123");
        assert_eq!(loaded.best_score, 8192);
        assert_eq!(loaded.baseline_score, 16384);
        assert_eq!(loaded.status, OptimizerStatus::Improving);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
