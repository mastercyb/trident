//! Packaging: produce a self-contained artifact for Trident programs.
//!
//! `trident package` creates a `.deploy/` directory containing the compiled
//! TASM and a `manifest.json` with metadata:
//! - `program_digest` — Poseidon2 hash of compiled TASM (what verifiers check)
//! - `source_hash` — content hash of the source AST
//! - target info (VM + optional OS)
//! - cost analysis
//! - function signatures with per-function content hashes
//!
//! The packaged artifact can then be deployed via `trident deploy`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::ast;
use crate::cost::ProgramCost;
use crate::hash::ContentHash;
use crate::target::{Arch, OsConfig, TargetConfig};

// ─── Data Types ────────────────────────────────────────────────────

/// Package manifest — all metadata about a packaged program artifact.
#[derive(Clone, Debug)]
pub struct PackageManifest {
    pub name: String,
    pub version: String,
    /// Poseidon2 hash of the compiled TASM bytes (hex).
    pub program_digest: String,
    /// Content hash of the source AST (hex).
    pub source_hash: String,
    pub target_vm: String,
    pub target_os: Option<String>,
    pub architecture: String,
    pub cost: ManifestCost,
    pub functions: Vec<ManifestFunction>,
    pub entry_point: String,
    /// ISO 8601 timestamp.
    pub built_at: String,
    pub compiler_version: String,
}

#[derive(Clone, Debug)]
pub struct ManifestCost {
    /// Cost values per table, indexed by position.
    pub table_values: Vec<u64>,
    /// Table names for serialization (e.g. ["processor", "hash", "u32", ...]).
    pub table_names: Vec<String>,
    pub padded_height: u64,
}

#[derive(Clone, Debug)]
pub struct ManifestFunction {
    pub name: String,
    /// Content hash (hex).
    pub hash: String,
    /// Signature string (e.g. "fn pay(from: Digest, amount: Field)").
    pub signature: String,
}

/// Result of a package operation.
pub struct PackageResult {
    pub manifest: PackageManifest,
    pub artifact_dir: PathBuf,
    pub tasm_path: PathBuf,
    pub manifest_path: PathBuf,
}

// ─── Artifact Generation ───────────────────────────────────────────

/// Generate a package artifact from a compiled project.
///
/// Creates a `<name>.deploy/` directory under `output_base` containing
/// `program.tasm` and `manifest.json`.
pub fn generate_artifact(
    name: &str,
    version: &str,
    tasm: &str,
    source_file: &ast::File,
    cost: &ProgramCost,
    target_vm: &TargetConfig,
    target_os: Option<&OsConfig>,
    output_base: &Path,
) -> Result<PackageResult, String> {
    // 1. Compute program_digest = Poseidon2(tasm bytes)
    let digest_bytes = crate::poseidon2::hash_bytes(tasm.as_bytes());
    let program_digest = ContentHash(digest_bytes);

    // 2. Compute source_hash from AST
    let source_hash = crate::hash::hash_file_content(source_file);

    // 3. Extract function signatures + per-function hashes
    let fn_hashes = crate::hash::hash_file(source_file);
    let functions = extract_functions(source_file, &fn_hashes);

    // 4. Determine entry point
    let entry_point = find_entry_point(source_file);

    // 5. Architecture string
    let architecture = match target_vm.architecture {
        Arch::Stack => "stack",
        Arch::Register => "register",
        Arch::Tree => "tree",
    }
    .to_string();

    // 6. Build manifest
    let manifest = PackageManifest {
        name: name.to_string(),
        version: version.to_string(),
        program_digest: program_digest.to_hex(),
        source_hash: source_hash.to_hex(),
        target_vm: target_vm.name.clone(),
        target_os: target_os.map(|os| os.name.clone()),
        architecture,
        cost: ManifestCost {
            table_values: (0..cost.total.count as usize)
                .map(|i| cost.total.get(i))
                .collect(),
            table_names: cost.table_names.clone(),
            padded_height: cost.padded_height,
        },
        functions,
        entry_point,
        built_at: iso8601_now(),
        compiler_version: env!("CARGO_PKG_VERSION").to_string(),
    };

    // 7. Create artifact directory
    let artifact_dir = output_base.join(format!("{}.deploy", name));
    std::fs::create_dir_all(&artifact_dir)
        .map_err(|e| format!("cannot create '{}': {}", artifact_dir.display(), e))?;

    // 8. Write program.tasm
    let tasm_path = artifact_dir.join("program.tasm");
    std::fs::write(&tasm_path, tasm)
        .map_err(|e| format!("cannot write '{}': {}", tasm_path.display(), e))?;

    // 9. Write manifest.json
    let manifest_path = artifact_dir.join("manifest.json");
    std::fs::write(&manifest_path, manifest.to_json())
        .map_err(|e| format!("cannot write '{}': {}", manifest_path.display(), e))?;

    Ok(PackageResult {
        manifest,
        artifact_dir,
        tasm_path,
        manifest_path,
    })
}

