use std::path::Path;
use std::process;

use clap::{Args, Subcommand};

#[derive(Args)]
pub struct TrainArgs {
    #[command(subcommand)]
    pub action: Option<TrainAction>,
    /// Epochs over the full corpus (default: 10)
    #[arg(short, long, default_value = "10")]
    pub epochs: u64,
}

#[derive(Subcommand)]
pub enum TrainAction {
    /// Delete all neural weights and generated .neural.tasm files
    Reset,
}

/// Pre-compiled file data — TIR + baselines, computed once.
struct CompiledFile {
    path: String,
    tir_ops: Vec<trident::tir::TIROp>,
    tasm_lines: Vec<String>,
    baseline_cost: u64,
}

pub fn cmd_train(args: TrainArgs) {
    if let Some(TrainAction::Reset) = args.action {
        cmd_train_reset();
        return;
    }

    use burn::backend::Autodiff;
    use burn::backend::NdArray;
    use trident::neural::data::pairs::extract_pairs;
    use trident::neural::model::composite::NeuralCompilerConfig;
    use trident::neural::model::vocab::Vocab;
    use trident::neural::training::supervised;

    type TrainBackend = Autodiff<NdArray>;

    let corpus = discover_corpus();
    if corpus.is_empty() {
        eprintln!("error: no .tri files found in vm/, std/, os/");
        process::exit(1);
    }

    eprintln!("trident train");
    eprintln!("  compiling corpus...");

    // Compile all files once with warnings suppressed
    let _guard = trident::diagnostic::suppress_warnings();
    let compiled = compile_corpus(&corpus);
    drop(_guard);
    let total_baseline: u64 = compiled.iter().map(|c| c.baseline_cost).sum();

    let config = NeuralCompilerConfig::new();
    let device = Default::default();
    let vocab = Vocab::new();

    // Build training pairs from all compiled files
    let blocks: Vec<(Vec<trident::tir::TIROp>, Vec<String>, String, u64)> = compiled
        .iter()
        .map(|cf| {
            (
                cf.tir_ops.clone(),
                cf.tasm_lines.clone(),
                cf.path.clone(),
                cf.baseline_cost,
            )
        })
        .collect();
    let pairs = extract_pairs(&blocks, &vocab);

    eprintln!(
        "  corpus    {} files, {} training pairs",
        corpus.len(),
        pairs.len(),
    );
    eprintln!("  baseline  {} total cost", total_baseline);
    eprintln!(
        "  model     ~{}M params | v2 GNN+Transformer",
        config.param_estimate() / 1_000_000,
    );
    eprintln!("  schedule  {} epochs, supervised CE", args.epochs,);
    eprintln!();

    if pairs.is_empty() {
        eprintln!("error: no training pairs extracted from corpus");
        process::exit(1);
    }

    let model = config.init::<TrainBackend>(&device);
    let sup_config = supervised::SupervisedConfig::default();
    let mut optimizer = supervised::create_optimizer::<TrainBackend>(&sup_config);
    let lr = sup_config.lr;

    let start = std::time::Instant::now();
    let mut model = model;
    let mut best_loss = f32::INFINITY;
    let mut stale_epochs = 0usize;

    for epoch in 0..args.epochs {
        let epoch_start = std::time::Instant::now();
        let (updated, result) = supervised::train_epoch(model, &pairs, &mut optimizer, lr, &device);
        model = updated;
        let epoch_elapsed = epoch_start.elapsed();

        let improved = result.avg_loss < best_loss;
        if improved {
            best_loss = result.avg_loss;
            stale_epochs = 0;
        } else {
            stale_epochs += 1;
        }

        let marker = if improved { " *" } else { "" };
        let conv_info = if stale_epochs >= sup_config.patience {
            " | converged (early stop)"
        } else if stale_epochs >= 2 {
            " | plateau"
        } else if improved {
            " | improving"
        } else {
            ""
        };

        eprintln!(
            "  epoch {}/{} | loss: {:.4}{} | {:.1}s{}",
            epoch + 1,
            args.epochs,
            result.avg_loss,
            marker,
            epoch_elapsed.as_secs_f64(),
            conv_info,
        );

        // Early stopping
        if stale_epochs >= sup_config.patience {
            eprintln!(
                "  early stopping: no improvement for {} epochs",
                sup_config.patience,
            );
            break;
        }
    }

    let elapsed = start.elapsed();

    eprintln!();
    eprintln!("done");
    eprintln!(
        "  trained      {} pairs in {:.1}s",
        pairs.len(),
        elapsed.as_secs_f64(),
    );
    eprintln!("  best loss    {:.4}", best_loss);
}

