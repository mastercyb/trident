use super::persist::{
    deserialize_definition, escape_newlines, serialize_definition,
    unescape_newlines,
};
use super::*;
use crate::hash::ContentHash;

fn parse_file(source: &str) -> ast::File {
    crate::parse_source_silent(source, "test.tri").unwrap()
}

#[test]
fn test_add_file_creates_definitions_and_names() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cb = Codebase::open_at(tmp.path()).unwrap();

    let file = parse_file(
        "program test\nfn helper(x: Field) -> Field { x + 1 }\nfn main() { pub_write(helper(pub_read())) }\n",
    );
    let result = cb.add_file(&file);

    assert_eq!(result.added, 2);
    assert_eq!(result.updated, 0);
    assert_eq!(result.unchanged, 0);

    // Both names should exist.
    assert!(cb.lookup("helper").is_some());
    assert!(cb.lookup("main").is_some());

    // Stats should reflect 2 definitions and 2 names.
    let stats = cb.stats();
    assert_eq!(stats.definitions, 2);
    assert_eq!(stats.names, 2);
    assert!(stats.total_source_bytes > 0);
}

#[test]
fn test_lookup_by_name() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cb = Codebase::open_at(tmp.path()).unwrap();

    let file =
        parse_file("program test\nfn add(a: Field, b: Field) -> Field { a + b }\nfn main() { }\n");
    cb.add_file(&file);

    let def = cb.lookup("add").unwrap();
    assert!(def.source.contains("fn add"));
    assert_eq!(def.params.len(), 2);
    assert_eq!(def.params[0].0, "a");
    assert_eq!(def.params[0].1, "Field");
    assert_eq!(def.return_ty.as_deref(), Some("Field"));
    assert!(!def.is_pub);
}

#[test]
fn test_rename() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cb = Codebase::open_at(tmp.path()).unwrap();

    let file = parse_file("program test\nfn old_name(x: Field) -> Field { x }\nfn main() { }\n");
    cb.add_file(&file);

    assert!(cb.lookup("old_name").is_some());
    cb.rename("old_name", "new_name").unwrap();

    assert!(cb.lookup("old_name").is_none());
    assert!(cb.lookup("new_name").is_some());

    // Both old and new name should appear in history.
    let history = cb.name_history("new_name");
    assert!(!history.is_empty());
}

#[test]
fn test_alias() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cb = Codebase::open_at(tmp.path()).unwrap();

    let file = parse_file("program test\nfn original(x: Field) -> Field { x }\nfn main() { }\n");
    cb.add_file(&file);

    cb.alias("original", "shortcut").unwrap();

    // Both names should resolve to the same hash.
    let hash_orig = cb.names.get("original").unwrap();
    let hash_alias = cb.names.get("shortcut").unwrap();
    assert_eq!(hash_orig, hash_alias);

    // Both names should appear in names_for_hash.
    let names = cb.names_for_hash(hash_orig);
    assert!(names.contains(&"original"));
    assert!(names.contains(&"shortcut"));
}

#[test]
fn test_add_updated_file_rebinds_name() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cb = Codebase::open_at(tmp.path()).unwrap();

    // First version.
    let file1 = parse_file("program test\nfn helper(x: Field) -> Field { x + 1 }\nfn main() { }\n");
    cb.add_file(&file1);
    let hash1 = *cb.names.get("helper").unwrap();

    // Updated version (different body).
    let file2 = parse_file("program test\nfn helper(x: Field) -> Field { x + 2 }\nfn main() { }\n");
    let result = cb.add_file(&file2);

    let hash2 = *cb.names.get("helper").unwrap();
    assert_ne!(hash1, hash2, "hash should change when body changes");
    assert!(result.updated >= 1);

    // History should have both entries.
    let history = cb.name_history("helper");
    assert!(history.len() >= 2);
}

#[test]
fn test_stats_empty() {
    let tmp = tempfile::tempdir().unwrap();
    let cb = Codebase::open_at(tmp.path()).unwrap();

    let stats = cb.stats();
    assert_eq!(stats.definitions, 0);
    assert_eq!(stats.names, 0);
    assert_eq!(stats.total_source_bytes, 0);
}

