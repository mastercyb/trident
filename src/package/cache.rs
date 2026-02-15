//! Compilation and verification cache for Trident.
//!
//! Caches are keyed by content hashes from `hash.rs`:
//! - **Compilation cache**: (source hash, target) → compiled TASM + cost info
//! - **Verification cache**: source hash → verification report
//!
//! Cache location: `~/.trident/cache/` (or `$TRIDENT_CACHE_DIR`)
//!
//! Layout:
//! ```text
//! ~/.trident/cache/
//! ├── compile/
//! │   └── <source_hash_hex>.<target>.tasm
//! └── verify/
//!     └── <source_hash_hex>.json
//! ```
//!
//! Cache entries are append-only: once written, never modified. A hash
//! uniquely identifies content, so the same hash always maps to the same
//! result.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::hash::ContentHash;

// ─── Cache Directory ───────────────────────────────────────────────

/// Resolve the cache directory.
///
/// Priority:
/// 1. `$TRIDENT_CACHE_DIR` environment variable
/// 2. `~/.trident/cache/`
pub fn cache_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("TRIDENT_CACHE_DIR") {
        return Some(PathBuf::from(dir));
    }

    dirs_home().map(|home| home.join(".trident").join("cache"))
}

/// Get the user's home directory.
fn dirs_home() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

/// Ensure a cache subdirectory exists.
fn ensure_cache_subdir(subdir: &str) -> Option<PathBuf> {
    let dir = cache_dir()?.join(subdir);
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

// ─── Compilation Cache ─────────────────────────────────────────────

/// A cached compilation result.
#[derive(Clone, Debug)]
pub struct CachedCompilation {
    /// The compiled TASM output.
    pub tasm: String,
    /// Padded height (if known).
    pub padded_height: Option<u64>,
}

/// Look up a cached compilation result.
pub fn lookup_compilation(source_hash: &ContentHash, target: &str) -> Option<CachedCompilation> {
    let dir = cache_dir()?.join("compile");
    let filename = format!("{}.{}.tasm", source_hash.to_hex(), target);
    let path = dir.join(&filename);

    let tasm = std::fs::read_to_string(&path).ok()?;

    // Check for metadata file
    let meta_path = dir.join(format!("{}.{}.meta", source_hash.to_hex(), target));
    let padded_height = std::fs::read_to_string(&meta_path)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok());

    Some(CachedCompilation {
        tasm,
        padded_height,
    })
}

/// Store a compilation result in the cache.
pub fn store_compilation(
    source_hash: &ContentHash,
    target: &str,
    tasm: &str,
    padded_height: Option<u64>,
) -> Result<PathBuf, String> {
    let dir = ensure_cache_subdir("compile")
        .ok_or_else(|| "cannot create cache directory".to_string())?;

    let filename = format!("{}.{}.tasm", source_hash.to_hex(), target);
    let path = dir.join(&filename);

    // Don't overwrite existing cache entries (append-only semantics)
    if path.exists() {
        return Ok(path);
    }

    std::fs::write(&path, tasm).map_err(|e| format!("cannot write cache file: {}", e))?;

    // Store metadata
    if let Some(height) = padded_height {
        let meta_path = dir.join(format!("{}.{}.meta", source_hash.to_hex(), target));
        let _ = std::fs::write(&meta_path, height.to_string());
    }

    Ok(path)
}

// ─── Verification Cache ────────────────────────────────────────────

/// A cached verification result.
#[derive(Clone, Debug)]
pub struct CachedVerification {
    /// Whether the program was verified safe.
    pub is_safe: bool,
    /// Number of constraints.
    pub constraints: usize,
    /// Number of variables.
    pub variables: u32,
    /// Verdict string.
    pub verdict: String,
    /// Timestamp of verification.
    pub timestamp: String,
}

impl CachedVerification {
    /// Serialize to a simple text format.
    fn serialize(&self) -> String {
        format!(
            "safe={}\nconstraints={}\nvariables={}\nverdict={}\ntimestamp={}\n",
            self.is_safe, self.constraints, self.variables, self.verdict, self.timestamp,
        )
    }

