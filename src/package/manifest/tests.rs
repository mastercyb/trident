use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::parse::{is_hex_hash, parse_inline_table};
use super::resolve::{cache_dependency, resolve_path_dep};
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

    let mut locked = BTreeMap::new();
    locked.insert(
        "crypto_utils".to_string(),
        LockedDep {
            name: "crypto_utils".to_string(),
            hash: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            source: "registry:https://registry.trident-lang.org".to_string(),
        },
    );
    locked.insert(
        "local_helper".to_string(),
        LockedDep {
            name: "local_helper".to_string(),
            hash: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
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

    let mut locked = BTreeMap::new();
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
    let mut locked2 = BTreeMap::new();
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

    let mut locked = BTreeMap::new();
    resolve_path_dep(project_root, "my_dep", Path::new("my_dep"), &mut locked).unwrap();

    assert_eq!(locked.len(), 1);
    assert!(locked["my_dep"].hash.len() == 64);
}

#[test]
fn test_resolve_path_dep_missing() {
    let dir = tempfile::tempdir().unwrap();
    let mut locked = BTreeMap::new();
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
    let mut locked = BTreeMap::new();
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

    let mut locked = BTreeMap::new();
    locked.insert(
        "zebra".to_string(),
        LockedDep {
            name: "zebra".to_string(),
            hash: "1111111111111111111111111111111111111111111111111111111111111111".to_string(),
            source: "hash".to_string(),
        },
    );
    locked.insert(
        "alpha".to_string(),
        LockedDep {
            name: "alpha".to_string(),
            hash: "2222222222222222222222222222222222222222222222222222222222222222".to_string(),
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
