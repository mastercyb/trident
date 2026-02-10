pub mod ast;
pub mod cost;
pub mod diagnostic;
pub mod emit;
pub mod format;
pub mod lexeme;
pub mod lexer;
pub mod linker;
pub mod lsp;
pub mod parser;
pub mod project;
pub mod resolve;
pub mod span;
pub mod stack;
pub mod typeck;
pub mod types;

use std::collections::HashSet;
use std::path::Path;

use ast::FileKind;
use diagnostic::{render_diagnostics, Diagnostic};
use emit::Emitter;
use lexer::Lexer;
use linker::{link, ModuleTasm};
use parser::Parser;
use resolve::resolve_modules;
use typeck::{ModuleExports, TypeChecker};

/// Options controlling conditional compilation.
#[derive(Clone, Debug)]
pub struct CompileOptions {
    pub target: String,
    pub cfg_flags: HashSet<String>,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            target: "debug".to_string(),
            cfg_flags: HashSet::from(["debug".to_string()]),
        }
    }
}

impl CompileOptions {
    /// Create options for a named built-in target.
    pub fn for_target(target: &str) -> Self {
        Self {
            target: target.to_string(),
            cfg_flags: HashSet::from([target.to_string()]),
        }
    }
}

/// Compile a single Trident source string to TASM.
pub fn compile(source: &str, filename: &str) -> Result<String, Vec<Diagnostic>> {
    compile_with_options(source, filename, &CompileOptions::default())
}

/// Compile a single Trident source string to TASM with options.
pub fn compile_with_options(
    source: &str,
    filename: &str,
    options: &CompileOptions,
) -> Result<String, Vec<Diagnostic>> {
    let file = parse_source(source, filename)?;

    // Type check
    let exports = match TypeChecker::new()
        .with_cfg_flags(options.cfg_flags.clone())
        .check_file(&file)
    {
        Ok(exports) => exports,
        Err(errors) => {
            render_diagnostics(&errors, filename, source);
            return Err(errors);
        }
    };

    // Emit TASM
    let tasm = Emitter::new()
        .with_cfg_flags(options.cfg_flags.clone())
        .with_mono_instances(exports.mono_instances)
        .with_call_resolutions(exports.call_resolutions)
        .emit_file(&file);
    Ok(tasm)
}

/// Compile a multi-module project from an entry point path.
pub fn compile_project(entry_path: &Path) -> Result<String, Vec<Diagnostic>> {
    compile_project_with_options(entry_path, &CompileOptions::default())
}

