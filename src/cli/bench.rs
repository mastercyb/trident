use std::path::PathBuf;
use std::process;

use clap::Args;

use super::trisha::{
    generate_test_harness, run_trisha, run_trisha_with_inputs, trisha_available, Harness,
};

#[derive(Args)]
pub struct BenchArgs {
    /// Directory containing baseline .tasm files (mirrors source tree)
    #[arg(default_value = "benches")]
    pub dir: PathBuf,
    /// Run all checks: compile, execute, prove, verify
    #[arg(long)]
    pub full: bool,
    /// Show per-function instruction breakdown
    #[arg(long)]
    pub functions: bool,
}

/// Timing triplet for a single dimension: execute, prove, verify (ms).
#[derive(Default)]
struct DimTiming {
    exec_ms: Option<f64>,
    prove_ms: Option<f64>,
    verify_ms: Option<f64>,
    proof_path: Option<PathBuf>,
}

/// Per-module benchmark data across all dimensions.
struct ModuleBench {
    name: String,
    /// Instruction counts
    classic_insn: usize,
    hand_insn: usize,
    /// Rust-native compilation time (ms)
    compile_ms: f64,
    /// Rust reference execution time (nanoseconds per op), if available
    rust_ns: Option<u64>,
    /// Per-dimension timing
    classic: DimTiming,
    hand: DimTiming,
    neural: DimTiming,
    /// Per-function breakdown (only collected with --functions)
    functions: Vec<trident::FunctionBenchmark>,
}

pub fn cmd_bench(args: BenchArgs) {
    let bench_dir = resolve_bench_dir(&args.dir);
    if !bench_dir.is_dir() {
        eprintln!("error: '{}' is not a directory", args.dir.display());
        process::exit(1);
    }

    let project_root = bench_dir
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));

    let mut baselines = find_baseline_files(&bench_dir, 0);
    baselines.sort();

    if baselines.is_empty() {
        eprintln!("No .baseline.tasm files found in '{}'", bench_dir.display());
        process::exit(1);
    }

    let options = trident::CompileOptions::default();
    let has_trisha = args.full && trisha_available();

    // Collect data for each module
    let mut modules: Vec<ModuleBench> = Vec::new();

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

        // Read baseline TASM
        let baseline_tasm = match std::fs::read_to_string(baseline_path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        // Compile module (instruction count) + time it
        let compile_start = std::time::Instant::now();
        let _guard = trident::diagnostic::suppress_warnings();
        let compiled_tasm = match trident::compile_module(&source_path, &options) {
            Ok(t) => t,
            Err(_) => continue,
        };
        drop(_guard);
        let compile_ms = compile_start.elapsed().as_secs_f64() * 1000.0;

        // Parse per-function instruction counts
        let compiled_fns = trident::parse_tasm_functions(&compiled_tasm);
        let baseline_fns = trident::parse_tasm_functions(&baseline_tasm);

        let mut fn_results: Vec<trident::FunctionBenchmark> = Vec::new();
        let mut total_compiled: usize = 0;
        let mut total_baseline: usize = 0;

        for (name, &baseline_count) in &baseline_fns {
            let compiled_count = compiled_fns.get(name).copied().unwrap_or(0);
            total_compiled += compiled_count;
            total_baseline += baseline_count;
            if args.functions {
                fn_results.push(trident::FunctionBenchmark {
                    name: name.clone(),
                    compiled_instructions: compiled_count,
                    baseline_instructions: baseline_count,
                });
            }
        }

        // Run Rust reference benchmark if available
        let ref_rs = baseline_path.with_file_name(
            baseline_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .replace(".baseline.tasm", ".reference.rs"),
        );
        let rust_ns = if args.full && ref_rs.exists() {
            let rel = ref_rs.strip_prefix(project_root).unwrap_or(&ref_rs);
            run_rust_reference(&rel.to_string_lossy())
        } else {
            None
        };

        let mut mb = ModuleBench {
            name: module_name.clone(),
            classic_insn: total_compiled,
            hand_insn: total_baseline,
            compile_ms,
            rust_ns,
            classic: DimTiming::default(),
            hand: DimTiming::default(),
            neural: DimTiming::default(),
            functions: fn_results,
        };

        // Run trisha passes for --full
        if has_trisha {
            // Classic: compile module, generate test harness
            let _guard2 = trident::diagnostic::suppress_warnings();
            let module_tasm = trident::compile_module(&source_path, &options).ok();
            drop(_guard2);

            if let Some(tasm) = module_tasm {
                let classic_harness = generate_test_harness(&tasm);
                run_dimension(&mut mb.classic, &module_name, "classic", &classic_harness);
            }

            // Hand: generate test harness from baseline
            let hand_harness = generate_test_harness(&baseline_tasm);
            run_dimension(&mut mb.hand, &module_name, "hand", &hand_harness);

            // Neural: not available in bench context yet
        }

        // Show progress
        eprint!("\r  collecting {}...{}", module_name, " ".repeat(30));
        use std::io::Write;
        let _ = std::io::stderr().flush();

        modules.push(mb);
    }

    // Verify pass (needs proof files from prove pass)
    if has_trisha {
        for mb in &mut modules {
            verify_dimension(&mut mb.classic);
            verify_dimension(&mut mb.hand);
            verify_dimension(&mut mb.neural);
        }
    }

    // Clear progress line
    eprint!("\r{}\r", " ".repeat(80));

    if modules.is_empty() {
        eprintln!("No benchmarks could be compiled.");
        process::exit(1);
    }

    // Render unified table
    eprintln!();
    if args.full {
        render_full_table(&modules, args.functions);
    } else {
        render_insn_table(&modules, args.functions);
    }

    // Clean up proof files
    for mb in &modules {
        for dim in [&mb.classic, &mb.hand, &mb.neural] {
            if let Some(ref path) = dim.proof_path {
                let _ = std::fs::remove_file(path);
            }
        }
    }

    eprintln!();
}

