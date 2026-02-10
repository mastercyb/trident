//! Content-addressed package manager for Trident.
//!
//! Parses `[dependencies]` from `trident.toml`, manages a lockfile
//! (`trident.lock`), and caches dependency sources under `.trident/deps/`.
//!
//! Three dependency kinds:
//!   - **Hash** — pinned by a 64-hex-char BLAKE3 content hash.
//!   - **Registry** — resolved via a `RegistryClient` by name.
//!   - **Path** — local filesystem, re-read every build.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::registry::{PullResult, RegistryClient};

// ─── Data Types ────────────────────────────────────────────────────

/// A declared dependency in trident.toml.
#[derive(Clone, Debug)]
pub enum Dependency {
    /// Pinned by content hash (64 hex chars).
    Hash { hash: String },
    /// Resolved via a registry by name.
    Registry { name: String, registry: String },
    /// Local filesystem path.
    Path { path: PathBuf },
}

/// A resolved (locked) dependency.
#[derive(Clone, Debug)]
pub struct LockedDep {
    pub name: String,
    pub hash: String,
    pub source: String, // "registry:<url>", "path:<relative>", "hash"
}

/// Package manifest: parsed `[dependencies]` from trident.toml.
#[derive(Clone, Debug, Default)]
pub struct Manifest {
    pub dependencies: HashMap<String, Dependency>,
}

/// Lock file contents.
#[derive(Clone, Debug, Default)]
pub struct Lockfile {
    pub locked: HashMap<String, LockedDep>,
}

// ─── Parsing ───────────────────────────────────────────────────────

/// Parse the `[dependencies]` section from trident.toml content.
///
/// Handles three forms:
///   name = "64hexchars"                          -> Hash dep
///   name = { name = "x", registry = "url" }      -> Registry dep
///   name = { path = "relative/path" }             -> Path dep
pub fn parse_dependencies(toml_content: &str) -> Manifest {
    let mut deps: HashMap<String, Dependency> = HashMap::new();
    let mut in_deps_section = false;

    for line in toml_content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Detect section headers.
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let section = trimmed[1..trimmed.len() - 1].trim();
            in_deps_section = section == "dependencies";
            continue;
        }

        if !in_deps_section {
            continue;
        }

        if let Some((key, value)) = trimmed.split_once('=') {
            let key = key.trim().trim_matches('"');
            let value = value.trim();

            if value.starts_with('{') {
                // Inline table: parse key-value pairs inside braces.
                let inner = value.trim_start_matches('{').trim_end_matches('}').trim();
                let fields = parse_inline_table(inner);

                if let Some(path_val) = fields.get("path") {
                    deps.insert(
                        key.to_string(),
                        Dependency::Path {
                            path: PathBuf::from(path_val),
                        },
                    );
                } else if let Some(reg_name) = fields.get("name") {
                    let registry = fields.get("registry").cloned().unwrap_or_default();
                    deps.insert(
                        key.to_string(),
                        Dependency::Registry {
                            name: reg_name.clone(),
                            registry,
                        },
                    );
                }
            } else {
                // Plain string value — strip quotes.
                let val = value.trim_matches('"');
                if is_hex_hash(val) {
                    deps.insert(
                        key.to_string(),
                        Dependency::Hash {
                            hash: val.to_string(),
                        },
                    );
                }
            }
        }
    }

    Manifest { dependencies: deps }
}

/// Parse a TOML inline table body: `name = "x", registry = "url"`.
fn parse_inline_table(s: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for pair in s.split(',') {
        let pair = pair.trim();
        if let Some((k, v)) = pair.split_once('=') {
            let k = k.trim().trim_matches('"');
            let v = v.trim().trim_matches('"');
            map.insert(k.to_string(), v.to_string());
        }
    }
    map
}

/// Check whether a string looks like a 64-char hex hash.
fn is_hex_hash(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}

// ─── Lockfile I/O ──────────────────────────────────────────────────