    /// Deserialize from text format.
    fn deserialize(text: &str) -> Option<Self> {
        let mut map = HashMap::new();
        for line in text.lines() {
            if let Some((key, value)) = line.split_once('=') {
                map.insert(key.trim().to_string(), value.trim().to_string());
            }
        }

        Some(CachedVerification {
            is_safe: map.get("safe")?.parse().ok()?,
            constraints: map.get("constraints")?.parse().ok()?,
            variables: map.get("variables")?.parse().ok()?,
            verdict: map.get("verdict")?.clone(),
            timestamp: map.get("timestamp").cloned().unwrap_or_default(),
        })
    }
}

/// Look up a cached verification result.
pub fn lookup_verification(source_hash: &ContentHash) -> Option<CachedVerification> {
    let dir = cache_dir()?.join("verify");
    let filename = format!("{}.verify", source_hash.to_hex());
    let path = dir.join(&filename);

    let text = std::fs::read_to_string(&path).ok()?;
    CachedVerification::deserialize(&text)
}

/// Store a verification result in the cache.
pub fn store_verification(
    source_hash: &ContentHash,
    result: &CachedVerification,
) -> Result<PathBuf, String> {
    let dir =
        ensure_cache_subdir("verify").ok_or_else(|| "cannot create cache directory".to_string())?;

    let filename = format!("{}.verify", source_hash.to_hex());
    let path = dir.join(&filename);

    // Don't overwrite existing cache entries
    if path.exists() {
        return Ok(path);
    }

    std::fs::write(&path, result.serialize())
        .map_err(|e| format!("cannot write cache file: {}", e))?;

    Ok(path)
}

// ─── Cache Statistics ──────────────────────────────────────────────

/// Statistics about the cache.
#[derive(Clone, Debug, Default)]
pub struct CacheStats {
    /// Number of cached compilations.
    pub compilations: usize,
    /// Number of cached verifications.
    pub verifications: usize,
    /// Total size in bytes.
    pub total_bytes: u64,
}

/// Get cache statistics.
pub fn stats() -> CacheStats {
    let mut stats = CacheStats::default();

    if let Some(base) = cache_dir() {
        let compile_dir = base.join("compile");
        if compile_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&compile_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().is_some_and(|e| e == "tasm") {
                        stats.compilations += 1;
                    }
                    if let Ok(meta) = std::fs::metadata(&path) {
                        stats.total_bytes += meta.len();
                    }
                }
            }
        }

        let verify_dir = base.join("verify");
        if verify_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&verify_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().is_some_and(|e| e == "verify") {
                        stats.verifications += 1;
                    }
                    if let Ok(meta) = std::fs::metadata(&path) {
                        stats.total_bytes += meta.len();
                    }
                }
            }
        }
    }

    stats
}

/// Clear the entire cache.
pub fn clear() -> Result<(), String> {
    if let Some(base) = cache_dir() {
        if base.exists() {
            std::fs::remove_dir_all(&base).map_err(|e| format!("cannot clear cache: {}", e))?;
        }
    }
    Ok(())
}

