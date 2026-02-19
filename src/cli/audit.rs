use std::path::PathBuf;
use std::process;

use clap::Args;

use super::trisha::{
    generate_test_harness, run_trisha, run_trisha_with_inputs, trisha_available, Harness,
};
use super::{load_and_parse, resolve_input};

#[derive(Args)]
pub struct AuditArgs {
    /// Input .tri file (symbolic audit) or omit for execution audit
    pub input: Option<PathBuf>,
    /// Show detailed output
    #[arg(long)]
    pub verbose: bool,
    /// Output SMT-LIB2 encoding to file (for external solvers)
    #[arg(long, value_name = "PATH")]
    pub smt: Option<PathBuf>,
    /// Run Z3 solver (if available) for formal verification
    #[arg(long)]
    pub z3: bool,
    /// Output machine-readable JSON report (for LLM/CI consumption)
    #[arg(long)]
    pub json: bool,
    /// Synthesize and suggest specifications (invariants, pre/postconditions)
    #[arg(long)]
    pub synthesize: bool,
}

pub fn cmd_audit(args: AuditArgs) {
    match args.input {
        Some(ref _input) => cmd_audit_symbolic(args),
        None => cmd_audit_exec(),
    }
}

// ── Execution correctness audit (default, no args) ─────────────────

/// Audit results for a single dimension (classic or hand).
struct DimAudit {
    compile: AuditStatus,
    execute: AuditStatus,
    prove: AuditStatus,
    verify: AuditStatus,
}

impl Default for DimAudit {
    fn default() -> Self {
        DimAudit {
            compile: AuditStatus::Skip,
            execute: AuditStatus::Skip,
            prove: AuditStatus::Skip,
            verify: AuditStatus::Skip,
        }
    }
}

/// Per-module audit result.
struct ModuleAudit {
    name: String,
    classic: DimAudit,
    hand: DimAudit,
}

enum AuditStatus {
    Ok,
    Fail(String),
    Skip,
}

impl AuditStatus {
    fn is_ok(&self) -> bool {
        matches!(self, AuditStatus::Ok)
    }

    fn label(&self) -> &str {
        match self {
            AuditStatus::Ok => "OK",
            AuditStatus::Fail(_) => "FAIL",
            AuditStatus::Skip => "-",
        }
    }
}

