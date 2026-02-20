use std::path::Path;
use std::process;

use std::cell::RefCell;

use clap::{Args, Subcommand};

thread_local! {
    static BEAM_DIAGNOSTIC: RefCell<Option<String>> = RefCell::new(None);
}

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
    /// Force a specific stage (1=supervised, 2=gflownet)
    #[arg(long)]
    pub stage: Option<u32>,
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
    total_blocks: usize,
    decoded: usize,
    checked: usize,
    proven: usize,
    wins: usize,
    checked_cost: u64,
    checked_baseline: u64,
}

pub fn cmd_train(args: TrainArgs) {
    if let Some(TrainAction::Reset) = args.action {
        cmd_train_reset();
        return;
    }

    use trident::neural::checkpoint::{self, TrainingStage};
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
    let total_blocks: usize = compiled.len();

    if pairs.is_empty() {
        eprintln!("error: no training pairs extracted from corpus");
        process::exit(1);
    }

    // Stage selection: explicit --stage flag, or default to Stage 1.
    // Auto-detection was causing problems (stale checkpoint from broken run
    // would skip Stage 1 forever). User controls stage transitions.
    let stage = match args.stage {
        Some(1) => TrainingStage::Stage1Supervised,
        Some(2) => TrainingStage::Stage2GFlowNet,
        Some(3) => TrainingStage::Stage3Online,
        _ => TrainingStage::Stage1Supervised,
    };

    // Show target sequence length stats
    let mut lens: Vec<usize> = pairs.iter().map(|p| p.target_tokens.len()).collect();
    lens.sort();
    let median = lens[lens.len() / 2];
    let min_len = lens[0];
    let max_len = lens[lens.len() - 1];

    let device_tag = if args.cpu { "CPU" } else { "GPU" };
    eprintln!(
        "  corpus    {} files, {} training pairs, {} blocks",
        corpus.len(),
        pairs.len(),
        total_blocks,
    );
    eprintln!(
        "  targets   len min={} median={} max={} (tokens incl EOS)",
        min_len, median, max_len,
    );
    eprintln!("  baseline  {} total cost", total_baseline);
    eprintln!(
        "  model     ~{}M params | v2 GNN+Transformer | {}",
        config.param_estimate() / 1_000_000,
        device_tag,
    );
    eprintln!(
        "  schedule  {} epochs, {} (use --stage 2 for GFlowNet)",
        args.epochs, stage,
    );

    // Show existing checkpoints
    let ckpts = checkpoint::available_checkpoints();
    if !ckpts.is_empty() {
        for (tag, path) in &ckpts {
            eprintln!("  checkpoint  {:?} -> {}", tag, path.display());
        }
    }
    eprintln!();

    if args.cpu {
        use burn::backend::Autodiff;
        use burn::backend::NdArray;
        type B = Autodiff<NdArray>;
        let device = Default::default();
        let model = config.init::<B>(&device);
        run_training_loop::<B>(
            model,
            &pairs,
            &compiled,
            &vocab,
            args.epochs,
            &device,
            stage,
        );
    } else {
        use burn::backend::wgpu::{Wgpu, WgpuDevice};
        use burn::backend::Autodiff;
        type B = Autodiff<Wgpu>;
        let device = WgpuDevice::default();
        let model = config.init::<B>(&device);
        run_training_loop::<B>(
            model,
            &pairs,
            &compiled,
            &vocab,
            args.epochs,
            &device,
            stage,
        );
    }
}

