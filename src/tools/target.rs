use std::path::Path;

use crate::diagnostic::Diagnostic;
use crate::span::Span;

/// VM architecture family.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Arch {
    /// Stack machine (Triton VM, Miden VM): direct emission, no IR.
    Stack,
    /// Register machine (Cairo, RISC-V zkVMs): requires lightweight IR.
    Register,
    /// Tree machine (Nock): combinator-based, subject-formula evaluation.
    Tree,
}

/// Describes a non-native field the target can emulate.
#[derive(Clone, Debug)]
pub struct EmulatedField {
    /// Short identifier (e.g. "bn254", "stark252").
    pub name: String,
    /// Field size in bits.
    pub bits: u32,
    /// Number of native field elements per emulated element.
    pub limbs: u32,
}

/// Target VM configuration — replaces all hardcoded constants.
///
/// Every numeric constant that was previously hardcoded for Triton VM
/// (stack depth 16, digest width 5, hash rate 10, etc.) now lives here.
#[derive(Clone, Debug)]
pub struct TargetConfig {
    /// Short identifier used in CLI and file paths (e.g. "triton").
    pub name: String,
    /// Human-readable name (e.g. "Triton VM").
    pub display_name: String,
    /// Architecture family.
    pub architecture: Arch,
    /// Field prime description (informational, e.g. "2^64 - 2^32 + 1").
    pub field_prime: String,
    /// Native field size in bits (e.g. 64 for Goldilocks, 31 for Mersenne31).
    pub field_bits: u32,
    /// Number of U32 limbs when splitting a field element.
    pub field_limbs: u32,
    /// Non-native fields this target can emulate (empty = native only).
    pub emulated_fields: Vec<EmulatedField>,
    /// Maximum operand stack depth before spilling to RAM.
    pub stack_depth: u32,
    /// Base RAM address for spilled variables.
    pub spill_ram_base: u64,
    /// Width of a hash digest in field elements.
    pub digest_width: u32,
    /// Degree of the extension field (0 if no extension field support).
    pub xfield_width: u32,
    /// Hash function absorption rate in field elements.
    pub hash_rate: u32,
    /// File extension for compiled output (e.g. ".tasm").
    pub output_extension: String,
    /// Names of the cost model tables (e.g. ["processor", "hash", ...]).
    pub cost_tables: Vec<String>,
}

impl TargetConfig {
    /// Built-in Triton VM configuration (hardcoded fallback).
    pub fn triton() -> Self {
        Self {
            name: "triton".to_string(),
            display_name: "Triton VM".to_string(),
            architecture: Arch::Stack,
            field_prime: "2^64 - 2^32 + 1".to_string(),
            field_bits: 64,
            field_limbs: 2,
            emulated_fields: Vec::new(),
            stack_depth: 16,
            spill_ram_base: 1 << 30,
            digest_width: 5,
            xfield_width: 3,
            hash_rate: 10,
            output_extension: ".tasm".to_string(),
            cost_tables: vec![
                "processor".to_string(),
                "hash".to_string(),
                "u32".to_string(),
                "op_stack".to_string(),
                "ram".to_string(),
                "jump_stack".to_string(),
            ],
        }
    }