#[test]
fn test_empty_lookups_return_none() {
    let tmp = tempfile::tempdir().unwrap();
    let cb = Codebase::open_at(tmp.path()).unwrap();

    assert!(cb.lookup("nonexistent").is_none());
    assert!(cb.lookup_hash(&ContentHash::zero()).is_none());
    assert!(cb.view("nonexistent").is_none());
}

#[test]
fn test_persistence_save_and_reload() {
    let tmp = tempfile::tempdir().unwrap();

    // Create and populate a codebase.
    {
        let mut cb = Codebase::open_at(tmp.path()).unwrap();
        let file = parse_file(
            "program test\npub fn add(a: Field, b: Field) -> Field { a + b }\nfn main() { pub_write(add(pub_read(), pub_read())) }\n",
        );
        cb.add_file(&file);
        cb.save().unwrap();
    }

    // Reload from disk.
    {
        let cb = Codebase::open_at(tmp.path()).unwrap();
        assert_eq!(cb.stats().definitions, 2);
        assert_eq!(cb.stats().names, 2);

        let def = cb.lookup("add").unwrap();
        assert!(def.source.contains("fn add"));
        assert_eq!(def.module, "test");
        assert!(def.is_pub);
        assert_eq!(def.params.len(), 2);
        assert_eq!(def.return_ty.as_deref(), Some("Field"));
    }
}

#[test]
fn test_dependencies_extracted() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cb = Codebase::open_at(tmp.path()).unwrap();

    let file = parse_file(
        "program test\nfn helper(x: Field) -> Field { x + 1 }\nfn main() { pub_write(helper(pub_read())) }\n",
    );
    cb.add_file(&file);

    let main_hash = cb.names.get("main").unwrap();
    let main_def = cb.lookup_hash(main_hash).unwrap();

    // main should depend on helper.
    let helper_hash = cb.names.get("helper").unwrap();
    assert!(
        main_def.dependencies.contains(helper_hash),
        "main should depend on helper"
    );

    // helper should have no dependencies (only calls builtins).
    let helper_def = cb.lookup_hash(helper_hash).unwrap();
    assert!(
        helper_def.dependencies.is_empty(),
        "helper should have no function deps"
    );
}

#[test]
fn test_dependents() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cb = Codebase::open_at(tmp.path()).unwrap();

    let file = parse_file(
        "program test\nfn helper(x: Field) -> Field { x + 1 }\nfn main() { pub_write(helper(pub_read())) }\n",
    );
    cb.add_file(&file);

    let helper_hash = cb.names.get("helper").unwrap();
    let dependents = cb.dependents(helper_hash);
    assert!(
        dependents.iter().any(|(name, _)| *name == "main"),
        "main should be a dependent of helper"
    );
}

#[test]
fn test_view_output() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cb = Codebase::open_at(tmp.path()).unwrap();

    let file = parse_file("program test\nfn id(x: Field) -> Field { x }\nfn main() { }\n");
    cb.add_file(&file);

    let view = cb.view("id").unwrap();
    assert!(view.contains("-- id #"));
    assert!(view.contains("fn id"));
}

#[test]
fn test_rename_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cb = Codebase::open_at(tmp.path()).unwrap();

    let file = parse_file("program test\nfn a() { }\nfn b() { }\nfn main() { }\n");
    cb.add_file(&file);

    // Rename nonexistent.
    assert!(cb.rename("nonexistent", "c").is_err());

    // Rename to existing name.
    assert!(cb.rename("a", "b").is_err());
}

#[test]
fn test_alias_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cb = Codebase::open_at(tmp.path()).unwrap();

    let file = parse_file("program test\nfn a() { }\nfn b() { }\nfn main() { }\n");
    cb.add_file(&file);

    // Alias nonexistent.
    assert!(cb.alias("nonexistent", "c").is_err());

    // Alias to existing name.
    assert!(cb.alias("a", "b").is_err());
}

#[test]
fn test_escape_unescape_roundtrip() {
    let original = "fn main() {\n    pub_write(pub_read())\n}";
    let escaped = escape_newlines(original);
    assert!(!escaped.contains('\n'));
    let unescaped = unescape_newlines(&escaped);
    assert_eq!(unescaped, original);
}

#[test]
fn test_escape_backslashes() {
    let original = "line1\\nstill_line1\nline2";
    let escaped = escape_newlines(original);
    let unescaped = unescape_newlines(&escaped);
    assert_eq!(unescaped, original);
}