/// Main training loop — generic over backend.
fn run_training_loop<B: burn::tensor::backend::AutodiffBackend>(
    model: trident::neural::model::composite::NeuralCompilerV2<B>,
    pairs: &[trident::neural::data::pairs::TrainingPair],
    compiled: &[CompiledFile],
    vocab: &trident::neural::model::vocab::Vocab,
    epochs: u64,
    device: &B::Device,
    stage: trident::neural::checkpoint::TrainingStage,
) where
    <B as burn::tensor::backend::Backend>::FloatElem: From<f32>,
{
    use trident::neural::checkpoint::{self, CheckpointTag, TrainingStage};

    // Try loading existing checkpoint
    let load_tag = match stage {
        TrainingStage::Stage1Supervised => CheckpointTag::Stage1Best,
        TrainingStage::Stage2GFlowNet => CheckpointTag::Stage1Best,
        TrainingStage::Stage3Online => CheckpointTag::Stage2Latest,
    };
    let model = match checkpoint::load_checkpoint(model, load_tag, device) {
        Ok(Some(loaded)) => {
            eprintln!("  loaded checkpoint {:?}", load_tag);
            loaded
        }
        Ok(None) => {
            eprintln!("  no checkpoint found, training from scratch");
            // Re-init since load_checkpoint consumed the model
            trident::neural::model::composite::NeuralCompilerConfig::new().init::<B>(device)
        }
        Err(e) => {
            eprintln!("  warning: checkpoint load failed: {}", e);
            trident::neural::model::composite::NeuralCompilerConfig::new().init::<B>(device)
        }
    };

    match stage {
        TrainingStage::Stage1Supervised => {
            run_stage1(model, pairs, compiled, vocab, epochs, device);
        }
        TrainingStage::Stage2GFlowNet => {
            run_stage2(model, compiled, vocab, epochs, device);
        }
        TrainingStage::Stage3Online => {
            run_stage2(model, compiled, vocab, epochs, device);
        }
    }
}