    /// Load a target configuration from a TOML file.
    pub fn load(path: &Path) -> Result<Self, Diagnostic> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            Diagnostic::error(
                format!("cannot read target config '{}': {}", path.display(), e),
                Span::dummy(),
            )
        })?;
        Self::parse_toml(&content, path)
    }

    /// Resolve a target by name: look for `vm/{name}.toml` relative to
    /// the compiler binary or working directory, falling back to built-in configs.
    pub fn resolve(name: &str) -> Result<Self, Diagnostic> {
        // Reject path traversal
        if name.contains('/') || name.contains('\\') || name.contains("..") || name.starts_with('.')
        {
            return Err(Diagnostic::error(
                format!("invalid target name '{}'", name),
                Span::dummy(),
            ));
        }

        // Built-in target
        if name == "triton" {
            return Ok(Self::triton());
        }

        // Search for vm/{name}/target.toml first, then vm/{name}.toml (legacy)
        let primary = format!("vm/{}/target.toml", name);
        let fallback = format!("vm/{}.toml", name);

        // 1. Relative to compiler binary
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                for ancestor in &[
                    Some(dir.to_path_buf()),
                    dir.parent().map(|p| p.to_path_buf()),
                    dir.parent()
                        .and_then(|p| p.parent())
                        .map(|p| p.to_path_buf()),
                ] {
                    if let Some(base) = ancestor {
                        let path = base.join(&primary);
                        if path.exists() {
                            return Self::load(&path);
                        }
                        let path = base.join(&fallback);
                        if path.exists() {
                            return Self::load(&path);
                        }
                    }
                }
            }
        }

        // 2. Current working directory
        let cwd_path = std::path::PathBuf::from(&primary);
        if cwd_path.exists() {
            return Self::load(&cwd_path);
        }
        let cwd_path = std::path::PathBuf::from(&fallback);
        if cwd_path.exists() {
            return Self::load(&cwd_path);
        }

        Err(Diagnostic::error(
            format!("unknown target '{}' (looked for '{}')", name, primary),
            Span::dummy(),
        )
        .with_help("available targets: triton, miden, openvm, sp1, cairo, nock".to_string()))
    }

    fn parse_toml(content: &str, path: &Path) -> Result<Self, Diagnostic> {
        let err =
            |msg: String| Diagnostic::error(format!("{}: {}", path.display(), msg), Span::dummy());

        let mut name = String::new();
        let mut display_name = String::new();
        let mut architecture = String::new();
        let mut output_extension = String::new();
        let mut field_prime = String::new();
        let mut field_bits: u32 = 0;
        let mut field_limbs: u32 = 0;
        let mut emulated_fields: Vec<EmulatedField> = Vec::new();
        let mut stack_depth: u32 = 0;
        let mut spill_ram_base: u64 = 0;
        let mut digest_width: u32 = 0;
        let mut hash_rate: u32 = 0;
        let mut xfield_degree: u32 = 0;
        let mut cost_tables: Vec<String> = Vec::new();

        let mut section = String::new();

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                section = trimmed[1..trimmed.len() - 1].trim().to_string();
                continue;
            }
            if let Some((key, value)) = trimmed.split_once('=') {
                let key = key.trim();
                let value = value.trim();
                let unquoted = value.trim_matches('"');

                match (section.as_str(), key) {
                    ("target", "name") => name = unquoted.to_string(),
                    ("target", "display_name") => display_name = unquoted.to_string(),
                    ("target", "architecture") => architecture = unquoted.to_string(),
                    ("target", "output_extension") => output_extension = unquoted.to_string(),
                    ("field", "prime") => field_prime = unquoted.to_string(),
                    ("field", "bits") => {
                        field_bits = value
                            .parse()
                            .map_err(|_| err(format!("invalid field.bits: {}", value)))?;
                    }
                    ("field", "limbs") => {
                        field_limbs = value
                            .parse()
                            .map_err(|_| err(format!("invalid field.limbs: {}", value)))?;
                    }
                    ("stack", "depth") => {
                        stack_depth = value
                            .parse()
                            .map_err(|_| err(format!("invalid stack.depth: {}", value)))?;
                    }
                    ("stack", "spill_ram_base") => {
                        spill_ram_base = value
                            .parse()
                            .map_err(|_| err(format!("invalid stack.spill_ram_base: {}", value)))?;
                    }
                    ("hash", "digest_width") => {
                        digest_width = value
                            .parse()
                            .map_err(|_| err(format!("invalid hash.digest_width: {}", value)))?;
                    }
                    ("hash", "rate") => {
                        hash_rate = value
                            .parse()
                            .map_err(|_| err(format!("invalid hash.rate: {}", value)))?;
                    }
                    ("extension_field", "degree") => {
                        xfield_degree = value.parse().map_err(|_| {
                            err(format!("invalid extension_field.degree: {}", value))
                        })?;
                    }
                    ("cost", "tables") => {
                        cost_tables = parse_string_array(value);
                    }
                    _ => {
                        // Parse [emulated_field.NAME] sections
                        if section.starts_with("emulated_field.") {
                            let ef_name = section.strip_prefix("emulated_field.").unwrap();
                            // Find or create the entry
                            let entry = emulated_fields.iter_mut().find(|ef| ef.name == ef_name);
                            let entry = if let Some(e) = entry {
                                e
                            } else {
                                emulated_fields.push(EmulatedField {
                                    name: ef_name.to_string(),
                                    bits: 0,
                                    limbs: 0,
                                });
                                emulated_fields.last_mut().unwrap()
                            };
                            match key {
                                "bits" => {
                                    entry.bits = value.parse().map_err(|_| {
                                        err(format!(
                                            "invalid emulated_field.{}.bits: {}",
                                            ef_name, value
                                        ))
                                    })?;
                                }
                                "limbs" => {
                                    entry.limbs = value.parse().map_err(|_| {
                                        err(format!(
                                            "invalid emulated_field.{}.limbs: {}",
                                            ef_name, value
                                        ))
                                    })?;
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }

        if name.is_empty() {
            return Err(err("missing target.name".to_string()));
        }
        if stack_depth == 0 {
            return Err(err("stack.depth must be > 0".to_string()));
        }
        if digest_width == 0 {
            return Err(err("hash.digest_width must be > 0".to_string()));
        }
        if hash_rate == 0 {
            return Err(err("hash.rate must be > 0".to_string()));
        }
        if field_bits == 0 {
            return Err(err("field.bits must be > 0".to_string()));
        }
        if field_limbs == 0 {
            return Err(err("field.limbs must be > 0".to_string()));
        }

        let arch = match architecture.as_str() {
            "stack" => Arch::Stack,
            "register" => Arch::Register,
            "tree" => Arch::Tree,
            other => {
                return Err(err(format!(
                    "unknown architecture '{}' (expected 'stack', 'register', or 'tree')",
                    other
                )))
            }
        };

        Ok(Self {
            name,
            display_name,
            architecture: arch,
            field_prime,
            field_bits,
            field_limbs,
            emulated_fields,
            stack_depth,
            spill_ram_base,
            digest_width,
            xfield_width: xfield_degree,
            hash_rate,
            output_extension,
            cost_tables,
        })
    }
}

// ─── OS Target Configuration ───────────────────────────────────────

/// OS target configuration parsed from `os/<name>/target.toml`.
///
/// An OS target describes a blockchain or runtime environment that
/// runs on top of a VM. The `vm` field maps the OS to its underlying
/// VM (e.g. "neptune" → "triton", "starknet" → "cairo").
#[derive(Clone, Debug)]
pub struct OsConfig {
    /// OS name (e.g. "neptune").
    pub name: String,
    /// Display name (e.g. "Neptune").
    pub display_name: String,
    /// Underlying VM name (e.g. "triton").
    pub vm: String,
    /// Runtime binding prefix (e.g. "os.neptune").
    pub binding_prefix: String,
    /// Account model (e.g. "utxo", "account").
    pub account_model: String,
    /// Storage model (e.g. "merkle-authenticated", "key-value").
    pub storage_model: String,
    /// Transaction model (e.g. "proof-based", "signed").
    pub transaction_model: String,
}

impl OsConfig {
    /// Try to resolve an OS config by name.
    ///
    /// Searches for `os/<name>/target.toml` relative to the compiler
    /// binary and the current working directory.
    /// Returns `Ok(None)` if no OS config file exists for this name.
    /// Returns `Err` if the file exists but is malformed.
    pub fn resolve(name: &str) -> Result<Option<Self>, Diagnostic> {
        // Reject path traversal
        if name.contains('/') || name.contains('\\') || name.contains("..") || name.starts_with('.')
        {
            return Ok(None);
        }

        let target_path = format!("os/{}/target.toml", name);

        // 1. Relative to compiler binary
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                for ancestor in &[
                    Some(dir.to_path_buf()),
                    dir.parent().map(|p| p.to_path_buf()),
                    dir.parent()
                        .and_then(|p| p.parent())
                        .map(|p| p.to_path_buf()),
                ] {
                    if let Some(base) = ancestor {
                        let path = base.join(&target_path);
                        if path.exists() {
                            return Self::load(&path).map(Some);
                        }
                    }
                }
            }
        }

        // 2. Current working directory
        let cwd_path = std::path::PathBuf::from(&target_path);
        if cwd_path.exists() {
            return Self::load(&cwd_path).map(Some);
        }

        Ok(None)
    }

    /// Load an OS config from a TOML file.
    pub fn load(path: &Path) -> Result<Self, Diagnostic> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            Diagnostic::error(
                format!("cannot read OS config '{}': {}", path.display(), e),
                Span::dummy(),
            )
        })?;
        Self::parse_toml(&content, path)
    }

    fn parse_toml(content: &str, path: &Path) -> Result<Self, Diagnostic> {
        let err =
            |msg: String| Diagnostic::error(format!("{}: {}", path.display(), msg), Span::dummy());

        let mut name = String::new();
        let mut display_name = String::new();
        let mut vm = String::new();
        let mut binding_prefix = String::new();
        let mut account_model = String::new();
        let mut storage_model = String::new();
        let mut transaction_model = String::new();

        let mut section = String::new();

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                section = trimmed[1..trimmed.len() - 1].trim().to_string();
                continue;
            }
            if let Some((key, value)) = trimmed.split_once('=') {
                let key = key.trim();
                let unquoted = value.trim().trim_matches('"');

                match (section.as_str(), key) {
                    ("os", "name") => name = unquoted.to_string(),
                    ("os", "display_name") => display_name = unquoted.to_string(),
                    ("os", "vm") => vm = unquoted.to_string(),
                    ("runtime", "binding_prefix") => binding_prefix = unquoted.to_string(),
                    ("runtime", "account_model") => account_model = unquoted.to_string(),
                    ("runtime", "storage_model") => storage_model = unquoted.to_string(),
                    ("runtime", "transaction_model") => transaction_model = unquoted.to_string(),
                    _ => {}
                }
            }
        }

        if name.is_empty() {
            return Err(err("missing os.name".to_string()));
        }
        if vm.is_empty() {
            return Err(err("missing os.vm".to_string()));
        }

        Ok(Self {
            name,
            display_name,
            vm,
            binding_prefix,
            account_model,
            storage_model,
            transaction_model,
        })
    }
}

