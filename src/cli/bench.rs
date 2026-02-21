use std::path::{Path, PathBuf};
use std::process;

use clap::Args;

use super::trisha::{generate_test_harness, run_trisha, trisha_available, Harness};

use burn::backend::wgpu::{Wgpu, WgpuDevice};
use trident::neural::model::composite::NeuralCompilerV2;

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
    neural_insn: usize,
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

    // Load neural model once for all modules
    let wgpu_device = WgpuDevice::default();
    let neural_model: Option<NeuralCompilerV2<Wgpu>> =
        trident::neural::load_model::<Wgpu>(&wgpu_device);
    if neural_model.is_some() {
        eprint!("  Neural model loaded.\n");
    }

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

        // Neural: compile per-function via neural model
        let neural_tasm_opt = if let Some(ref model) = neural_model {
            let result = compile_neural_tasm_inline(
                &source_path,
                &compiled_tasm,
                &options,
                model,
                &wgpu_device,
            );
            result
        } else {
            None
        };
        let neural_insn_count = neural_tasm_opt
            .as_ref()
            .map(|t| {
                t.lines()
                    .filter(|l| {
                        let s = l.trim();
                        !s.is_empty() && !s.starts_with("//") && !s.ends_with(':') && s != "halt"
                    })
                    .count()
            })
            .unwrap_or(0);

        let mut mb = ModuleBench {
            name: module_name.clone(),
            classic_insn: total_compiled,
            hand_insn: total_baseline,
            neural_insn: neural_insn_count,
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

            // Neural: use inline-compiled neural TASM
            if let Some(ref neural_tasm) = neural_tasm_opt {
                if !neural_tasm.is_empty() {
                    let neural_harness = generate_test_harness(neural_tasm);
                    run_dimension(&mut mb.neural, &module_name, "neural", &neural_harness);
                }
            }
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
    eprintln!(
        "{:<40} {:>6} {:>6} {:>6} {:>7}",
        "Module", "Tri", "Hand", "Neural", "Ratio"
    );
    eprintln!("{}", "-".repeat(70));

    for mb in modules {
        let ratio = if mb.hand_insn > 0 {
            format!("{:.2}x", mb.classic_insn as f64 / mb.hand_insn as f64)
        } else {
            "-".to_string()
        };
        let neural_str = if mb.neural_insn > 0 {
            mb.neural_insn.to_string()
        } else {
            "-".to_string()
        };
        eprintln!(
            "{:<40} {:>6} {:>6} {:>6} {:>7}",
            mb.name, mb.classic_insn, mb.hand_insn, neural_str, ratio,
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
                    "  {:<38} {:>6} {:>6} {:>6} {:>7}",
                    f.name,
                    if f.compiled_instructions > 0 {
                        f.compiled_instructions.to_string()
                    } else {
                        "-".to_string()
                    },
                    f.baseline_instructions,
                    "", // per-function neural not tracked here
                    fr,
                );
            }
        }
    }

    eprintln!("{}", "-".repeat(70));
    let sum_classic: usize = modules.iter().map(|m| m.classic_insn).sum();
    let sum_hand: usize = modules.iter().map(|m| m.hand_insn).sum();
    let sum_neural: usize = modules.iter().map(|m| m.neural_insn).sum();
    let avg_ratio = if sum_hand > 0 {
        format!("{:.2}x", sum_classic as f64 / sum_hand as f64)
    } else {
        "-".to_string()
    };
    let neural_total = if sum_neural > 0 {
        sum_neural.to_string()
    } else {
        "-".to_string()
    };
    eprintln!(
        "{:<40} {:>6} {:>6} {:>6} {:>7}",
        format!("TOTAL ({} modules)", modules.len()),
        sum_classic,
        sum_hand,
        neural_total,
        avg_ratio,
    );
}

/// Format a millisecond value, or "-" if None.
fn fmt_ms(ms: Option<f64>) -> String {
    ms.map(|v| format!("{:.0}ms", v))
        .unwrap_or_else(|| "-".into())
}

/// Compact verify status for a row: shows PASS/FAIL based on best result across dimensions.
fn fmt_verify_row(classic: &DimTiming, hand: &DimTiming, neural: &DimTiming) -> &'static str {
    let any_pass =
        classic.verify_ms.is_some() || hand.verify_ms.is_some() || neural.verify_ms.is_some();
    let any_proof =
        classic.proof_path.is_some() || hand.proof_path.is_some() || neural.proof_path.is_some();
    if any_pass {
        "PASS"
    } else if any_proof {
        "FAIL"
    } else {
        "-"
    }
}