/// Stage 1: Supervised pre-training with cosine LR decay.
fn run_stage1<B: burn::tensor::backend::AutodiffBackend>(
    model: trident::neural::model::composite::NeuralCompilerV2<B>,
    pairs: &[trident::neural::data::pairs::TrainingPair],
    compiled: &[CompiledFile],
    vocab: &trident::neural::model::vocab::Vocab,
    epochs: u64,
    device: &B::Device,
) where
    <B as burn::tensor::backend::Backend>::FloatElem: From<f32>,
{
    use burn::module::AutodiffModule;
    use trident::neural::checkpoint::{self, CheckpointTag};
    use trident::neural::inference::beam::BeamConfig;
    use trident::neural::training::supervised;
    use trident::neural::training::supervised::cosine_lr;

    let sup_config = supervised::SupervisedConfig::default();
    let mut optimizer = supervised::create_optimizer::<B>(&sup_config);

    let start = std::time::Instant::now();
    let mut model = model;
    let mut best_loss = f32::INFINITY;
    let mut stale_epochs = 0usize;
    let mut prev_table_lines: usize = 0;

    // Small beam for eval during training — full K=32 is too slow (197s/epoch).
    // K=4, max_steps=64 gives fast feedback; production inference uses K=32.
    let beam_config = BeamConfig {
        k: 4,
        max_steps: 64,
        ..Default::default()
    };

    for epoch in 0..epochs {
        let epoch_start = std::time::Instant::now();

        // Cosine LR decay
        let lr = cosine_lr(&sup_config, epoch as usize, epochs as usize);

        // Train one epoch
        let (updated, result) = supervised::train_epoch(model, pairs, &mut optimizer, lr, device);
        model = updated;

        let improved = result.avg_loss < best_loss;
        if improved {
            best_loss = result.avg_loss;
            stale_epochs = 0;
            // Save best checkpoint
            if let Err(e) = checkpoint::save_checkpoint(&model, CheckpointTag::Stage1Best, device) {
                eprintln!("  warning: checkpoint save failed: {}", e);
            }
        } else {
            stale_epochs += 1;
        }

        // Evaluate via beam search
        let inner = model.valid();
        let inner_device = device.clone();
        let file_evals = eval_files(&inner, compiled, vocab, &beam_config, &inner_device);

        let epoch_elapsed = epoch_start.elapsed();

        // Display table
        prev_table_lines = display_epoch_table(
            epoch,
            epochs,
            &result,
            improved,
            lr,
            &file_evals,
            compiled,
            epoch_elapsed,
            prev_table_lines,
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
    eprintln!(
        "  Stage 1 done in {:.1}s, best loss: {:.4}",
        elapsed.as_secs_f64(),
        best_loss
    );
}

/// Stage 2: GFlowNet fine-tuning.
fn run_stage2<B: burn::tensor::backend::AutodiffBackend>(
    model: trident::neural::model::composite::NeuralCompilerV2<B>,
    compiled: &[CompiledFile],
    vocab: &trident::neural::model::vocab::Vocab,
    epochs: u64,
    device: &B::Device,
) where
    <B as burn::tensor::backend::Backend>::FloatElem: From<f32>,
{
    use burn::grad_clipping::GradientClippingConfig;
    use burn::module::AutodiffModule;
    use burn::optim::{AdamWConfig, GradientsParams, Optimizer};
    use trident::neural::checkpoint::{self, CheckpointTag};
    use trident::neural::inference::beam::BeamConfig;
    use trident::neural::training::gflownet::{self, GFlowNetConfig};

    let gf_config = GFlowNetConfig::default();
    let mut optimizer = AdamWConfig::new()
        .with_weight_decay(0.01)
        .with_grad_clipping(Some(GradientClippingConfig::Norm(1.0)))
        .init();

    let beam_config = BeamConfig {
        k: 4,
        max_steps: 64,
        ..Default::default()
    };

    let start = std::time::Instant::now();
    let mut model = model;
    let mut global_step = 0usize;
    let mut prev_table_lines: usize = 0;
    let mut total_valid = 0usize;
    let mut total_sampled = 0usize;

    for epoch in 0..epochs {
        let epoch_start = std::time::Instant::now();
        let mut epoch_loss = 0.0f32;
        let mut epoch_valid = 0usize;
        let mut epoch_reward = 0.0f32;

        // For each compiled file, sample a sequence and compute TB loss
        for cf in compiled.iter() {
            let graph = trident::neural::data::tir_graph::TirGraph::from_tir_ops(&cf.tir_ops);
            if graph.nodes.is_empty() {
                continue;
            }

            let (loss, reward, valid) = gflownet::gflownet_step(
                &model,
                &graph,
                &cf.tasm_lines,
                cf.baseline_cost,
                burn::tensor::Tensor::<B, 1>::zeros([1], device), // log_z (simplified)
                global_step,
                &gf_config,
                vocab,
                device,
            );

            let loss_val: f32 = loss.clone().into_data().to_vec::<f32>().unwrap()[0];
            epoch_loss += loss_val;
            epoch_reward += reward;
            if valid {
                epoch_valid += 1;
            }
            total_sampled += 1;
            if valid {
                total_valid += 1;
            }

            // Backward + step
            let grads = loss.backward();
            let grads = GradientsParams::from_grads(grads, &model);
            let lr = 1e-4; // Lower LR for fine-tuning
            model = optimizer.step(lr, model, grads);

            global_step += 1;
        }

        // Save checkpoint periodically
        if (epoch + 1) % 5 == 0 || epoch == epochs - 1 {
            if let Err(e) = checkpoint::save_checkpoint(&model, CheckpointTag::Stage2Latest, device)
            {
                eprintln!("  warning: checkpoint save failed: {}", e);
            }
        }

        // Evaluate
        let inner = model.valid();
        let inner_device = device.clone();
        let file_evals = eval_files(&inner, compiled, vocab, &beam_config, &inner_device);

        let epoch_elapsed = epoch_start.elapsed();

        // Aggregate
        let epoch_decoded: usize = file_evals.iter().map(|e| e.decoded).sum();
        let epoch_checked: usize = file_evals.iter().map(|e| e.checked).sum();
        let epoch_wins: usize = file_evals.iter().map(|e| e.wins).sum();
        let total_blk: usize = file_evals.iter().map(|e| e.total_blocks).sum();
        let epoch_sampled = compiled.len();
        let validity_rate = if epoch_sampled > 0 {
            epoch_valid as f32 / epoch_sampled as f32 * 100.0
        } else {
            0.0
        };

        let num_files = compiled.len().max(1) as f32;
        let avg_loss = epoch_loss / num_files;
        let avg_reward = epoch_reward / num_files;
        let tau = gflownet::temperature_at_step(global_step, &gf_config);

        // Display
        let table_lines = 1 + 1 + 1 + file_evals.len() + 1;
        if epoch > 0 && prev_table_lines > 0 {
            eprint!("\x1B[{}A", prev_table_lines);
        }

        eprintln!(
            "\r  epoch {}/{} | TB loss: {:.4} | reward: {:.3} | tau: {:.2} | valid: {:.0}% | decoded {}/{} checked {} won {} | {:.1}s\x1B[K",
            epoch + 1, epochs, avg_loss, avg_reward, tau, validity_rate,
            epoch_decoded, total_blk, epoch_checked, epoch_wins,
            epoch_elapsed.as_secs_f64(),
        );

        display_file_table(&file_evals, compiled);

        prev_table_lines = table_lines;
    }

    let elapsed = start.elapsed();
    eprintln!();
    eprintln!(
        "  Stage 2 done in {:.1}s, validity: {:.0}%",
        elapsed.as_secs_f64(),
        if total_sampled > 0 {
            total_valid as f64 / total_sampled as f64 * 100.0
        } else {
            0.0
        },
    );
}

/// Evaluate all compiled files via beam search (no grads).
fn eval_files<B: burn::prelude::Backend>(
    model: &trident::neural::model::composite::NeuralCompilerV2<B>,
    compiled: &[CompiledFile],
    vocab: &trident::neural::model::vocab::Vocab,
    beam_config: &trident::neural::inference::beam::BeamConfig,
    device: &B::Device,
) -> Vec<FileEval> {
    use trident::neural::inference::beam::beam_search;
    use trident::neural::inference::execute::validate_and_rank;
    use trident::neural::training::supervised::{graph_to_edges, graph_to_features};

    let mut evals = Vec::with_capacity(compiled.len());

    for (file_idx, cf) in compiled.iter().enumerate() {
        let graph = trident::neural::data::tir_graph::TirGraph::from_tir_ops(&cf.tir_ops);
        if graph.nodes.is_empty() {
            evals.push(FileEval {
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

        let node_features = graph_to_features::<B>(&graph, device);
        let (edge_src, edge_dst, edge_types) = graph_to_edges::<B>(&graph, device);

        let beam_result = beam_search(
            &model.encoder,
            &model.decoder,
            node_features,
            edge_src,
            edge_dst,
            edge_types,
            beam_config,
            0, // must match training initial_stack_depth
            device,
        );

        let ranked = validate_and_rank(
            &beam_result.sequences,
            vocab,
            &cf.tasm_lines,
            file_idx as u64,
        );

        let decoded = beam_result
            .sequences
            .iter()
            .filter(|s| !s.is_empty())
            .count();

        // Diagnostic: save first file's top beam for display after eval
        if file_idx == 0 && !beam_result.sequences.is_empty() {
            let top = &beam_result.sequences[0];
            let tasm = vocab.decode_sequence(top);
            let preview: Vec<&str> = tasm.iter().map(|s| s.as_str()).take(10).collect();
            // Store in thread-local for display_epoch_table to pick up
            BEAM_DIAGNOSTIC.with(|d| {
                *d.borrow_mut() = Some(format!(
                    "beam[0] {} tokens: [{}]{}",
                    top.len(),
                    preview.join(", "),
                    if tasm.len() > 10 { " ..." } else { "" },
                ));
            });
        }

        if let Some(r) = ranked {
            let wins = if r.cost < cf.baseline_cost { 1 } else { 0 };
            evals.push(FileEval {
                total_blocks: cf.tasm_lines.len(),
                decoded,
                checked: r.valid_count,
                proven: 0,
                wins,
                checked_cost: r.cost,
                checked_baseline: cf.baseline_cost,
            });
        } else {
            evals.push(FileEval {
                total_blocks: cf.tasm_lines.len(),
                decoded,
                checked: 0,
                proven: 0,
                wins: 0,
                checked_cost: 0,
                checked_baseline: 0,
            });
        }
    }

    evals
}

/// Display epoch summary + per-file table. Returns number of lines for overwriting.
fn display_epoch_table(
    epoch: u64,
    total_epochs: u64,
    result: &trident::neural::training::supervised::EpochResult,
    improved: bool,
    lr: f64,
    file_evals: &[FileEval],
    compiled: &[CompiledFile],
    elapsed: std::time::Duration,
    prev_table_lines: usize,
) -> usize {
    let epoch_decoded: usize = file_evals.iter().map(|e| e.decoded).sum();
    let epoch_checked: usize = file_evals.iter().map(|e| e.checked).sum();
    let epoch_proven: usize = file_evals.iter().map(|e| e.proven).sum();
    let epoch_wins: usize = file_evals.iter().map(|e| e.wins).sum();
    let total_blk: usize = file_evals.iter().map(|e| e.total_blocks).sum();

    let loss_marker = if improved { " *" } else { "" };

    // Pick up beam diagnostic if available
    let diag = BEAM_DIAGNOSTIC.with(|d| d.borrow_mut().take());
    let diag_lines = if diag.is_some() { 1 } else { 0 };

    let table_lines = 1 + diag_lines + 1 + 1 + file_evals.len() + 1;
    if epoch > 0 && prev_table_lines > 0 {
        eprint!("\x1B[{}A", prev_table_lines);
    }

    eprintln!(
        "\r  epoch {}/{} | loss: {:.4}{} | lr: {:.1e} | decoded {}/{} | checked {} proven {} won {} | {:.1}s\x1B[K",
        epoch + 1, total_epochs, result.avg_loss, loss_marker, lr,
        epoch_decoded, total_blk, epoch_checked, epoch_proven, epoch_wins,
        elapsed.as_secs_f64(),
    );
    if let Some(d) = diag {
        eprintln!("    {}\x1B[K", d);
    }

    display_file_table(file_evals, compiled);

    table_lines
}

/// Per-file table rows (shared between Stage 1 and Stage 2).
fn display_file_table(file_evals: &[FileEval], compiled: &[CompiledFile]) {
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
}

fn cmd_train_reset() {
    let repo_root = find_repo_root();
    let mut deleted = 0usize;

    // Delete neural weights (both v1 and v2)
    for subdir in &["data/neural", "data/neural/v2"] {
        let dir = repo_root.join(subdir);
        if dir.exists() {
            if let Err(e) = std::fs::remove_dir_all(&dir) {
                eprintln!("error: failed to delete {}: {}", dir.display(), e);
                process::exit(1);
            }
            eprintln!(
                "  deleted {}",
                dir.strip_prefix(&repo_root).unwrap_or(&dir).display()
            );
            deleted += 1;
        }
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
    use trident::neural::data::pairs::split_tir_by_function;

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

        let file_path = short_path(file);

        // Split TIR into per-function chunks and lower each independently.
        // This produces many shorter training pairs (50-300 tokens) instead of
        // one huge per-file pair (500-2000 tokens).
        let functions = split_tir_by_function(&ir);

        for (fn_name, fn_tir) in &functions {
            // Skip entry/trailing scaffolding — not useful training data
            if fn_name.starts_with("__") {
                continue;
            }
            if fn_tir.is_empty() {
                continue;
            }

            // Lower this function's TIR to TASM
            let lowering =
                trident::ir::tir::lower::create_stack_lowering(&options.target_config.name);
            let tasm_lines = lowering.lower(fn_tir);

            // Filter out labels and empty lines — keep only instructions
            let tasm_lines: Vec<String> = tasm_lines
                .into_iter()
                .filter(|l| {
                    let t = l.trim();
                    !t.is_empty() && !t.ends_with(':') && !t.starts_with("//")
                })
                .map(|l| l.trim().to_string())
                .collect();

            if tasm_lines.is_empty() {
                continue;
            }

            let profile = trident::cost::scorer::profile_tasm(
                &tasm_lines.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            );
            let baseline_cost = profile.cost().max(1);

            compiled.push(CompiledFile {
                path: format!("{}:{}", file_path, fn_name),
                tir_ops: fn_tir.clone(),
                tasm_lines,
                baseline_cost,
            });
        }
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
