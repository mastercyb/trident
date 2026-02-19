use std::path::PathBuf;
use std::process;

use clap::Args;

#[derive(Args)]
pub struct BenchArgs {
    /// Directory containing baseline .tasm files (mirrors source tree)
    #[arg(default_value = "benches")]
    pub dir: PathBuf,
    /// Verify correctness: compare classical, manual, neural TASM via stack verifier
    #[arg(long)]
    pub verify: bool,
    /// Measure execution speed (cycle count via trisha run)
    #[arg(long)]
    pub exec: bool,
    /// Measure proving time (via trisha prove)
    #[arg(long)]
    pub prove: bool,
    /// Measure verification time (via trisha verify)
    #[arg(long)]
    pub check: bool,
    /// Run all checks: --verify --exec --prove --check
    #[arg(long)]
    pub full: bool,
}

pub fn cmd_bench(args: BenchArgs) {
    let do_verify = args.verify || args.full;
    let do_exec = args.exec || args.full;
    let do_prove = args.prove || args.full;
    let do_check = args.check || args.full;

    let bench_dir = resolve_bench_dir(&args.dir);
    if !bench_dir.is_dir() {
        eprintln!("error: '{}' is not a directory", args.dir.display());
        process::exit(1);
    }

    // Find the project root (parent of benches/)
    let project_root = bench_dir
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));

    // Recursively find all .baseline.tasm files
    let mut baselines = find_baseline_files(&bench_dir, 0);
    baselines.sort();

    if baselines.is_empty() {
        eprintln!("No .baseline.tasm files found in '{}'", bench_dir.display());
        process::exit(1);
    }

    let options = trident::CompileOptions::default();
    let mut results: Vec<trident::ModuleBenchmarkResult> = Vec::new();

    for baseline_path in &baselines {
        // Map baseline to source: benches/std/crypto/auth.baseline.tasm -> std/crypto/auth.tri
        let rel = baseline_path
            .strip_prefix(&bench_dir)
            .unwrap_or(baseline_path);
        let rel_str = rel.to_string_lossy();
        let source_rel = rel_str.replace(".baseline.tasm", ".tri");
        let source_path = project_root.join(&source_rel);
        let module_name = source_rel.trim_end_matches(".tri").replace('/', ".");

        if !source_path.exists() {
            eprintln!(
                "  SKIP  {}  (source not found: {})",
                module_name,
                source_path.display()
            );
            continue;
        }

        // Compile the module (no linking, no DCE)
        let compiled_tasm = match trident::compile_module(&source_path, &options) {
            Ok(t) => t,
            Err(_) => {
                eprintln!("  FAIL  {}  (compilation error)", module_name);
                continue;
            }
        };

        // Read baseline
        let baseline_tasm = match std::fs::read_to_string(baseline_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("  FAIL  {}  (read error: {})", module_name, e);
                continue;
            }
        };

        // Parse both into per-function instruction maps
        let compiled_fns = trident::parse_tasm_functions(&compiled_tasm);
        let baseline_fns = trident::parse_tasm_functions(&baseline_tasm);

        // Compare: only functions present in the baseline
        let mut fn_results: Vec<trident::FunctionBenchmark> = Vec::new();
        let mut total_compiled: usize = 0;
        let mut total_baseline: usize = 0;

        for (name, &baseline_count) in &baseline_fns {
            let compiled_count = compiled_fns.get(name).copied().unwrap_or(0);
            total_compiled += compiled_count;
            total_baseline += baseline_count;
            fn_results.push(trident::FunctionBenchmark {
                name: name.clone(),
                compiled_instructions: compiled_count,
                baseline_instructions: baseline_count,
            });
        }

        results.push(trident::ModuleBenchmarkResult {
            module_path: module_name,
            functions: fn_results,
            total_compiled,
            total_baseline,
        });
    }

    if results.is_empty() {
        eprintln!("No benchmarks could be compiled.");
        process::exit(1);
    }

    // Print results table
    eprintln!();
    eprintln!("{}", trident::ModuleBenchmarkResult::format_header());
    for (i, result) in results.iter().enumerate() {
        if i > 0 {
            eprintln!("{}", result.format_module_header());
        } else {
            // First module: print header row directly
            eprintln!(
                "\u{2502} {:<28} \u{2502} {:>8} \u{2502} {:>8} \u{2502} {:>7} \u{2502} {} \u{2502}",
                result.module_path,
                trident::fmt_num(result.total_compiled),
                trident::fmt_num(result.total_baseline),
                trident::fmt_ratio(result.total_compiled, result.total_baseline),
                trident::status_icon(result.total_compiled, result.total_baseline),
            );
        }
        for f in &result.functions {
            eprintln!("{}", result.format_function(f));
        }
    }
    eprintln!("{}", trident::ModuleBenchmarkResult::format_separator());

    // Summary: average = total_compiled_all / total_baseline_all
    // max = module with highest compiled/baseline ratio (by cross-multiply)
    if !results.is_empty() {
        let sum_compiled: usize = results.iter().map(|r| r.total_compiled).sum();
        let sum_baseline: usize = results.iter().map(|r| r.total_baseline).sum();
        // Find module with maximum ratio via cross-multiplication
        let (max_compiled, max_baseline) = results
            .iter()
            .map(|r| (r.total_compiled, r.total_baseline))
            .fold((0usize, 1usize), |(ac, ad), (bc, bd)| {
                // Compare ac/ad vs bc/bd via cross-multiply: ac*bd vs bc*ad
                if ac * bd >= bc * ad {
                    (ac, ad)
                } else {
                    (bc, bd)
                }
            });
        eprintln!(
            "{}",
            trident::ModuleBenchmarkResult::format_summary(
                sum_compiled,
                sum_baseline,
                max_compiled,
                max_baseline,
                results.len(),
            )
        );
    }
    // --- Rust reference: compilation timing ---
    if do_verify || do_exec || do_prove || do_check {
        eprintln!();
        eprintln!("Compilation (Rust native):");
        run_compile_pass(&bench_dir, project_root, &baselines, &options);
    }

    // --- 4D Verification: --verify ---
    if do_verify {
        eprintln!();
        eprintln!("Verification (stack verifier):");
        run_verify_pass(&bench_dir, project_root, &baselines, &options);
    }

    // --- Execution speed: --exec ---
    if do_exec {
        eprintln!();
        eprintln!("Execution (trisha run --tasm):");
        run_exec_pass(&bench_dir, project_root, &baselines, &options);
    }

    // --- Proving time: --prove ---
    let proof_files = if do_prove {
        eprintln!();
        eprintln!("Proving (trisha prove --tasm):");
        run_prove_pass(&bench_dir, project_root, &baselines, &options)
    } else {
        Vec::new()
    };

    // --- Verification time: --check ---
    if do_check {
        eprintln!();
        if proof_files.is_empty() {
            eprintln!("Verification time:");
            eprintln!("  no proof files — run with --prove or --full first");
        } else {
            eprintln!("Verification (trisha verify):");
            run_check_pass(&proof_files);
        }
    }
    // Clean up proof files
    for (_, path) in &proof_files {
        let _ = std::fs::remove_file(path);
    }

    eprintln!();
}