/// Render full 4D table: grouped by step (Exec | Prove | Verify), sub-columns C/H/N.
fn render_full_table(modules: &[ModuleBench], show_functions: bool) {
    // Header: Module  Compile  Rust | Exec (C H N) | Prove (C H N) | Verify (C H N) | Ratio
    eprintln!(
        "{:<28} {:>7} {:>7}  | {:>5} {:>5} {:>5} | {:>7} {:>7} {:>7} | {:>5} {:>5} {:>5} {:>4} | {:>5}",
        "Module", "Compile", "Rust",
        "C", "H", "N",
        "C", "H", "N",
        "C", "H", "N", "",
        "Ratio",
    );
    eprintln!(
        "{:<28} {:>7} {:>7}  | {:<17} | {:<23} | {:<22} | {:>5}",
        "", "", "", "Exec", "Prove", "Verify", "",
    );
    eprintln!("{}", "-".repeat(132));

    for mb in modules {
        let ratio = if mb.hand_insn > 0 {
            format!("{:.2}x", mb.classic_insn as f64 / mb.hand_insn as f64)
        } else {
            "-".to_string()
        };

        eprintln!(
            "{:<28} {:>7} {:>7}  | {:>5} {:>5} {:>5} | {:>7} {:>7} {:>7} | {:>5} {:>5} {:>5} {:>4} | {:>5}",
            mb.name,
            format!("{:.1}ms", mb.compile_ms),
            fmt_rust(mb.rust_ns),
            fmt_ms(mb.classic.exec_ms), fmt_ms(mb.hand.exec_ms), fmt_ms(mb.neural.exec_ms),
            fmt_ms(mb.classic.prove_ms), fmt_ms(mb.hand.prove_ms), fmt_ms(mb.neural.prove_ms),
            fmt_ms(mb.classic.verify_ms), fmt_ms(mb.hand.verify_ms), fmt_ms(mb.neural.verify_ms),
            fmt_verify_row(&mb.classic, &mb.hand, &mb.neural),
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

    eprintln!("{}", "-".repeat(132));

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
    let hand_exec: f64 = sum_dim_col(modules, |m| &m.hand, |d| d.exec_ms);
    let hand_prove: f64 = sum_dim_col(modules, |m| &m.hand, |d| d.prove_ms);
    let hand_verify: f64 = sum_dim_col(modules, |m| &m.hand, |d| d.verify_ms);
    let neural_exec: f64 = sum_dim_col(modules, |m| &m.neural, |d| d.exec_ms);
    let neural_prove: f64 = sum_dim_col(modules, |m| &m.neural, |d| d.prove_ms);
    let neural_verify: f64 = sum_dim_col(modules, |m| &m.neural, |d| d.verify_ms);

    let classic_verified = modules
        .iter()
        .filter(|m| m.classic.verify_ms.is_some())
        .count();
    let hand_verified = modules
        .iter()
        .filter(|m| m.hand.verify_ms.is_some())
        .count();
    let neural_verified = modules
        .iter()
        .filter(|m| m.neural.verify_ms.is_some())
        .count();
    let n = modules.len();

    let rust_total_str = if rust_count > 0 {
        fmt_rust(Some(total_rust_ns))
    } else {
        "-".into()
    };

    let fmt_t = |v: f64, has: bool| -> String {
        if has {
            format!("{:.0}ms", v)
        } else {
            "-".into()
        }
    };

    eprintln!(
        "{:<28} {:>7} {:>7}  | {:>5} {:>5} {:>5} | {:>7} {:>7} {:>7} | {:>5} {:>5} {:>5} {:>4} | {:>5}",
        format!("TOTAL ({} modules)", n),
        format!("{:.0}ms", total_compile),
        rust_total_str,
        fmt_t(classic_exec, classic_verified > 0), fmt_t(hand_exec, hand_verified > 0), fmt_t(neural_exec, neural_verified > 0),
        fmt_t(classic_prove, classic_verified > 0), fmt_t(hand_prove, hand_verified > 0), fmt_t(neural_prove, neural_verified > 0),
        fmt_t(classic_verify, classic_verified > 0), fmt_t(hand_verify, hand_verified > 0), fmt_t(neural_verify, neural_verified > 0),
        format!("{}/{}", classic_verified, n),
        avg_ratio,
    );
    eprintln!(
        "{:<28} {:>7} {:>7}  | insn: {:<11} |",
        "",
        "",
        "",
        format!("{}C / {}H", sum_classic, sum_hand),
    );
}

/// Run trisha with a timeout. Kills the process if it exceeds the deadline.
fn run_trisha_timed(
    base_args: &[&str],
    harness: &Harness,
    timeout: std::time::Duration,
) -> Result<super::trisha::TrishaResult, String> {
    use super::trisha::trisha_args_with_inputs;

    let args = trisha_args_with_inputs(base_args, harness);
    let start = std::time::Instant::now();
    let mut child = std::process::Command::new("trisha")
        .args(&args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("failed to spawn trisha: {}", e))?;

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
                if !status.success() {
                    return Err("failed".to_string());
                }
                return Ok(super::trisha::TrishaResult {
                    output: Vec::new(),
                    cycle_count: 0,
                    elapsed_ms,
                });
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("timed out".to_string());
                }
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
            Err(e) => return Err(format!("wait error: {}", e)),
        }
    }
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
    // Execute (30s timeout)
    if let Ok(r) = run_trisha_timed(
        &["run", "--tasm", &tmp_str],
        harness,
        std::time::Duration::from_secs(30),
    ) {
        dim.exec_ms = Some(r.elapsed_ms);
    }
    // Prove (2min timeout)
    let proof_path = std::env::temp_dir().join(format!(
        "trident_bench_{}_{}.proof.toml",
        module_name.replace("::", "_"),
        label,
    ));
    let proof_str = proof_path.to_string_lossy().to_string();
    if let Ok(r) = run_trisha_timed(
        &["prove", "--tasm", &tmp_str, "--output", &proof_str],
        harness,
        std::time::Duration::from_secs(120),
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

/// Compile a module with neural optimization using a pre-loaded model.
///
/// Splits TIR into per-function blocks (matching training), runs neural
/// beam search on each, and assembles the result. For each function,
/// picks neural output if valid and cost <= compiler, else keeps compiler output.
fn compile_neural_tasm_inline(
    source_path: &Path,
    _classical_tasm: &str,
    options: &trident::CompileOptions,
    model: &NeuralCompilerV2<Wgpu>,
    device: &WgpuDevice,
) -> Option<String> {
    use trident::neural::data::pairs::split_tir_by_function;

    // Build TIR
    let _guard = trident::diagnostic::suppress_warnings();
    let ir = match trident::build_tir_project(source_path, options) {
        Ok(ir) => ir,
        Err(_) => return None,
    };
    drop(_guard);

    let functions = split_tir_by_function(&ir);
    if functions.is_empty() {
        return None;
    }

    let lowering = trident::ir::tir::lower::create_stack_lowering(&options.target_config.name);
    let mut result_lines: Vec<String> = Vec::new();
    let mut any_neural = false;

    for (fn_name, fn_tir) in &functions {
        if fn_name.starts_with("__") || fn_tir.is_empty() {
            // Keep compiler output for internal functions
            let fn_baseline = lowering.lower(fn_tir);
            result_lines.extend(fn_baseline);
            continue;
        }

        // Lower this function's TIR to get full compiler output (with labels)
        let fn_full = lowering.lower(fn_tir);

        // Extract just instructions (no labels/comments) for neural comparison
        let fn_insns: Vec<String> = fn_full
            .iter()
            .filter(|l| {
                let t = l.trim();
                !t.is_empty() && !t.ends_with(':') && !t.starts_with("//")
            })
            .map(|l| l.trim().to_string())
            .collect();

        if fn_insns.is_empty() {
            result_lines.extend(fn_full);
            continue;
        }

        let compiler_cost = trident::cost::scorer::profile_tasm(
            &fn_insns.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        )
        .cost()
        .max(1);

        // Try neural compilation
        match trident::neural::compile_with_model(fn_tir, &fn_insns, model, device) {
            Ok(r) if r.neural && r.cost <= compiler_cost => {
                any_neural = true;
                // Emit labeled function: __fn_name: + neural body + return
                result_lines.push(format!("// {}: neural (cost {})", fn_name, r.cost));
                result_lines.push(format!("__{}:", fn_name));
                let needs_return = !r.tasm_lines.last().is_some_and(|l| l.trim() == "return");
                result_lines.extend(r.tasm_lines);
                if needs_return {
                    result_lines.push("return".to_string());
                }
            }
            _ => {
                // Use full compiler output (already has labels)
                result_lines.push(format!("// {}: compiler (cost {})", fn_name, compiler_cost));
                result_lines.extend(fn_full);
            }
        }
    }

    if !any_neural {
        return None;
    }

    // Add halt at end for executability
    result_lines.push("halt".to_string());

    // Also write .neural.tasm to disk as cache for future runs
    let classical_path = source_path.to_string_lossy();
    if let Some(bench_path) = derive_neural_tasm_path(&classical_path) {
        let _ = std::fs::write(&bench_path, result_lines.join("\n"));
    }

    Some(result_lines.join("\n"))
}

/// Derive the .neural.tasm path from a source .tri path.
/// E.g. std/crypto/poseidon2.tri -> benches/std/crypto/poseidon2.neural.tasm
fn derive_neural_tasm_path(source_path: &str) -> Option<PathBuf> {
    // Find the relative part after the project root
    let source = Path::new(source_path);
    let file_stem = source.file_stem()?.to_string_lossy();
    let parent = source.parent()?;

    // Walk up to find "benches" sibling
    let mut ancestor = parent;
    let mut rel_parts = vec![file_stem.to_string()];
    loop {
        if let Some(name) = ancestor.file_name() {
            rel_parts.push(name.to_string_lossy().to_string());
            ancestor = ancestor.parent()?;
            // Check if benches/ exists as sibling
            let benches = ancestor.join("benches");
            if benches.is_dir() {
                rel_parts.reverse();
                let rel = rel_parts.join("/");
                return Some(benches.join(format!("{}.neural.tasm", rel)));
            }
        } else {
            return None;
        }
    }
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
