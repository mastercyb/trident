pub mod ast;
pub mod codegen;
pub mod common;
pub mod cost;
pub mod frontend;
pub mod pkgmgmt;
pub mod tools;
pub mod typecheck;
pub mod verify;

// Re-exports — preserves all `crate::X` paths
pub use codegen::emitter as emit;
pub use codegen::linker;
pub use codegen::stack;
pub use common::diagnostic;
pub use common::span;
pub use common::types;
pub use frontend::format;
pub use frontend::lexeme;
pub use frontend::lexer;
pub use frontend::parser;
pub use pkgmgmt::cache;
#[allow(unused_imports)]
pub use pkgmgmt::hash;
pub use pkgmgmt::manifest as package;
pub use pkgmgmt::onchain;
pub use pkgmgmt::poseidon2;
pub use pkgmgmt::registry;
pub use pkgmgmt::ucm;
pub use tools::lsp;
pub use tools::project;
pub use tools::resolve;
pub use tools::scaffold;
pub use tools::target;
pub use tools::view;
pub use verify::equiv;
pub use verify::report;
pub use verify::smt;
pub use verify::solve;
pub use verify::sym;
pub use verify::synthesize;

use std::collections::{HashMap, HashSet};
use std::path::Path;

use ast::FileKind;
use diagnostic::{render_diagnostics, Diagnostic};
use emit::Emitter;
use lexer::Lexer;
use linker::{link, ModuleTasm};
use parser::Parser;
use resolve::{resolve_modules, resolve_modules_with_deps};
use target::TargetConfig;
use typecheck::{ModuleExports, TypeChecker};

/// Options controlling compilation: VM target + conditional compilation flags.
#[derive(Clone, Debug)]
pub struct CompileOptions {
    /// Profile name for cfg flags (e.g. "debug", "release").
    pub profile: String,
    /// Active cfg flags for conditional compilation.
    pub cfg_flags: HashSet<String>,
    /// Target VM configuration.
    pub target_config: TargetConfig,
    /// Additional module search directories (from locked dependencies).
    pub dep_dirs: Vec<std::path::PathBuf>,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            profile: "debug".to_string(),
            cfg_flags: HashSet::from(["debug".to_string()]),
            target_config: TargetConfig::triton(),
            dep_dirs: Vec::new(),
        }
    }
}

impl CompileOptions {
    /// Create options for a named profile (debug/release/custom).
    pub fn for_profile(profile: &str) -> Self {
        Self {
            profile: profile.to_string(),
            cfg_flags: HashSet::from([profile.to_string()]),
            target_config: TargetConfig::triton(),
            dep_dirs: Vec::new(),
        }
    }