/// Get the current timestamp as a string.
pub fn timestamp() -> String {
    format!("{}", crate::package::unix_timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_hash() -> ContentHash {
        ContentHash([0xAB; 32])
    }

    #[test]
    fn test_cache_dir_resolution() {
        // Just verify it doesn't panic
        let _ = cache_dir();
    }

    #[test]
    fn test_cached_verification_round_trip() {
        let result = CachedVerification {
            is_safe: true,
            constraints: 42,
            variables: 10,
            verdict: "Safe".to_string(),
            timestamp: "1234567890".to_string(),
        };

        let serialized = result.serialize();
        let deserialized = CachedVerification::deserialize(&serialized).unwrap();

        assert_eq!(deserialized.is_safe, true);
        assert_eq!(deserialized.constraints, 42);
        assert_eq!(deserialized.variables, 10);
        assert_eq!(deserialized.verdict, "Safe");
    }

    /// Tests below use direct file I/O to a unique temp dir instead of
    /// the `TRIDENT_CACHE_DIR` env var (which races in parallel tests).

    fn store_compilation_at(
        dir: &std::path::Path,
        hash: &ContentHash,
        target: &str,
        tasm: &str,
        height: Option<u64>,
    ) {
        let compile_dir = dir.join("compile");
        std::fs::create_dir_all(&compile_dir).unwrap();
        let filename = format!("{}.{}.tasm", hash.to_hex(), target);
        let path = compile_dir.join(&filename);
        if !path.exists() {
            std::fs::write(&path, tasm).unwrap();
        }
        if let Some(h) = height {
            let meta = compile_dir.join(format!("{}.{}.meta", hash.to_hex(), target));
            std::fs::write(&meta, h.to_string()).unwrap();
        }
    }

    fn lookup_compilation_at(
        dir: &std::path::Path,
        hash: &ContentHash,
        target: &str,
    ) -> Option<CachedCompilation> {
        let compile_dir = dir.join("compile");
        let filename = format!("{}.{}.tasm", hash.to_hex(), target);
        let path = compile_dir.join(&filename);
        let tasm = std::fs::read_to_string(&path).ok()?;
        let meta = compile_dir.join(format!("{}.{}.meta", hash.to_hex(), target));
        let padded_height = std::fs::read_to_string(&meta)
            .ok()
            .and_then(|s| s.trim().parse().ok());
        Some(CachedCompilation {
            tasm,
            padded_height,
        })
    }

    fn store_verification_at(
        dir: &std::path::Path,
        hash: &ContentHash,
        result: &CachedVerification,
    ) {
        let verify_dir = dir.join("verify");
        std::fs::create_dir_all(&verify_dir).unwrap();
        let filename = format!("{}.verify", hash.to_hex());
        let path = verify_dir.join(&filename);
        if !path.exists() {
            std::fs::write(&path, result.serialize()).unwrap();
        }
    }

    fn lookup_verification_at(
        dir: &std::path::Path,
        hash: &ContentHash,
    ) -> Option<CachedVerification> {
        let verify_dir = dir.join("verify");
        let filename = format!("{}.verify", hash.to_hex());
        let path = verify_dir.join(&filename);
        let text = std::fs::read_to_string(&path).ok()?;
        CachedVerification::deserialize(&text)
    }

    #[test]
    fn test_store_and_lookup_compilation() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let hash = test_hash();
        let tasm = "push 1\npush 2\nadd\n";

        store_compilation_at(dir, &hash, "triton", tasm, Some(32));

        let cached = lookup_compilation_at(dir, &hash, "triton").unwrap();
        assert_eq!(cached.tasm, tasm);
        assert_eq!(cached.padded_height, Some(32));

        // Lookup non-existent target
        assert!(lookup_compilation_at(dir, &hash, "miden").is_none());
    }

    #[test]
    fn test_store_and_lookup_verification() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let hash = test_hash();
        let result = CachedVerification {
            is_safe: true,
            constraints: 5,
            variables: 3,
            verdict: "Safe".to_string(),
            timestamp: "12345".to_string(),
        };

        store_verification_at(dir, &hash, &result);

        let cached = lookup_verification_at(dir, &hash).unwrap();
        assert!(cached.is_safe);
        assert_eq!(cached.constraints, 5);
    }

    #[test]
    fn test_cache_stats_direct() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let hash = test_hash();

        store_compilation_at(dir, &hash, "triton", "push 1\n", None);
        let result = CachedVerification {
            is_safe: true,
            constraints: 1,
            variables: 1,
            verdict: "Safe".to_string(),
            timestamp: "12345".to_string(),
        };
        store_verification_at(dir, &hash, &result);

        // Count files manually
        let compile_count = std::fs::read_dir(dir.join("compile"))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "tasm"))
            .count();
        let verify_count = std::fs::read_dir(dir.join("verify"))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "verify"))
            .count();
        assert_eq!(compile_count, 1);
        assert_eq!(verify_count, 1);
    }

    #[test]
    fn test_append_only_semantics() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let hash = test_hash();

        // First write
        store_compilation_at(dir, &hash, "triton", "push 1\n", None);

        // Second write with different content — should NOT overwrite
        store_compilation_at(dir, &hash, "triton", "push 2\n", None);

        let cached = lookup_compilation_at(dir, &hash, "triton").unwrap();
        assert_eq!(cached.tasm, "push 1\n", "append-only: first write wins");
    }
}