/// Render instruction-count-only table (default, no --full).
fn render_insn_table(modules: &[ModuleBench], show_functions: bool) {
    eprintln!("{:<40} {:>6} {:>6} {:>7}", "Module", "Tri", "Hand", "Ratio");
    eprintln!("{}", "-".repeat(63));

    for mb in modules {
        let ratio = if mb.hand_insn > 0 {
            format!("{:.2}x", mb.classic_insn as f64 / mb.hand_insn as f64)
        } else {
            "-".to_string()
        };
        eprintln!(
            "{:<40} {:>6} {:>6} {:>7}",
            mb.name, mb.classic_insn, mb.hand_insn, ratio,
        );
        if show_functions {
            for f in &mb.functions {
                let fr = if f.baseline_instructions > 0 {
                    format!(
                        "{:.2}x",
                        f.compiled_instructions as f64 / f.baseline_instructions as f64
                    )
                } else {
                    "-".to_string()
                };
                eprintln!(
                    "  {:<38} {:>6} {:>6} {:>7}",
                    f.name,
                    if f.compiled_instructions > 0 {
                        f.compiled_instructions.to_string()
                    } else {
                        "-".to_string()
                    },
                    f.baseline_instructions,
                    fr,
                );
            }
        }
    }

    eprintln!("{}", "-".repeat(63));
    let sum_classic: usize = modules.iter().map(|m| m.classic_insn).sum();
    let sum_hand: usize = modules.iter().map(|m| m.hand_insn).sum();
    let avg_ratio = if sum_hand > 0 {
        format!("{:.2}x", sum_classic as f64 / sum_hand as f64)
    } else {
        "-".to_string()
    };
    eprintln!(
        "{:<40} {:>6} {:>6} {:>7}",
        format!("TOTAL ({} modules)", modules.len()),
        sum_classic,
        sum_hand,
        avg_ratio,
    );
}

/// Format a millisecond value, or "-" if None.
fn fmt_ms(ms: Option<f64>) -> String {
    ms.map(|v| format!("{:.0}ms", v))
        .unwrap_or_else(|| "-".into())
}

/// Format verify result for a dimension.
fn fmt_verify(dim: &DimTiming) -> &'static str {
    if dim.verify_ms.is_some() {
        "PASS"
    } else if dim.proof_path.is_some() {
        "FAIL"
    } else {
        "-"
    }
}