// ─── JSON Serialization ────────────────────────────────────────────

impl PackageManifest {
    /// Serialize to JSON (hand-rolled, no serde dependency).
    pub fn to_json(&self) -> String {
        let mut out = String::from("{\n");

        out.push_str(&format!("  \"name\": {},\n", json_string(&self.name)));
        out.push_str(&format!("  \"version\": {},\n", json_string(&self.version)));
        out.push_str(&format!(
            "  \"program_digest\": {},\n",
            json_string(&self.program_digest)
        ));
        out.push_str(&format!(
            "  \"source_hash\": {},\n",
            json_string(&self.source_hash)
        ));

        // target object
        out.push_str("  \"target\": {\n");
        out.push_str(&format!("    \"vm\": {},\n", json_string(&self.target_vm)));
        if let Some(ref os) = self.target_os {
            out.push_str(&format!("    \"os\": {},\n", json_string(os)));
        } else {
            out.push_str("    \"os\": null,\n");
        }
        out.push_str(&format!(
            "    \"architecture\": {}\n",
            json_string(&self.architecture)
        ));
        out.push_str("  },\n");

        // cost object
        out.push_str("  \"cost\": {\n");
        for (i, name) in self.cost.table_names.iter().enumerate() {
            let val = self.cost.table_values.get(i).copied().unwrap_or(0);
            out.push_str(&format!("    {}: {},\n", json_string(name), val));
        }
        out.push_str(&format!(
            "    \"padded_height\": {}\n",
            self.cost.padded_height
        ));
        out.push_str("  },\n");

        // functions array
        out.push_str("  \"functions\": [\n");
        for (i, func) in self.functions.iter().enumerate() {
            let comma = if i + 1 < self.functions.len() {
                ","
            } else {
                ""
            };
            out.push_str(&format!(
                "    {{ \"name\": {}, \"hash\": {}, \"signature\": {} }}{}\n",
                json_string(&func.name),
                json_string(&func.hash),
                json_string(&func.signature),
                comma,
            ));
        }
        out.push_str("  ],\n");

        out.push_str(&format!(
            "  \"entry_point\": {},\n",
            json_string(&self.entry_point)
        ));
        out.push_str(&format!(
            "  \"built_at\": {},\n",
            json_string(&self.built_at)
        ));
        out.push_str(&format!(
            "  \"compiler_version\": {}\n",
            json_string(&self.compiler_version)
        ));

        out.push_str("}\n");
        out
    }
}

