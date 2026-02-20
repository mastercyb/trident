//! Checkpoint management for neural compiler v2.
//!
//! Uses burn's native record format (NamedMpk) for model weights.
//! Supports stage-tagged checkpoints: stage1_best, stage2_latest, production.

use std::path::PathBuf;

use burn::module::Module;
use burn::prelude::*;
use burn::record::{FullPrecisionSettings, NamedMpkFileRecorder};

/// Checkpoint directory relative to repo root.
const CHECKPOINT_DIR: &str = "data/neural/v2";

/// Checkpoint tag for naming saved files.
#[derive(Debug, Clone, Copy)]
pub enum CheckpointTag {
    Stage1Best,
    Stage2Latest,
    Production,
}

impl CheckpointTag {
    fn stem(&self) -> &'static str {
        match self {
            Self::Stage1Best => "stage1_best",
            Self::Stage2Latest => "stage2_latest",
            Self::Production => "production",
        }
    }
}

/// Resolve the checkpoint directory, creating it if needed.
fn checkpoint_dir() -> PathBuf {
    let mut dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    // Walk up to repo root (has Cargo.toml + vm/)
    loop {
        if dir.join("Cargo.toml").exists() && dir.join("vm").is_dir() {
            break;
        }
        if !dir.pop() {
            dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            break;
        }
    }
    dir.join(CHECKPOINT_DIR)
}

/// Save a model checkpoint to disk.
///
/// Uses NamedMpk format with full precision (lossless).
/// File will be at `data/neural/v2/{tag}.mpk`.
pub fn save_checkpoint<B: Backend, M: Module<B> + Clone>(
    model: &M,
    tag: CheckpointTag,
    _device: &B::Device,
) -> Result<PathBuf, String> {
    let dir = checkpoint_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir {}: {}", dir.display(), e))?;

    let path = dir.join(tag.stem());
    let recorder = NamedMpkFileRecorder::<FullPrecisionSettings>::new();
    model
        .clone()
        .save_file(path.clone(), &recorder)
        .map_err(|e| format!("save {}: {}", path.display(), e))?;

    // burn appends .mpk extension
    let full_path = path.with_extension("mpk");
    Ok(full_path)
}

/// Load a model checkpoint from disk.
///
/// Returns the model with loaded weights, or None if checkpoint doesn't exist.
pub fn load_checkpoint<B: Backend, M: Module<B>>(
    model: M,
    tag: CheckpointTag,
    device: &B::Device,
) -> Result<Option<M>, String> {
    let dir = checkpoint_dir();
    let path = dir.join(tag.stem());

    // burn's NamedMpkFileRecorder appends .mpk
    let full_path = path.with_extension("mpk");
    if !full_path.exists() {
        return Ok(None);
    }

    let recorder = NamedMpkFileRecorder::<FullPrecisionSettings>::new();
    let loaded = model
        .load_file(path, &recorder, device)
        .map_err(|e| format!("load {}: {}", full_path.display(), e))?;

    Ok(Some(loaded))
}

/// Check which checkpoints exist on disk.
pub fn available_checkpoints() -> Vec<(CheckpointTag, PathBuf)> {
    let dir = checkpoint_dir();
    let mut found = Vec::new();
    for tag in [
        CheckpointTag::Production,
        CheckpointTag::Stage1Best,
        CheckpointTag::Stage2Latest,
    ] {
        let path = dir.join(tag.stem()).with_extension("mpk");
        if path.exists() {
            found.push((tag, path));
        }
    }
    found
}

/// Detect which training stage to run based on existing checkpoints.
///
/// - No checkpoints → Stage 1 (supervised)
/// - Stage1Best exists → Stage 2 (GFlowNet)
/// - Stage2Latest exists + replay ≥ threshold → Stage 3 (online)
pub fn detect_stage(replay_count: usize, replay_threshold: usize) -> TrainingStage {
    let dir = checkpoint_dir();

    let has_stage2 = dir.join("stage2_latest.mpk").exists();
    let has_stage1 = dir.join("stage1_best.mpk").exists();
    let has_production = dir.join("production.mpk").exists();

    if has_stage2 && replay_count >= replay_threshold {
        TrainingStage::Stage3Online
    } else if has_stage1 || has_production {
        TrainingStage::Stage2GFlowNet
    } else {
        TrainingStage::Stage1Supervised
    }
}

/// Which training stage the system should execute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrainingStage {
    Stage1Supervised,
    Stage2GFlowNet,
    Stage3Online,
}

impl std::fmt::Display for TrainingStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stage1Supervised => write!(f, "Stage 1: supervised CE"),
            Self::Stage2GFlowNet => write!(f, "Stage 2: GFlowNet TB"),
            Self::Stage3Online => write!(f, "Stage 3: online learning"),
        }
    }
}

/// Promote a checkpoint to production (copy file).
pub fn promote_to_production(source: CheckpointTag) -> Result<(), String> {
    let dir = checkpoint_dir();
    let src = dir.join(source.stem()).with_extension("mpk");
    let dst = dir.join("production.mpk");

    if !src.exists() {
        return Err(format!("{} does not exist", src.display()));
    }

    std::fs::copy(&src, &dst)
        .map_err(|e| format!("copy {} → {}: {}", src.display(), dst.display(), e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_stage_returns_valid_stage() {
        // detect_stage examines real filesystem; just verify it returns a valid stage
        let stage = detect_stage(0, 100);
        match stage {
            TrainingStage::Stage1Supervised
            | TrainingStage::Stage2GFlowNet
            | TrainingStage::Stage3Online => {} // all valid
        }
    }

    #[test]
    fn checkpoint_tag_stems() {
        assert_eq!(CheckpointTag::Stage1Best.stem(), "stage1_best");
        assert_eq!(CheckpointTag::Stage2Latest.stem(), "stage2_latest");
        assert_eq!(CheckpointTag::Production.stem(), "production");
    }
}