/// Render full 4D table: one row per file, all dimensions.
fn render_full_table(modules: &[ModuleBench], show_functions: bool) {
    eprintln!(
        "{:<28} {:>7} {:>7}  | {:>6} {:>6} {:>6} {:>4} | {:>6} {:>6} {:>6} {:>4} | {:>6} {:>6} {:>6} {:>4} | {:>5}",
        "Module", "Compile", "Rust",
        "Exec", "Prove", "Verif", "",
        "Exec", "Prove", "Verif", "",
        "Exec", "Prove", "Verif", "",
        "Ratio",
    );
    eprintln!(
        "{:<28} {:>7} {:>7}  | {:<27} | {:<27} | {:<27} | {:>5}",
        "", "", "", "Classic", "Hand", "Neural", "",
    );
    eprintln!("{}", "-".repeat(148));

    for mb in modules {
        let ratio = if mb.hand_insn > 0 {
            format!("{:.2}x", mb.classic_insn as f64 / mb.hand_insn as f64)
        } else {
            "-".to_string()
        };

        eprintln!(
            "{:<28} {:>7} {:>7}  | {:>6} {:>6} {:>6} {:>4} | {:>6} {:>6} {:>6} {:>4} | {:>6} {:>6} {:>6} {:>4} | {:>5}",
            mb.name,
            format!("{:.1}ms", mb.compile_ms),
            fmt_rust(mb.rust_ns),
            fmt_ms(mb.classic.exec_ms), fmt_ms(mb.classic.prove_ms), fmt_ms(mb.classic.verify_ms), fmt_verify(&mb.classic),
            fmt_ms(mb.hand.exec_ms), fmt_ms(mb.hand.prove_ms), fmt_ms(mb.hand.verify_ms), fmt_verify(&mb.hand),
            fmt_ms(mb.neural.exec_ms), fmt_ms(mb.neural.prove_ms), fmt_ms(mb.neural.verify_ms), fmt_verify(&mb.neural),
            ratio,
        );

        if show_functions {
            for f in &mb.functions {
                let fr = if f.baseline_instructions > 0 {
                    format!(
                        "{:.2}x",
                        f.compiled_instructions as f64 / f.baseline_instructions as f64
                    )
                } else {
                    "-".to_string()
                };
                eprintln!(
                    "  {:<26} {:>5}/{:<5} {}",
                    f.name,
                    if f.compiled_instructions > 0 {
                        f.compiled_instructions.to_string()
                    } else {
                        "-".to_string()
                    },
                    f.baseline_instructions,
                    fr,
                );
            }
        }
    }

    eprintln!("{}", "-".repeat(148));

    // Summary row
    let sum_classic: usize = modules.iter().map(|m| m.classic_insn).sum();
    let sum_hand: usize = modules.iter().map(|m| m.hand_insn).sum();
    let avg_ratio = if sum_hand > 0 {
        format!("{:.2}x", sum_classic as f64 / sum_hand as f64)
    } else {
        "-".to_string()
    };

    let total_compile: f64 = modules.iter().map(|m| m.compile_ms).sum();
    let total_rust_ns: u64 = modules.iter().filter_map(|m| m.rust_ns).sum();
    let rust_count = modules.iter().filter(|m| m.rust_ns.is_some()).count();
    let sum_dim_col = |modules: &[ModuleBench],
                       dim: fn(&ModuleBench) -> &DimTiming,
                       get: fn(&DimTiming) -> Option<f64>|
     -> f64 { modules.iter().filter_map(|m| get(dim(m))).sum() };
    let classic_exec: f64 = sum_dim_col(modules, |m| &m.classic, |d| d.exec_ms);
    let classic_prove: f64 = sum_dim_col(modules, |m| &m.classic, |d| d.prove_ms);
    let classic_verify: f64 = sum_dim_col(modules, |m| &m.classic, |d| d.verify_ms);
    let classic_verified = modules
        .iter()
        .filter(|m| m.classic.verify_ms.is_some())
        .count();

    let hand_exec: f64 = sum_dim_col(modules, |m| &m.hand, |d| d.exec_ms);
    let hand_prove: f64 = sum_dim_col(modules, |m| &m.hand, |d| d.prove_ms);
    let hand_verify: f64 = sum_dim_col(modules, |m| &m.hand, |d| d.verify_ms);
    let hand_verified = modules
        .iter()
        .filter(|m| m.hand.verify_ms.is_some())
        .count();

    let neural_exec: f64 = sum_dim_col(modules, |m| &m.neural, |d| d.exec_ms);
    let neural_prove: f64 = sum_dim_col(modules, |m| &m.neural, |d| d.prove_ms);
    let neural_verify: f64 = sum_dim_col(modules, |m| &m.neural, |d| d.verify_ms);
    let neural_verified = modules
        .iter()
        .filter(|m| m.neural.verify_ms.is_some())
        .count();

    let n = modules.len();

    let fmt_total = |v: f64, count: usize, _total: usize| -> String {
        if count > 0 {
            format!("{:.0}ms", v)
        } else {
            "-".to_string()
        }
    };
    let fmt_count = |count: usize, total: usize| -> String {
        if count > 0 {
            format!("{}/{}", count, total)
        } else {
            "-".to_string()
        }
    };

    let rust_total_str = if rust_count > 0 {
        fmt_rust(Some(total_rust_ns))
    } else {
        "-".into()
    };

    eprintln!(
        "{:<28} {:>7} {:>7}  | {:>6} {:>6} {:>6} {:>4} | {:>6} {:>6} {:>6} {:>4} | {:>6} {:>6} {:>6} {:>4} | {:>5}",
        format!("TOTAL ({} modules)", n),
        format!("{:.0}ms", total_compile),
        rust_total_str,
        fmt_total(classic_exec, classic_verified, n), fmt_total(classic_prove, classic_verified, n), fmt_total(classic_verify, classic_verified, n), fmt_count(classic_verified, n),
        fmt_total(hand_exec, hand_verified, n), fmt_total(hand_prove, hand_verified, n), fmt_total(hand_verify, hand_verified, n), fmt_count(hand_verified, n),
        fmt_total(neural_exec, neural_verified, n), fmt_total(neural_prove, neural_verified, n), fmt_total(neural_verify, neural_verified, n), fmt_count(neural_verified, n),
        avg_ratio,
    );
    eprintln!(
        "{:<28} {:>7} {:>7}  | insn: {:<21} | insn: {:<21} |",
        "",
        "",
        "",
        format!("{}", sum_classic),
        format!("{}", sum_hand),
    );
}