/// JSON-escape a string and wrap in quotes.
fn json_string(s: &str) -> String {
    let mut out = String::from('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

// ─── Helpers ───────────────────────────────────────────────────────

/// Extract function names, signatures, and hashes from a parsed file.
/// Skips test functions.
fn extract_functions(
    file: &ast::File,
    fn_hashes: &HashMap<String, ContentHash>,
) -> Vec<ManifestFunction> {
    let mut functions = Vec::new();
    for item in &file.items {
        if let ast::Item::Fn(func) = &item.node {
            if func.is_test {
                continue;
            }
            let sig = format_fn_signature(func);
            let hash = fn_hashes
                .get(&func.name.node)
                .map(|h| h.to_hex())
                .unwrap_or_default();
            functions.push(ManifestFunction {
                name: func.name.node.clone(),
                hash,
                signature: sig,
            });
        }
    }
    functions
}

/// Format a function signature for the manifest.
fn format_fn_signature(func: &ast::FnDef) -> String {
    let mut sig = String::from("fn ");
    sig.push_str(&func.name.node);

    if !func.type_params.is_empty() {
        let params: Vec<_> = func.type_params.iter().map(|p| p.node.clone()).collect();
        sig.push_str(&format!("<{}>", params.join(", ")));
    }

    sig.push('(');
    let params: Vec<String> = func
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name.node, format_ast_type(&p.ty.node)))
        .collect();
    sig.push_str(&params.join(", "));
    sig.push(')');

    if let Some(ref ret) = func.return_ty {
        sig.push_str(&format!(" -> {}", format_ast_type(&ret.node)));
    }

    sig
}

/// Format an AST type for display.
fn format_ast_type(ty: &ast::Type) -> String {
    match ty {
        ast::Type::Field => "Field".to_string(),
        ast::Type::XField => "XField".to_string(),
        ast::Type::Bool => "Bool".to_string(),
        ast::Type::U32 => "U32".to_string(),
        ast::Type::Digest => "Digest".to_string(),
        ast::Type::Array(inner, size) => format!("[{}; {}]", format_ast_type(inner), size),
        ast::Type::Tuple(elems) => {
            let parts: Vec<_> = elems.iter().map(|e| format_ast_type(e)).collect();
            format!("({})", parts.join(", "))
        }
        ast::Type::Named(path) => path.as_dotted(),
    }
}

/// Find the entry point function name (looks for "main").
fn find_entry_point(file: &ast::File) -> String {
    for item in &file.items {
        if let ast::Item::Fn(func) = &item.node {
            if func.name.node == "main" {
                return "main".to_string();
            }
        }
    }
    // Fallback: first non-test function
    for item in &file.items {
        if let ast::Item::Fn(func) = &item.node {
            if !func.is_test {
                return func.name.node.clone();
            }
        }
    }
    "main".to_string()
}

/// Get current time as ISO 8601 string (no chrono dependency).
fn iso8601_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Convert epoch seconds to a basic ISO 8601 date-time.
    // This is a simplified conversion (no leap second handling).
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Calculate year/month/day from days since epoch (1970-01-01).
    let (year, month, day) = days_to_date(days);

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

