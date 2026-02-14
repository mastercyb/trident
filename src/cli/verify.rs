use std::path::PathBuf;
use std::process;

use super::{load_and_parse, resolve_input};

pub fn cmd_verify(
    input: PathBuf,
    verbose: bool,
    smt_output: Option<PathBuf>,
    run_z3: bool,
    json: bool,
    synthesize: bool,
) {
    let ri = resolve_input(&input);
    let entry = ri.entry;

    eprintln!("Verifying {}...", input.display());

    let need_parse = verbose || smt_output.is_some() || run_z3 || json || synthesize;
    let (system, parsed_file) = if need_parse {
        let (_source, file) = load_and_parse(&entry);
        let sys = trident::sym::analyze(&file);
        if verbose {
            eprintln!("\nConstraint system: {}", sys.summary());
        }
        (Some(sys), Some(file))
    } else {
        (None, None)
    };

    if let Some(ref smt_path) = smt_output {
        if let Some(ref sys) = system {
            let smt_script = trident::smt::encode_system(sys, trident::smt::QueryMode::SafetyCheck);
            if let Err(e) = std::fs::write(smt_path, &smt_script) {
                eprintln!("error: cannot write '{}': {}", smt_path.display(), e);
                process::exit(1);
            }
            eprintln!("SMT-LIB2 written to {}", smt_path.display());
        }
    }

    if run_z3 {
        if let Some(ref sys) = system {
            run_z3_analysis(sys);
        }
    }

    if synthesize {
        if let Some(ref file) = parsed_file {
            let specs = trident::synthesize::synthesize_specs(file);
            eprintln!("\n{}", trident::synthesize::format_report(&specs));
        } else {
            eprintln!("warning: could not parse file for synthesis");
        }
    }

    match trident::verify_project(&entry) {
        Ok(report) => {
            if json {
                if let Some(ref sys) = system {
                    let file_name = entry.to_string_lossy().to_string();
                    let json_output =
                        trident::report::generate_json_report(&file_name, sys, &report);
                    println!("{}", json_output);
                } else {
                    eprintln!("error: could not build constraint system for JSON report");
                    process::exit(1);
                }
            } else {
                eprintln!("\n{}", report.format_report());
            }
            if !report.is_safe() {
                process::exit(1);
            }
        }
        Err(_) => {
            process::exit(1);
        }
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

pub fn cmd_equiv(input: PathBuf, fn_a: &str, fn_b: &str, verbose: bool) {
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
        if let Some(h) = fn_hashes.get(fn_a) {
            eprintln!("  {} hash: {}", fn_a, h);
        }
        if let Some(h) = fn_hashes.get(fn_b) {
            eprintln!("  {} hash: {}", fn_b, h);
        }
    }

    let result = trident::equiv::check_equivalence(&file, fn_a, fn_b);

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
