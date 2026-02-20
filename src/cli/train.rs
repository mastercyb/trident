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
    /// Disable GPU (use CPU for training)
    #[arg(long)]
    pub cpu: bool,
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

/// Per-file eval result after beam search.
struct FileEval {
    /// Number of TASM lines in the baseline.
    total_blocks: usize,
    /// Beams where model produced a non-empty candidate.
    decoded: usize,
    /// Candidates that passed stack verification.
    checked: usize,
    /// Candidates proven via Triton VM (set post-eval).
    proven: usize,
    /// Files where neural cost < baseline cost.
    wins: usize,
    /// Neural cost for checked candidates.
    checked_cost: u64,
    /// Baseline cost for checked candidates.
    checked_baseline: u64,
}

pub fn cmd_train(args: TrainArgs) {
    if let Some(TrainAction::Reset) = args.action {
        cmd_train_reset();
        return;
    }

    use trident::neural::data::pairs::extract_pairs;
    use trident::neural::model::composite::NeuralCompilerConfig;
    use trident::neural::model::vocab::Vocab;

    let corpus = discover_corpus();
    if corpus.is_empty() {
        eprintln!("error: no .tri files found in vm/, std/, os/");
        process::exit(1);
    }

    eprintln!("trident train");
    eprintln!("  compiling corpus...");

    let _guard = trident::diagnostic::suppress_warnings();
    let compiled = compile_corpus(&corpus);
    drop(_guard);
    let total_baseline: u64 = compiled.iter().map(|c| c.baseline_cost).sum();

    let config = NeuralCompilerConfig::new();
    let vocab = Vocab::new();

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
    let total_blocks: usize = compiled.iter().map(|c| c.tasm_lines.len()).sum();

    if pairs.is_empty() {
        eprintln!("error: no training pairs extracted from corpus");
        process::exit(1);
    }

    let device_tag = if args.cpu { "CPU" } else { "GPU" };
    eprintln!(
        "  corpus    {} files, {} training pairs, {} blocks",
        corpus.len(),
        pairs.len(),
        total_blocks,
    );
    eprintln!("  baseline  {} total cost", total_baseline);
    eprintln!(
        "  model     ~{}M params | v2 GNN+Transformer | {}",
        config.param_estimate() / 1_000_000,
        device_tag,
    );
    eprintln!("  schedule  {} epochs, supervised CE", args.epochs);
    eprintln!();

    if args.cpu {
        use burn::backend::Autodiff;
        use burn::backend::NdArray;
        type B = Autodiff<NdArray>;
        let device = Default::default();
        let model = config.init::<B>(&device);
        run_training_loop::<B>(model, &pairs, &compiled, &vocab, args.epochs, &device);
    } else {
        use burn::backend::wgpu::{Wgpu, WgpuDevice};
        use burn::backend::Autodiff;
        type B = Autodiff<Wgpu>;
        let device = WgpuDevice::default();
        let model = config.init::<B>(&device);
        run_training_loop::<B>(model, &pairs, &compiled, &vocab, args.epochs, &device);
    }
}

