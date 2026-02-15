use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::registry::{PullResult, RegistryClient};

use super::{Dependency, LockedDep, Lockfile, Manifest};

// ─── Dependency Cache ──────────────────────────────────────────────

/// Get the local path where a dependency's source is cached.
///
/// Layout: `<project_root>/.trident/deps/<hash>/main.tri`
pub fn dep_source_path(project_root: &Path, hash: &str) -> PathBuf {
    project_root
        .join(".trident")
        .join("deps")
        .join(hash)
        .join("main.tri")
}

/// Write dependency source into the cache.
pub(super) fn cache_dependency(
    project_root: &Path,
    hash: &str,
    source: &str,
    name: &str,
    source_desc: &str,
) -> Result<(), String> {
    let dep_dir = project_root.join(".trident").join("deps").join(hash);
    std::fs::create_dir_all(&dep_dir)
        .map_err(|e| format!("cannot create cache dir '{}': {}", dep_dir.display(), e))?;

    let source_path = dep_dir.join("main.tri");
    std::fs::write(&source_path, source)
        .map_err(|e| format!("cannot write cached source: {}", e))?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let meta = format!(
        "name={}\nsource={}\nfetched_at={}\n",
        name, source_desc, now
    );
    let meta_path = dep_dir.join("meta.txt");
    std::fs::write(&meta_path, &meta).map_err(|e| format!("cannot write cache metadata: {}", e))?;

    Ok(())
}

// ─── Resolution ────────────────────────────────────────────────────

/// Resolve all dependencies: fetch from registry or verify local paths,
/// populate the cache, and produce/update the lockfile.
///
/// `default_registry` is the fallback registry URL when not specified per-dep.
pub fn resolve_dependencies(
    project_root: &Path,
    manifest: &Manifest,
    existing_lock: &Option<Lockfile>,
    default_registry: &str,
) -> Result<Lockfile, String> {
    let mut locked: BTreeMap<String, LockedDep> = BTreeMap::new();

    for (dep_name, dep) in &manifest.dependencies {
        match dep {
            Dependency::Hash { hash } => {
                resolve_hash_dep(
                    project_root,
                    dep_name,
                    hash,
                    existing_lock,
                    default_registry,
                    &mut locked,
                )?;
            }
            Dependency::Registry { name, registry } => {
                resolve_registry_dep(
                    project_root,
                    dep_name,
                    name,
                    registry,
                    default_registry,
                    &mut locked,
                )?;
            }
            Dependency::Path { path } => {
                resolve_path_dep(project_root, dep_name, path, &mut locked)?;
            }
        }
    }

    Ok(Lockfile { locked })
}

fn resolve_hash_dep(
    project_root: &Path,
    dep_name: &str,
    hash: &str,
    existing_lock: &Option<Lockfile>,
    default_registry: &str,
    locked: &mut BTreeMap<String, LockedDep>,
) -> Result<(), String> {
    let cached = dep_source_path(project_root, hash);
    if cached.exists() {
        // Already in cache — use it.
        let source_desc = existing_lock
            .as_ref()
            .and_then(|lf| lf.locked.get(dep_name))
            .map(|ld| ld.source.clone())
            .unwrap_or_else(|| "hash".to_string());
        locked.insert(
            dep_name.to_string(),
            LockedDep {
                name: dep_name.to_string(),
                hash: hash.to_string(),
                source: source_desc,
            },
        );
        return Ok(());
    }

    // Not cached — try to fetch from the default registry.
    let client = RegistryClient::new(default_registry);
    let pull: PullResult = client
        .pull(hash)
        .map_err(|e| format!("cannot fetch dep '{}' (hash {}): {}", dep_name, hash, e))?;

    let source_desc = format!("registry:{}", default_registry);
    cache_dependency(project_root, hash, &pull.source, dep_name, &source_desc)?;

    locked.insert(
        dep_name.to_string(),
        LockedDep {
            name: dep_name.to_string(),
            hash: hash.to_string(),
            source: source_desc,
        },
    );
    Ok(())
}

fn resolve_registry_dep(
    project_root: &Path,
    dep_name: &str,
    registry_name: &str,
    registry_url: &str,
    default_registry: &str,
    locked: &mut BTreeMap<String, LockedDep>,
) -> Result<(), String> {
    let url = if registry_url.is_empty() {
        default_registry
    } else {
        registry_url
    };

    let client = RegistryClient::new(url);
    let pull: PullResult = client
        .pull_by_name(registry_name)
        .map_err(|e| format!("cannot fetch dep '{}' from {}: {}", dep_name, url, e))?;

    let hash = &pull.hash;
    let source_desc = format!("registry:{}", url);
    cache_dependency(project_root, hash, &pull.source, dep_name, &source_desc)?;

    locked.insert(
        dep_name.to_string(),
        LockedDep {
            name: dep_name.to_string(),
            hash: hash.to_string(),
            source: source_desc,
        },
    );
    Ok(())
}

pub(super) fn resolve_path_dep(
    project_root: &Path,
    dep_name: &str,
    rel_path: &Path,
    locked: &mut BTreeMap<String, LockedDep>,
) -> Result<(), String> {
    let abs_path = project_root.join(rel_path);

    // Try both the path as given and with a .tri extension.
    let source_file = if abs_path.is_file() {
        abs_path.clone()
    } else {
        let with_ext = abs_path.with_extension("tri");
        if with_ext.is_file() {
            with_ext
        } else {
            // Try main.tri inside the directory.
            let main_tri = abs_path.join("main.tri");
            if main_tri.is_file() {
                main_tri
            } else {
                return Err(format!(
                    "path dep '{}': cannot find source at '{}' (tried .tri, main.tri)",
                    dep_name,
                    abs_path.display(),
                ));
            }
        }
    };

    let source = std::fs::read_to_string(&source_file).map_err(|e| {
        format!(
            "path dep '{}': cannot read '{}': {}",
            dep_name,
            source_file.display(),
            e,
        )
    })?;

    // Content-hash the source with Poseidon2 (SNARK-friendly).
    let hash_raw = crate::poseidon2::hash_bytes(source.as_bytes());
    let hash_hex: String = hash_raw.iter().map(|b| format!("{:02x}", b)).collect();

    let source_desc = format!("path:{}", rel_path.display());

    locked.insert(
        dep_name.to_string(),
        LockedDep {
            name: dep_name.to_string(),
            hash: hash_hex,
            source: source_desc,
        },
    );
    Ok(())
}

// ─── Query Helpers ─────────────────────────────────────────────────

/// List all dependency source directories (for use in module resolution).
///
/// Returns the parent directory of each cached `main.tri`, so that a
/// module resolver can add them to its search path.
pub fn dependency_search_paths(project_root: &Path, lockfile: &Lockfile) -> Vec<PathBuf> {
    lockfile
        .locked
        .values()
        .map(|dep| project_root.join(".trident").join("deps").join(&dep.hash))
        .collect()
}