/// Compile a multi-module project with options.
pub fn compile_project_with_options(
    entry_path: &Path,
    options: &CompileOptions,
) -> Result<String, Vec<Diagnostic>> {
    // Resolve all modules in dependency order
    let modules = resolve_modules(entry_path)?;

    let mut parsed_modules = Vec::new();
    let mut all_exports: Vec<ModuleExports> = Vec::new();

    // Parse all modules
    for module in &modules {
        let file = parse_source(&module.source, &module.file_path.to_string_lossy())?;
        parsed_modules.push((
            module.name.clone(),
            module.file_path.clone(),
            module.source.clone(),
            file,
        ));
    }

    // Type-check in topological order (deps first), collecting exports
    for (_module_name, file_path, source, file) in &parsed_modules {
        let mut tc = TypeChecker::new().with_cfg_flags(options.cfg_flags.clone());

        // Import signatures from already-checked dependencies
        for exports in &all_exports {
            tc.import_module(exports);
        }

        match tc.check_file(file) {
            Ok(exports) => {
                if !exports.warnings.is_empty() {
                    render_diagnostics(&exports.warnings, &file_path.to_string_lossy(), source);
                }
                all_exports.push(exports);
            }
            Err(errors) => {
                render_diagnostics(&errors, &file_path.to_string_lossy(), source);
                return Err(errors);
            }
        }
    }

    // Build global intrinsic map from all modules
    let mut intrinsic_map = std::collections::HashMap::new();
    for (_module_name, _file_path, _source, file) in &parsed_modules {
        for item in &file.items {
            if let ast::Item::Fn(func) = &item.node {
                if let Some(ref intrinsic) = func.intrinsic {
                    // Extract the inner value from "intrinsic(VALUE)"
                    let intr_value = if let Some(start) = intrinsic.node.find('(') {
                        let end = intrinsic.node.rfind(')').unwrap_or(intrinsic.node.len());
                        intrinsic.node[start + 1..end].to_string()
                    } else {
                        intrinsic.node.clone()
                    };
                    // Register under short function name
                    intrinsic_map.insert(func.name.node.clone(), intr_value.clone());
                    // Register under qualified name (module.func)
                    let qualified = format!("{}.{}", file.name.node, func.name.node);
                    intrinsic_map.insert(qualified, intr_value.clone());
                    // For dotted module names like std.hash, also
                    // register under short alias (hash.func)
                    if let Some(short) = file.name.node.rsplit('.').next() {
                        if short != file.name.node {
                            let short_qualified = format!("{}.{}", short, func.name.node);
                            intrinsic_map.insert(short_qualified, intr_value.clone());
                        }
                    }
                }
            }
        }
    }

    // Build module alias map: short name → full name for dotted modules
    let mut module_aliases = std::collections::HashMap::new();
    for (_module_name, _file_path, _source, file) in &parsed_modules {
        let full_name = &file.name.node;
        if let Some(short) = full_name.rsplit('.').next() {
            if short != full_name.as_str() {
                module_aliases.insert(short.to_string(), full_name.clone());
            }
        }
    }

    // Build external constants map from all module exports
    let mut external_constants = std::collections::HashMap::new();
    for exports in &all_exports {
        let full = &exports.module_name;
        let short = full.rsplit('.').next().unwrap_or(full);
        let has_short = short != full;
        for (const_name, _ty, value) in &exports.constants {
            let qualified = format!("{}.{}", full, const_name);
            external_constants.insert(qualified, *value);
            if has_short {
                let short_qualified = format!("{}.{}", short, const_name);
                external_constants.insert(short_qualified, *value);
            }
        }
    }

    // Emit TASM for each module
    let mut tasm_modules = Vec::new();
    for (i, (_module_name, _file_path, _source, file)) in parsed_modules.iter().enumerate() {
        let is_program = file.kind == FileKind::Program;
        let mono = all_exports
            .get(i)
            .map(|e| e.mono_instances.clone())
            .unwrap_or_default();
        let call_res = all_exports
            .get(i)
            .map(|e| e.call_resolutions.clone())
            .unwrap_or_default();
        let tasm = Emitter::new()
            .with_cfg_flags(options.cfg_flags.clone())
            .with_intrinsics(intrinsic_map.clone())
            .with_module_aliases(module_aliases.clone())
            .with_constants(external_constants.clone())
            .with_mono_instances(mono)
            .with_call_resolutions(call_res)
            .emit_file(file);
        tasm_modules.push(ModuleTasm {
            module_name: file.name.node.clone(),
            is_program,
            tasm,
        });
    }

    // Link
    let linked = link(tasm_modules);
    Ok(linked)
}

/// Type-check only (no TASM emission).
pub fn check(source: &str, filename: &str) -> Result<(), Vec<Diagnostic>> {
    let file = parse_source(source, filename)?;

    if let Err(errors) = TypeChecker::new().check_file(&file) {
        render_diagnostics(&errors, filename, source);
        return Err(errors);
    }

    Ok(())
}

/// Project-aware type-check from an entry point path.
/// Resolves all modules (including std.*) and type-checks in dependency order.
pub fn check_project(entry_path: &Path) -> Result<(), Vec<Diagnostic>> {
    let modules = resolve_modules(entry_path)?;

    let mut all_exports: Vec<ModuleExports> = Vec::new();

    for module in &modules {
        let file = parse_source(&module.source, &module.file_path.to_string_lossy())?;

        let mut tc = TypeChecker::new();
        for exports in &all_exports {
            tc.import_module(exports);
        }

        match tc.check_file(&file) {
            Ok(exports) => {
                if !exports.warnings.is_empty() {
                    render_diagnostics(
                        &exports.warnings,
                        &module.file_path.to_string_lossy(),
                        &module.source,
                    );
                }
                all_exports.push(exports);
            }
            Err(errors) => {
                render_diagnostics(&errors, &module.file_path.to_string_lossy(), &module.source);
                return Err(errors);
            }
        }
    }

    Ok(())
}