#[test]
fn test_parse_hex_hash() {
    let hash = ContentHash([0xAB; 32]);
    let hex = hash.to_hex();
    let parsed = ContentHash::from_hex(&hex).unwrap();
    assert_eq!(parsed, hash);

    // Invalid: too short.
    assert!(ContentHash::from_hex("abcd").is_none());
    // Invalid: wrong chars.
    assert!(ContentHash::from_hex(&"zz".repeat(32)).is_none());
}

#[test]
fn test_definition_serialization_roundtrip() {
    let def = Definition {
        source: "fn add(a: Field, b: Field) -> Field {\n    a + b\n}".to_string(),
        module: "test".to_string(),
        is_pub: true,
        params: vec![
            ("a".to_string(), "Field".to_string()),
            ("b".to_string(), "Field".to_string()),
        ],
        return_ty: Some("Field".to_string()),
        dependencies: vec![ContentHash([0x01; 32])],
        requires: vec!["a > 0".to_string()],
        ensures: vec!["result == a + b".to_string()],
        first_seen: 1707580000,
    };

    let serialized = serialize_definition(&def);
    let deserialized = deserialize_definition(&serialized).unwrap();

    assert_eq!(deserialized.source, def.source);
    assert_eq!(deserialized.module, def.module);
    assert_eq!(deserialized.is_pub, def.is_pub);
    assert_eq!(deserialized.params, def.params);
    assert_eq!(deserialized.return_ty, def.return_ty);
    assert_eq!(deserialized.dependencies.len(), 1);
    assert_eq!(deserialized.dependencies[0], ContentHash([0x01; 32]));
    assert_eq!(deserialized.requires, def.requires);
    assert_eq!(deserialized.ensures, def.ensures);
    assert_eq!(deserialized.first_seen, def.first_seen);
}

#[test]
fn test_lookup_by_prefix() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cb = Codebase::open_at(tmp.path()).unwrap();

    let file = parse_file("program test\nfn id(x: Field) -> Field { x }\nfn main() { }\n");
    cb.add_file(&file);

    let hash = *cb.names.get("id").unwrap();
    let hex = hash.to_hex();

    // Lookup by hex prefix.
    let (found_hash, _) = cb.lookup_by_prefix(&hex[..8]).unwrap();
    assert_eq!(*found_hash, hash);

    // Lookup by short hash.
    let short = hash.to_short();
    let (found_hash, _) = cb.lookup_by_prefix(&short).unwrap();
    assert_eq!(*found_hash, hash);
}

#[test]
fn test_unchanged_on_readd() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cb = Codebase::open_at(tmp.path()).unwrap();

    let file = parse_file("program test\nfn id(x: Field) -> Field { x }\nfn main() { }\n");
    cb.add_file(&file);

    // Adding the same file again should show unchanged.
    let result = cb.add_file(&file);
    assert_eq!(result.added, 0);
    assert_eq!(result.updated, 0);
    assert_eq!(result.unchanged, 2);
}

#[test]
fn test_list_names_sorted() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cb = Codebase::open_at(tmp.path()).unwrap();

    let file = parse_file("program test\nfn zzz() { }\nfn aaa() { }\nfn main() { }\n");
    cb.add_file(&file);

    let names: Vec<&str> = cb.list_names().iter().map(|(n, _)| *n).collect();
    assert_eq!(names, vec!["aaa", "main", "zzz"]);
}

#[test]
fn test_pub_function_stored() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cb = Codebase::open_at(tmp.path()).unwrap();

    let file = parse_file("program test\npub fn visible(x: Field) -> Field { x }\nfn main() { }\n");
    cb.add_file(&file);

    let def = cb.lookup("visible").unwrap();
    assert!(def.is_pub);
}

#[test]
fn test_spec_annotations_stored() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cb = Codebase::open_at(tmp.path()).unwrap();

    let file = parse_file(
        "program test\n#[requires(a > 0)]\n#[ensures(result == a + 1)]\nfn inc(a: Field) -> Field { a + 1 }\nfn main() { }\n",
    );
    cb.add_file(&file);

    let def = cb.lookup("inc").unwrap();
    assert_eq!(def.requires, vec!["a > 0"]);
    assert_eq!(def.ensures, vec!["result == a + 1"]);
}