/// Load a lockfile from disk.
///
/// Format:
/// ```text
/// # trident.lock — DO NOT EDIT MANUALLY
/// [lock]
/// name = { hash = "abc...", source = "registry:https://..." }
/// ```
pub fn load_lockfile(path: &Path) -> Result<Lockfile, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read lockfile '{}': {}", path.display(), e))?;

    let mut locked: HashMap<String, LockedDep> = HashMap::new();
    let mut in_lock_section = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let section = trimmed[1..trimmed.len() - 1].trim();
            in_lock_section = section == "lock";
            continue;
        }
        if !in_lock_section {
            continue;
        }

        if let Some((key, value)) = trimmed.split_once('=') {
            let name = key.trim().to_string();
            let value = value.trim();

            if value.starts_with('{') {
                let inner = value.trim_start_matches('{').trim_end_matches('}').trim();
                let fields = parse_inline_table(inner);
                let hash = fields.get("hash").cloned().unwrap_or_default();
                let source = fields.get("source").cloned().unwrap_or_default();
                locked.insert(
                    name.clone(),
                    LockedDep {
                        name: name.clone(),
                        hash,
                        source,
                    },
                );
            }
        }
    }

    Ok(Lockfile { locked })
}

/// Save a lockfile to disk.
pub fn save_lockfile(path: &Path, lockfile: &Lockfile) -> Result<(), String> {
    let mut out = String::new();
    out.push_str("# trident.lock — DO NOT EDIT MANUALLY\n");
    out.push_str("[lock]\n");

    // Sort entries for deterministic output.
    let mut entries: Vec<_> = lockfile.locked.iter().collect();
    entries.sort_by_key(|(k, _)| (*k).clone());

    for (name, dep) in entries {
        out.push_str(&format!(
            "{} = {{ hash = \"{}\", source = \"{}\" }}\n",
            name, dep.hash, dep.source,
        ));
    }

    std::fs::write(path, &out)
        .map_err(|e| format!("cannot write lockfile '{}': {}", path.display(), e))
}

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
fn cache_dependency(
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
    let mut locked: HashMap<String, LockedDep> = HashMap::new();

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
    locked: &mut HashMap<String, LockedDep>,
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
    locked: &mut HashMap<String, LockedDep>,
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

fn resolve_path_dep(
    project_root: &Path,
    dep_name: &str,
    rel_path: &Path,
    locked: &mut HashMap<String, LockedDep>,
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

// ─── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_dependencies ─────────────────────────────────────

    #[test]
    fn test_parse_dependencies_hash() {
        let toml = r#"
[project]
name = "my_app"

[dependencies]
crypto_utils = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
"#;
        let manifest = parse_dependencies(toml);
        assert_eq!(manifest.dependencies.len(), 1);
        match &manifest.dependencies["crypto_utils"] {
            Dependency::Hash { hash } => {
                assert_eq!(hash.len(), 64);
                assert_eq!(
                    hash,
                    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                );
            }
            other => panic!("expected Hash dep, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_dependencies_registry() {
        let toml = r#"
[dependencies]
math_lib = { name = "math_lib", registry = "https://registry.trident-lang.org" }
"#;
        let manifest = parse_dependencies(toml);
        assert_eq!(manifest.dependencies.len(), 1);
        match &manifest.dependencies["math_lib"] {
            Dependency::Registry { name, registry } => {
                assert_eq!(name, "math_lib");
                assert_eq!(registry, "https://registry.trident-lang.org");
            }
            other => panic!("expected Registry dep, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_dependencies_path() {
        let toml = r#"
[dependencies]
local_helper = { path = "../shared/helper" }
"#;
        let manifest = parse_dependencies(toml);
        assert_eq!(manifest.dependencies.len(), 1);
        match &manifest.dependencies["local_helper"] {
            Dependency::Path { path } => {
                assert_eq!(path, &PathBuf::from("../shared/helper"));
            }
            other => panic!("expected Path dep, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_dependencies_mixed() {
        let toml = r#"
[project]
name = "my_app"
version = "0.1.0"

[dependencies]
crypto_utils = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
math_lib = { name = "math_lib", registry = "https://registry.example.com" }
local_helper = { path = "../shared/helper" }

[targets.debug]
flags = ["debug"]
"#;
        let manifest = parse_dependencies(toml);
        assert_eq!(manifest.dependencies.len(), 3);

        assert!(matches!(
            manifest.dependencies["crypto_utils"],
            Dependency::Hash { .. }
        ));
        assert!(matches!(
            manifest.dependencies["math_lib"],
            Dependency::Registry { .. }
        ));
        assert!(matches!(
            manifest.dependencies["local_helper"],
            Dependency::Path { .. }
        ));
    }

    #[test]
    fn test_parse_dependencies_empty() {
        let toml = r#"
[project]
name = "empty_app"
"#;
        let manifest = parse_dependencies(toml);
        assert!(manifest.dependencies.is_empty());
    }

    #[test]
    fn test_parse_dependencies_no_deps_section() {
        let toml = r#"
[project]
name = "no_deps"
version = "0.1.0"

[targets.release]
flags = ["release"]
"#;
        let manifest = parse_dependencies(toml);
        assert!(manifest.dependencies.is_empty());
    }

    // ── lockfile round-trip ────────────────────────────────────

    #[test]
    fn test_lockfile_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().join("trident.lock");

        let mut locked = HashMap::new();
        locked.insert(
            "crypto_utils".to_string(),
            LockedDep {
                name: "crypto_utils".to_string(),
                hash: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_string(),
                source: "registry:https://registry.trident-lang.org".to_string(),
            },
        );
        locked.insert(
            "local_helper".to_string(),
            LockedDep {
                name: "local_helper".to_string(),
                hash: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                    .to_string(),
                source: "path:../shared/helper".to_string(),
            },
        );
        let lockfile = Lockfile { locked };

        save_lockfile(&lock_path, &lockfile).unwrap();
        let loaded = load_lockfile(&lock_path).unwrap();

        assert_eq!(loaded.locked.len(), 2);

        let crypto = &loaded.locked["crypto_utils"];
        assert_eq!(crypto.name, "crypto_utils");
        assert_eq!(
            crypto.hash,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        assert_eq!(crypto.source, "registry:https://registry.trident-lang.org");

        let helper = &loaded.locked["local_helper"];
        assert_eq!(helper.name, "local_helper");
        assert_eq!(
            helper.hash,
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        );
        assert_eq!(helper.source, "path:../shared/helper");
    }

    #[test]
    fn test_lockfile_load_missing_file() {
        let result = load_lockfile(Path::new("/nonexistent/trident.lock"));
        assert!(result.is_err());
    }

    // ── dep_source_path ────────────────────────────────────────

    #[test]
    fn test_dep_source_path() {
        let root = PathBuf::from("/home/user/myproject");
        let hash = "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";
        let path = dep_source_path(&root, hash);
        assert_eq!(
            path,
            PathBuf::from(format!(
                "/home/user/myproject/.trident/deps/{}/main.tri",
                hash
            ))
        );
    }

    // ── resolve_path_dep ───────────────────────────────────────

    #[test]
    fn test_resolve_path_dep() {
        let dir = tempfile::tempdir().unwrap();
        let project_root = dir.path();

        // Create a local dep source file.
        let dep_dir = project_root.join("libs");
        std::fs::create_dir_all(&dep_dir).unwrap();
        let dep_file = dep_dir.join("helper.tri");
        std::fs::write(
            &dep_file,
            "module helper\nfn add(a: Field, b: Field) -> Field { a + b }\n",
        )
        .unwrap();

        let mut locked = HashMap::new();
        resolve_path_dep(
            project_root,
            "helper",
            Path::new("libs/helper.tri"),
            &mut locked,
        )
        .unwrap();

        assert_eq!(locked.len(), 1);
        let dep = &locked["helper"];
        assert_eq!(dep.name, "helper");
        assert_eq!(dep.hash.len(), 64, "hash should be 64 hex chars");
        assert!(dep.source.starts_with("path:"));

        // Hash is deterministic.
        let mut locked2 = HashMap::new();
        resolve_path_dep(
            project_root,
            "helper",
            Path::new("libs/helper.tri"),
            &mut locked2,
        )
        .unwrap();
        assert_eq!(locked["helper"].hash, locked2["helper"].hash);
    }

    #[test]
    fn test_resolve_path_dep_directory() {
        let dir = tempfile::tempdir().unwrap();
        let project_root = dir.path();

        // Create a directory with main.tri inside it.
        let dep_dir = project_root.join("my_dep");
        std::fs::create_dir_all(&dep_dir).unwrap();
        std::fs::write(
            dep_dir.join("main.tri"),
            "module my_dep\nfn id(x: Field) -> Field { x }\n",
        )
        .unwrap();

        let mut locked = HashMap::new();
        resolve_path_dep(project_root, "my_dep", Path::new("my_dep"), &mut locked).unwrap();

        assert_eq!(locked.len(), 1);
        assert!(locked["my_dep"].hash.len() == 64);
    }

    #[test]
    fn test_resolve_path_dep_missing() {
        let dir = tempfile::tempdir().unwrap();
        let mut locked = HashMap::new();
        let result = resolve_path_dep(
            dir.path(),
            "missing",
            Path::new("nonexistent/lib.tri"),
            &mut locked,
        );
        assert!(result.is_err());
    }

    // ── dependency_search_paths ────────────────────────────────

    #[test]
    fn test_dependency_search_paths() {
        let root = PathBuf::from("/project");
        let mut locked = HashMap::new();
        locked.insert(
            "a".to_string(),
            LockedDep {
                name: "a".to_string(),
                hash: "aaaa".to_string(),
                source: "hash".to_string(),
            },
        );
        locked.insert(
            "b".to_string(),
            LockedDep {
                name: "b".to_string(),
                hash: "bbbb".to_string(),
                source: "hash".to_string(),
            },
        );
        let lockfile = Lockfile { locked };

        let paths = dependency_search_paths(&root, &lockfile);
        assert_eq!(paths.len(), 2);

        let expected_a = PathBuf::from("/project/.trident/deps/aaaa");
        let expected_b = PathBuf::from("/project/.trident/deps/bbbb");
        assert!(paths.contains(&expected_a));
        assert!(paths.contains(&expected_b));
    }

    // ── cache_dependency ───────────────────────────────────────

    #[test]
    fn test_cache_dependency_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let project_root = dir.path();
        let hash = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
        let source = "module cached\nfn f() { }\n";

        cache_dependency(
            project_root,
            hash,
            source,
            "my_dep",
            "registry:http://example.com",
        )
        .unwrap();

        let cached_path = dep_source_path(project_root, hash);
        assert!(cached_path.exists());
        let cached_source = std::fs::read_to_string(&cached_path).unwrap();
        assert_eq!(cached_source, source);

        let meta_path = project_root
            .join(".trident")
            .join("deps")
            .join(hash)
            .join("meta.txt");
        assert!(meta_path.exists());
        let meta = std::fs::read_to_string(&meta_path).unwrap();
        assert!(meta.contains("name=my_dep"));
        assert!(meta.contains("source=registry:http://example.com"));
        assert!(meta.contains("fetched_at="));
    }

    // ── is_hex_hash ────────────────────────────────────────────

    #[test]
    fn test_is_hex_hash() {
        let valid = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        assert!(is_hex_hash(valid));

        assert!(!is_hex_hash("too_short"));
        assert!(!is_hex_hash(
            "gggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggg"
        ));
        assert!(!is_hex_hash(""));
    }

    // ── parse_inline_table ─────────────────────────────────────

    #[test]
    fn test_parse_inline_table() {
        let fields = parse_inline_table(r#"name = "math_lib", registry = "https://r.com""#);
        assert_eq!(fields.get("name").unwrap(), "math_lib");
        assert_eq!(fields.get("registry").unwrap(), "https://r.com");
    }

    // ── lockfile deterministic ordering ────────────────────────

    #[test]
    fn test_lockfile_sorted_output() {
        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().join("trident.lock");

        let mut locked = HashMap::new();
        locked.insert(
            "zebra".to_string(),
            LockedDep {
                name: "zebra".to_string(),
                hash: "1111111111111111111111111111111111111111111111111111111111111111"
                    .to_string(),
                source: "hash".to_string(),
            },
        );
        locked.insert(
            "alpha".to_string(),
            LockedDep {
                name: "alpha".to_string(),
                hash: "2222222222222222222222222222222222222222222222222222222222222222"
                    .to_string(),
                source: "hash".to_string(),
            },
        );
        let lockfile = Lockfile { locked };

        save_lockfile(&lock_path, &lockfile).unwrap();
        let content = std::fs::read_to_string(&lock_path).unwrap();

        // "alpha" should appear before "zebra" in the sorted output.
        let alpha_pos = content.find("alpha").unwrap();
        let zebra_pos = content.find("zebra").unwrap();
        assert!(
            alpha_pos < zebra_pos,
            "lockfile should be sorted alphabetically"
        );
    }
}