/// Parse, type-check, and compute cost analysis for a single file.
pub fn analyze_costs(source: &str, filename: &str) -> Result<cost::ProgramCost, Vec<Diagnostic>> {
    let file = parse_source(source, filename)?;

    if let Err(errors) = TypeChecker::new().check_file(&file) {
        render_diagnostics(&errors, filename, source);
        return Err(errors);
    }

    let cost = cost::CostAnalyzer::new().analyze_file(&file);
    Ok(cost)
}

/// Format Trident source code, preserving comments.
pub fn format_source(source: &str, _filename: &str) -> Result<String, Vec<Diagnostic>> {
    let (tokens, comments, lex_errors) = Lexer::new(source, 0).tokenize();
    if !lex_errors.is_empty() {
        return Err(lex_errors);
    }
    let file = Parser::new(tokens).parse_file()?;
    Ok(format::format_file(&file, &comments, source))
}

/// Type-check only, without rendering diagnostics to stderr.
/// Used by the LSP server to get structured errors.
pub fn check_silent(source: &str, filename: &str) -> Result<(), Vec<Diagnostic>> {
    let file = parse_source_silent(source, filename)?;
    TypeChecker::new().check_file(&file)?;
    Ok(())
}

/// Project-aware type-check for the LSP.
/// Finds trident.toml, resolves dependencies, and type-checks
/// the given file with full module context.
/// Falls back to single-file check if no project is found.
pub fn check_file_in_project(source: &str, file_path: &Path) -> Result<(), Vec<Diagnostic>> {
    let dir = file_path.parent().unwrap_or(Path::new("."));
    let entry = match project::Project::find(dir) {
        Some(toml_path) => match project::Project::load(&toml_path) {
            Ok(p) => p.entry,
            Err(_) => file_path.to_path_buf(),
        },
        None => file_path.to_path_buf(),
    };

    // Resolve all modules from the entry point (handles std.* even without project)
    let modules = match resolve_modules(&entry) {
        Ok(m) => m,
        Err(_) => return check_silent(source, &file_path.to_string_lossy()),
    };

    // Parse and type-check all modules in dependency order
    let mut all_exports: Vec<ModuleExports> = Vec::new();
    let file_path_canon = file_path
        .canonicalize()
        .unwrap_or_else(|_| file_path.to_path_buf());

    for module in &modules {
        let mod_path_canon = module
            .file_path
            .canonicalize()
            .unwrap_or_else(|_| module.file_path.clone());
        let is_target = mod_path_canon == file_path_canon;

        // Use live buffer for the file being edited
        let src = if is_target { source } else { &module.source };
        let parsed = parse_source_silent(src, &module.file_path.to_string_lossy())?;

        let mut tc = TypeChecker::new();
        for exports in &all_exports {
            tc.import_module(exports);
        }

        match tc.check_file(&parsed) {
            Ok(exports) => {
                all_exports.push(exports);
            }
            Err(errors) => {
                if is_target {
                    return Err(errors);
                }
                // Dep has errors — stop, but don't report
                // dep errors as if they're in this file
                return Ok(());
            }
        }
    }

    Ok(())
}

fn parse_source(source: &str, filename: &str) -> Result<ast::File, Vec<Diagnostic>> {
    let (tokens, _comments, lex_errors) = Lexer::new(source, 0).tokenize();
    if !lex_errors.is_empty() {
        render_diagnostics(&lex_errors, filename, source);
        return Err(lex_errors);
    }

    match Parser::new(tokens).parse_file() {
        Ok(file) => Ok(file),
        Err(errors) => {
            render_diagnostics(&errors, filename, source);
            Err(errors)
        }
    }
}