fn cmd_audit_exec() {
    if !trisha_available() {
        eprintln!("error: trisha not found on PATH (required for execution audit)");
        eprintln!("  install: cd ~/git/trisha && cargo install --path . --force");
        process::exit(1);
    }

    let bench_dir = resolve_bench_dir(&PathBuf::from("benches"));
    if !bench_dir.is_dir() {
        eprintln!("error: 'benches/' directory not found");
        process::exit(1);
    }

    let project_root = bench_dir
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));

    let mut baselines = find_baseline_files(&bench_dir, 0);
    baselines.sort();

    if baselines.is_empty() {
        eprintln!("No .baseline.tasm files found in benches/");
        process::exit(1);
    }

    let options = trident::CompileOptions::default();
    let mut results: Vec<ModuleAudit> = Vec::new();

    for baseline_path in &baselines {
        let rel = baseline_path
            .strip_prefix(&bench_dir)
            .unwrap_or(baseline_path);
        let rel_str = rel.to_string_lossy();
        let source_rel = rel_str.replace(".baseline.tasm", ".tri");
        let source_path = project_root.join(&source_rel);
        let module_name = source_rel.trim_end_matches(".tri").replace('/', "::");

        if !source_path.exists() {
            continue;
        }

        eprint!("\r  auditing {}...{}", module_name, " ".repeat(30));
        use std::io::Write;
        let _ = std::io::stderr().flush();

        let mut audit = ModuleAudit {
            name: module_name.clone(),
            classic: DimAudit::default(),
            hand: DimAudit::default(),
        };

        // ── Classic dimension ──
        let _guard = trident::diagnostic::suppress_warnings();
        let module_tasm = trident::compile_module(&source_path, &options);
        drop(_guard);

        if let Ok(tasm) = module_tasm {
            audit.classic.compile = AuditStatus::Ok;
            let harness = generate_test_harness(&tasm);
            audit_run_pipeline(&mut audit.classic, &module_name, "classic", &harness);
        } else {
            audit.classic.compile = AuditStatus::Fail("compilation failed".into());
        }

        // ── Hand dimension ──
        let baseline_tasm = std::fs::read_to_string(baseline_path).unwrap_or_default();
        if !baseline_tasm.is_empty() {
            audit.hand.compile = AuditStatus::Ok;
            let harness = generate_test_harness(&baseline_tasm);
            audit_run_pipeline(&mut audit.hand, &module_name, "hand", &harness);
        }

        results.push(audit);
    }

    // Clear progress
    eprint!("\r{}\r", " ".repeat(80));

    if results.is_empty() {
        eprintln!("No modules found to audit.");
        process::exit(1);
    }

    // Render table
    eprintln!();
    eprintln!(
        "{:<32} | {:>4} {:>4} {:>4} {:>4} | {:>4} {:>4} {:>4} {:>4}",
        "Module", "Comp", "Exec", "Prov", "Vrfy", "Comp", "Exec", "Prov", "Vrfy"
    );
    eprintln!("{:<32} | {:<19} | {:<19}", "", "Classic", "Hand");
    eprintln!("{}", "-".repeat(76));

    let mut any_fail = false;
    for r in &results {
        eprintln!(
            "{:<32} | {:>4} {:>4} {:>4} {:>4} | {:>4} {:>4} {:>4} {:>4}",
            r.name,
            r.classic.compile.label(),
            r.classic.execute.label(),
            r.classic.prove.label(),
            r.classic.verify.label(),
            r.hand.compile.label(),
            r.hand.execute.label(),
            r.hand.prove.label(),
            r.hand.verify.label(),
        );
        any_fail |= print_dim_failures("classic", &r.classic);
        any_fail |= print_dim_failures("hand", &r.hand);
    }

    eprintln!("{}", "-".repeat(76));

    let n = results.len();
    let count = |f: fn(&ModuleAudit) -> &AuditStatus| -> usize {
        results.iter().filter(|r| f(r).is_ok()).count()
    };
    eprintln!(
        "Classic: {}/{} compile  {}/{} execute  {}/{} prove  {}/{} verify",
        count(|r| &r.classic.compile),
        n,
        count(|r| &r.classic.execute),
        n,
        count(|r| &r.classic.prove),
        n,
        count(|r| &r.classic.verify),
        n,
    );
    eprintln!(
        "Hand:    {}/{} compile  {}/{} execute  {}/{} prove  {}/{} verify",
        count(|r| &r.hand.compile),
        n,
        count(|r| &r.hand.execute),
        n,
        count(|r| &r.hand.prove),
        n,
        count(|r| &r.hand.verify),
        n,
    );

    if any_fail {
        eprintln!();
        process::exit(1);
    }

    eprintln!("\nAll modules pass.");
}

/// Run execute -> prove -> verify pipeline for one dimension.
fn audit_run_pipeline(dim: &mut DimAudit, module_name: &str, label: &str, harness: &Harness) {
    let tmp_path = std::env::temp_dir().join(format!(
        "trident_audit_{}_{}.tasm",
        module_name.replace("::", "_"),
        label,
    ));
    if std::fs::write(&tmp_path, &harness.tasm).is_err() {
        dim.execute = AuditStatus::Fail("cannot write temp file".into());
        return;
    }
    let tmp_str = tmp_path.to_string_lossy().to_string();

    // Execute
    match run_trisha_with_inputs(&["run", "--tasm", &tmp_str], harness) {
        Ok(_) => dim.execute = AuditStatus::Ok,
        Err(e) => {
            dim.execute = AuditStatus::Fail(e);
            let _ = std::fs::remove_file(&tmp_path);
            return;
        }
    }

    // Prove
    let proof_path = std::env::temp_dir().join(format!(
        "trident_audit_{}_{}.proof.toml",
        module_name.replace("::", "_"),
        label,
    ));
    let proof_str = proof_path.to_string_lossy().to_string();
    match run_trisha_with_inputs(
        &["prove", "--tasm", &tmp_str, "--output", &proof_str],
        harness,
    ) {
        Ok(_) if proof_path.exists() => dim.prove = AuditStatus::Ok,
        Ok(_) => {
            dim.prove = AuditStatus::Fail("no proof file produced".into());
            let _ = std::fs::remove_file(&tmp_path);
            return;
        }
        Err(e) => {
            dim.prove = AuditStatus::Fail(e);
            let _ = std::fs::remove_file(&tmp_path);
            return;
        }
    }

    let _ = std::fs::remove_file(&tmp_path);

    // Verify (no inputs needed — verification uses the proof file)
    match run_trisha(&["verify", &proof_str]) {
        Ok(_) => dim.verify = AuditStatus::Ok,
        Err(e) => dim.verify = AuditStatus::Fail(e),
    }

    let _ = std::fs::remove_file(&proof_path);
}

