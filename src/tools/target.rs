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

        // Search for vm/{name}.toml
        let filename = format!("vm/{}.toml", name);

        // 1. Relative to compiler binary
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let path = dir.join(&filename);
                if path.exists() {
                    return Self::load(&path);
                }
                // One level up (target/debug/../vm/)
                if let Some(parent) = dir.parent() {
                    let path = parent.join(&filename);
                    if path.exists() {
                        return Self::load(&path);
                    }
                    if let Some(grandparent) = parent.parent() {
                        let path = grandparent.join(&filename);
                        if path.exists() {
                            return Self::load(&path);
                        }
                    }
                }
            }
        }

        // 2. Current working directory
        let cwd_path = std::path::PathBuf::from(&filename);
        if cwd_path.exists() {
            return Self::load(&cwd_path);
        }

        Err(Diagnostic::error(
            format!("unknown target '{}' (looked for '{}')", name, filename),
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
}
