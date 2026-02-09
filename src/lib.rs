pub mod ast;
pub mod cost;
pub mod diagnostic;
pub mod emit;
pub mod format;
pub mod lexeme;
pub mod lexer;
pub mod linker;
pub mod parser;
pub mod project;
pub mod resolve;
pub mod span;
pub mod stack;
pub mod typeck;
pub mod types;

use std::path::Path;

use ast::FileKind;
use diagnostic::{render_diagnostics, Diagnostic};
use emit::Emitter;
use lexer::Lexer;
use linker::{link, ModuleTasm};
use parser::Parser;
use resolve::resolve_modules;
use typeck::{ModuleExports, TypeChecker};

/// Compile a single Trident source string to TASM.
pub fn compile(source: &str, filename: &str) -> Result<String, Vec<Diagnostic>> {
    let file = parse_source(source, filename)?;

    // Type check
    match TypeChecker::new().check_file(&file) {
        Ok(_) => {}
        Err(errors) => {
            render_diagnostics(&errors, filename, source);
            return Err(errors);
        }
    }

    // Emit TASM
    let tasm = Emitter::new().emit_file(&file);
    Ok(tasm)
}

/// Compile a multi-module project from an entry point path.
pub fn compile_project(entry_path: &Path) -> Result<String, Vec<Diagnostic>> {
    // Resolve all modules in dependency order
    let modules = resolve_modules(entry_path)?;

    let mut parsed_modules = Vec::new();
    let mut all_exports: Vec<ModuleExports> = Vec::new();

    // Parse all modules
    for module in &modules {
        let file = parse_source(&module.source, &module.file_path.to_string_lossy())?;
        parsed_modules.push((module.name.clone(), module.file_path.clone(), file));
    }

    // Type-check in topological order (deps first), collecting exports
    for (_module_name, file_path, file) in &parsed_modules {
        let mut tc = TypeChecker::new();

        // Import signatures from already-checked dependencies
        for exports in &all_exports {
            tc.import_module(exports);
        }

        match tc.check_file(file) {
            Ok(exports) => {
                all_exports.push(exports);
            }
            Err(errors) => {
                render_diagnostics(&errors, &file_path.to_string_lossy(), "");
                return Err(errors);
            }
        }
    }

    // Build global intrinsic map from all modules
    let mut intrinsic_map = std::collections::HashMap::new();
    for (_module_name, _file_path, file) in &parsed_modules {
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
    for (_module_name, _file_path, file) in &parsed_modules {
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
    for (_module_name, _file_path, file) in &parsed_modules {
        let is_program = file.kind == FileKind::Program;
        let tasm = Emitter::new()
            .with_intrinsics(intrinsic_map.clone())
            .with_module_aliases(module_aliases.clone())
            .with_constants(external_constants.clone())
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

fn parse_source_silent(source: &str, filename: &str) -> Result<ast::File, Vec<Diagnostic>> {
    let _ = filename;
    let (tokens, _comments, lex_errors) = Lexer::new(source, 0).tokenize();
    if !lex_errors.is_empty() {
        return Err(lex_errors);
    }
    Parser::new(tokens).parse_file()
}