/// Run compilation timing: measure how long trident takes to compile each module.
fn run_compile_pass(
    bench_dir: &std::path::Path,
    project_root: &std::path::Path,
    baselines: &[PathBuf],
    options: &trident::CompileOptions,
) {
    let mut total_ms = 0.0f64;
    let mut count = 0;

    for baseline_path in baselines {
        let rel = baseline_path
            .strip_prefix(bench_dir)
            .unwrap_or(baseline_path);
        let rel_str = rel.to_string_lossy();
        let source_rel = rel_str.replace(".baseline.tasm", ".tri");
        let source_path = project_root.join(&source_rel);
        let module_name = source_rel.trim_end_matches(".tri").replace('/', "::");

        if !source_path.exists() {
            continue;
        }

        let start = std::time::Instant::now();
        let _guard = trident::diagnostic::suppress_warnings();
        let result = trident::compile_project_with_options(&source_path, options);
        drop(_guard);
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;

        match result {
            Ok(tasm) => {
                let lines = tasm.lines().count();
                eprintln!(
                    "  {:<45} {:>6.1}ms  {} lines TASM",
                    module_name, elapsed_ms, lines,
                );
                total_ms += elapsed_ms;
                count += 1;
            }
            Err(_) => {
                eprintln!("  {:<45} SKIP (compilation error)", module_name);
            }
        }
    }

    if count > 0 {
        eprintln!(
            "  total: {:.1}ms ({} modules, {:.1}ms avg)",
            total_ms,
            count,
            total_ms / count as f64,
        );
    }
}