fn cmd_train_reset() {
    let repo_root = find_repo_root();
    let mut deleted = 0usize;

    // Delete v2 weights (data/neural/v2/)
    let weights_dir = repo_root.join("data").join("neural").join("v2");
    if weights_dir.exists() {
        if let Err(e) = std::fs::remove_dir_all(&weights_dir) {
            eprintln!("error: failed to delete {}: {}", weights_dir.display(), e);
            process::exit(1);
        }
        eprintln!(
            "  deleted {}",
            weights_dir
                .strip_prefix(&repo_root)
                .unwrap_or(&weights_dir)
                .display()
        );
        deleted += 1;
    }

    // Also delete legacy v1 weights (data/neural/)
    let legacy_weights_dir = repo_root.join("data").join("neural");
    if legacy_weights_dir.exists() {
        if let Err(e) = std::fs::remove_dir_all(&legacy_weights_dir) {
            eprintln!(
                "error: failed to delete {}: {}",
                legacy_weights_dir.display(),
                e
            );
            process::exit(1);
        }
        eprintln!(
            "  deleted {}",
            legacy_weights_dir
                .strip_prefix(&repo_root)
                .unwrap_or(&legacy_weights_dir)
                .display()
        );
        deleted += 1;
    }

    // Delete all .neural.tasm files under benches/
    let benches_dir = repo_root.join("benches");
    if benches_dir.exists() {
        for entry in walkdir(&benches_dir) {
            if entry.extension().and_then(|e| e.to_str()) == Some("tasm") {
                let name = entry.file_name().unwrap_or_default().to_string_lossy();
                if name.ends_with(".neural.tasm") {
                    if let Err(e) = std::fs::remove_file(&entry) {
                        eprintln!("  warning: failed to delete {}: {}", entry.display(), e);
                    } else {
                        eprintln!(
                            "  deleted {}",
                            entry.strip_prefix(&repo_root).unwrap_or(&entry).display()
                        );
                        deleted += 1;
                    }
                }
            }
        }
    }

    if deleted == 0 {
        eprintln!("trident train reset: nothing to delete (already clean)");
    } else {
        eprintln!("trident train reset: deleted {} artifacts", deleted);
    }
}

/// Recursively collect all files under a directory.
fn walkdir(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut result = Vec::new();
    walkdir_recursive(dir, &mut result, 0);
    result
}

fn walkdir_recursive(dir: &Path, result: &mut Vec<std::path::PathBuf>, depth: usize) {
    if depth >= 32 {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walkdir_recursive(&path, result, depth + 1);
        } else {
            result.push(path);
        }
    }
}

/// Compile all files once, return only those with compilable TIR.
fn compile_corpus(files: &[std::path::PathBuf]) -> Vec<CompiledFile> {
    let options = super::resolve_options("triton", "debug", None);
    let mut compiled = Vec::new();

    for file in files {
        let ir = match trident::build_tir_project(file, &options) {
            Ok(ir) => ir,
            Err(_) => continue,
        };
        if ir.is_empty() {
            continue;
        }

        let lowering = trident::ir::tir::lower::create_stack_lowering(&options.target_config.name);
        let tasm_lines = lowering.lower(&ir);

        if tasm_lines.is_empty() {
            continue;
        }

        let profile = trident::cost::scorer::profile_tasm(
            &tasm_lines.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        );
        let baseline_cost = profile.cost().max(1);

        compiled.push(CompiledFile {
            path: short_path(file),
            tir_ops: ir,
            tasm_lines,
            baseline_cost,
        });
    }

    compiled
}

fn discover_corpus() -> Vec<std::path::PathBuf> {
    let root = find_repo_root();
    let mut files = Vec::new();
    for dir in &["vm", "std", "os"] {
        let dir_path = root.join(dir);
        if dir_path.is_dir() {
            files.extend(super::resolve_tri_files(&dir_path));
        }
    }
    files.sort();
    files
}

fn find_repo_root() -> std::path::PathBuf {
    let mut dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    loop {
        if dir.join("Cargo.toml").exists() && dir.join("vm").is_dir() {
            return dir;
        }
        if !dir.pop() {
            return std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        }
    }
}

fn short_path(path: &Path) -> String {
    let s = path.to_string_lossy();
    for prefix in &["vm/", "std/", "os/"] {
        if let Some(pos) = s.find(prefix) {
            return s[pos..].to_string();
        }
    }
    s.to_string()
}