/// Convert days since Unix epoch to (year, month, day).
fn days_to_date(days: u64) -> (u64, u64, u64) {
    // Algorithm from https://howardhinnant.github.io/date_algorithms.html
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

// ─── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_string_escaping() {
        assert_eq!(json_string("hello"), "\"hello\"");
        assert_eq!(json_string("a\"b"), "\"a\\\"b\"");
        assert_eq!(json_string("a\\b"), "\"a\\\\b\"");
        assert_eq!(json_string("a\nb"), "\"a\\nb\"");
    }

    #[test]
    fn test_iso8601_now_format() {
        let ts = iso8601_now();
        // Should match pattern: YYYY-MM-DDTHH:MM:SSZ
        assert!(ts.ends_with('Z'));
        assert_eq!(ts.len(), 20);
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[7..8], "-");
        assert_eq!(&ts[10..11], "T");
        assert_eq!(&ts[13..14], ":");
        assert_eq!(&ts[16..17], ":");
    }

    #[test]
    fn test_days_to_date_epoch() {
        let (y, m, d) = days_to_date(0);
        assert_eq!((y, m, d), (1970, 1, 1));
    }

    #[test]
    fn test_days_to_date_known() {
        // 2026-02-11 is day 20,495 since epoch
        let (y, m, d) = days_to_date(20495);
        assert_eq!((y, m, d), (2026, 2, 11));
    }

    #[test]
    fn test_manifest_to_json_structure() {
        let manifest = PackageManifest {
            name: "test".to_string(),
            version: "0.1.0".to_string(),
            program_digest: "aabb".to_string(),
            source_hash: "ccdd".to_string(),
            target_vm: "triton".to_string(),
            target_os: Some("neptune".to_string()),
            architecture: "stack".to_string(),
            cost: ManifestCost {
                table_values: vec![100, 50, 25, 0, 0, 0],
                table_names: vec![
                    "processor".into(),
                    "hash".into(),
                    "u32".into(),
                    "op_stack".into(),
                    "ram".into(),
                    "jump_stack".into(),
                ],
                padded_height: 256,
            },
            functions: vec![ManifestFunction {
                name: "main".to_string(),
                hash: "eeff".to_string(),
                signature: "fn main()".to_string(),
            }],
            entry_point: "main".to_string(),
            built_at: "2026-02-11T00:00:00Z".to_string(),
            compiler_version: "0.1.0".to_string(),
        };

        let json = manifest.to_json();
        assert!(json.contains("\"name\": \"test\""));
        assert!(json.contains("\"program_digest\": \"aabb\""));
        assert!(json.contains("\"os\": \"neptune\""));
        assert!(json.contains("\"vm\": \"triton\""));
        assert!(json.contains("\"processor\": 100"));
        assert!(json.contains("\"padded_height\": 256"));
        assert!(json.contains("\"entry_point\": \"main\""));
        assert!(json.contains("\"fn main()\""));
    }

    #[test]
    fn test_manifest_null_os() {
        let manifest = PackageManifest {
            name: "bare".to_string(),
            version: "0.1.0".to_string(),
            program_digest: "aa".to_string(),
            source_hash: "bb".to_string(),
            target_vm: "triton".to_string(),
            target_os: None,
            architecture: "stack".to_string(),
            cost: ManifestCost {
                table_values: vec![0, 0, 0, 0, 0, 0],
                table_names: vec![
                    "processor".into(),
                    "hash".into(),
                    "u32".into(),
                    "op_stack".into(),
                    "ram".into(),
                    "jump_stack".into(),
                ],
                padded_height: 0,
            },
            functions: vec![],
            entry_point: "main".to_string(),
            built_at: "2026-01-01T00:00:00Z".to_string(),
            compiler_version: "0.1.0".to_string(),
        };

        let json = manifest.to_json();
        assert!(json.contains("\"os\": null"));
    }

    #[test]
    fn test_program_digest_deterministic() {
        let tasm = "push 1\npush 2\nadd\nwrite_io 1\nhalt\n";
        let hash1 = ContentHash(crate::poseidon2::hash_bytes(tasm.as_bytes()));
        let hash2 = ContentHash(crate::poseidon2::hash_bytes(tasm.as_bytes()));
        assert_eq!(hash1.to_hex(), hash2.to_hex());
    }

    #[test]
    fn test_generate_artifact_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let source = "program test\nfn main() {\n    pub_write(pub_read())\n}\n";
        let filename = "test.tri";

        // Parse the source
        let file = crate::parse_source_silent(source, filename).unwrap();

        // Create a minimal cost
        let cost = crate::cost::CostAnalyzer::default().analyze_file(&file);

        let target_vm = TargetConfig::triton();
        let tasm = "push 1\nwrite_io 1\nhalt\n";

        let result = generate_artifact(
            "test",
            "0.1.0",
            tasm,
            &file,
            &cost,
            &target_vm,
            None,
            dir.path(),
        )
        .unwrap();

        // Verify directory and files exist
        assert!(result.artifact_dir.exists());
        assert!(result.tasm_path.exists());
        assert!(result.manifest_path.exists());
        assert_eq!(
            result.artifact_dir.file_name().unwrap().to_str().unwrap(),
            "test.deploy"
        );

        // Verify TASM content
        let written_tasm = std::fs::read_to_string(&result.tasm_path).unwrap();
        assert_eq!(written_tasm, tasm);

        // Verify manifest content
        let manifest_json = std::fs::read_to_string(&result.manifest_path).unwrap();
        assert!(manifest_json.contains("\"name\": \"test\""));
        assert!(manifest_json.contains("\"program_digest\""));
        assert!(manifest_json.contains("\"source_hash\""));
        assert!(manifest_json.contains("\"vm\": \"triton\""));

        // Verify digest is non-empty
        assert!(!result.manifest.program_digest.is_empty());
        assert!(!result.manifest.source_hash.is_empty());
    }
}
