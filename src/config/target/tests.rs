use super::*;

#[test]
fn test_triton_defaults() {
    let config = TerrainConfig::triton();
    assert_eq!(config.name, "triton");
    assert_eq!(config.architecture, Arch::Stack);
    assert_eq!(config.field_bits, 64);
    assert_eq!(config.field_limbs, 2);
    assert!(config.emulated_fields.is_empty());
    assert_eq!(config.stack_depth, 16);
    assert_eq!(config.spill_ram_base, 1 << 30);
    assert_eq!(config.digest_width, 5);
    assert_eq!(config.xfield_width, 3);
    assert_eq!(config.hash_rate, 10);
    assert_eq!(config.output_extension, ".tasm");
    assert_eq!(config.cost_tables.len(), 6);
}

#[test]
fn test_resolve_triton() {
    let config = TerrainConfig::resolve("triton").unwrap();
    assert_eq!(config.name, "triton");
    assert_eq!(config.digest_width, 5);
}

#[test]
fn test_resolve_rejects_path_traversal() {
    assert!(TerrainConfig::resolve("../etc/passwd").is_err());
    assert!(TerrainConfig::resolve("./sneaky").is_err());
    assert!(TerrainConfig::resolve("foo/bar").is_err());
    assert!(TerrainConfig::resolve(".hidden").is_err());
}

#[test]
fn test_load_from_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.toml");
    std::fs::write(
        &path,
        r#"
[target]
name = "test_vm"
display_name = "Test VM"
architecture = "register"
output_extension = ".test"

[field]
prime = "p"
bits = 32
limbs = 4

[stack]
depth = 32
spill_ram_base = 0

[hash]
digest_width = 8
rate = 3

[extension_field]
degree = 0

[cost]
tables = ["cycles"]
"#,
    )
    .unwrap();

    let config = TerrainConfig::load(&path).unwrap();
    assert_eq!(config.name, "test_vm");
    assert_eq!(config.architecture, Arch::Register);
    assert_eq!(config.field_bits, 32);
    assert_eq!(config.field_limbs, 4);
    assert!(config.emulated_fields.is_empty());
    assert_eq!(config.stack_depth, 32);
    assert_eq!(config.digest_width, 8);
    assert_eq!(config.hash_rate, 3);
    assert_eq!(config.xfield_width, 0);
    assert_eq!(config.cost_tables, vec!["cycles"]);
}

#[test]
fn test_emulated_field_parsing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("emu.toml");
    std::fs::write(
        &path,
        r#"
[target]
name = "emu_vm"
display_name = "Emu VM"
architecture = "stack"
output_extension = ".asm"

[field]
prime = "2^64 - 2^32 + 1"
bits = 64
limbs = 2

[stack]
depth = 16
spill_ram_base = 1073741824

[hash]
digest_width = 5
rate = 10

[extension_field]
degree = 3

[cost]
tables = ["processor"]

[emulated_field.bn254]
bits = 254
limbs = 4

[emulated_field.stark252]
bits = 251
limbs = 4
"#,
    )
    .unwrap();

    let config = TerrainConfig::load(&path).unwrap();
    assert_eq!(config.field_bits, 64);
    assert_eq!(config.emulated_fields.len(), 2);

    let bn254 = config
        .emulated_fields
        .iter()
        .find(|ef| ef.name == "bn254")
        .unwrap();
    assert_eq!(bn254.bits, 254);
    assert_eq!(bn254.limbs, 4);

    let stark252 = config
        .emulated_fields
        .iter()
        .find(|ef| ef.name == "stark252")
        .unwrap();
    assert_eq!(stark252.bits, 251);
    assert_eq!(stark252.limbs, 4);
}

#[test]
fn test_resolve_unknown_target() {
    let result = TerrainConfig::resolve("nonexistent_vm");
    assert!(result.is_err());
}

// ── UnionConfig ───────────────────────────────────────────────