/// Run correctness verification: for each baseline function, compare
/// classical compiler output vs manual baseline via stack verifier.
fn run_verify_pass(
    bench_dir: &std::path::Path,
    project_root: &std::path::Path,
    baselines: &[PathBuf],
    options: &trident::CompileOptions,
) {
    use trident::cost::stack_verifier;

    let mut total_pass = 0usize;
    let mut total_fail = 0usize;
    let mut total_skip = 0usize;

    for baseline_path in baselines {
        let rel = baseline_path
            .strip_prefix(bench_dir)
            .unwrap_or(baseline_path);
        let rel_str = rel.to_string_lossy();
        let source_rel = rel_str.replace(".baseline.tasm", ".tri");
        let source_path = project_root.join(&source_rel);
        let module_name = source_rel.trim_end_matches(".tri").replace('/', "::");

        if !source_path.exists() {
            continue;
        }

        // Compile to TASM via classical pipeline
        let compiled_tasm = match trident::compile_module(&source_path, options) {
            Ok(t) => t,
            Err(_) => continue,
        };

        // Read manual baseline
        let baseline_tasm = match std::fs::read_to_string(baseline_path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        // Parse into per-function instruction lists
        let compiled_fns = parse_tasm_to_lines(&compiled_tasm);
        let baseline_fns = parse_tasm_to_lines(&baseline_tasm);

        let mut module_pass = 0;
        let mut module_fail = 0;
        let mut module_skip = 0;

        for (fn_name, baseline_lines) in &baseline_fns {
            let compiled_lines = match compiled_fns.get(fn_name) {
                Some(lines) => lines,
                None => {
                    module_skip += 1;
                    continue;
                }
            };

            // Run stack verifier: does classical produce same stack as manual baseline?
            // Test with multiple seeds for confidence
            let mut all_pass = true;
            let mut any_simulated = false;
            for seed in 0..5u64 {
                let test_stack = stack_verifier::generate_test_stack(seed, 16);
                let mut baseline_state = stack_verifier::StackState::new(test_stack.clone());
                baseline_state.execute(baseline_lines);
                if baseline_state.error {
                    continue; // can't simulate this function (has control flow, etc.)
                }
                any_simulated = true;
                let mut compiled_state = stack_verifier::StackState::new(test_stack);
                compiled_state.execute(compiled_lines);
                if compiled_state.error || compiled_state.stack != baseline_state.stack {
                    all_pass = false;
                    break;
                }
            }

            if !any_simulated {
                module_skip += 1;
            } else if all_pass {
                module_pass += 1;
            } else {
                module_fail += 1;
                eprintln!("  FAIL  {}::{}", module_name, fn_name);
            }
        }

        let status = if module_fail > 0 {
            "FAIL"
        } else if module_pass > 0 {
            " ok "
        } else {
            "skip"
        };
        if module_fail > 0 || module_pass > 0 {
            eprintln!(
                "  [{}] {:<40} {} pass, {} fail, {} skip",
                status, module_name, module_pass, module_fail, module_skip
            );
        }

        total_pass += module_pass;
        total_fail += module_fail;
        total_skip += module_skip;
    }

    eprintln!();
    if total_fail > 0 {
        eprintln!(
            "  RESULT: {} passed, {} FAILED, {} skipped",
            total_pass, total_fail, total_skip
        );
    } else {
        eprintln!(
            "  RESULT: {} passed, {} skipped (all ok)",
            total_pass, total_skip
        );
    }
}

/// Parse TASM text into per-function instruction line lists.
/// Returns map of function_name -> Vec<instruction lines>.
fn parse_tasm_to_lines(tasm: &str) -> std::collections::HashMap<String, Vec<String>> {
    let mut fns: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    let mut current_fn: Option<String> = None;
    let mut current_lines: Vec<String> = Vec::new();

    for line in tasm.lines() {
        let trimmed = line.trim();
        // Function label: __name: at start of line (not indented)
        if trimmed.ends_with(':') && !trimmed.starts_with("//") && !line.starts_with(' ') {
            // Save previous function
            if let Some(ref name) = current_fn {
                fns.insert(name.clone(), std::mem::take(&mut current_lines));
            }
            let label = trimmed.trim_end_matches(':').trim_start_matches('_');
            current_fn = Some(label.to_string());
            current_lines.clear();
        } else if current_fn.is_some() && !trimmed.is_empty() {
            current_lines.push(trimmed.to_string());
        }
    }
    // Save last function
    if let Some(name) = current_fn {
        fns.insert(name, current_lines);
    }
    fns
}

/// Run execution pass: compile each benchmark's .tri to TASM, execute via trisha.
fn run_exec_pass(
    bench_dir: &std::path::Path,
    project_root: &std::path::Path,
    baselines: &[PathBuf],
    options: &trident::CompileOptions,
) {
    // Check trisha is available
    if !trisha_available() {
        eprintln!("  trisha not found in PATH — install trisha first");
        return;
    }

    for baseline_path in baselines {
        let rel = baseline_path
            .strip_prefix(bench_dir)
            .unwrap_or(baseline_path);
        let rel_str = rel.to_string_lossy();
        let source_rel = rel_str.replace(".baseline.tasm", ".tri");
        let source_path = project_root.join(&source_rel);
        let module_name = source_rel.trim_end_matches(".tri").replace('/', "::");

        if !source_path.exists() {
            continue;
        }

        // Compile to full TASM program via project pipeline (includes entry point + halt)
        let tasm = match trident::compile_project_with_options(&source_path, options) {
            Ok(t) => t,
            Err(_) => {
                eprintln!("  SKIP  {}  (compilation error)", module_name);
                continue;
            }
        };

        // Write to temp file and execute via trisha
        let tmp_path = std::env::temp_dir().join(format!(
            "trident_bench_{}.tasm",
            module_name.replace("::", "_")
        ));
        if std::fs::write(&tmp_path, &tasm).is_err() {
            continue;
        }

        match run_trisha(&["run", "--tasm", &tmp_path.to_string_lossy()]) {
            Ok(trisha_result) => {
                eprintln!(
                    "  {:<45} {} cyc  {:>6.1}ms  output: [{}]",
                    module_name,
                    trisha_result.cycle_count,
                    trisha_result.elapsed_ms,
                    trisha_result
                        .output
                        .iter()
                        .map(|v| v.to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                );
            }
            Err(e) => {
                eprintln!("  {:<45} ERROR: {}", module_name, e);
            }
        }

        let _ = std::fs::remove_file(&tmp_path);
    }
}

/// Run proving pass: compile, prove via trisha. Returns (module_name, proof_path) pairs.
fn run_prove_pass(
    bench_dir: &std::path::Path,
    project_root: &std::path::Path,
    baselines: &[PathBuf],
    options: &trident::CompileOptions,
) -> Vec<(String, PathBuf)> {
    let mut proof_files: Vec<(String, PathBuf)> = Vec::new();

    if !trisha_available() {
        eprintln!("  trisha not found in PATH — install trisha first");
        return proof_files;
    }

    for baseline_path in baselines {
        let rel = baseline_path
            .strip_prefix(bench_dir)
            .unwrap_or(baseline_path);
        let rel_str = rel.to_string_lossy();
        let source_rel = rel_str.replace(".baseline.tasm", ".tri");
        let source_path = project_root.join(&source_rel);
        let module_name = source_rel.trim_end_matches(".tri").replace('/', "::");

        if !source_path.exists() {
            continue;
        }

        let tasm = match trident::compile_project_with_options(&source_path, options) {
            Ok(t) => t,
            Err(_) => {
                eprintln!("  SKIP  {}  (compilation error)", module_name);
                continue;
            }
        };

        let tmp_path = std::env::temp_dir().join(format!(
            "trident_bench_{}.tasm",
            module_name.replace("::", "_")
        ));
        if std::fs::write(&tmp_path, &tasm).is_err() {
            continue;
        }

        let proof_path = std::env::temp_dir().join(format!(
            "trident_bench_{}.proof.toml",
            module_name.replace("::", "_")
        ));
        match run_trisha(&[
            "prove",
            "--tasm",
            &tmp_path.to_string_lossy(),
            "--output",
            &proof_path.to_string_lossy(),
        ]) {
            Ok(trisha_result) => {
                eprintln!(
                    "  {:<45} prove {:>8.1}ms",
                    module_name, trisha_result.elapsed_ms,
                );
                if proof_path.exists() {
                    proof_files.push((module_name.clone(), proof_path));
                }
            }
            Err(e) => {
                eprintln!("  {:<45} ERROR: {}", module_name, e);
            }
        }

        let _ = std::fs::remove_file(&tmp_path);
    }

    proof_files
}

/// Run verification pass: verify each proof file via trisha verify.
fn run_check_pass(proof_files: &[(String, PathBuf)]) {
    if !trisha_available() {
        eprintln!("  trisha not found in PATH — install trisha first");
        return;
    }

    let mut pass = 0;
    let mut fail = 0;

    for (module_name, proof_path) in proof_files {
        match run_trisha(&["verify", &proof_path.to_string_lossy()]) {
            Ok(trisha_result) => {
                eprintln!(
                    "  {:<45} PASS  {:>8.1}ms",
                    module_name, trisha_result.elapsed_ms,
                );
                pass += 1;
            }
            Err(e) => {
                if e.contains("FAIL") {
                    eprintln!("  {:<45} FAIL", module_name);
                } else {
                    eprintln!("  {:<45} ERROR: {}", module_name, e);
                }
                fail += 1;
            }
        }
    }

    eprintln!();
    if fail > 0 {
        eprintln!("  RESULT: {} passed, {} FAILED", pass, fail);
    } else {
        eprintln!("  RESULT: {} passed (all verified)", pass);
    }
}

/// Result from a trisha subprocess call.
struct TrishaResult {
    output: Vec<u64>,
    cycle_count: u64,
    elapsed_ms: f64,
}

/// Check if trisha binary is available.
fn trisha_available() -> bool {
    std::process::Command::new("trisha")
        .arg("--help")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

/// Run trisha as a subprocess, parse output.
fn run_trisha(args: &[&str]) -> Result<TrishaResult, String> {
    let start = std::time::Instant::now();
    let result = std::process::Command::new("trisha")
        .args(args)
        .output()
        .map_err(|e| format!("failed to run trisha: {}", e))?;

    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        return Err(stderr.trim().to_string());
    }

    // stdout: output values (one per line)
    let stdout = String::from_utf8_lossy(&result.stdout);
    let output: Vec<u64> = stdout
        .lines()
        .filter_map(|l| l.trim().parse().ok())
        .collect();

    // stderr: "Executed in N cycles" or proving time
    let stderr = String::from_utf8_lossy(&result.stderr);
    let cycle_count = stderr
        .lines()
        .find_map(|l| {
            if l.contains("cycles") {
                l.split_whitespace().find_map(|w| w.parse::<u64>().ok())
            } else {
                None
            }
        })
        .unwrap_or(0);

    Ok(TrishaResult {
        output,
        cycle_count,
        elapsed_ms,
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