pub fn parse_source_silent(source: &str, _filename: &str) -> Result<ast::File, Vec<Diagnostic>> {
    let (tokens, _comments, lex_errors) = Lexer::new(source, 0).tokenize();
    if !lex_errors.is_empty() {
        return Err(lex_errors);
    }
    Parser::new(tokens).parse_file()
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_fungible_token_compiles() {
        let source = include_str!("../examples/fungible_token/token.tri");
        let tasm = compile(source, "token.tri").expect("token program should compile");

        // Verify all 5 operations are in the TASM output
        assert!(tasm.contains("__pay:"), "missing pay function");
        assert!(tasm.contains("__mint:"), "missing mint function");
        assert!(tasm.contains("__burn:"), "missing burn function");
        assert!(tasm.contains("__lock:"), "missing lock function");
        assert!(tasm.contains("__update:"), "missing update function");

        // Verify helper functions
        assert!(tasm.contains("__hash_leaf:"), "missing hash_leaf function");
        assert!(
            tasm.contains("__hash_config:"),
            "missing hash_config function"
        );
        assert!(
            tasm.contains("__hash_metadata:"),
            "missing hash_metadata function"
        );
        assert!(
            tasm.contains("__verify_auth:"),
            "missing verify_auth function"
        );
        assert!(
            tasm.contains("__verify_config:"),
            "missing verify_config function"
        );

        // Verify hash operations are emitted (leaf/config/metadata/auth + seal nullifiers)
        let hash_count = tasm.lines().filter(|l| l.trim() == "hash").count();
        assert!(
            hash_count >= 6,
            "expected at least 6 hash ops, got {}",
            hash_count
        );

        // Verify seal produces write_io 5 (nullifier commitments in pay and burn)
        assert!(
            tasm.contains("write_io 5"),
            "seal should produce write_io 5"
        );

        // Verify assertions are present (security checks)
        let assert_count = tasm
            .lines()
            .filter(|l| l.trim().starts_with("assert"))
            .count();
        assert!(
            assert_count >= 6,
            "expected at least 6 assertions, got {}",
            assert_count
        );

        eprintln!(
            "Token TASM: {} lines, {} instructions",
            tasm.lines().count(),
            tasm.lines()
                .filter(|l| l.starts_with("    ") && !l.trim().is_empty())
                .count()
        );
    }

    #[test]
    fn test_fungible_token_cost_analysis() {
        let source = include_str!("../examples/fungible_token/token.tri");
        let cost = analyze_costs(source, "token.tri").expect("cost analysis should succeed");

        // Processor table should be nonzero
        assert!(cost.total.processor > 0);

        // Token uses hash heavily (leaf hashing, config hashing, auth verification)
        assert!(cost.total.hash > 0, "token should have hash table cost");

        // Token uses u32 range checks for balance verification
        assert!(
            cost.total.u32_table > 0,
            "token should have u32 table cost for range checks"
        );

        // Padded height should be reasonable (power of 2)
        assert!(cost.padded_height.is_power_of_two());
        assert!(
            cost.padded_height <= 4096,
            "padded height {} seems too high",
            cost.padded_height
        );

        // Should have functions for all 5 operations
        let fn_names: Vec<&str> = cost.functions.iter().map(|f| f.name.as_str()).collect();
        assert!(fn_names.contains(&"pay"), "missing pay cost");
        assert!(fn_names.contains(&"mint"), "missing mint cost");
        assert!(fn_names.contains(&"burn"), "missing burn cost");
        assert!(fn_names.contains(&"lock"), "missing lock cost");
        assert!(fn_names.contains(&"update"), "missing update cost");

        // Config helper functions should appear
        assert!(
            fn_names.contains(&"hash_config"),
            "missing hash_config cost"
        );
        assert!(
            fn_names.contains(&"hash_metadata"),
            "missing hash_metadata cost"
        );
        assert!(
            fn_names.contains(&"verify_config"),
            "missing verify_config cost"
        );

        eprintln!(
            "Token cost: padded_height={}, cc={}, hash={}, u32={}",
            cost.padded_height, cost.total.processor, cost.total.hash, cost.total.u32_table
        );
        eprintln!("{}", cost.format_report());
    }

    #[test]
    fn test_events_emit_and_seal() {
        let source = r#"program test

event Transfer {
    from: Field,
    to: Field,
    amount: Field,
}

event Commitment {
    value: Field,
}

fn main() {
    let a: Field = pub_read()
    let b: Field = pub_read()
    let c: Field = pub_read()

    // Open emit: tag + 3 fields written directly
    emit Transfer { from: a, to: b, amount: c }

    // Sealed: hash(tag, value, 0...) written as digest
    seal Commitment { value: a }
}
"#;
        let tasm = compile(source, "events.tri").expect("events program should compile");

        // emit Transfer: push 0, write_io 1, [field], write_io 1 × 3
        // Total write_io 1 from emit: 4 (tag + 3 fields)
        let write_io_1 = tasm.lines().filter(|l| l.trim() == "write_io 1").count();
        assert!(
            write_io_1 >= 4,
            "expected >= 4 write_io 1 (emit tag + 3 fields), got {}",
            write_io_1
        );

        // seal Commitment: hash + write_io 5
        assert!(tasm.contains("hash"), "seal should contain hash");
        assert!(tasm.contains("write_io 5"), "seal should write_io 5");

        eprintln!("Events TASM:\n{}", tasm);
    }

    // --- Multi-module type checking (check_file_in_project) ---

    #[test]
    fn test_check_silent_valid() {
        let source = "program test\nfn main() {\n    pub_write(pub_read())\n}";
        assert!(check_silent(source, "test.tri").is_ok());
    }

    #[test]
    fn test_check_silent_error() {
        let source = "program test\nfn main() {\n    pub_write(undefined_var)\n}";
        assert!(check_silent(source, "test.tri").is_err());
    }

    #[test]
    fn test_check_silent_parse_error() {
        let source = "program test\nfn main( {\n}";
        assert!(check_silent(source, "test.tri").is_err());
    }

    #[test]
    fn test_format_source_valid() {
        let source = "program test\n\nfn main() {\n    pub_write(pub_read())\n}\n";
        let result = format_source(source, "test.tri");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), source);
    }

    #[test]
    fn test_format_source_lex_error() {
        // Unterminated string or invalid character
        let source = "program test\n\nfn main() {\n    let x = @\n}\n";
        let result = format_source(source, "test.tri");
        assert!(result.is_err());
    }

    #[test]
    fn test_compile_type_error_returns_err() {
        let source = "program test\nfn main() {\n    let x: U32 = pub_read()\n}";
        let result = compile(source, "test.tri");
        assert!(result.is_err());
    }

    #[test]
    fn test_compile_valid_program() {
        let source =
            "program test\nfn main() {\n    let x: Field = pub_read()\n    pub_write(x + 1)\n}";
        let result = compile(source, "test.tri");
        assert!(result.is_ok());
        let tasm = result.unwrap();
        assert!(tasm.contains("read_io 1"));
        assert!(tasm.contains("write_io 1"));
    }

    #[test]
    fn test_check_valid_program() {
        let source = "program test\nfn main() {\n    pub_write(pub_read())\n}";
        assert!(check(source, "test.tri").is_ok());
    }

    #[test]
    fn test_check_type_error() {
        let source = "program test\nfn main() {\n    let x: Bool = pub_read()\n}";
        assert!(check(source, "test.tri").is_err());
    }

    #[test]
    fn test_analyze_costs_valid() {
        let source = "program test\nfn main() {\n    pub_write(pub_read())\n}";
        let result = analyze_costs(source, "test.tri");
        assert!(result.is_ok());
        let cost = result.unwrap();
        assert!(cost.total.processor > 0);
        assert!(cost.padded_height.is_power_of_two());
    }

    #[test]
    fn test_analyze_costs_type_error() {
        let source = "program test\nfn main() {\n    let x: U32 = pub_read()\n}";
        assert!(analyze_costs(source, "test.tri").is_err());
    }

    // --- Edge cases: deep nesting ---

    #[test]
    fn test_deeply_nested_if() {
        let source = r#"program test
fn main() {
    let x: Field = pub_read()
    if x == 0 {
        if x == 1 {
            if x == 2 {
                if x == 3 {
                    if x == 4 {
                        pub_write(x)
                    }
                }
            }
        }
    }
}
"#;
        assert!(compile(source, "test.tri").is_ok());
    }

    #[test]
    fn test_deeply_nested_for() {
        let source = r#"program test
fn main() {
    let mut s: Field = 0
    for i in 0..3 bounded 3 {
        for j in 0..3 bounded 3 {
            for k in 0..3 bounded 3 {
                s = s + 1
            }
        }
    }
    pub_write(s)
}
"#;
        let result = compile(source, "test.tri");
        assert!(result.is_ok());
        let tasm = result.unwrap();
        assert!(tasm.contains("write_io 1"));
    }

    #[test]
    fn test_many_variables_spill() {
        // Force stack spilling by having many live variables
        let source = r#"program test
fn main() {
    let a: Field = pub_read()
    let b: Field = pub_read()
    let c: Field = pub_read()
    let d: Field = pub_read()
    let e: Field = pub_read()
    let f: Field = pub_read()
    let g: Field = pub_read()
    let h: Field = pub_read()
    let i: Field = pub_read()
    let j: Field = pub_read()
    let k: Field = pub_read()
    let l: Field = pub_read()
    let m: Field = pub_read()
    let n: Field = pub_read()
    let o: Field = pub_read()
    let p: Field = pub_read()
    let q: Field = pub_read()
    let r: Field = pub_read()
    pub_write(a + b + c + d + e + f + g + h + i + j + k + l + m + n + o + p + q + r)
}
"#;
        let result = compile(source, "test.tri");
        assert!(
            result.is_ok(),
            "should handle 18 live variables with spilling"
        );
        let tasm = result.unwrap();
        // Should contain RAM operations from spilling
        assert!(
            tasm.contains("write_mem") || tasm.contains("read_mem"),
            "18 variables should trigger spilling"
        );
    }

    #[test]
    fn test_chain_of_function_calls() {
        let source = r#"program test
fn add1(x: Field) -> Field {
    x + 1
}

fn add2(x: Field) -> Field {
    add1(add1(x))
}

fn add4(x: Field) -> Field {
    add2(add2(x))
}

fn main() {
    let x: Field = pub_read()
    pub_write(add4(add4(x)))
}
"#;
        let result = compile(source, "test.tri");
        assert!(result.is_ok());
    }

    #[test]
    fn test_all_binary_operators() {
        let source = r#"program test
fn main() {
    let a: Field = pub_read()
    let b: Field = pub_read()
    let sum: Field = a + b
    let prod: Field = a * b
    let eq: Bool = a == b
    let (hi, lo) = split(a)
    let lt: Bool = hi < lo
    let band: U32 = hi & lo
    let bxor: U32 = hi ^ lo
    let (q, r) = hi /% lo
    pub_write(sum)
    pub_write(prod)
}
"#;
        assert!(compile(source, "test.tri").is_ok());
    }

    #[test]
    fn test_struct_with_digest_field() {
        let source = r#"program test
struct AuthData {
    owner: Digest,
    nonce: Field,
}

fn main() {
    let d: Digest = divine5()
    let auth: AuthData = AuthData { owner: d, nonce: 42 }
    pub_write(auth.nonce)
}
"#;
        assert!(compile(source, "test.tri").is_ok());
    }

    #[test]
    fn test_array_of_structs_type_check() {
        // Arrays of structs should type-check correctly
        let source = r#"program test
struct Pt {
    x: Field,
    y: Field,
}

fn main() {
    let a: Pt = Pt { x: 1, y: 2 }
    let b: Pt = Pt { x: 3, y: 4 }
    pub_write(a.x + b.y)
}
"#;
        assert!(check(source, "test.tri").is_ok());
    }

    #[test]
    fn test_xfield_operations() {
        // *. operator is XField * Field -> XField (scalar multiplication)
        let source = r#"program test
fn main() {
    let a: XField = xfield(1, 2, 3)
    let s: Field = pub_read()
    let c: XField = a *. s
    let d: XField = xinvert(c)
    pub_write(0)
}
"#;
        assert!(compile(source, "test.tri").is_ok());
    }

    #[test]
    fn test_tail_expression() {
        let source = r#"program test
fn double(x: Field) -> Field {
    x + x
}

fn main() {
    pub_write(double(pub_read()))
}
"#;
        assert!(compile(source, "test.tri").is_ok());
    }

    #[test]
    fn test_multiple_return_paths() {
        let source = r#"program test
fn abs_diff(a: Field, b: Field) -> Field {
    if a == b {
        return 0
    }
    a + b
}

fn main() {
    pub_write(abs_diff(pub_read(), pub_read()))
}
"#;
        assert!(compile(source, "test.tri").is_ok());
    }

    #[test]
    fn test_parse_source_silent_no_stderr() {
        // parse_source_silent should not render diagnostics
        let source = "program test\nfn main() {\n    pub_write(pub_read())\n}";
        let result = parse_source_silent(source, "test.tri");
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_source_silent_returns_errors() {
        let source = "program test\nfn main( {\n}";
        let result = parse_source_silent(source, "test.tri");
        assert!(result.is_err());
    }

    // --- Size-generic function integration tests ---

    #[test]
    fn test_generic_fn_compile_explicit() {
        let source = r#"program test

fn first<N>(arr: [Field; N]) -> Field {
    arr[0]
}

fn main() {
    let a: [Field; 3] = [1, 2, 3]
    let s: Field = first<3>(a)
    pub_write(s)
}
"#;
        let result = compile(source, "test.tri");
        assert!(
            result.is_ok(),
            "generic fn should compile: {:?}",
            result.err()
        );
        let tasm = result.unwrap();
        assert!(
            tasm.contains("__first__N3:"),
            "should emit monomorphized label"
        );
    }

    #[test]
    fn test_generic_fn_compile_inferred() {
        let source = r#"program test

fn first<N>(arr: [Field; N]) -> Field {
    arr[0]
}

fn main() {
    let a: [Field; 3] = [1, 2, 3]
    let s: Field = first(a)
    pub_write(s)
}
"#;
        let result = compile(source, "test.tri");
        assert!(
            result.is_ok(),
            "generic fn with inference should compile: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_generic_fn_type_error() {
        let source = r#"program test

fn first<N>(arr: [Field; N]) -> Field {
    arr[0]
}

fn main() {
    let a: [Field; 3] = [1, 2, 3]
    let s: Field = first<5>(a)
}
"#;
        let result = compile(source, "test.tri");
        assert!(result.is_err(), "wrong size arg should fail compilation");
    }

    #[test]
    fn test_generic_fn_multiple_instantiations_compile() {
        let source = r#"program test

fn first<N>(arr: [Field; N]) -> Field {
    arr[0]
}

fn main() {
    let a: [Field; 3] = [1, 2, 3]
    let b: [Field; 5] = [1, 2, 3, 4, 5]
    pub_write(first<3>(a) + first<5>(b))
}
"#;
        let result = compile(source, "test.tri");
        assert!(
            result.is_ok(),
            "multiple instantiations should compile: {:?}",
            result.err()
        );
        let tasm = result.unwrap();
        assert!(tasm.contains("__first__N3:"));
        assert!(tasm.contains("__first__N5:"));
    }

    #[test]
    fn test_generic_fn_existing_code_unaffected() {
        // Non-generic code should still work exactly as before
        let source = r#"program test

fn add(a: Field, b: Field) -> Field {
    a + b
}

fn main() {
    let x: Field = pub_read()
    let y: Field = pub_read()
    pub_write(add(x, y))
}
"#;
        let result = compile(source, "test.tri");
        assert!(result.is_ok());
        let tasm = result.unwrap();
        assert!(tasm.contains("call __add"));
        assert!(tasm.contains("__add:"));
    }

    #[test]
    fn test_generic_fn_check_only() {
        let source = r#"program test

fn sum<N>(arr: [Field; N]) -> Field {
    arr[0]
}

fn main() {
    let a: [Field; 3] = [1, 2, 3]
    let s: Field = sum<3>(a)
    pub_write(s)
}
"#;
        assert!(
            check(source, "test.tri").is_ok(),
            "type-check only should work"
        );
    }

    #[test]
    fn test_generic_fn_format_roundtrip() {
        let source = "program test\n\nfn first<N>(arr: [Field; N]) -> Field {\n    arr[0]\n}\n\nfn main() {\n    let a: [Field; 3] = [1, 2, 3]\n    let s: Field = first<3>(a)\n    pub_write(s)\n}\n";
        let formatted = format_source(source, "test.tri").expect("should format");
        assert!(
            formatted.contains("<N>"),
            "formatted output should preserve <N>"
        );
        assert!(
            formatted.contains("first<3>"),
            "formatted output should preserve first<3>"
        );
    }

    // --- conditional compilation integration tests ---

    #[test]
    fn test_cfg_debug_compiles() {
        let source = "program test\n#[cfg(debug)]\nfn check() {\n    assert(true)\n}\nfn main() {\n    check()\n}";
        let options = CompileOptions::for_target("debug");
        let result = compile_with_options(source, "test.tri", &options);
        assert!(result.is_ok(), "debug cfg should compile in debug mode");
        let tasm = result.unwrap();
        assert!(tasm.contains("__check:"), "check fn should be emitted");
    }

    #[test]
    fn test_cfg_release_excludes_debug_fn() {
        let source = "program test\n#[cfg(debug)]\nfn check() {\n    assert(true)\n}\nfn main() {}";
        let options = CompileOptions::for_target("release");
        let result = compile_with_options(source, "test.tri", &options);
        assert!(result.is_ok(), "should compile without debug fn");
        let tasm = result.unwrap();
        assert!(
            !tasm.contains("__check:"),
            "check fn should NOT be emitted in release"
        );
    }

    #[test]
    fn test_cfg_different_targets_different_output() {
        let source = "program test\n#[cfg(debug)]\nfn mode() -> Field { 0 }\n#[cfg(release)]\nfn mode() -> Field { 1 }\nfn main() {\n    let x: Field = mode()\n    pub_write(x)\n}";

        let debug_opts = CompileOptions::for_target("debug");
        let debug_tasm =
            compile_with_options(source, "test.tri", &debug_opts).expect("debug should compile");

        let release_opts = CompileOptions::for_target("release");
        let release_tasm = compile_with_options(source, "test.tri", &release_opts)
            .expect("release should compile");

        // Both should have __mode: but with different bodies
        assert!(debug_tasm.contains("__mode:"));
        assert!(release_tasm.contains("__mode:"));
        // Debug pushes 0, release pushes 1
        assert!(debug_tasm.contains("push 0"));
        assert!(release_tasm.contains("push 1"));
    }

    #[test]
    fn test_cfg_const_excluded_in_release() {
        let source = "program test\n#[cfg(debug)]\nconst LEVEL: Field = 3\nfn main() {}";
        let options = CompileOptions::for_target("release");
        let result = compile_with_options(source, "test.tri", &options);
        assert!(
            result.is_ok(),
            "should compile even though const is excluded"
        );
    }

    #[test]
    fn test_cfg_format_roundtrip() {
        let source = "program test\n\n#[cfg(debug)]\nfn check() {}\n\n#[cfg(release)]\nconst X: Field = 0\n\nfn main() {}\n";
        let formatted = format_source(source, "test.tri").expect("should format");
        assert!(
            formatted.contains("#[cfg(debug)]"),
            "should preserve cfg(debug)"
        );
        assert!(
            formatted.contains("#[cfg(release)]"),
            "should preserve cfg(release)"
        );
    }

    #[test]
    fn test_no_cfg_backward_compatible() {
        // All existing code should work unchanged (no cfg = always active)
        let source = "program test\nfn helper() -> Field { 42 }\nfn main() {\n    let x: Field = helper()\n    pub_write(x)\n}";
        let result = compile(source, "test.tri");
        assert!(result.is_ok(), "no-cfg code should compile as before");
    }
}