/// Print failure details for a dimension, return true if any failures.
fn print_dim_failures(label: &str, dim: &DimAudit) -> bool {
    let mut failed = false;
    for (stage, status) in [
        ("compile", &dim.compile),
        ("execute", &dim.execute),
        ("prove", &dim.prove),
        ("verify", &dim.verify),
    ] {
        if let AuditStatus::Fail(ref e) = status {
            eprintln!("  {} {}: {}", label, stage, first_line(e));
            failed = true;
        }
    }
    failed
}

fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or(s)
}

// ── Symbolic audit (with file arg) ────────────────────────────────

fn cmd_audit_symbolic(args: AuditArgs) {
    let input = args.input.expect("symbolic audit requires input");
    let AuditArgs {
        verbose,
        smt: smt_output,
        z3: run_z3,
        json,
        synthesize,
        ..
    } = args;
    let ri = resolve_input(&input);
    let entry = ri.entry;

    eprintln!("Auditing {}...", input.display());

    let (system, parsed_file) = {
        let (_source, file) = load_and_parse(&entry);
        let per_fn = trident::sym::analyze_all(&file);
        if verbose {
            if per_fn.is_empty() {
                eprintln!("\n  No analyzable functions found.");
            } else {
                eprintln!();
                for (fn_name, sys) in &per_fn {
                    let violated = sys.violated_constraints().len();
                    let status = if violated > 0 {
                        format!("VIOLATED ({})", violated)
                    } else if sys.constraints.is_empty() {
                        "- (no constraints)".to_string()
                    } else {
                        "SAFE".to_string()
                    };
                    eprintln!(
                        "  {:<30} {:>3} constraints, {:>3} variables  [{}]",
                        fn_name,
                        sys.active_constraints(),
                        sys.num_variables,
                        status,
                    );
                }
            }
        }
        let mut sys = trident::sym::ConstraintSystem::new();
        for (_, fn_sys) in &per_fn {
            sys.constraints.extend(fn_sys.constraints.clone());
            sys.num_variables += fn_sys.num_variables;
            for (k, v) in &fn_sys.variables {
                sys.variables.insert(k.clone(), *v);
            }
            sys.pub_inputs.extend(fn_sys.pub_inputs.clone());
            sys.pub_outputs.extend(fn_sys.pub_outputs.clone());
            sys.divine_inputs.extend(fn_sys.divine_inputs.clone());
        }
        if verbose {
            eprintln!("\nCombined: {}", sys.summary());
        }
        (sys, Some(file))
    };

    if let Some(ref smt_path) = smt_output {
        let smt_script = trident::smt::encode_system(&system, trident::smt::QueryMode::SafetyCheck);
        if let Err(e) = std::fs::write(smt_path, &smt_script) {
            eprintln!("error: cannot write '{}': {}", smt_path.display(), e);
            process::exit(1);
        }
        eprintln!("SMT-LIB2 written to {}", smt_path.display());
    }

    if run_z3 {
        run_z3_analysis(&system);
    }

    if synthesize {
        if let Some(ref file) = parsed_file {
            let specs = trident::synthesize::synthesize_specs(file);
            eprintln!("\n{}", trident::synthesize::format_report(&specs));
        }
    }

    let report = trident::solve::verify(&system);

    if json {
        let file_name = entry.to_string_lossy().to_string();
        let json_output = trident::report::generate_json_report(&file_name, &system, &report);
        println!("{}", json_output);
    } else {
        eprintln!("\n{}", report.format_report());
    }
    if !report.is_safe() {
        process::exit(1);
    }
}