#[test]
fn test_os_config_parse_toml() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("target.toml");
    std::fs::write(
        &path,
        r#"
[os]
name = "test_os"
display_name = "Test OS"
vm = "triton"

[runtime]
binding_prefix = "os.test_os"
account_model = "utxo"
storage_model = "merkle-authenticated"
transaction_model = "proof-based"
"#,
    )
    .unwrap();

    let config = UnionConfig::load(&path).unwrap();
    assert_eq!(config.name, "test_os");
    assert_eq!(config.display_name, "Test OS");
    assert_eq!(config.vm, "triton");
    assert_eq!(config.binding_prefix, "os.test_os");
    assert_eq!(config.account_model, "utxo");
    assert_eq!(config.storage_model, "merkle-authenticated");
    assert_eq!(config.transaction_model, "proof-based");
}

#[test]
fn test_os_config_missing_vm() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("target.toml");
    std::fs::write(
        &path,
        r#"
[os]
name = "broken"
display_name = "Broken"
"#,
    )
    .unwrap();

    assert!(UnionConfig::load(&path).is_err());
}

#[test]
fn test_os_config_resolve_nonexistent() {
    let result = UnionConfig::resolve("definitely_not_an_os").unwrap();
    assert!(result.is_none());
}

#[test]
fn test_os_config_resolve_rejects_traversal() {
    let result = UnionConfig::resolve("../etc/passwd").unwrap();
    assert!(result.is_none());
}

// ── ResolvedTarget ─────────────────────────────────────────

#[test]
fn test_resolved_target_vm_only() {
    let resolved = ResolvedTarget::resolve("triton").unwrap();
    assert_eq!(resolved.vm.name, "triton");
    assert!(resolved.os.is_none());
    assert!(resolved.state.is_none());
}

// ── StateConfig ────────────────────────────────────────────

#[test]
fn test_state_config_parse_toml() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mainnet.toml");
    std::fs::write(
        &path,
        r#"
[state]
name = "mainnet"
display_name = "Neptune Mainnet"
union = "neptune"
chain_id = "1"
is_default = true

[endpoints]
rpc_url = "https://rpc.neptune.cash"
explorer_url = "https://explorer.neptune.cash"

[currency]
symbol = "NEPT"
"#,
    )
    .unwrap();

    let config = StateConfig::load(&path).unwrap();
    assert_eq!(config.name, "mainnet");
    assert_eq!(config.display_name, "Neptune Mainnet");
    assert_eq!(config.union, "neptune");
    assert_eq!(config.chain_id, "1");
    assert!(config.is_default);
    assert_eq!(config.rpc_url, "https://rpc.neptune.cash");
    assert_eq!(config.explorer_url, "https://explorer.neptune.cash");
    assert_eq!(config.currency_symbol, "NEPT");
}

#[test]
fn test_state_config_missing_name() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.toml");
    std::fs::write(
        &path,
        r#"
[state]
union = "neptune"
"#,
    )
    .unwrap();

    assert!(StateConfig::load(&path).is_err());
}

#[test]
fn test_state_config_missing_union() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.toml");
    std::fs::write(
        &path,
        r#"
[state]
name = "orphan"
"#,
    )
    .unwrap();

    assert!(StateConfig::load(&path).is_err());
}

#[test]
fn test_state_config_resolve_rejects_traversal() {
    let result = StateConfig::resolve("../etc", "passwd").unwrap();
    assert!(result.is_none());
    let result = StateConfig::resolve("neptune", "../etc/passwd").unwrap();
    assert!(result.is_none());
}

#[test]
fn test_state_requires_union_target() {
    // Bare terrain (VM) + state should fail
    let result = ResolvedTarget::resolve_with_state("triton", Some("mainnet"));
    assert!(result.is_err());
}

#[test]
fn test_state_config_optional_fields() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("minimal.toml");
    std::fs::write(
        &path,
        r#"
[state]
name = "devnet"
union = "neptune"
"#,
    )
    .unwrap();

    let config = StateConfig::load(&path).unwrap();
    assert_eq!(config.name, "devnet");
    assert_eq!(config.union, "neptune");
    assert!(!config.is_default);
    assert!(config.chain_id.is_empty());
    assert!(config.rpc_url.is_empty());
    assert!(config.currency_symbol.is_empty());
}