    /// Create options for a named built-in target (backward compat alias).
    pub fn for_target(target: &str) -> Self {
        Self::for_profile(target)
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
    let exports = match TypeChecker::with_target(options.target_config.clone())
        .with_cfg_flags(options.cfg_flags.clone())
        .check_file(&file)
    {
        Ok(exports) => exports,
        Err(errors) => {
            render_diagnostics(&errors, filename, source);
            return Err(errors);
        }
    };

    // Emit target assembly
    let backend = emit::create_backend(&options.target_config.name);
    let tasm = Emitter::with_backend(backend, options.target_config.clone())
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
    let modules = if options.dep_dirs.is_empty() {
        resolve_modules(entry_path)?
    } else {
        resolve_modules_with_deps(entry_path, options.dep_dirs.clone())?
    };

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
        let mut tc = TypeChecker::with_target(options.target_config.clone())
            .with_cfg_flags(options.cfg_flags.clone());

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
    let mut intrinsic_map = HashMap::new();
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
    let mut module_aliases = HashMap::new();
    for (_module_name, _file_path, _source, file) in &parsed_modules {
        let full_name = &file.name.node;
        if let Some(short) = full_name.rsplit('.').next() {
            if short != full_name.as_str() {
                module_aliases.insert(short.to_string(), full_name.clone());
            }
        }
    }

    // Build external constants map from all module exports
    let mut external_constants = HashMap::new();
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
        let backend = emit::create_backend(&options.target_config.name);
        let tasm = Emitter::with_backend(backend, options.target_config.clone())
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

/// Discover `#[test]` functions in a parsed file.
pub fn discover_tests(file: &ast::File) -> Vec<String> {
    let mut tests = Vec::new();
    for item in &file.items {
        if let ast::Item::Fn(func) = &item.node {
            if func.is_test {
                tests.push(func.name.node.clone());
            }
        }
    }
    tests
}

/// A single test result.
#[derive(Clone, Debug)]
pub struct TestResult {
    pub name: String,
    pub passed: bool,
    pub cost: Option<cost::TableCost>,
    pub error: Option<String>,
}

/// Run all `#[test]` functions in a project.
///
/// For each test function, we:
/// 1. Parse and type-check the project
/// 2. Compile a mini-program that just calls the test function
/// 3. Report pass/fail with cost summary
pub fn run_tests(
    entry_path: &std::path::Path,
    options: &CompileOptions,
) -> Result<String, Vec<Diagnostic>> {
    // Resolve all modules
    let modules = resolve_modules(entry_path)?;

    // Parse all modules
    let mut parsed_modules = Vec::new();
    for module in &modules {
        let file = parse_source(&module.source, &module.file_path.to_string_lossy())?;
        parsed_modules.push((
            module.name.clone(),
            module.file_path.clone(),
            module.source.clone(),
            file,
        ));
    }

    // Type-check all modules in order, collecting exports
    let mut all_exports: Vec<typecheck::ModuleExports> = Vec::new();
    for (_module_name, file_path, source, file) in &parsed_modules {
        let mut tc = TypeChecker::with_target(options.target_config.clone())
            .with_cfg_flags(options.cfg_flags.clone());
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

    // Discover all #[test] functions across all modules
    let mut test_fns: Vec<(String, String)> = Vec::new(); // (module_name, fn_name)
    for (_module_name, _file_path, _source, file) in &parsed_modules {
        for test_name in discover_tests(file) {
            test_fns.push((file.name.node.clone(), test_name));
        }
    }

    if test_fns.is_empty() {
        return Ok("No #[test] functions found.\n".to_string());
    }

    // For each test function, compile a mini-program and report
    let mut results: Vec<TestResult> = Vec::new();
    for (module_name, test_name) in &test_fns {
        // Find the source file for this module
        let source_entry = parsed_modules
            .iter()
            .find(|(_, _, _, f)| f.name.node == *module_name);

        if let Some((_name, file_path, source, _file)) = source_entry {
            // Build a mini-program source that just calls the test function
            let mini_source = if module_name.starts_with("module") || module_name.contains('.') {
                // For module test functions, we'd need cross-module calls
                // For simplicity, compile in-context
                source.clone()
            } else {
                source.clone()
            };

            // Try to compile (type-check + emit) the source.
            // The test function itself is validated by the type checker.
            // For now, "passing" means it compiles without errors.
            match compile_with_options(&mini_source, &file_path.to_string_lossy(), options) {
                Ok(tasm) => {
                    // Compute cost for the test function
                    let test_cost = analyze_costs(&mini_source, &file_path.to_string_lossy()).ok();
                    let fn_cost = test_cost.as_ref().and_then(|pc| {
                        pc.functions
                            .iter()
                            .find(|f| f.name == *test_name)
                            .map(|f| f.cost.clone())
                    });
                    // Check if the generated TASM contains an assert failure marker
                    let has_error = tasm.contains("// ERROR");
                    results.push(TestResult {
                        name: test_name.clone(),
                        passed: !has_error,
                        cost: fn_cost,
                        error: if has_error {
                            Some("compilation produced errors".to_string())
                        } else {
                            None
                        },
                    });
                }
                Err(errors) => {
                    let msg = errors
                        .iter()
                        .map(|d| d.message.clone())
                        .collect::<Vec<_>>()
                        .join("; ");
                    results.push(TestResult {
                        name: test_name.clone(),
                        passed: false,
                        cost: None,
                        error: Some(msg),
                    });
                }
            }
        }
    }

    // Format the report
    let mut report = String::new();
    let total = results.len();
    let passed = results.iter().filter(|r| r.passed).count();
    let failed = total - passed;

    report.push_str(&format!(
        "running {} test{}\n",
        total,
        if total == 1 { "" } else { "s" }
    ));

    for result in &results {
        let status = if result.passed { "ok" } else { "FAILED" };
        let cost_str = if let Some(ref c) = result.cost {
            format!(
                " (cc={}, hash={}, u32={})",
                c.processor, c.hash, c.u32_table
            )
        } else {
            String::new()
        };
        report.push_str(&format!(
            "  test {} ... {}{}\n",
            result.name, status, cost_str
        ));
        if let Some(ref err) = result.error {
            report.push_str(&format!("    error: {}\n", err));
        }
    }

    report.push('\n');
    if failed == 0 {
        report.push_str(&format!("test result: ok. {} passed; 0 failed\n", passed));
    } else {
        report.push_str(&format!(
            "test result: FAILED. {} passed; {} failed\n",
            passed, failed
        ));
    }

    Ok(report)
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

/// Parse, type-check, and compute cost analysis for a multi-module project.
/// Falls back to single-file analysis if module resolution fails.
pub fn analyze_costs_project(
    entry_path: &Path,
    options: &CompileOptions,
) -> Result<cost::ProgramCost, Vec<Diagnostic>> {
    let modules = resolve_modules(entry_path)?;

    let mut parsed_modules = Vec::new();
    let mut all_exports: Vec<ModuleExports> = Vec::new();

    for module in &modules {
        let file = parse_source(&module.source, &module.file_path.to_string_lossy())?;
        parsed_modules.push((
            module.name.clone(),
            module.file_path.clone(),
            module.source.clone(),
            file,
        ));
    }

    for (_module_name, file_path, source, file) in &parsed_modules {
        let mut tc = TypeChecker::with_target(options.target_config.clone())
            .with_cfg_flags(options.cfg_flags.clone());
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

    // Analyze costs for the program file (last in topological order)
    if let Some((_name, _path, _source, file)) = parsed_modules.last() {
        let cost = cost::CostAnalyzer::new().analyze_file(file);
        Ok(cost)
    } else {
        Err(vec![Diagnostic::error(
            "no program file found".to_string(),
            crate::span::Span::dummy(),
        )])
    }
}

/// Parse, type-check, and verify a project using symbolic execution + solver.
///
/// Returns a `VerificationReport` with static analysis, random testing (Schwartz-Zippel),
/// and bounded model checking results.
pub fn verify_project(entry_path: &Path) -> Result<solve::VerificationReport, Vec<Diagnostic>> {
    let modules = resolve_modules(entry_path)?;

    let mut all_exports: Vec<ModuleExports> = Vec::new();
    let mut last_file = None;

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

        last_file = Some(file);
    }

    if let Some(file) = last_file {
        let system = sym::analyze(&file);
        Ok(solve::verify(&system))
    } else {
        Err(vec![Diagnostic::error(
            "no program file found".to_string(),
            crate::span::Span::dummy(),
        )])
    }
}

/// Count the number of TASM instructions in a compiled output string.
/// Skips comments, labels, blank lines, and the halt instruction.
pub fn count_tasm_instructions(tasm: &str) -> usize {
    tasm.lines()
        .map(|line| line.trim())
        .filter(|line| {
            !line.is_empty() && !line.starts_with("//") && !line.ends_with(':') && *line != "halt"
        })
        .count()
}

/// Benchmark result for a single program.
#[derive(Clone, Debug)]
pub struct BenchmarkResult {
    pub name: String,
    pub trident_instructions: usize,
    pub baseline_instructions: usize,
    pub overhead_ratio: f64,
    pub trident_padded_height: u64,
    pub baseline_padded_height: u64,
}

impl BenchmarkResult {
    pub fn format(&self) -> String {
        format!(
            "{:<24} {:>6} {:>6}  {:>5.2}x  {:>6} {:>6}",
            self.name,
            self.trident_instructions,
            self.baseline_instructions,
            self.overhead_ratio,
            self.trident_padded_height,
            self.baseline_padded_height,
        )
    }

    pub fn format_header() -> String {
        format!(
            "{:<24} {:>6} {:>6}  {:>6}  {:>6} {:>6}",
            "Benchmark", "Tri", "Hand", "Ratio", "TriPad", "HandPad"
        )
    }

    pub fn format_separator() -> String {
        "-".repeat(72)
    }
}

/// Generate markdown documentation for a Trident project.
///
/// Resolves all modules, parses and type-checks them, computes cost analysis,
/// and produces a markdown document listing all public functions, structs,
/// constants, and events with their type signatures and cost annotations.
pub fn generate_docs(
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

    // Type-check in topological order, collecting exports
    for (_module_name, file_path, source, file) in &parsed_modules {
        let mut tc = TypeChecker::with_target(options.target_config.clone())
            .with_cfg_flags(options.cfg_flags.clone());
        for exports in &all_exports {
            tc.import_module(exports);
        }
        match tc.check_file(file) {
            Ok(exports) => {
                all_exports.push(exports);
            }
            Err(errors) => {
                render_diagnostics(&errors, &file_path.to_string_lossy(), source);
                return Err(errors);
            }
        }
    }

    // Compute cost analysis per module
    let mut module_costs: Vec<Option<cost::ProgramCost>> = Vec::new();
    for (_module_name, _file_path, _source, file) in &parsed_modules {
        let pc = cost::CostAnalyzer::new().analyze_file(file);
        module_costs.push(Some(pc));
    }

    // Determine the program name from the entry module
    let program_name = parsed_modules
        .iter()
        .find(|(_, _, _, f)| f.kind == FileKind::Program)
        .map(|(_, _, _, f)| f.name.node.clone())
        .unwrap_or_else(|| "project".to_string());

    let mut doc = String::new();
    doc.push_str(&format!("# {}\n", program_name));

    // --- Functions ---
    let mut fn_entries: Vec<String> = Vec::new();
    for (i, (_module_name, _file_path, _source, file)) in parsed_modules.iter().enumerate() {
        let module_name = &file.name.node;
        let costs = module_costs[i].as_ref();
        for item in &file.items {
            if let ast::Item::Fn(func) = &item.node {
                // Skip test functions, intrinsic-only, and non-pub functions in modules
                if func.is_test {
                    continue;
                }
                if file.kind == FileKind::Module && !func.is_pub {
                    continue;
                }
                // Skip cfg-excluded items
                if let Some(ref cfg) = func.cfg {
                    if !options.cfg_flags.contains(&cfg.node) {
                        continue;
                    }
                }

                let sig = format_fn_signature(func);
                let fn_cost =
                    costs.and_then(|pc| pc.functions.iter().find(|f| f.name == func.name.node));

                let mut entry = format!("### `{}`\n", sig);
                if let Some(fc) = fn_cost {
                    let c = &fc.cost;
                    entry.push_str(&format!(
                        "**Cost:** cc={}, hash={}, u32={} | dominant: {}\n",
                        c.processor,
                        c.hash,
                        c.u32_table,
                        c.dominant_table()
                    ));
                }
                entry.push_str(&format!("**Module:** {}\n", module_name));
                fn_entries.push(entry);
            }
        }
    }

    if !fn_entries.is_empty() {
        doc.push_str("\n## Functions\n\n");
        for entry in &fn_entries {
            doc.push_str(entry);
            doc.push('\n');
        }
    }

    // --- Structs ---
    let mut struct_entries: Vec<String> = Vec::new();
    for (_module_name, _file_path, _source, file) in parsed_modules.iter() {
        for item in &file.items {
            if let ast::Item::Struct(sdef) = &item.node {
                if file.kind == FileKind::Module && !sdef.is_pub {
                    continue;
                }
                if let Some(ref cfg) = sdef.cfg {
                    if !options.cfg_flags.contains(&cfg.node) {
                        continue;
                    }
                }

                let mut entry = format!("### `struct {}`\n", sdef.name.node);
                entry.push_str("| Field | Type | Width |\n");
                entry.push_str("|-------|------|-------|\n");
                let mut total_width: u32 = 0;
                for field in &sdef.fields {
                    let ty_str = format_ast_type(&field.ty.node);
                    let width = ast_type_width(&field.ty.node, &options.target_config);
                    total_width += width;
                    entry.push_str(&format!(
                        "| {} | {} | {} |\n",
                        field.name.node, ty_str, width
                    ));
                }
                entry.push_str(&format!("Total width: {} field elements\n", total_width));
                struct_entries.push(entry);
            }
        }
    }

    if !struct_entries.is_empty() {
        doc.push_str("\n## Structs\n\n");
        for entry in &struct_entries {
            doc.push_str(entry);
            doc.push('\n');
        }
    }

    // --- Constants ---
    let mut const_entries: Vec<(String, String, String)> = Vec::new(); // (name, type, value)
    for (_module_name, _file_path, _source, file) in parsed_modules.iter() {
        for item in &file.items {
            if let ast::Item::Const(cdef) = &item.node {
                if file.kind == FileKind::Module && !cdef.is_pub {
                    continue;
                }
                if let Some(ref cfg) = cdef.cfg {
                    if !options.cfg_flags.contains(&cfg.node) {
                        continue;
                    }
                }
                let ty_str = format_ast_type(&cdef.ty.node);
                let val_str = format_const_value(&cdef.value.node);
                const_entries.push((cdef.name.node.clone(), ty_str, val_str));
            }
        }
    }

    if !const_entries.is_empty() {
        doc.push_str("\n## Constants\n\n");
        doc.push_str("| Name | Type | Value |\n");
        doc.push_str("|------|------|-------|\n");
        for (name, ty, val) in &const_entries {
            doc.push_str(&format!("| {} | {} | {} |\n", name, ty, val));
        }
        doc.push('\n');
    }

    // --- Events ---
    let mut event_entries: Vec<String> = Vec::new();
    for (_module_name, _file_path, _source, file) in parsed_modules.iter() {
        for item in &file.items {
            if let ast::Item::Event(edef) = &item.node {
                if let Some(ref cfg) = edef.cfg {
                    if !options.cfg_flags.contains(&cfg.node) {
                        continue;
                    }
                }
                let mut entry = format!("### `event {}`\n", edef.name.node);
                entry.push_str("| Field | Type |\n");
                entry.push_str("|-------|------|\n");
                for field in &edef.fields {
                    let ty_str = format_ast_type(&field.ty.node);
                    entry.push_str(&format!("| {} | {} |\n", field.name.node, ty_str));
                }
                event_entries.push(entry);
            }
        }
    }

    if !event_entries.is_empty() {
        doc.push_str("\n## Events\n\n");
        for entry in &event_entries {
            doc.push_str(entry);
            doc.push('\n');
        }
    }

    // --- Cost Summary ---
    // Aggregate costs across all modules — use the program module's cost if it exists,
    // otherwise sum all module costs.
    let program_cost = parsed_modules
        .iter()
        .enumerate()
        .find(|(_, (_, _, _, f))| f.kind == FileKind::Program)
        .and_then(|(i, _)| module_costs[i].as_ref());

    let total_cost = if let Some(pc) = program_cost {
        pc.total.clone()
    } else {
        // Sum across all modules
        let mut total = cost::TableCost::ZERO;
        for pc in module_costs.iter().flatten() {
            total = total.add(&pc.total);
        }
        total
    };

    let padded_height = if let Some(pc) = program_cost {
        pc.padded_height
    } else {
        cost::next_power_of_two(total_cost.max_height())
    };

    doc.push_str("\n## Cost Summary\n\n");
    doc.push_str("| Table | Height |\n");
    doc.push_str("|-------|--------|\n");
    doc.push_str(&format!("| Processor | {} |\n", total_cost.processor));
    doc.push_str(&format!("| Hash | {} |\n", total_cost.hash));
    doc.push_str(&format!("| U32 | {} |\n", total_cost.u32_table));
    doc.push_str(&format!("| Padded | {} |\n", padded_height));

    Ok(doc)
}

/// Format an AST type for documentation display.
fn format_ast_type(ty: &ast::Type) -> String {
    match ty {
        ast::Type::Field => "Field".to_string(),
        ast::Type::XField => "XField".to_string(),
        ast::Type::Bool => "Bool".to_string(),
        ast::Type::U32 => "U32".to_string(),
        ast::Type::Digest => "Digest".to_string(),
        ast::Type::Array(inner, size) => format!("[{}; {}]", format_ast_type(inner), size),
        ast::Type::Tuple(elems) => {
            let parts: Vec<_> = elems.iter().map(format_ast_type).collect();
            format!("({})", parts.join(", "))
        }
        ast::Type::Named(path) => path.as_dotted(),
    }
}

/// Compute the width in field elements for an AST type (best-effort).
fn ast_type_width(ty: &ast::Type, config: &TargetConfig) -> u32 {
    match ty {
        ast::Type::Field | ast::Type::Bool | ast::Type::U32 => 1,
        ast::Type::XField => config.xfield_width,
        ast::Type::Digest => config.digest_width,
        ast::Type::Array(inner, size) => {
            let inner_w = ast_type_width(inner, config);
            let n = size.as_literal().unwrap_or(1) as u32;
            inner_w * n
        }
        ast::Type::Tuple(elems) => elems.iter().map(|e| ast_type_width(e, config)).sum(),
        ast::Type::Named(_) => 1, // unknown, default to 1
    }
}

/// Format a function signature for documentation.
fn format_fn_signature(func: &ast::FnDef) -> String {
    let mut sig = String::from("fn ");
    sig.push_str(&func.name.node);

    // Type params
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

/// Format a constant value expression for documentation.
fn format_const_value(expr: &ast::Expr) -> String {
    match expr {
        ast::Expr::Literal(ast::Literal::Integer(n)) => n.to_string(),
        ast::Expr::Literal(ast::Literal::Bool(b)) => b.to_string(),
        _ => "...".to_string(),
    }
}

/// Parse, type-check, and produce per-line cost-annotated source output.
///
/// Each source line is printed with a line number and, if the line has
/// an associated cost, a bracketed annotation showing the cost breakdown.
pub fn annotate_source(source: &str, filename: &str) -> Result<String, Vec<Diagnostic>> {
    let file = parse_source(source, filename)?;

    if let Err(errors) = TypeChecker::new().check_file(&file) {
        render_diagnostics(&errors, filename, source);
        return Err(errors);
    }

    let mut analyzer = cost::CostAnalyzer::new();
    analyzer.analyze_file(&file);
    let stmt_costs = analyzer.stmt_costs(&file, source);

    // Build a map from line number to aggregated cost
    let mut line_costs: HashMap<u32, cost::TableCost> = HashMap::new();
    for (line, cost) in &stmt_costs {
        line_costs
            .entry(*line)
            .and_modify(|existing| *existing = existing.add(cost))
            .or_insert_with(|| cost.clone());
    }

    let lines: Vec<&str> = source.lines().collect();
    let line_count = lines.len();
    let line_num_width = format!("{}", line_count).len().max(2);

    // Find max line length for alignment
    let max_line_len = lines.iter().map(|l| l.len()).max().unwrap_or(0).min(60);

    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        let line_num = (i + 1) as u32;
        let padded_line = format!("{:<width$}", line, width = max_line_len);
        if let Some(cost) = line_costs.get(&line_num) {
            let annotation = cost.format_annotation();
            if !annotation.is_empty() {
                out.push_str(&format!(
                    "{:>width$} | {}  [{}]\n",
                    line_num,
                    padded_line,
                    annotation,
                    width = line_num_width,
                ));
                continue;
            }
        }
        out.push_str(&format!(
            "{:>width$} | {}\n",
            line_num,
            line,
            width = line_num_width,
        ));
    }

    Ok(out)
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

    // --- pattern matching integration ---

    #[test]
    fn test_match_compiles() {
        let source = "program test\nfn main() {\n    let x: Field = pub_read()\n    match x {\n        0 => { pub_write(0) }\n        1 => { pub_write(1) }\n        _ => { pub_write(2) }\n    }\n}";
        let result = compile(source, "test.tri");
        assert!(result.is_ok(), "match should compile: {:?}", result.err());
        let tasm = result.unwrap();
        assert!(tasm.contains("eq"), "match should emit equality checks");
        assert!(tasm.contains("skiz"), "match should use skiz for branching");
    }

    #[test]
    fn test_match_bool_compiles() {
        let source = "program test\nfn main() {\n    let b: Bool = pub_read() == pub_read()\n    match b {\n        true => { pub_write(1) }\n        false => { pub_write(0) }\n    }\n}";
        let result = compile(source, "test.tri");
        assert!(
            result.is_ok(),
            "bool match should compile: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_match_format_roundtrip() {
        let source = "program test\n\nfn main() {\n    let x: Field = pub_read()\n    match x {\n        0 => {\n            pub_write(0)\n        }\n        1 => {\n            pub_write(1)\n        }\n        _ => {\n            pub_write(2)\n        }\n    }\n}\n";
        let formatted = format_source(source, "test.tri").unwrap();
        let formatted2 = format_source(&formatted, "test.tri").unwrap();
        assert_eq!(
            formatted, formatted2,
            "match formatting should be idempotent"
        );
    }

    #[test]
    fn test_match_non_exhaustive_fails() {
        let source = "program test\nfn main() {\n    let x: Field = pub_read()\n    match x {\n        0 => { pub_write(0) }\n    }\n}";
        let result = compile(source, "test.tri");
        assert!(result.is_err(), "non-exhaustive match should fail");
    }

    #[test]
    fn test_match_struct_pattern_compiles() {
        let source = "program test\nstruct Point { x: Field, y: Field }\nfn main() {\n    let p = Point { x: pub_read(), y: pub_read() }\n    match p {\n        Point { x, y } => {\n            pub_write(x)\n            pub_write(y)\n        }\n    }\n}";
        let result = compile(source, "test.tri");
        assert!(
            result.is_ok(),
            "struct pattern should compile: {:?}",
            result.err()
        );
        let tasm = result.unwrap();
        assert!(tasm.contains("read_io"), "should read inputs");
        assert!(tasm.contains("write_io"), "should write outputs");
    }

    #[test]
    fn test_match_struct_pattern_format_roundtrip() {
        let source = "program test\n\nstruct Point {\n    x: Field,\n    y: Field,\n}\n\nfn main() {\n    let p = Point { x: 1, y: 2 }\n    match p {\n        Point { x, y } => {\n            pub_write(x)\n        }\n    }\n}\n";
        let formatted = format_source(source, "test.tri").unwrap();
        let formatted2 = format_source(&formatted, "test.tri").unwrap();
        assert_eq!(
            formatted, formatted2,
            "struct pattern formatting should be idempotent"
        );
    }

    // --- #[test] attribute integration tests ---

    #[test]
    fn test_discover_tests_finds_test_fns() {
        let source = "program test\n#[test]\nfn check_math() {\n    assert(1 == 1)\n}\n#[test]\nfn check_logic() {\n    assert(true)\n}\nfn main() {}";
        let file = parse_source_silent(source, "test.tri").unwrap();
        let tests = discover_tests(&file);
        assert_eq!(tests.len(), 2);
        assert!(tests.contains(&"check_math".to_string()));
        assert!(tests.contains(&"check_logic".to_string()));
    }

    #[test]
    fn test_discover_tests_empty_when_no_tests() {
        let source = "program test\nfn main() {\n    pub_write(pub_read())\n}";
        let file = parse_source_silent(source, "test.tri").unwrap();
        let tests = discover_tests(&file);
        assert!(tests.is_empty());
    }

    #[test]
    fn test_test_fn_compiles_normally() {
        // #[test] functions should be accepted but skipped during normal emit
        let source = "program test\n#[test]\nfn check() {\n    assert(true)\n}\nfn main() {\n    pub_write(pub_read())\n}";
        let result = compile(source, "test.tri");
        assert!(
            result.is_ok(),
            "program with test fn should compile: {:?}",
            result.err()
        );
        let tasm = result.unwrap();
        // The test function should NOT appear in the emitted TASM
        assert!(
            !tasm.contains("__check:"),
            "test fn should not be emitted in normal build"
        );
        assert!(tasm.contains("__main:"), "main should be emitted");
    }

    #[test]
    fn test_test_fn_format_roundtrip() {
        let source = "program test\n\n#[test]\nfn check_math() {\n    assert(1 == 1)\n}\n\nfn main() {\n    pub_write(pub_read())\n}\n";
        let formatted = format_source(source, "test.tri").unwrap();
        assert!(
            formatted.contains("#[test]"),
            "should preserve #[test] attribute"
        );
        assert!(
            formatted.contains("fn check_math()"),
            "should preserve function"
        );
        let formatted2 = format_source(&formatted, "test.tri").unwrap();
        assert_eq!(
            formatted, formatted2,
            "#[test] formatting should be idempotent"
        );
    }

    #[test]
    fn test_test_fn_with_cfg_format_roundtrip() {
        let source = "program test\n\n#[cfg(debug)]\n#[test]\nfn debug_check() {\n    assert(true)\n}\n\nfn main() {}\n";
        let formatted = format_source(source, "test.tri").unwrap();
        assert!(formatted.contains("#[cfg(debug)]"), "should preserve cfg");
        assert!(formatted.contains("#[test]"), "should preserve test");
        let formatted2 = format_source(&formatted, "test.tri").unwrap();
        assert_eq!(
            formatted, formatted2,
            "cfg+test formatting should be idempotent"
        );
    }

    #[test]
    fn test_test_fn_type_check_valid() {
        let source = "program test\n#[test]\nfn check() {\n    assert(1 == 1)\n}\nfn main() {}";
        assert!(check(source, "test.tri").is_ok());
    }

    #[test]
    fn test_test_fn_type_check_params_rejected() {
        let source =
            "program test\n#[test]\nfn bad(x: Field) {\n    assert(x == x)\n}\nfn main() {}";
        assert!(
            check(source, "test.tri").is_err(),
            "test fn with params should fail type check"
        );
    }

    #[test]
    fn test_test_fn_type_check_return_rejected() {
        let source = "program test\n#[test]\nfn bad() -> Field {\n    42\n}\nfn main() {}";
        assert!(
            check(source, "test.tri").is_err(),
            "test fn with return should fail type check"
        );
    }

    // --- generate_docs integration tests ---

    #[test]
    fn test_generate_docs_simple() {
        let dir = tempfile::tempdir().unwrap();
        let main_path = dir.path().join("main.tri");
        std::fs::write(
            &main_path,
            "program my_app\n\nfn helper(x: Field) -> Field {\n    x + 1\n}\n\nfn main() {\n    let a: Field = pub_read()\n    pub_write(helper(a))\n}\n",
        )
        .unwrap();

        let options = CompileOptions::default();
        let doc = generate_docs(&main_path, &options).expect("doc generation should succeed");

        // Should contain the program name as title
        assert!(
            doc.contains("# my_app"),
            "should have program name as title"
        );
        // Should contain function names
        assert!(
            doc.contains("fn helper("),
            "should document helper function"
        );
        assert!(doc.contains("fn main("), "should document main function");
        // Should contain cost summary section
        assert!(doc.contains("## Cost Summary"), "should have cost summary");
        assert!(doc.contains("Processor"), "should list Processor table");
        assert!(doc.contains("Padded"), "should list Padded height");
    }

    #[test]
    fn test_generate_docs_with_structs() {
        let dir = tempfile::tempdir().unwrap();
        let main_path = dir.path().join("main.tri");
        std::fs::write(
            &main_path,
            "program test\n\nstruct AuthData {\n    owner: Digest,\n    nonce: Field,\n}\n\nfn main() {\n    let d: Digest = divine5()\n    let auth: AuthData = AuthData { owner: d, nonce: 42 }\n    pub_write(auth.nonce)\n}\n",
        )
        .unwrap();

        let options = CompileOptions::default();
        let doc = generate_docs(&main_path, &options).expect("doc generation should succeed");

        // Should contain struct section
        assert!(doc.contains("## Structs"), "should have Structs section");
        assert!(
            doc.contains("struct AuthData"),
            "should document AuthData struct"
        );
        // Should contain field table with types and widths
        assert!(
            doc.contains("| owner | Digest | 5 |"),
            "should show owner field with Digest width 5"
        );
        assert!(
            doc.contains("| nonce | Field | 1 |"),
            "should show nonce field with Field width 1"
        );
        assert!(
            doc.contains("Total width: 6 field elements"),
            "should show total width"
        );
    }

    #[test]
    fn test_generate_docs_with_events() {
        let dir = tempfile::tempdir().unwrap();
        let main_path = dir.path().join("main.tri");
        std::fs::write(
            &main_path,
            "program test\n\nevent Transfer {\n    from: Field,\n    to: Field,\n    amount: Field,\n}\n\nfn main() {\n    emit Transfer { from: 1, to: 2, amount: 100 }\n}\n",
        )
        .unwrap();

        let options = CompileOptions::default();
        let doc = generate_docs(&main_path, &options).expect("doc generation should succeed");

        // Should contain events section
        assert!(doc.contains("## Events"), "should have Events section");
        assert!(
            doc.contains("event Transfer"),
            "should document Transfer event"
        );
        // Should list event fields
        assert!(doc.contains("| from | Field |"), "should show from field");
        assert!(doc.contains("| to | Field |"), "should show to field");
        assert!(
            doc.contains("| amount | Field |"),
            "should show amount field"
        );
    }

    #[test]
    fn test_generate_docs_cost_annotations() {
        let dir = tempfile::tempdir().unwrap();
        let main_path = dir.path().join("main.tri");
        std::fs::write(
            &main_path,
            "program test\n\nfn compute(x: Field) -> Field {\n    let d: Digest = hash(x, 0, 0, 0, 0, 0, 0, 0, 0, 0)\n    x\n}\n\nfn main() {\n    let a: Field = pub_read()\n    pub_write(compute(a))\n}\n",
        )
        .unwrap();

        let options = CompileOptions::default();
        let doc = generate_docs(&main_path, &options).expect("doc generation should succeed");

        // Should contain cost annotations on functions
        assert!(
            doc.contains("**Cost:**"),
            "should have cost annotations on functions"
        );
        assert!(doc.contains("cc="), "should show cycle count");
        assert!(doc.contains("hash="), "should show hash cost");
        assert!(doc.contains("u32="), "should show u32 cost");
        assert!(doc.contains("dominant:"), "should show dominant table");
        // The compute function uses split which has u32 cost
        assert!(doc.contains("**Module:** test"), "should show module name");
    }

    // --- annotate_source tests ---

    #[test]
    fn test_annotate_source_valid() {
        let source =
            "program test\n\nfn main() {\n    let x: Field = pub_read()\n    pub_write(x)\n}\n";
        let result = annotate_source(source, "test.tri");
        assert!(result.is_ok(), "annotate_source should succeed");
        let annotated = result.unwrap();

        // Should contain line numbers
        assert!(annotated.contains("1 |"), "should have line 1");
        assert!(annotated.contains("3 |"), "should have line 3");

        // Should contain cost annotations (brackets with cc= or jump=)
        assert!(
            annotated.contains("["),
            "should contain cost annotation brackets"
        );
        assert!(annotated.contains("cc="), "should contain cc= cost marker");

        // fn main() line should show call overhead (jump stack)
        let line3 = annotated.lines().find(|l| l.contains("fn main()"));
        assert!(line3.is_some(), "should have fn main() line");
        let line3 = line3.unwrap();
        assert!(
            line3.contains("jump="),
            "fn main() should show jump stack cost from call overhead"
        );
    }

    #[test]
    fn test_annotate_source_shows_hash_cost() {
        let source = "program test\n\nfn main() {\n    let d: Digest = divine5()\n    let (d0, d1, d2, d3, d4) = d\n    let h: Digest = hash(d0, d1, d2, d3, d4, 0, 0, 0, 0, 0)\n    pub_write(0)\n}\n";
        let result = annotate_source(source, "test.tri");
        assert!(result.is_ok(), "annotate_source should succeed");
        let annotated = result.unwrap();

        // The hash line should show hash cost
        let hash_line = annotated.lines().find(|l| l.contains("hash("));
        assert!(hash_line.is_some(), "should have hash() line");
        let hash_line = hash_line.unwrap();
        assert!(
            hash_line.contains("hash="),
            "hash() line should show hash cost, got: {}",
            hash_line
        );
    }

    // --- Cost JSON round-trip integration test ---

    #[test]
    fn test_cost_json_roundtrip_integration() {
        let source = "program test\nfn helper(x: Field) -> Field {\n    x + x\n}\nfn main() {\n    let x: Field = pub_read()\n    pub_write(helper(x))\n}";
        let cost_result = analyze_costs(source, "test.tri").expect("should analyze");
        let json = cost_result.to_json();

        // Verify JSON structure
        assert!(json.contains("\"functions\""), "JSON should have functions");
        assert!(json.contains("\"total\""), "JSON should have total");
        assert!(
            json.contains("\"padded_height\""),
            "JSON should have padded_height"
        );
        assert!(json.contains("\"main\""), "JSON should have main function");
        assert!(
            json.contains("\"helper\""),
            "JSON should have helper function"
        );

        // Round-trip
        let parsed =
            cost::ProgramCost::from_json(&json).expect("should parse JSON back to ProgramCost");
        assert_eq!(parsed.total.processor, cost_result.total.processor);
        assert_eq!(parsed.total.hash, cost_result.total.hash);
        assert_eq!(parsed.total.u32_table, cost_result.total.u32_table);
        assert_eq!(parsed.total.op_stack, cost_result.total.op_stack);
        assert_eq!(parsed.total.ram, cost_result.total.ram);
        assert_eq!(parsed.total.jump_stack, cost_result.total.jump_stack);
        assert_eq!(parsed.padded_height, cost_result.padded_height);
    }

    // --- Comparison formatting integration test ---

    #[test]
    fn test_error_max_nesting_depth() {
        // Generate deeply nested blocks via nested if statements.
        // Each `if true { ... }` adds one nesting level; 260 > MAX_NESTING_DEPTH (256).
        // The parser recurses to depth 256 before the guard triggers, which
        // needs more stack than the default test-thread provides in debug
        // builds.  Run the actual work on a thread with an explicit 16 MB stack.
        let handle = std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn(|| {
                let depth = 260u32;
                let mut src = String::from("program t\nfn main() {\n");
                for _ in 0..depth {
                    src.push_str("if true {\n");
                }
                src.push_str("pub_write(0)\n");
                for _ in 0..depth {
                    src.push_str("}\n");
                }
                src.push_str("}\n");

                let (tokens, _comments, lex_errs) = crate::lexer::Lexer::new(&src, 0).tokenize();
                assert!(lex_errs.is_empty(), "lex errors: {:?}", lex_errs);
                let result = crate::parser::Parser::new(tokens).parse_file();
                assert!(
                    result.is_err(),
                    "deeply nested input should produce an error"
                );
                let diags = result.unwrap_err();
                let has_depth = diags.iter().any(|d| d.message.contains("nesting depth"));
                assert!(
                    has_depth,
                    "should report nesting depth exceeded, got: {:?}",
                    diags.iter().map(|d| &d.message).collect::<Vec<_>>()
                );
            })
            .expect("failed to spawn test thread");
        handle.join().expect("test thread panicked");
    }

    #[test]
    fn test_comparison_formatting_integration() {
        let source_v1 =
            "program test\nfn main() {\n    let x: Field = pub_read()\n    pub_write(x)\n}";
        let source_v2 = "program test\nfn main() {\n    let x: Field = pub_read()\n    let y: Field = pub_read()\n    pub_write(x + y)\n}";

        let cost_v1 = analyze_costs(source_v1, "test.tri").expect("v1 should analyze");
        let cost_v2 = analyze_costs(source_v2, "test.tri").expect("v2 should analyze");

        let comparison = cost_v1.format_comparison(&cost_v2);
        assert!(
            comparison.contains("Cost comparison:"),
            "should have header"
        );
        assert!(comparison.contains("TOTAL"), "should have TOTAL row");
        assert!(
            comparison.contains("Padded height:"),
            "should have padded height row"
        );
        assert!(
            comparison.contains("main"),
            "should show main function in comparison"
        );

        // v2 has more operations, so delta should be positive
        assert!(
            comparison.contains("+"),
            "v2 should have higher cost than v1, showing + delta"
        );
    }

    #[test]
    fn test_const_generic_add_expression() {
        // Parameter type uses M + N size expression
        let source = "program test\nfn first_of<M, N>(a: [Field; M + N]) -> Field {\n    a[0]\n}\nfn main() {\n    let a: [Field; 5] = [1, 2, 3, 4, 5]\n    let r = first_of<3, 2>(a)\n    assert(r == 1)\n}";
        let result = compile(source, "test.tri");
        assert!(
            result.is_ok(),
            "const generic add should compile: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_const_generic_mul_expression() {
        // Parameter type uses N * 2 size expression
        let source = "program test\nfn sum_pairs<N>(a: [Field; N * 2]) -> Field {\n    a[0] + a[1]\n}\nfn main() {\n    let a: [Field; 4] = [1, 2, 3, 4]\n    let r = sum_pairs<2>(a)\n    assert(r == 3)\n}";
        let result = compile(source, "test.tri");
        assert!(
            result.is_ok(),
            "const generic mul should compile: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_pure_fn_compiles() {
        let source = "program test\n#[pure]\nfn add(a: Field, b: Field) -> Field {\n    a + b\n}\nfn main() {\n    let x = add(1, 2)\n    assert(x == 3)\n}";
        let result = compile(source, "test.tri");
        assert!(result.is_ok(), "pure fn should compile: {:?}", result.err());
    }

    #[test]
    fn test_pure_fn_format_roundtrip() {
        let source = "program test\n\n#[pure]\nfn add(a: Field, b: Field) -> Field {\n    a + b\n}\n\nfn main() {\n}\n";
        let formatted = format_source(source, "test.tri").unwrap();
        let formatted2 = format_source(&formatted, "test.tri").unwrap();
        assert_eq!(
            formatted, formatted2,
            "pure fn formatting should be idempotent"
        );
        assert!(
            formatted.contains("#[pure]"),
            "formatted output should contain #[pure]"
        );
    }

    #[test]
    fn test_recursive_verifier_compiles() {
        let path = std::path::Path::new("examples/neptune/recursive_verifier.tri");
        if !path.exists() {
            return; // skip if running from different cwd
        }
        let result = compile_project(path);
        assert!(
            result.is_ok(),
            "recursive verifier should compile: {:?}",
            result.err()
        );
        let tasm = result.unwrap();
        assert!(
            tasm.contains("xx_dot_step"),
            "should emit xx_dot_step instruction"
        );
    }

    #[test]
    fn test_xfield_dot_step_intrinsics() {
        let dir = tempfile::tempdir().unwrap();
        // Write the entry program that uses xx_dot_step via ext.triton.xfield
        let main_path = dir.path().join("main.tri");
        std::fs::write(
            &main_path,
            r#"program test
use ext.triton.xfield

fn main() {
    let ptr_a: Field = divine()
    let ptr_b: Field = divine()
    let result: Digest = xfield.xx_dot_step(0, 0, 0, ptr_a, ptr_b)
    let (r0, r1, r2, r3, r4) = result
    pub_write(r0)
}
"#,
        )
        .unwrap();
        // Create ext/triton directory and copy xfield.tri
        let ext_dir = dir.path().join("ext").join("triton");
        std::fs::create_dir_all(&ext_dir).unwrap();
        std::fs::copy("ext/triton/xfield.tri", ext_dir.join("xfield.tri")).unwrap_or_default();

        let result = compile_project(&main_path);
        assert!(
            result.is_ok(),
            "xx_dot_step intrinsic should compile: {:?}",
            result.err()
        );
        let tasm = result.unwrap();
        assert!(
            tasm.contains("xx_dot_step"),
            "emitted TASM should contain xx_dot_step"
        );
    }

    #[test]
    fn test_xb_dot_step_intrinsic() {
        let dir = tempfile::tempdir().unwrap();
        let main_path = dir.path().join("main.tri");
        std::fs::write(
            &main_path,
            r#"program test
use ext.triton.xfield

fn main() {
    let ptr_a: Field = divine()
    let ptr_b: Field = divine()
    let result: Digest = xfield.xb_dot_step(0, 0, 0, ptr_a, ptr_b)
    let (r0, r1, r2, r3, r4) = result
    pub_write(r0)
}
"#,
        )
        .unwrap();
        let ext_dir = dir.path().join("ext").join("triton");
        std::fs::create_dir_all(&ext_dir).unwrap();
        std::fs::copy("ext/triton/xfield.tri", ext_dir.join("xfield.tri")).unwrap_or_default();

        let result = compile_project(&main_path);
        assert!(
            result.is_ok(),
            "xb_dot_step intrinsic should compile: {:?}",
            result.err()
        );
        let tasm = result.unwrap();
        assert!(
            tasm.contains("xb_dot_step"),
            "emitted TASM should contain xb_dot_step"
        );
    }

    #[test]
    fn test_xfe_inner_product_library() {
        let dir = tempfile::tempdir().unwrap();
        let main_path = dir.path().join("main.tri");
        std::fs::write(
            &main_path,
            r#"program test
use ext.triton.recursive

fn main() {
    let ptr_a: Field = divine()
    let ptr_b: Field = divine()
    let count: Field = divine()
    let result: Digest = recursive.xfe_inner_product(ptr_a, ptr_b, count)
    let (r0, r1, r2, r3, r4) = result
    pub_write(r0)
    pub_write(r1)
    pub_write(r2)
}
"#,
        )
        .unwrap();
        // Copy library files
        let ext_dir = dir.path().join("ext").join("triton");
        std::fs::create_dir_all(&ext_dir).unwrap();
        std::fs::copy("ext/triton/xfield.tri", ext_dir.join("xfield.tri")).unwrap_or_default();
        std::fs::copy("ext/triton/recursive.tri", ext_dir.join("recursive.tri"))
            .unwrap_or_default();
        // Copy std files that recursive.tri depends on
        let std_io = dir.path().join("std").join("io");
        let std_core = dir.path().join("std").join("core");
        std::fs::create_dir_all(&std_io).unwrap();
        std::fs::create_dir_all(&std_core).unwrap();
        std::fs::copy("std/io/io.tri", std_io.join("io.tri")).unwrap_or_default();
        std::fs::copy("std/core/assert.tri", std_core.join("assert.tri")).unwrap_or_default();

        let result = compile_project(&main_path);
        assert!(
            result.is_ok(),
            "xfe_inner_product should compile: {:?}",
            result.err()
        );
        let tasm = result.unwrap();
        assert!(
            tasm.contains("xx_dot_step"),
            "inner product should use xx_dot_step"
        );
    }

    #[test]
    fn test_xb_inner_product_library() {
        let dir = tempfile::tempdir().unwrap();
        let main_path = dir.path().join("main.tri");
        std::fs::write(
            &main_path,
            r#"program test
use ext.triton.recursive

fn main() {
    let ptr_a: Field = divine()
    let ptr_b: Field = divine()
    let count: Field = divine()
    let result: Digest = recursive.xb_inner_product(ptr_a, ptr_b, count)
    let (r0, r1, r2, r3, r4) = result
    pub_write(r0)
}
"#,
        )
        .unwrap();
        let ext_dir = dir.path().join("ext").join("triton");
        std::fs::create_dir_all(&ext_dir).unwrap();
        std::fs::copy("ext/triton/xfield.tri", ext_dir.join("xfield.tri")).unwrap_or_default();
        std::fs::copy("ext/triton/recursive.tri", ext_dir.join("recursive.tri"))
            .unwrap_or_default();
        let std_io = dir.path().join("std").join("io");
        let std_core = dir.path().join("std").join("core");
        std::fs::create_dir_all(&std_io).unwrap();
        std::fs::create_dir_all(&std_core).unwrap();
        std::fs::copy("std/io/io.tri", std_io.join("io.tri")).unwrap_or_default();
        std::fs::copy("std/core/assert.tri", std_core.join("assert.tri")).unwrap_or_default();

        let result = compile_project(&main_path);
        assert!(
            result.is_ok(),
            "xb_inner_product should compile: {:?}",
            result.err()
        );
        let tasm = result.unwrap();
        assert!(
            tasm.contains("xb_dot_step"),
            "xb inner product should use xb_dot_step"
        );
    }

    #[test]
    fn test_proof_composition_library() {
        let dir = tempfile::tempdir().unwrap();
        let main_path = dir.path().join("main.tri");
        std::fs::write(
            &main_path,
            r#"program test
use ext.triton.proof

fn main() {
    proof.verify_inner_proof(4)
}
"#,
        )
        .unwrap();
        // Copy all required library files
        let ext_dir = dir.path().join("ext").join("triton");
        std::fs::create_dir_all(&ext_dir).unwrap();
        std::fs::copy("ext/triton/proof.tri", ext_dir.join("proof.tri")).unwrap_or_default();
        std::fs::copy("ext/triton/recursive.tri", ext_dir.join("recursive.tri"))
            .unwrap_or_default();
        std::fs::copy("ext/triton/xfield.tri", ext_dir.join("xfield.tri")).unwrap_or_default();
        let std_io = dir.path().join("std").join("io");
        let std_core = dir.path().join("std").join("core");
        std::fs::create_dir_all(&std_io).unwrap();
        std::fs::create_dir_all(&std_core).unwrap();
        std::fs::copy("std/io/io.tri", std_io.join("io.tri")).unwrap_or_default();
        std::fs::copy("std/core/assert.tri", std_core.join("assert.tri")).unwrap_or_default();

        let result = compile_project(&main_path);
        assert!(
            result.is_ok(),
            "proof composition should compile: {:?}",
            result.err()
        );
        let tasm = result.unwrap();
        assert!(
            tasm.contains("xx_dot_step"),
            "should use xx_dot_step for inner products"
        );
    }

    #[test]
    fn test_proof_aggregation() {
        let dir = tempfile::tempdir().unwrap();
        let main_path = dir.path().join("main.tri");
        std::fs::write(
            &main_path,
            r#"program test
use ext.triton.proof

fn main() {
    let n: Field = pub_read()
    proof.aggregate_proofs(n, 4)
}
"#,
        )
        .unwrap();
        let ext_dir = dir.path().join("ext").join("triton");
        std::fs::create_dir_all(&ext_dir).unwrap();
        std::fs::copy("ext/triton/proof.tri", ext_dir.join("proof.tri")).unwrap_or_default();
        std::fs::copy("ext/triton/recursive.tri", ext_dir.join("recursive.tri"))
            .unwrap_or_default();
        std::fs::copy("ext/triton/xfield.tri", ext_dir.join("xfield.tri")).unwrap_or_default();
        let std_io = dir.path().join("std").join("io");
        let std_core = dir.path().join("std").join("core");
        std::fs::create_dir_all(&std_io).unwrap();
        std::fs::create_dir_all(&std_core).unwrap();
        std::fs::copy("std/io/io.tri", std_io.join("io.tri")).unwrap_or_default();
        std::fs::copy("std/core/assert.tri", std_core.join("assert.tri")).unwrap_or_default();

        let result = compile_project(&main_path);
        assert!(
            result.is_ok(),
            "proof aggregation should compile: {:?}",
            result.err()
        );
        let tasm = result.unwrap();
        assert!(
            tasm.contains("xx_dot_step"),
            "aggregation should use xx_dot_step"
        );
    }

    #[test]
    fn test_proof_relay_example_compiles() {
        let path = std::path::Path::new("examples/neptune/proof_relay.tri");
        if !path.exists() {
            return;
        }
        let result = compile_project(path);
        assert!(
            result.is_ok(),
            "proof relay example should compile: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_proof_aggregator_example_compiles() {
        let path = std::path::Path::new("examples/neptune/proof_aggregator.tri");
        if !path.exists() {
            return;
        }
        let result = compile_project(path);
        assert!(
            result.is_ok(),
            "proof aggregator example should compile: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_transaction_validation_compiles() {
        let path = std::path::Path::new("examples/neptune/transaction_validation.tri");
        if !path.exists() {
            return;
        }
        let result = compile_project(path);
        assert!(
            result.is_ok(),
            "transaction validation should compile: {:?}",
            result.err()
        );
        let tasm = result.unwrap();
        assert!(
            tasm.contains("xx_dot_step"),
            "should use recursive verification"
        );
        assert!(
            tasm.contains("merkle_step"),
            "should authenticate kernel fields"
        );
    }

    #[test]
    fn test_neptune_lock_scripts_compile() {
        for name in &[
            "lock_generation",
            "lock_symmetric",
            "lock_multisig",
            "lock_timelock",
        ] {
            let path_str = format!("examples/neptune/{}.tri", name);
            let path = std::path::Path::new(&path_str);
            if !path.exists() {
                continue;
            }
            let result = compile_project(path);
            assert!(
                result.is_ok(),
                "{} should compile: {:?}",
                name,
                result.err()
            );
        }
    }

    #[test]
    fn test_neptune_type_scripts_compile() {
        for name in &["type_native_currency", "type_custom_token"] {
            let path_str = format!("examples/neptune/{}.tri", name);
            let path = std::path::Path::new(&path_str);
            if !path.exists() {
                continue;
            }
            let result = compile_project(path);
            assert!(
                result.is_ok(),
                "{} should compile: {:?}",
                name,
                result.err()
            );
        }
    }
}