fn run_z3_analysis(sys: &trident::sym::ConstraintSystem) {
    let smt_script = trident::smt::encode_system(sys, trident::smt::QueryMode::SafetyCheck);
    match trident::smt::run_z3(&smt_script) {
        Ok(result) => {
            eprintln!("\nZ3 safety check:");
            match result.status {
                trident::smt::SmtStatus::Unsat => {
                    eprintln!("  Result: UNSAT (formally verified safe)");
                }
                trident::smt::SmtStatus::Sat => {
                    eprintln!("  Result: SAT (counterexample found)");
                    if let Some(model) = &result.model {
                        eprintln!("  Model:\n{}", model);
                    }
                }
                trident::smt::SmtStatus::Unknown => {
                    eprintln!("  Result: UNKNOWN (solver timed out or gave up)");
                }
                trident::smt::SmtStatus::Error(ref e) => {
                    eprintln!("  Result: ERROR\n  {}", e);
                }
            }

            if !sys.divine_inputs.is_empty() {
                let witness_script =
                    trident::smt::encode_system(sys, trident::smt::QueryMode::WitnessExistence);
                if let Ok(witness_result) = trident::smt::run_z3(&witness_script) {
                    eprintln!(
                        "\nZ3 witness existence ({} divine inputs):",
                        sys.divine_inputs.len()
                    );
                    match witness_result.status {
                        trident::smt::SmtStatus::Sat => {
                            eprintln!("  Result: SAT (valid witness exists)");
                        }
                        trident::smt::SmtStatus::Unsat => {
                            eprintln!(
                                "  Result: UNSAT (no valid witness — constraints unsatisfiable)"
                            );
                        }
                        _ => {
                            eprintln!(
                                "  Result: {}",
                                witness_result.output.lines().next().unwrap_or("unknown")
                            );
                        }
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("\nZ3 not available: {}", e);
            eprintln!("  Install Z3 or use --smt to export for external solvers.");
        }
    }
}

// ── Equivalence checking ──────────────────────────────────────────

#[derive(Args)]
pub struct EquivArgs {
    /// Input .tri file containing both functions
    pub input: PathBuf,
    /// First function name
    pub fn_a: String,
    /// Second function name
    pub fn_b: String,
    /// Show detailed symbolic analysis
    #[arg(long)]
    pub verbose: bool,
}

pub fn cmd_equiv(args: EquivArgs) {
    let EquivArgs {
        input,
        fn_a,
        fn_b,
        verbose,
    } = args;
    if !input.extension().is_some_and(|e| e == "tri") {
        eprintln!("error: input must be a .tri file");
        process::exit(1);
    }

    let (_, file) = load_and_parse(&input);

    eprintln!(
        "Checking equivalence: {} vs {} in {}",
        fn_a,
        fn_b,
        input.display()
    );

    if verbose {
        let fn_hashes = trident::hash::hash_file(&file);
        if let Some(h) = fn_hashes.get(fn_a.as_str()) {
            eprintln!("  {} hash: {}", fn_a, h);
        }
        if let Some(h) = fn_hashes.get(fn_b.as_str()) {
            eprintln!("  {} hash: {}", fn_b, h);
        }
    }

    let result = trident::equiv::check_equivalence(&file, &fn_a, &fn_b);

    eprintln!("\n{}", result.format_report());

    match result.verdict {
        trident::equiv::EquivalenceVerdict::Equivalent => {}
        trident::equiv::EquivalenceVerdict::NotEquivalent => {
            process::exit(1);
        }
        trident::equiv::EquivalenceVerdict::Unknown => {
            process::exit(2);
        }
    }
}

// ── Shared helpers ────────────────────────────────────────────────

/// Recursively find all .baseline.tasm files in a directory.
fn find_baseline_files(dir: &std::path::Path, depth: usize) -> Vec<PathBuf> {
    if depth >= 64 {
        return Vec::new();
    }
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(find_baseline_files(&path, depth + 1));
            } else if path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().ends_with(".baseline.tasm"))
            {
                files.push(path);
            }
        }
    }
    files
}

/// Resolve the bench directory by searching ancestor directories.
fn resolve_bench_dir(dir: &std::path::Path) -> PathBuf {
    if dir.is_dir() {
        return dir.to_path_buf();
    }
    if dir.is_relative() {
        if let Ok(cwd) = std::env::current_dir() {
            let mut ancestor = cwd.as_path();
            loop {
                let candidate = ancestor.join(dir);
                if candidate.is_dir() {
                    return candidate;
                }
                match ancestor.parent() {
                    Some(parent) => ancestor = parent,
                    None => break,
                }
            }
        }
    }
    dir.to_path_buf()
}
