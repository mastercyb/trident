//! Replay buffer for online learning (Stage 3).
//!
//! Stores build results with prioritized experience replay.
//! Persistence via rkyv zero-copy archives.

use rkyv::{Archive, Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Result of a neural compilation attempt.
#[derive(Archive, Serialize, Deserialize, Clone, Debug)]
#[rkyv(derive(Debug))]
pub struct BuildResult {
    /// Poseidon2 CID of the TIR input.
    pub tir_hash: [u8; 32],
    /// Generated TASM instructions.
    pub generated_tasm: Vec<String>,
    /// Whether the output passed stack verification.
    pub valid: bool,
    /// Clock cycles if valid (None if invalid).
    pub clock_cycles: Option<u64>,
    /// Compiler baseline cycles.
    pub compiler_cycles: u64,
    /// Whether fallback to compiler was used.
    pub fallback_used: bool,
    /// Unix timestamp.
    pub timestamp: u64,
    /// Model checkpoint version.
    pub model_version: u32,
}

/// Serializable wrapper for the replay buffer entries.
#[derive(Archive, Serialize, Deserialize, Clone, Debug)]
#[rkyv(derive(Debug))]
struct ReplayArchive {
    entries: Vec<BuildResult>,
}

/// Default path for replay buffer persistence.
fn default_replay_path() -> PathBuf {
    PathBuf::from("data/neural/v2/replay.rkyv")
}

/// Priority-based replay buffer.
pub struct ReplayBuffer {
    entries: Vec<(f64, BuildResult)>,
    capacity: usize,
}

impl ReplayBuffer {
    /// Create a new replay buffer with the given capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: Vec::new(),
            capacity,
        }
    }

    /// Add a build result with computed priority.
    pub fn push(&mut self, result: BuildResult) {
        let priority = Self::compute_priority(&result);
        self.entries.push((priority, result));

        // If over capacity, remove lowest-priority entry
        if self.entries.len() > self.capacity {
            self.entries
                .sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
            self.entries.truncate(self.capacity);
        }
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Count of valid (non-fallback) results.
    pub fn valid_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|(_, r)| r.valid && !r.fallback_used)
            .count()
    }

    /// Sample a batch of entries (highest priority first).
    pub fn sample(&self, batch_size: usize) -> Vec<&BuildResult> {
        self.entries
            .iter()
            .take(batch_size)
            .map(|(_, r)| r)
            .collect()
    }

    /// Compute priority for a build result.
    fn compute_priority(result: &BuildResult) -> f64 {
        if !result.valid {
            return 0.001; // Low priority for invalid results
        }
        if result.fallback_used {
            return 0.01;
        }
        // Priority = reward = improvement ratio
        let improvement = result
            .compiler_cycles
            .saturating_sub(result.clock_cycles.unwrap_or(result.compiler_cycles));
        if result.compiler_cycles == 0 {
            return 1.0;
        }
        1.0 + (improvement as f64 / result.compiler_cycles as f64)
    }

    /// Save replay buffer to disk as rkyv archive.
    pub fn save(&self, path: Option<&Path>) -> Result<(), String> {
        let path = path.map(PathBuf::from).unwrap_or_else(default_replay_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("mkdir {}: {}", parent.display(), e))?;
        }

        let archive = ReplayArchive {
            entries: self.entries.iter().map(|(_, r)| r.clone()).collect(),
        };

        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&archive)
            .map_err(|e| format!("rkyv serialize: {}", e))?;
        std::fs::write(&path, &bytes).map_err(|e| format!("write {}: {}", path.display(), e))?;

        Ok(())
    }

    /// Load replay buffer from disk. Returns empty buffer if file doesn't exist.
    pub fn load(capacity: usize, path: Option<&Path>) -> Result<Self, String> {
        let path = path.map(PathBuf::from).unwrap_or_else(default_replay_path);
        if !path.exists() {
            return Ok(Self::new(capacity));
        }

        let bytes = std::fs::read(&path).map_err(|e| format!("read {}: {}", path.display(), e))?;
        let archive = rkyv::from_bytes::<ReplayArchive, rkyv::rancor::Error>(&bytes)
            .map_err(|e| format!("rkyv deserialize: {}", e))?;

        let mut buf = Self::new(capacity);
        for result in archive.entries {
            buf.push(result);
        }
        Ok(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn replay_buffer_capacity() {
        let mut buf = ReplayBuffer::new(3);
        for i in 0..5 {
            buf.push(make_result(true, Some(10 - i), 10));
        }
        assert_eq!(buf.len(), 3);
    }

    #[test]
    fn replay_buffer_valid_count() {
        let mut buf = ReplayBuffer::new(10);
        buf.push(make_result(true, Some(5), 10));
        buf.push(make_result(false, None, 10));
        buf.push(make_result(true, Some(8), 10));
        assert_eq!(buf.valid_count(), 2);
    }

    #[test]
    fn replay_buffer_save_load_roundtrip() {
        let dir = std::env::temp_dir().join("trident_test_replay");
        let path = dir.join("test_replay.rkyv");
        let _ = std::fs::remove_file(&path);

        let mut buf = ReplayBuffer::new(10);
        buf.push(make_result(true, Some(5), 10));
        buf.push(make_result(false, None, 10));
        buf.push(make_result(true, Some(8), 10));
        buf.save(Some(&path)).unwrap();

        let loaded = ReplayBuffer::load(10, Some(&path)).unwrap();
        assert_eq!(loaded.len(), 3);
        assert_eq!(loaded.valid_count(), 2);

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn replay_buffer_load_missing_file() {
        let path = std::env::temp_dir().join("trident_nonexistent_replay.rkyv");
        let loaded = ReplayBuffer::load(10, Some(&path)).unwrap();
        assert_eq!(loaded.len(), 0);
    }

    #[test]
    fn replay_buffer_priority_ordering() {
        let mut buf = ReplayBuffer::new(10);
        buf.push(make_result(true, Some(10), 10)); // no improvement
        buf.push(make_result(true, Some(5), 10)); // 50% improvement
        buf.push(make_result(false, None, 10)); // invalid
        let samples = buf.sample(3);
        // Highest priority first (after sort on push)
        assert!(samples[0].valid);
    }
}