/// Run execute + prove for a single dimension, writing results into DimTiming.
fn run_dimension(dim: &mut DimTiming, module_name: &str, label: &str, harness: &Harness) {
    let tmp_path = std::env::temp_dir().join(format!(
        "trident_bench_{}_{}.tasm",
        module_name.replace("::", "_"),
        label,
    ));
    if std::fs::write(&tmp_path, &harness.tasm).is_err() {
        return;
    }
    let tmp_str = tmp_path.to_string_lossy().to_string();
    // Execute
    if let Ok(r) = run_trisha_with_inputs(&["run", "--tasm", &tmp_str], harness) {
        dim.exec_ms = Some(r.elapsed_ms);
    }
    // Prove
    let proof_path = std::env::temp_dir().join(format!(
        "trident_bench_{}_{}.proof.toml",
        module_name.replace("::", "_"),
        label,
    ));
    let proof_str = proof_path.to_string_lossy().to_string();
    if let Ok(r) = run_trisha_with_inputs(
        &["prove", "--tasm", &tmp_str, "--output", &proof_str],
        harness,
    ) {
        dim.prove_ms = Some(r.elapsed_ms);
        if proof_path.exists() {
            dim.proof_path = Some(proof_path);
        }
    }
    let _ = std::fs::remove_file(&tmp_path);
}

/// Run verify for a dimension (requires proof_path from prove pass).
fn verify_dimension(dim: &mut DimTiming) {
    if let Some(ref proof_path) = dim.proof_path {
        if let Ok(r) = run_trisha(&["verify", &proof_path.to_string_lossy()]) {
            dim.verify_ms = Some(r.elapsed_ms);
        }
    }
}

/// Format nanoseconds for display: µs for >= 1000ns, ns otherwise, "-" if None.
fn fmt_rust(ns: Option<u64>) -> String {
    match ns {
        None => "-".into(),
        Some(n) if n >= 1_000_000 => format!("{:.1}ms", n as f64 / 1_000_000.0),
        Some(n) if n >= 1_000 => format!("{:.1}µs", n as f64 / 1_000.0),
        Some(n) => format!("{}ns", n),
    }
}

/// Run a Rust reference benchmark. Expects a `.reference.rs` file that is
/// registered as a cargo example. Builds with --release, runs, parses
/// `rust_ns: <N>` from stdout.
fn run_rust_reference(ref_path: &str) -> Option<u64> {
    // Derive example name from path: benches/std/crypto/poseidon2.reference.rs -> ref_std_crypto_poseidon2
    let name = ref_path
        .trim_start_matches("benches/")
        .trim_end_matches(".reference.rs")
        .replace('/', "_");
    let example_name = format!("ref_{}", name);

    // Build (should be near-instant if already built)
    let build = std::process::Command::new("cargo")
        .args(["build", "--example", &example_name, "--release", "--quiet"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .ok()?;
    if !build.success() {
        return None;
    }

    // Run
    let output = std::process::Command::new("cargo")
        .args(["run", "--example", &example_name, "--release", "--quiet"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    // Parse "rust_ns: <N>" from stdout
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.lines().find_map(|l| {
        l.strip_prefix("rust_ns: ")
            .and_then(|v| v.trim().parse().ok())
    })
}

/// Recursively find all .baseline.tasm files in a directory (depth-limited).
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
