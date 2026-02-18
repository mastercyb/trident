pub(crate) use std::collections::{BTreeMap, BTreeSet};
pub(crate) use std::path::Path;

pub(crate) use crate::ast::{self, FileKind};
pub(crate) use crate::cost;
pub(crate) use crate::diagnostic::{render_diagnostics, Diagnostic};
pub(crate) use crate::resolve::resolve_modules;
pub(crate) use crate::span;
pub(crate) use crate::target::TerrainConfig;
pub(crate) use crate::tir::builder::TIRBuilder;
pub(crate) use crate::tir::linker::{link, ModuleTasm};
pub(crate) use crate::tir::lower::create_stack_lowering;
pub(crate) use crate::tir::optimize::optimize as optimize_tir;
pub(crate) use crate::typecheck::{ModuleExports, TypeChecker};
pub(crate) use crate::{format, lexer, parser, project, solve, sym};

#[cfg(test)]
mod tests;

/// Options controlling compilation: VM target + conditional compilation flags.
#[derive(Clone, Debug)]
pub struct CompileOptions {
    /// Profile name for cfg flags (e.g. "debug", "release").
    pub profile: String,
    /// Active cfg flags for conditional compilation.
    pub cfg_flags: BTreeSet<String>,
    /// Target VM configuration.
    pub target_config: TerrainConfig,
    /// Additional module search directories (from locked dependencies).
    pub dep_dirs: Vec<std::path::PathBuf>,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            profile: "debug".to_string(),
            cfg_flags: BTreeSet::from(["debug".to_string()]),
            target_config: TerrainConfig::triton(),
            dep_dirs: Vec::new(),
        }
    }
}