// ─── Combined Target Resolution ────────────────────────────────────

/// Resolved target: either a bare VM or an OS+VM combination.
///
/// When the user passes `--target neptune`, we load the OS config first
/// (which tells us the VM is "triton"), then load the VM config. When
/// they pass `--target triton`, we load the VM config directly.
#[derive(Clone, Debug)]
pub struct ResolvedTarget {
    /// VM configuration (always present).
    pub vm: TargetConfig,
    /// OS configuration (present only if the target name was an OS).
    pub os: Option<OsConfig>,
}

impl ResolvedTarget {
    /// Resolve a target name: try OS first, then fall back to VM.
    ///
    /// This matches the resolution order from `docs/reference/targets.md`:
    /// 1. Is `<name>` an OS? Load `os/<name>/target.toml`, derive VM.
    /// 2. Is `<name>` a VM? Load `vm/<name>/target.toml`.
    /// 3. Neither? Error.
    pub fn resolve(name: &str) -> Result<Self, Diagnostic> {
        // 1. Try OS
        if let Some(os_config) = OsConfig::resolve(name)? {
            let vm = TargetConfig::resolve(&os_config.vm)?;
            return Ok(ResolvedTarget {
                vm,
                os: Some(os_config),
            });
        }

        // 2. Try VM
        let vm = TargetConfig::resolve(name)?;
        Ok(ResolvedTarget { vm, os: None })
    }
}

