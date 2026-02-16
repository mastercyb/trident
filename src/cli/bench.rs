use std::path::PathBuf;
use std::process;

use clap::Args;

#[derive(Args)]
pub struct BenchArgs {
    /// Directory containing baseline .tasm files (mirrors source tree)
    #[arg(default_value = "benches")]
    pub dir: PathBuf,
}

pub fn cmd_bench(args: BenchArgs) {
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
    let mut baselines = find_baseline_files(&bench_dir);
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
                fmt_num(result.total_compiled),
                fmt_num(result.total_baseline),
                fmt_ratio(result.total_compiled, result.total_baseline),
                status_icon(result.total_compiled, result.total_baseline),
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
    eprintln!();
}

fn fmt_num(n: usize) -> String {
    if n == 0 {
        return "\u{2014}".to_string();
    }
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    result
}

fn fmt_ratio(num: usize, den: usize) -> String {
    if den == 0 {
        "\u{2014}".to_string()
    } else {
        let ratio_100 = num * 100 / den;
        format!("{}.{:02}x", ratio_100 / 100, ratio_100 % 100)
    }
}

fn status_icon(num: usize, den: usize) -> &'static str {
    if den == 0 {
        " "
    } else if num <= 2 * den {
        "\u{2713}"
    } else {
        "\u{25b3}"
    }
}

/// Recursively find all .baseline.tasm files in a directory.
fn find_baseline_files(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(find_baseline_files(&path));
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