/// Main training loop — generic over backend.
///
/// Each epoch: train on all pairs, then eval each file via beam search, display table.
fn run_training_loop<B: burn::tensor::backend::AutodiffBackend>(
    model: trident::neural::model::composite::NeuralCompilerV2<B>,
    pairs: &[trident::neural::data::pairs::TrainingPair],
    compiled: &[CompiledFile],
    vocab: &trident::neural::model::vocab::Vocab,
    epochs: u64,
    device: &B::Device,
) {
    use burn::module::AutodiffModule;
    use trident::neural::inference::beam::{beam_search, BeamConfig};
    use trident::neural::inference::execute::validate_and_rank;
    use trident::neural::training::supervised;
    use trident::neural::training::supervised::{graph_to_edges, graph_to_features};

    let sup_config = supervised::SupervisedConfig::default();
    let mut optimizer = supervised::create_optimizer::<B>(&sup_config);
    let lr = sup_config.lr;

    let start = std::time::Instant::now();
    let mut model = model;
    let mut best_loss = f32::INFINITY;
    let mut stale_epochs = 0usize;
    let mut prev_table_lines: usize = 0;

    // EMA convergence tracking
    let mut epoch_history: Vec<u64> = Vec::new();
    let mut ema_rate: Option<f64> = None;
    let mut ema_volatility: Option<f64> = None;
    const EMA_ALPHA: f64 = 0.3;

    let beam_config = BeamConfig {
        k: 8,
        max_steps: 64,
    };

    for epoch in 0..epochs {
        let epoch_start = std::time::Instant::now();

        // 1. Train one epoch
        let (updated, result) = supervised::train_epoch(model, pairs, &mut optimizer, lr, device);
        model = updated;

        let improved = result.avg_loss < best_loss;
        if improved {
            best_loss = result.avg_loss;
            stale_epochs = 0;
        } else {
            stale_epochs += 1;
        }

        // 2. Evaluate each file via beam search (using inner model, no grads)
        let inner = model.valid();
        // B::InnerBackend has Device = B::Device, so same device works
        let inner_device = device.clone();
        let mut file_evals: Vec<FileEval> = Vec::with_capacity(compiled.len());

        for (file_idx, cf) in compiled.iter().enumerate() {
            eprint!(
                "\r  epoch {}/{} | eval {}/{}    ",
                epoch + 1,
                epochs,
                file_idx + 1,
                compiled.len(),
            );
            use std::io::Write;
            let _ = std::io::stderr().flush();

            let graph = trident::neural::data::tir_graph::TirGraph::from_tir_ops(&cf.tir_ops);
            if graph.nodes.is_empty() {
                file_evals.push(FileEval {
                    total_blocks: cf.tasm_lines.len(),
                    decoded: 0,
                    checked: 0,
                    proven: 0,
                    wins: 0,
                    checked_cost: 0,
                    checked_baseline: 0,
                });
                continue;
            }

            let node_features = graph_to_features::<B::InnerBackend>(&graph, &inner_device);
            let (edge_src, edge_dst, edge_types) =
                graph_to_edges::<B::InnerBackend>(&graph, &inner_device);

            let beam_result = beam_search(
                &inner.encoder,
                &inner.decoder,
                node_features,
                edge_src,
                edge_dst,
                edge_types,
                &beam_config,
                0,
                &inner_device,
            );

            let ranked = validate_and_rank(
                &beam_result.sequences,
                vocab,
                &cf.tasm_lines,
                file_idx as u64,
            );

            let total_blocks = cf.tasm_lines.len();
            let decoded = beam_result
                .sequences
                .iter()
                .filter(|s| !s.is_empty())
                .count();

            if let Some(r) = ranked {
                let wins = if r.cost < cf.baseline_cost { 1 } else { 0 };
                file_evals.push(FileEval {
                    total_blocks,
                    decoded,
                    checked: r.valid_count,
                    proven: 0,
                    wins,
                    checked_cost: r.cost,
                    checked_baseline: cf.baseline_cost,
                });
            } else {
                file_evals.push(FileEval {
                    total_blocks,
                    decoded,
                    checked: 0,
                    proven: 0,
                    wins: 0,
                    checked_cost: 0,
                    checked_baseline: 0,
                });
            }
        }

        // 3. Proven verification via trisha
        // TODO: restore when v2 produces full spliced programs
        // For now, proven stays 0 — checked (stack verifier) is the signal.

        let epoch_elapsed = epoch_start.elapsed();

        // 4. Aggregate stats
        let epoch_decoded: usize = file_evals.iter().map(|e| e.decoded).sum();
        let epoch_checked: usize = file_evals.iter().map(|e| e.checked).sum();
        let epoch_proven: usize = file_evals.iter().map(|e| e.proven).sum();
        let epoch_wins: usize = file_evals.iter().map(|e| e.wins).sum();
        let epoch_chk_cost: u64 = file_evals.iter().map(|e| e.checked_cost).sum();
        let total_blk: usize = file_evals.iter().map(|e| e.total_blocks).sum();

        // EMA convergence
        epoch_history.push(epoch_chk_cost);
        let conv_info = if epoch_history.len() >= 2 {
            let prev = epoch_history[epoch_history.len() - 2];
            let curr = epoch_history[epoch_history.len() - 1];
            let instant_rate = if prev > 0 {
                (prev as f64 - curr as f64) / prev as f64
            } else {
                0.0
            };
            let instant_vol = instant_rate.abs();
            let rate = match ema_rate {
                Some(prev_ema) => EMA_ALPHA * instant_rate + (1.0 - EMA_ALPHA) * prev_ema,
                None => instant_rate,
            };
            let vol = match ema_volatility {
                Some(prev_vol) => EMA_ALPHA * instant_vol + (1.0 - EMA_ALPHA) * prev_vol,
                None => instant_vol,
            };
            ema_rate = Some(rate);
            ema_volatility = Some(vol);
            if vol > 0.02 {
                format!(" | unstable ({:.1}%/ep swing)", vol * 100.0)
            } else if vol > 0.005 {
                format!(" | searching ({:.1}%/ep swing)", vol * 100.0)
            } else if rate < 0.001 {
                " | converged".to_string()
            } else if rate < 0.005 {
                format!(" | plateau ({:.2}%/ep)", rate * 100.0)
            } else {
                format!(" | improving ({:.1}%/ep)", rate * 100.0)
            }
        } else {
            String::new()
        };

        let loss_marker = if improved { " *" } else { "" };

        // 5. Build per-file table
        let mut sorted: Vec<_> = file_evals
            .iter()
            .enumerate()
            .map(|(i, e)| {
                (
                    compiled[i].path.as_str(),
                    e.total_blocks,
                    e.decoded,
                    e.checked,
                    e.proven,
                    e.wins,
                    e.checked_cost,
                    e.checked_baseline,
                )
            })
            .collect();
        sorted.sort_by(|a, b| b.5.cmp(&a.5).then(b.3.cmp(&a.3)).then(b.2.cmp(&a.2)));

        // Overwrite previous table
        let table_lines = 1 + 1 + 1 + sorted.len() + 1;
        if epoch > 0 && prev_table_lines > 0 {
            eprint!("\x1B[{}A", prev_table_lines);
        }

        // Epoch summary line
        eprintln!(
            "\r  epoch {}/{} | loss: {:.2}{} | decoded {}/{} | checked {} proven {} won {} | {:.1}s{}\x1B[K",
            epoch + 1,
            epochs,
            result.avg_loss,
            loss_marker,
            epoch_decoded,
            total_blk,
            epoch_checked,
            epoch_proven,
            epoch_wins,
            epoch_elapsed.as_secs_f64(),
            conv_info,
        );

        // Table header
        eprintln!(
            "  {:<60} | {:>9} | {:>7} {:>8} {:>7} {:>5} | {:>15}\x1B[K",
            "Module", "Blocks", "Decoded", "Checked", "Proven", "Won", "Cost (ratio)"
        );
        eprintln!("  {}\x1B[K", "-".repeat(122));

        for &(path, total, decoded, checked, proven, wins, vc, vb) in &sorted {
            let blocks_col = format!("{}/{}", total, total);
            let cost_col = if checked > 0 {
                let vr = vc as f64 / vb.max(1) as f64;
                format!("{}/{} ({:.2}x)", vc, vb, vr)
            } else {
                "\u{2013}".to_string()
            };
            eprintln!(
                "  {:<60} | {:>9} | {:>7} {:>8} {:>7} {:>5} | {:>15}\x1B[K",
                path, blocks_col, decoded, checked, proven, wins, cost_col,
            );
        }
        eprintln!("  {}\x1B[K", "-".repeat(122));

        prev_table_lines = table_lines;

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
    if let Some(rate) = ema_rate {
        let vol = ema_volatility.unwrap_or(0.0);
        let (label, hint) = if vol > 0.02 {
            ("unstable", "high variance — model exploring, keep training")
        } else if vol > 0.005 {
            ("searching", "moderate variance — model still exploring")
        } else if rate < 0.001 {
            ("converged", "further training unlikely to help")
        } else if rate < 0.005 {
            ("plateau", "diminishing returns, may stop soon")
        } else {
            ("improving", "model still learning, keep training")
        };
        eprintln!(
            "  convergence  {} ({:.2}%/ep, {:.1}% swing) — {}",
            label,
            rate * 100.0,
            vol * 100.0,
            hint,
        );
    }
}

fn cmd_train_reset() {
    let repo_root = find_repo_root();
    let mut deleted = 0usize;

    // Delete neural weights
    let weights_dir = repo_root.join("data").join("neural");
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