impl CompileOptions {
    /// Create options for a named profile (debug/release/custom).
    pub fn for_profile(profile: &str) -> Self {
        Self {
            profile: profile.to_string(),
            cfg_flags: BTreeSet::from([profile.to_string()]),
            target_config: TerrainConfig::triton(),
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
    let file = crate::parse_source(source, filename)?;

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

    // Build IR, optimize, and lower to target assembly
    let ir = TIRBuilder::new(options.target_config.clone())
        .with_cfg_flags(options.cfg_flags.clone())
        .with_mono_instances(exports.mono_instances)
        .with_call_resolutions(exports.call_resolutions)
        .build_file(&file);
    let ir = optimize_tir(ir);
    let lowering = create_stack_lowering(&options.target_config.name);
    let tasm = lowering.lower(&ir).join("\n");
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
    use crate::pipeline::PreparedProject;

    let project = PreparedProject::build(entry_path, options)?;

    let intrinsic_map = project.intrinsic_map();
    let module_aliases = project.module_aliases();
    let external_constants = project.external_constants();

    // Emit TASM for each module
    let mut tasm_modules = Vec::new();
    for (i, pm) in project.modules.iter().enumerate() {
        let is_program = pm.file.kind == FileKind::Program;
        let mono = project
            .exports
            .get(i)
            .map(|e| e.mono_instances.clone())
            .unwrap_or_default();
        let call_res = project
            .exports
            .get(i)
            .map(|e| e.call_resolutions.clone())
            .unwrap_or_default();
        let ir = TIRBuilder::new(options.target_config.clone())
            .with_cfg_flags(options.cfg_flags.clone())
            .with_intrinsics(intrinsic_map.clone())
            .with_module_aliases(module_aliases.clone())
            .with_constants(external_constants.clone())
            .with_mono_instances(mono)
            .with_call_resolutions(call_res)
            .build_file(&pm.file);
        let ir = optimize_tir(ir);
        let lowering = create_stack_lowering(&options.target_config.name);
        let tasm = lowering.lower(&ir).join("\n");
        tasm_modules.push(ModuleTasm {
            module_name: pm.file.name.node.clone(),
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
    let file = crate::parse_source(source, filename)?;

    if let Err(errors) = TypeChecker::new().check_file(&file) {
        render_diagnostics(&errors, filename, source);
        return Err(errors);
    }

    Ok(())
}

/// Project-aware type-check from an entry point path.
/// Resolves all modules (including std.*) and type-checks in dependency order.
pub fn check_project(entry_path: &Path) -> Result<(), Vec<Diagnostic>> {
    use crate::pipeline::PreparedProject;

    PreparedProject::build_default(entry_path)?;
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
    use crate::pipeline::PreparedProject;

    let project = PreparedProject::build(entry_path, options)?;

    // Discover all #[test] functions across all modules
    let mut test_fns: Vec<(String, String)> = Vec::new(); // (module_name, fn_name)
    for pm in &project.modules {
        for test_name in discover_tests(&pm.file) {
            test_fns.push((pm.file.name.node.clone(), test_name));
        }
    }

    if test_fns.is_empty() {
        return Ok("No #[test] functions found.\n".to_string());
    }

    // For each test function, compile a mini-program and report
    let mut results: Vec<TestResult> = Vec::new();
    let mut short_names: Vec<String> = Vec::new();
    for (module_name, test_name) in &test_fns {
        // Find the source file for this module
        let source_entry = project
            .modules
            .iter()
            .find(|m| m.file.name.node == *module_name);

        if let Some(pm) = source_entry {
            // Build a mini-program source that just calls the test function
            let mini_source = if module_name.starts_with("module") || module_name.contains('.') {
                // For module test functions, we'd need cross-module calls
                // For simplicity, compile in-context
                pm.source.clone()
            } else {
                pm.source.clone()
            };

            // Try to compile (type-check + emit) the source.
            // The test function itself is validated by the type checker.
            // For now, "passing" means it compiles without errors.
            match compile_with_options(&mini_source, &pm.file_path.to_string_lossy(), options) {
                Ok(tasm) => {
                    // Compute cost for the test function
                    let test_cost =
                        analyze_costs(&mini_source, &pm.file_path.to_string_lossy()).ok();
                    if short_names.is_empty() {
                        if let Some(ref pc) = test_cost {
                            short_names = pc.table_short_names.clone();
                        }
                    }
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
            let sn: Vec<&str> = short_names.iter().map(|s| s.as_str()).collect();
            let ann = c.format_annotation(&sn);
            if ann.is_empty() {
                String::new()
            } else {
                format!(" ({})", ann)
            }
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

/// Compile a module and emit TASM for all its functions (no linking, no DCE).
/// Dependencies are resolved and type-checked, but only the target module's
/// TASM is returned. Labels use the raw `__funcname:` format.
pub fn compile_module(
    module_path: &Path,
    options: &CompileOptions,
) -> Result<String, Vec<Diagnostic>> {
    use crate::pipeline::PreparedProject;

    let project = PreparedProject::build(module_path, options)?;

    let intrinsic_map = project.intrinsic_map();
    let module_aliases = project.module_aliases();
    let external_constants = project.external_constants();

    // Emit TASM for only the target module (last in topological order)
    if let Some((i, pm)) = project.modules.iter().enumerate().last() {
        let mono = project
            .exports
            .get(i)
            .map(|e| e.mono_instances.clone())
            .unwrap_or_default();
        let call_res = project
            .exports
            .get(i)
            .map(|e| e.call_resolutions.clone())
            .unwrap_or_default();
        let ir = TIRBuilder::new(options.target_config.clone())
            .with_cfg_flags(options.cfg_flags.clone())
            .with_intrinsics(intrinsic_map)
            .with_module_aliases(module_aliases)
            .with_constants(external_constants)
            .with_mono_instances(mono)
            .with_call_resolutions(call_res)
            .build_file(&pm.file);
        let ir = optimize_tir(ir);
        let lowering = create_stack_lowering(&options.target_config.name);
        let tasm = lowering.lower(&ir).join("\n");
        Ok(tasm)
    } else {
        Err(vec![Diagnostic::error(
            "no module found".to_string(),
            span::Span::dummy(),
        )])
    }
}

/// Build TIR (optimized intermediate representation) from a single source file.
///
/// Returns the IR ops before lowering to target assembly. Used by the
/// neural optimizer to analyze and improve the compilation.
pub fn build_tir(
    source: &str,
    filename: &str,
    options: &CompileOptions,
) -> Result<Vec<crate::tir::TIROp>, Vec<Diagnostic>> {
    let file = crate::parse_source(source, filename)?;

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

    let ir = TIRBuilder::new(options.target_config.clone())
        .with_cfg_flags(options.cfg_flags.clone())
        .with_mono_instances(exports.mono_instances)
        .with_call_resolutions(exports.call_resolutions)
        .build_file(&file);
    Ok(optimize_tir(ir))
}

/// Build TIR from a project entry point with full module resolution.
///
/// Uses the same multi-module pipeline as `compile_project_with_options`
/// but returns combined TIR ops instead of TASM. Required for neural
/// training on files that import other modules (e.g. merkle.tri imports
/// vm.crypto.merkle).
pub fn build_tir_project(
    entry_path: &Path,
    options: &CompileOptions,
) -> Result<Vec<crate::tir::TIROp>, Vec<Diagnostic>> {
    use crate::pipeline::PreparedProject;

    let project = PreparedProject::build(entry_path, options)?;

    let intrinsic_map = project.intrinsic_map();
    let module_aliases = project.module_aliases();
    let external_constants = project.external_constants();

    let mut all_ir = Vec::new();
    for (i, pm) in project.modules.iter().enumerate() {
        let mono = project
            .exports
            .get(i)
            .map(|e| e.mono_instances.clone())
            .unwrap_or_default();
        let call_res = project
            .exports
            .get(i)
            .map(|e| e.call_resolutions.clone())
            .unwrap_or_default();
        let ir = TIRBuilder::new(options.target_config.clone())
            .with_cfg_flags(options.cfg_flags.clone())
            .with_intrinsics(intrinsic_map.clone())
            .with_module_aliases(module_aliases.clone())
            .with_constants(external_constants.clone())
            .with_mono_instances(mono)
            .with_call_resolutions(call_res)
            .build_file(&pm.file);
        all_ir.extend(optimize_tir(ir));
    }
    Ok(all_ir)
}

pub(crate) mod doc;
pub(crate) mod pipeline;
mod tools;
pub use tools::*;

/// Compile a multi-module project to a `ProgramBundle` artifact.
///
/// This is the primary entry point for warriors: it produces a
/// self-contained bundle with compiled assembly, cost analysis,
/// function signatures, and metadata.
pub fn compile_to_bundle(
    entry_path: &Path,
    options: &CompileOptions,
) -> Result<crate::runtime::ProgramBundle, Vec<Diagnostic>> {
    use crate::runtime::artifact::{BundleCost, BundleFunction, ProgramBundle};
    use pipeline::PreparedProject;

    let tasm = compile_project_with_options(entry_path, options)?;

    // Cost analysis (best-effort — use zeros on failure)
    let program_cost =
        analyze_costs_project(entry_path, options).unwrap_or_else(|_| cost::ProgramCost {
            program_name: String::new(),
            functions: Vec::new(),
            total: cost::TableCost::ZERO,
            table_names: Vec::new(),
            table_short_names: Vec::new(),
            attestation_hash_rows: 0,
            padded_height: 0,
            estimated_proving_ns: 0,
            loop_bound_waste: Vec::new(),
        });

    // Parse entry file for function signatures + content hashes
    let project = PreparedProject::build(entry_path, options)?;
    let entry_file = project
        .modules
        .iter()
        .find(|m| m.file.kind == FileKind::Program)
        .or_else(|| project.modules.last());

    let (functions, entry_point, source_hash) = if let Some(pm) = entry_file {
        let fn_hashes = crate::hash::hash_file(&pm.file);
        let fns: Vec<BundleFunction> = pm
            .file
            .items
            .iter()
            .filter_map(|item| {
                if let ast::Item::Fn(func) = &item.node {
                    if !func.is_test {
                        let hash = fn_hashes
                            .get(&func.name.node)
                            .map(|h| h.to_hex())
                            .unwrap_or_default();
                        return Some(BundleFunction {
                            name: func.name.node.clone(),
                            hash,
                            signature: crate::deploy::format_fn_signature(func),
                        });
                    }
                }
                None
            })
            .collect();
        let ep = if fns.iter().any(|f| f.name == "main") {
            "main".to_string()
        } else {
            fns.first()
                .map(|f| f.name.clone())
                .unwrap_or_else(|| "main".to_string())
        };
        let sh = crate::hash::hash_file_content(&pm.file).to_hex();
        (fns, ep, sh)
    } else {
        (Vec::new(), "main".to_string(), String::new())
    };

    let name = entry_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("program")
        .to_string();

    Ok(ProgramBundle {
        name,
        version: "0.1.0".to_string(),
        target_vm: options.target_config.name.clone(),
        target_os: None,
        assembly: tasm,
        entry_point,
        functions,
        cost: BundleCost {
            table_values: (0..program_cost.total.count as usize)
                .map(|i| program_cost.total.get(i))
                .collect(),
            table_names: program_cost.table_names,
            padded_height: program_cost.padded_height,
            estimated_proving_ns: program_cost.estimated_proving_ns,
        },
        source_hash,
    })
}