/// Parse a minimal TOML string array: `["a", "b", "c"]` → `vec!["a", "b", "c"]`.
fn parse_string_array(s: &str) -> Vec<String> {
    let s = s.trim();
    if !s.starts_with('[') || !s.ends_with(']') {
        return Vec::new();
    }
    let inner = &s[1..s.len() - 1];
    inner
        .split(',')
        .map(|part| part.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_triton_defaults() {
        let config = TargetConfig::triton();
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
        let config = TargetConfig::resolve("triton").unwrap();
        assert_eq!(config.name, "triton");
        assert_eq!(config.digest_width, 5);
    }

    #[test]
    fn test_resolve_rejects_path_traversal() {
        assert!(TargetConfig::resolve("../etc/passwd").is_err());
        assert!(TargetConfig::resolve("./sneaky").is_err());
        assert!(TargetConfig::resolve("foo/bar").is_err());
        assert!(TargetConfig::resolve(".hidden").is_err());
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

        let config = TargetConfig::load(&path).unwrap();
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

        let config = TargetConfig::load(&path).unwrap();
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
        let result = TargetConfig::resolve("nonexistent_vm");
        assert!(result.is_err());
    }

    // ── OsConfig ───────────────────────────────────────────────

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

        let config = OsConfig::load(&path).unwrap();
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

        assert!(OsConfig::load(&path).is_err());
    }

    #[test]
    fn test_os_config_resolve_nonexistent() {
        let result = OsConfig::resolve("definitely_not_an_os").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_os_config_resolve_rejects_traversal() {
        let result = OsConfig::resolve("../etc/passwd").unwrap();
        assert!(result.is_none());
    }

    // ── ResolvedTarget ─────────────────────────────────────────

    #[test]
    fn test_resolved_target_vm_only() {
        let resolved = ResolvedTarget::resolve("triton").unwrap();
        assert_eq!(resolved.vm.name, "triton");
        assert!(resolved.os.is_none());
    }
}
