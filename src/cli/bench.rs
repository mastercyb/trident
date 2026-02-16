use std::path::PathBuf;
use std::process;

use clap::Args;

#[derive(Args)]
pub struct BenchArgs {
    /// Directory containing benchmark .tri + .baseline.tasm files
    #[arg(default_value = "benches")]
    pub dir: PathBuf,
}

pub fn cmd_bench(args: BenchArgs) {
    let dir = resolve_bench_dir(&args.dir);
    if !dir.is_dir() {
        eprintln!("error: '{}' is not a directory", args.dir.display());
        process::exit(1);
    }

    // Find all .tri files in the bench directory
    let mut tri_files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| {
            eprintln!("error: cannot read '{}': {}", dir.display(), e);
            process::exit(1);
        })
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|p| p.extension().is_some_and(|e| e == "tri"))
        .collect();
    tri_files.sort();

    if tri_files.is_empty() {
        eprintln!("No benchmark .tri files found in '{}'", dir.display());
        process::exit(1);
    }

    let options = trident::CompileOptions::default();
    let mut results: Vec<trident::BenchmarkResult> = Vec::new();

    for tri_path in &tri_files {
        let stem = tri_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let baseline_path = dir.join(format!("{}.baseline.tasm", stem));

        // Compile the Trident program
        let tasm = match trident::compile_project_with_options(tri_path, &options) {
            Ok(t) => t,
            Err(_) => {
                eprintln!("  FAIL  {}  (compilation error)", stem);
                continue;
            }
        };

        let trident_count = trident::count_tasm_instructions(&tasm);

        // Get cost analysis for padded height
        let trident_padded = trident::analyze_costs_project(tri_path, &options)
            .map(|c| c.padded_height)
            .unwrap_or(0);

        // Read baseline if available
        let (baseline_count, baseline_padded) = if baseline_path.exists() {
            let baseline = std::fs::read_to_string(&baseline_path).unwrap_or_default();
            let count = trident::count_tasm_instructions(&baseline);
            // Baseline padded height: count instructions as approximate processor rows
            let padded = (count as u64).next_power_of_two();
            (count, padded)
        } else {
            (0, 0)
        };

        let ratio = if baseline_count > 0 {
            trident_count as f64 / baseline_count as f64
        } else {
            0.0
        };

        results.push(trident::BenchmarkResult {
            name: stem,
            trident_instructions: trident_count,
            baseline_instructions: baseline_count,
            overhead_ratio: ratio,
            trident_padded_height: trident_padded,
            baseline_padded_height: baseline_padded,
        });
    }

    // Print results table
    eprintln!();
    eprintln!("{}", trident::BenchmarkResult::format_header());
    for result in &results {
        eprintln!("{}", result.format());
    }
    eprintln!("{}", trident::BenchmarkResult::format_separator());

    // Summary
    let with_baseline: Vec<_> = results
        .iter()
        .filter(|r| r.baseline_instructions > 0)
        .collect();
    if !with_baseline.is_empty() {
        let avg_ratio: f64 = with_baseline.iter().map(|r| r.overhead_ratio).sum::<f64>()
            / with_baseline.len() as f64;
        let max_ratio = with_baseline
            .iter()
            .map(|r| r.overhead_ratio)
            .fold(0.0f64, f64::max);
        eprintln!(
            "{}",
            trident::BenchmarkResult::format_summary(avg_ratio, max_ratio, with_baseline.len())
        );
    }
    eprintln!();
}

/// Resolve the bench directory by searching ancestor directories.
/// If the given path exists, use it directly. Otherwise walk up from CWD
/// looking for a directory with that name (e.g. "benches").
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
