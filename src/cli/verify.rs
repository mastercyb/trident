use std::path::PathBuf;
use std::process;

use clap::Args;

use super::{load_and_parse, resolve_input};

#[derive(Args)]
pub struct VerifyArgs {
    /// Input .tri file or directory with trident.toml
    pub input: PathBuf,
    /// Show detailed constraint system summary
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

pub fn cmd_verify(args: VerifyArgs) {
    let VerifyArgs {
        input,
        verbose,
        smt: smt_output,
        z3: run_z3,
        json,
        synthesize,
    } = args;
    let ri = resolve_input(&input);
    let entry = ri.entry;

    eprintln!("Verifying {}...", input.display());

    let (system, parsed_file) = {
        let (_source, file) = load_and_parse(&entry);
        // Analyze all functions (works for both programs and modules)
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
        // Build combined constraint system
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
