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
    /// Generations per file per epoch (default: 10)
    #[arg(short, long, default_value = "10")]
    pub generations: u64,
    /// Disable GPU (use CPU parallel instead)
    #[arg(long)]
    pub cpu: bool,
}

#[derive(Subcommand)]
pub enum TrainAction {
    /// Delete all neural weights and generated .neural.tasm files
    Reset,
}

/// Pre-compiled file data — TIR + blocks + baselines, computed once.
struct CompiledFile {
    path: String,
    blocks: Vec<trident::ir::tir::encode::TIRBlock>,
    per_block_baselines: Vec<u64>,
    per_block_tasm: Vec<Vec<String>>,
    /// Per-block: true if baseline uses only verifiable ops (no side effects).
    per_block_verifiable: Vec<bool>,
    baseline_cost: u64,
    /// Full compiler output (with labels, control flow, subroutines).
    /// Neural blocks get spliced into this to produce a complete program.
    compiled_tasm: String,
}

pub fn cmd_train(args: TrainArgs) {
    if let Some(TrainAction::Reset) = args.action {
        cmd_train_reset();
        return;
    }

    use trident::ir::tir::neural::weights;

    let corpus = discover_corpus();
    if corpus.is_empty() {
        eprintln!("error: no .tri files found in vm/, std/, os/");
        process::exit(1);
    }

    let meta = weights::load_best_meta().ok();
    let gen_start = meta.as_ref().map_or(0, |m| m.generation);

    let use_gpu = !args.cpu;
    let device_tag = if use_gpu { "GPU" } else { "CPU" };

    eprintln!("trident train");
    eprintln!("  compiling corpus...");

    // Compile all files once with warnings suppressed
    let _guard = trident::diagnostic::suppress_warnings();
    let compiled = compile_corpus(&corpus);
    drop(_guard);
    let total_blocks: usize = compiled.iter().map(|c| c.blocks.len()).sum();
    let total_baseline: u64 = compiled.iter().map(|c| c.baseline_cost).sum();
    let total_gens = args.epochs * compiled.len() as u64 * args.generations;

    let verifiable_blocks: usize = compiled
        .iter()
        .flat_map(|cf| &cf.per_block_verifiable)
        .filter(|&&v| v)
        .count();
    let rejected_blocks = total_blocks - verifiable_blocks;

    eprintln!(
        "  corpus    {} files ({} trainable, {} blocks)",
        corpus.len(),
        compiled.len(),
        total_blocks
    );
    eprintln!(
        "  blocks    {} verifiable, {} rejected (baseline uses side-effect ops)",
        verifiable_blocks, rejected_blocks,
    );
    eprintln!("  baseline  {} total cost", total_baseline);
    eprintln!(
        "  schedule  {} epochs x {} gens/file = {} total gens",
        args.epochs, args.generations, total_gens
    );
    eprintln!("  model     gen {} | 10K MLP | {}", gen_start, device_tag);
    eprintln!();

    // Create GPU accelerator once, sized for the largest file in corpus
    let mut gpu_accel: Option<trident::gpu::neural_accel::NeuralAccelerator> = if use_gpu {
        let max_blocks = compiled.iter().map(|c| c.blocks.len()).max().unwrap_or(1) as u32;
        let pop_size = trident::ir::tir::neural::evolve::POP_SIZE as u32;
        match trident::gpu::neural_accel::NeuralAccelerator::try_create(max_blocks, pop_size) {
            Some(accel) => {
                eprintln!("  GPU initialized (f32, capacity: {} blocks)", max_blocks);
                Some(accel)
            }
            None => {
                eprintln!("  GPU unavailable, falling back to CPU");
                None
            }
        }
    } else {
        None
    };

    let start = std::time::Instant::now();
    let mut total_trained = 0u64;
    let mut epoch_history: Vec<u64> = Vec::new();
    // EMA of per-epoch improvement rate + volatility (alpha=0.3 — smooths over ~3 epochs)
    // Initialized to None — seeded from first observed values, not 0.0.
    let mut ema_rate: Option<f64> = None;
    let mut ema_volatility: Option<f64> = None;
    const EMA_ALPHA: f64 = 0.3;
    let mut prev_table_lines: usize = 0;
    let repo_root = find_repo_root();

    for epoch in 0..args.epochs {
        // Shuffle file indices each epoch
        let mut indices: Vec<usize> = (0..compiled.len()).collect();
        shuffle(&mut indices, gen_start + epoch);

        let epoch_start = std::time::Instant::now();
        // (file_idx, cost, wins, total_blocks, verified, decoded,
        //  decoded_cost, decoded_baseline, verified_cost, verified_baseline)
        let mut epoch_costs: Vec<(usize, u64, usize, usize, usize, usize, u64, u64, u64, u64)> =
            Vec::new();
        let mut epoch_diagnostics: Vec<(String, Vec<BlockDiagnostic>)> = Vec::new();

        for (i, &file_idx) in indices.iter().enumerate() {
            let cf = &compiled[file_idx];
            eprint!(
                "\r  epoch {}/{} | {}/{} | {}",
                epoch + 1,
                args.epochs,
                i + 1,
                compiled.len(),
                cf.path,
            );
            // Pad to clear previous longer lines
            let pad = 50usize.saturating_sub(cf.path.len());
            eprint!("{}", " ".repeat(pad));
            use std::io::Write;
            let _ = std::io::stderr().flush();

            let result = train_one_compiled(cf, args.generations, &mut gpu_accel);
            if !result.diagnostics.is_empty() {
                epoch_diagnostics.push((cf.path.clone(), result.diagnostics));
            }
            epoch_costs.push((
                file_idx,
                result.cost,
                result.neural_wins,
                result.total_blocks,
                result.neural_verified,
                result.neural_decoded,
                result.decoded_cost,
                result.decoded_baseline,
                result.verified_cost,
                result.verified_baseline,
            ));
            total_trained += 1;

            // On last epoch, write captured neural TASM to disk
            if epoch + 1 == args.epochs {
                if let Some(ref per_block) = result.neural_tasm {
                    write_neural_tasm(cf, per_block, &repo_root);
                }
            }
        }

        let epoch_elapsed = epoch_start.elapsed();
        let epoch_ver_cost: u64 = epoch_costs.iter().map(|e| e.8).sum();
        let epoch_ver_bl: u64 = epoch_costs.iter().map(|e| e.9).sum();

        let trend = if epoch == 0 {
            String::new()
        } else {
            let prev = if epoch_history.is_empty() {
                0
            } else {
                epoch_history[epoch_history.len() - 1]
            };
            if epoch_ver_cost < prev && prev > 0 {
                format!(" (-{} vs prev)", prev - epoch_ver_cost)
            } else if epoch_ver_cost > prev && prev > 0 {
                format!(" (+{} vs prev)", epoch_ver_cost - prev)
            } else {
                String::new()
            }
        };
        epoch_history.push(epoch_ver_cost);

        // EMA convergence: track smoothed improvement rate + volatility
        let conv_info = if epoch_history.len() >= 2 {
            let prev = epoch_history[epoch_history.len() - 2];
            let curr = epoch_history[epoch_history.len() - 1];
            let instant_rate = if prev > 0 {
                (prev as f64 - curr as f64) / prev as f64
            } else {
                0.0
            };
            let instant_vol = instant_rate.abs();
            // Seed EMAs from first observation; update normally after
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
            // Unstable = volatility > 2% per epoch (big swings mean not converged)
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

        let epoch_wins: usize = epoch_costs.iter().map(|e| e.2).sum();
        let epoch_verified: usize = epoch_costs.iter().map(|e| e.4).sum();
        let epoch_decoded: usize = epoch_costs.iter().map(|e| e.5).sum();
        let ver_info = if epoch_ver_bl > 0 {
            format!(
                " | verified {} {}/{} ({:.2}x) | won {}",
                epoch_verified,
                epoch_ver_cost,
                epoch_ver_bl,
                epoch_ver_cost as f64 / epoch_ver_bl as f64,
                epoch_wins
            )
        } else {
            format!(" | verified 0 | won 0")
        };
        // Build per-file table
        // (path, total_blocks, trainable, dec_cost, dec_bl, ver_cost, ver_bl, decoded, verified, wins)
        let mut sorted: Vec<_> = epoch_costs
            .iter()
            .map(|e| {
                let cf = &compiled[e.0];
                let trainable = cf.per_block_verifiable.iter().filter(|&&v| v).count();
                (
                    cf.path.as_str(),
                    cf.blocks.len(),
                    trainable,
                    e.6, // decoded_cost
                    e.7, // decoded_baseline
                    e.8, // verified_cost
                    e.9, // verified_baseline
                    e.5, // decoded
                    e.4, // verified
                    e.2, // wins
                )
            })
            .collect();
        // Sort by verified wins (most first), then verified count, then decoded count
        sorted.sort_by(|a, b| {
            b.9.cmp(&a.9) // wins descending
                .then(b.8.cmp(&a.8)) // verified descending
                .then(b.7.cmp(&a.7)) // decoded descending
        });
        let total_blk: usize = sorted.iter().map(|s| s.1).sum();

        // Move cursor up to overwrite previous table (epoch > 0)
        // table_lines = 1 (epoch) + 1 (header) + 1 (separator) + file_count + 1 (separator)
        let table_lines = 1 + 1 + 1 + sorted.len() + 1;
        if epoch > 0 && prev_table_lines > 0 {
            eprint!("\x1B[{}A", prev_table_lines);
        }

        eprintln!(
            "\r  epoch {}/{} | decoded {}/{} | verified {}{} | {:.1}s{}{}\x1B[K",
            epoch + 1,
            args.epochs,
            epoch_decoded,
            total_blk,
            epoch_verified,
            ver_info,
            epoch_elapsed.as_secs_f64(),
            trend,
            conv_info,
        );

        // Table header
        eprintln!(
            "  {:<60} | {:>9} | {:>7} {:>8} {:>5} | {:>15}\x1B[K",
            "Module", "Blocks", "Decoded", "Verified", "Won", "Cost (ratio)"
        );
        eprintln!("  {}\x1B[K", "-".repeat(113));

        for &(path, total, trainable, _dc, _db, vc, vb, decoded, verified, wins) in &sorted {
            let blocks_col = format!("{}/{}", trainable, total);
            let cost_col = if verified > 0 {
                let vr = vc as f64 / vb.max(1) as f64;
                format!("{}/{} ({:.2}x)", vc, vb, vr)
            } else {
                "\u{2013}".to_string()
            };
            eprintln!(
                "  {:<60} | {:>9} | {:>7} {:>8} {:>5} | {:>15}\x1B[K",
                path, blocks_col, decoded, verified, wins, cost_col,
            );
        }
        eprintln!("  {}\x1B[K", "-".repeat(113));

        // Print diagnostics: first few decoded-but-not-verified blocks
        let mut diag_lines = 0;
        let max_diag_files = 2;
        let max_diag_blocks = 2;
        for (path, diags) in epoch_diagnostics.iter().take(max_diag_files) {
            eprintln!("  diag: {}\x1B[K", path);
            diag_lines += 1;
            for d in diags.iter().take(max_diag_blocks) {
                eprintln!("    block {} | {}\x1B[K", d.block_idx, d.reason);
                diag_lines += 1;
                let bl_summary: Vec<&str> = d.baseline.iter().take(5).map(|s| s.as_str()).collect();
                let cd_summary: Vec<&str> =
                    d.candidate.iter().take(5).map(|s| s.as_str()).collect();
                eprintln!(
                    "      baseline({}): {}{}\x1B[K",
                    d.baseline.len(),
                    bl_summary.join(" | "),
                    if d.baseline.len() > 5 { " ..." } else { "" },
                );
                eprintln!(
                    "      candidate({}): {}{}\x1B[K",
                    d.candidate.len(),
                    cd_summary.join(" | "),
                    if d.candidate.len() > 5 { " ..." } else { "" },
                );
                diag_lines += 2;
            }
        }

        prev_table_lines = table_lines + diag_lines;
    }

    let elapsed = start.elapsed();
    let meta = weights::load_best_meta().ok();
    let gen_end = meta.as_ref().map_or(0, |m| m.generation);

    // Corpus-level final cost (last epoch)
    // Last epoch's verified totals for summary
    let final_ver_cost = epoch_history.last().copied().unwrap_or(0);

    // Save corpus-level meta
    {
        let dummy_root = std::path::Path::new(".");
        let best_weights = weights::load_best_weights().ok();
        let weight_hash = best_weights
            .as_ref()
            .map(|w| weights::hash_weights(w))
            .unwrap_or_default();
        let new_meta = weights::OptimizerMeta {
            generation: gen_end,
            weight_hash,
            best_score: final_ver_cost,
            prev_score: total_baseline,
            baseline_score: total_baseline,
            status: weights::OptimizerStatus::Improving,
        };
        let _ = weights::save_meta(&new_meta, &weights::meta_path(dummy_root));
    }

    eprintln!("done");
    eprintln!(
        "  generations  {} -> {} (+{})",
        gen_start,
        gen_end,
        gen_end - gen_start
    );
    eprintln!(
        "  trained      {} file-passes in {:.1}s",
        total_trained,
        elapsed.as_secs_f64()
    );
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
    if let Some(meta) = meta {
        eprintln!("  weights      {}", meta.weight_hash);
    }
}

fn cmd_train_reset() {
    use trident::ir::tir::neural::weights;

    let repo_root = find_repo_root();
    let mut deleted = 0usize;

    // Delete weights (data/neural/)
    let weights_dir = weights::weights_path(Path::new("."))
        .parent()
        .unwrap()
        .to_path_buf();
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

/// Compile all files once, return only those with trainable blocks.
fn compile_corpus(files: &[std::path::PathBuf]) -> Vec<CompiledFile> {
    let options = super::resolve_options("triton", "debug", None);
    let mut compiled = Vec::new();

    for file in files {
        let ir = match trident::build_tir_project(file, &options) {
            Ok(ir) => ir,
            Err(_) => continue,
        };
        let blocks = trident::ir::tir::encode::encode_blocks(&ir);
        if blocks.is_empty() {
            continue;
        }

        let lowering = trident::ir::tir::lower::create_stack_lowering(&options.target_config.name);
        let compiled_tasm = lowering.lower(&ir).join("\n");

        let mut per_block_baselines: Vec<u64> = Vec::new();
        let mut per_block_tasm: Vec<Vec<String>> = Vec::new();
        for block in &blocks {
            let block_ops = &ir[block.start_idx..block.end_idx];
            if block_ops.is_empty() {
                per_block_baselines.push(1);
                per_block_tasm.push(Vec::new());
                continue;
            }
            let block_tasm = lowering.lower(block_ops);
            if block_tasm.is_empty() {
                per_block_baselines.push(1);
                per_block_tasm.push(Vec::new());
                continue;
            }
            let profile = trident::cost::scorer::profile_tasm(
                &block_tasm.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            );
            per_block_baselines.push(profile.cost().max(1));
            per_block_tasm.push(block_tasm);
        }

        let baseline_cost: u64 = per_block_baselines.iter().sum();

        // All non-empty blocks are trainable — the verifier now handles
        // side-effect ops (write_io, halt, assert, divine, split) via
        // side-channel comparison.
        let per_block_verifiable: Vec<bool> =
            per_block_tasm.iter().map(|tasm| !tasm.is_empty()).collect();

        compiled.push(CompiledFile {
            path: short_path(file),
            blocks,
            per_block_baselines,
            per_block_tasm,
            per_block_verifiable,
            baseline_cost,
            compiled_tasm,
        });
    }

    compiled
}

/// Result of training a single file: cost + optional per-block neural TASM.
struct TrainResult {
    cost: u64,
    /// Per-block TASM for every block in the file.
    neural_tasm: Option<Vec<Vec<String>>>,
    /// Number of blocks where neural candidate was cheaper than baseline.
    neural_wins: usize,
    /// Number of blocks where neural candidate passed verification (any cost).
    neural_verified: usize,
    /// Number of blocks where model produced a non-empty decodable candidate.
    neural_decoded: usize,
    /// Sum of neural costs for decoded blocks (unverified — shows what model produces).
    decoded_cost: u64,
    /// Sum of baselines for decoded blocks (same set as decoded_cost).
    decoded_baseline: u64,
    /// Sum of neural costs for verified blocks only.
    verified_cost: u64,
    /// Sum of baselines for verified blocks only.
    verified_baseline: u64,
    /// Total number of blocks in this file.
    total_blocks: usize,
    /// Diagnostic examples: decoded-but-not-verified blocks (up to 3 per file).
    diagnostics: Vec<BlockDiagnostic>,
}

/// Diagnostic info for a single block that decoded but failed verification.
struct BlockDiagnostic {
    block_idx: usize,
    baseline: Vec<String>,
    candidate: Vec<String>,
    reason: String,
}

/// Train on a pre-compiled file. Returns cost + captured neural TASM.
fn train_one_compiled(
    cf: &CompiledFile,
    generations: u64,
    gpu_accel: &mut Option<trident::gpu::neural_accel::NeuralAccelerator>,
) -> TrainResult {
    use trident::ir::tir::neural::evolve::Population;
    use trident::ir::tir::neural::model::NeuralModel;
    use trident::ir::tir::neural::weights::{self, OptimizerMeta, OptimizerStatus};

    let default_meta = OptimizerMeta {
        generation: 0,
        weight_hash: String::new(),
        best_score: 0,
        prev_score: 0,
        baseline_score: 0,
        status: OptimizerStatus::Improving,
    };

    let (current_weights, meta) = match weights::load_best_weights() {
        Ok(w) => {
            let meta = weights::load_best_meta().unwrap_or_else(|_| {
                let mut m = default_meta.clone();
                m.weight_hash = weights::hash_weights(&w);
                m
            });
            (w, meta)
        }
        Err(_) => {
            let w = NeuralModel::zeros().to_weight_vec();
            (w, default_meta)
        }
    };

    let gen_start = meta.generation;
    let weight_count = current_weights.len();
    let mut pop = if current_weights.iter().all(|w| w.to_f64() == 0.0) {
        Population::new_random_with_size(weight_count, gen_start.wrapping_add(42))
    } else {
        Population::from_weights(&current_weights, gen_start.wrapping_add(42))
    };

    let score_before = if meta.best_score > 0 {
        meta.best_score
    } else {
        cf.baseline_cost
    };

    // Upload this file's blocks to the shared GPU accelerator
    if let Some(ref mut accel) = gpu_accel {
        accel.upload_blocks(&cf.blocks);
    }

    let mut best_seen = i64::MIN;
    // (per_block_tasm, honest_cost, wins, verified, decoded, dec_cost, dec_bl, ver_cost, ver_bl)
    let mut best_captured: Option<(
        Vec<Vec<String>>,
        u64,
        usize,
        usize,
        usize,
        u64,
        u64,
        u64,
        u64,
    )> = None;
    for gen in 0..generations {
        let evals: Vec<(
            i64,
            Vec<Vec<String>>,
            u64,
            usize,
            usize,
            usize,
            u64,
            u64,
            u64,
            u64,
        )> = if let Some(ref accel) = gpu_accel {
            let weight_vecs: Vec<Vec<trident::field::fixed::Fixed>> = pop
                .individuals
                .iter()
                .map(|ind| ind.weights.clone())
                .collect();
            let gpu_outputs = accel.batch_forward(&weight_vecs);
            let baselines = &cf.per_block_baselines;
            let block_tasm = &cf.per_block_tasm;
            std::thread::scope(|s| {
                let handles: Vec<_> = gpu_outputs
                    .iter()
                    .map(|ind_outputs| {
                        let verifiable = &cf.per_block_verifiable;
                        s.spawn(move || {
                            eval_individual_gpu(ind_outputs, baselines, block_tasm, verifiable)
                        })
                    })
                    .collect();
                handles.into_iter().map(|h| h.join().unwrap()).collect()
            })
        } else {
            std::thread::scope(|s| {
                let handles: Vec<_> = pop
                    .individuals
                    .iter()
                    .map(|individual| s.spawn(move || eval_individual_cpu(individual, cf)))
                    .collect();
                handles
                    .into_iter()
                    .map(|h| h.join().expect("evaluate thread panicked"))
                    .collect()
            })
        };

        for (i, eval) in evals.iter().enumerate() {
            pop.individuals[i].fitness = eval.0;
        }
        pop.update_best();

        if let Some((idx, best)) = evals.iter().enumerate().max_by_key(|(_, e)| e.0) {
            if best.0 > best_seen {
                best_seen = best.0;
                let (_, tasm, cost, w, v, d, dc, db, vc, vb) = evals.into_iter().nth(idx).unwrap();
                best_captured = Some((tasm, cost, w, v, d, dc, db, vc, vb));
            }
        }
        pop.evolve(gen_start.wrapping_add(gen));
    }

    let best = pop.best_weights();
    // best_seen = total improvement (baseline - cost). Cost = baseline - improvement.
    let score_after = if best_seen > 0 {
        cf.baseline_cost.saturating_sub(best_seen as u64)
    } else {
        cf.baseline_cost
    };

    let weight_hash = weights::hash_weights(best);
    let dummy_root = Path::new(".");
    let _ = weights::save_weights(best, &weights::weights_path(dummy_root));

    let mut tracker = weights::ConvergenceTracker::new();
    let status = tracker.record(score_after);
    let new_meta = OptimizerMeta {
        generation: gen_start + generations,
        weight_hash,
        best_score: score_after,
        prev_score: score_before,
        baseline_score: cf.baseline_cost,
        status,
    };
    let _ = weights::save_meta(&new_meta, &weights::meta_path(dummy_root));

    let (
        honest_cost,
        neural_tasm,
        neural_wins,
        neural_verified,
        neural_decoded,
        d_cost,
        d_bl,
        v_cost,
        v_bl,
        diagnostics,
    ) = if let Some((tasm, cost, wins, ver, dec, dc, db, vc, vb)) = best_captured {
        // Collect diagnostics: decoded-but-not-verified blocks (up to 3)
        use trident::cost::stack_verifier;
        let mut diag = Vec::new();
        for (i, neural_block) in tasm.iter().enumerate() {
            if diag.len() >= 3 {
                break;
            }
            let baseline_block = &cf.per_block_tasm[i];
            if baseline_block.is_empty() || neural_block == baseline_block {
                continue;
            }
            if !stack_verifier::verify_equivalent(baseline_block, neural_block, i as u64) {
                let reason =
                    stack_verifier::diagnose_failure(baseline_block, neural_block, i as u64);
                diag.push(BlockDiagnostic {
                    block_idx: i,
                    baseline: baseline_block.clone(),
                    candidate: neural_block.clone(),
                    reason,
                });
            }
        }
        (
            cost,
            if ver > 0 { Some(tasm) } else { None },
            wins,
            ver,
            dec,
            dc,
            db,
            vc,
            vb,
            diag,
        )
    } else {
        (cf.baseline_cost, None, 0, 0, 0, 0, 0, 0, 0, Vec::new())
    };

    TrainResult {
        cost: honest_cost,
        neural_tasm,
        neural_wins,
        neural_verified,
        neural_decoded,
        decoded_cost: d_cost,
        decoded_baseline: d_bl,
        verified_cost: v_cost,
        verified_baseline: v_bl,
        total_blocks: cf.blocks.len(),
        diagnostics,
    }
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

fn shuffle(indices: &mut Vec<usize>, seed: u64) {
    let n = indices.len();
    if n <= 1 {
        return;
    }
    let mut state = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    for i in (1..n).rev() {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let j = (state >> 33) as usize % (i + 1);
        indices.swap(i, j);
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

/// Write captured neural TASM to disk at benches/<path>.neural.tasm.
///
/// Produces a complete, executable TASM program by splicing neural blocks
/// into the full compiler output. Only straight-line blocks (no labels,
/// no control flow) are substituted — the rest of the program (function
/// labels, loops, subroutines, memory ops) is preserved from the compiler.
fn write_neural_tasm(cf: &CompiledFile, per_block: &[Vec<String>], repo_root: &Path) {
    let benches_dir = repo_root.join("benches");
    let neural_path = benches_dir.join(cf.path.replace(".tri", ".neural.tasm"));
    if let Some(parent) = neural_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    // Start with full compiler output; splice in neural blocks where they differ.
    let mut result = cf.compiled_tasm.clone();
    let mut substitutions = 0usize;

    for (i, neural_block) in per_block.iter().enumerate() {
        let classical_block = &cf.per_block_tasm[i];
        if classical_block.is_empty() || neural_block == classical_block {
            continue; // No substitution needed
        }
        // Find classical block text in the full TASM and replace with neural.
        // Straight-line blocks have no labels, so substring match is reliable.
        let needle = classical_block.join("\n");
        let replacement = neural_block.join("\n");
        if let Some(pos) = result.find(&needle) {
            result = format!(
                "{}{}{}",
                &result[..pos],
                replacement,
                &result[pos + needle.len()..],
            );
            substitutions += 1;
        }
    }

    if std::fs::write(&neural_path, &result).is_ok() {
        let tag = if substitutions > 0 {
            format!(" ({} blocks substituted)", substitutions)
        } else {
            " (no substitutions)".into()
        };
        eprintln!(
            "\r  wrote {}{}{}",
            neural_path
                .strip_prefix(repo_root)
                .unwrap_or(&neural_path)
                .display(),
            tag,
            " ".repeat(20),
        );
    }
}

/// Evaluate one individual on GPU outputs.
/// Returns (fitness, per_block_tasm, honest_cost, wins, verified, decoded,
///          decoded_cost, decoded_baseline, verified_cost, verified_baseline).
fn eval_individual_gpu(
    ind_outputs: &[Vec<u32>],
    baselines: &[u64],
    block_tasm: &[Vec<String>],
    verifiable: &[bool],
) -> (
    i64,
    Vec<Vec<String>>,
    u64,
    usize,
    usize,
    usize,
    u64,
    u64,
    u64,
    u64,
) {
    use trident::cost::{scorer, stack_verifier};
    use trident::ir::tir::lower::decode_output;

    let mut fitness = 0i64;
    let mut per_block: Vec<Vec<String>> = Vec::with_capacity(ind_outputs.len());
    let mut honest_cost = 0u64;
    let mut wins = 0usize;
    let mut verified = 0usize;
    let mut decoded = 0usize;
    let mut decoded_cost = 0u64;
    let mut decoded_bl = 0u64;
    let mut verified_cost = 0u64;
    let mut verified_bl = 0u64;

    for (b, block_out) in ind_outputs.iter().enumerate() {
        let baseline = baselines[b];
        let baseline_lines = &block_tasm[b];

        if !verifiable.get(b).copied().unwrap_or(false) {
            honest_cost += baseline;
            per_block.push(baseline_lines.clone());
            continue;
        }

        let codes: Vec<u64> = block_out
            .iter()
            .take_while(|&&c| c != 0)
            .map(|&c| c as u64)
            .collect();
        if !codes.is_empty() {
            let candidate = decode_output(&codes);
            if !candidate.is_empty() && !baseline_lines.is_empty() {
                decoded += 1;
                let profile =
                    scorer::profile_tasm(&candidate.iter().map(|s| s.as_str()).collect::<Vec<_>>());
                let cost = profile.cost();
                decoded_cost += cost;
                decoded_bl += baseline;

                if stack_verifier::verify_equivalent(baseline_lines, &candidate, b as u64) {
                    verified += 1;
                    verified_cost += cost;
                    verified_bl += baseline;
                    honest_cost += cost;
                    if cost < baseline {
                        fitness += (baseline as i64) - (cost as i64);
                        wins += 1;
                    }
                    per_block.push(candidate);
                    continue;
                }
            }
        }
        honest_cost += baseline;
        per_block.push(baseline_lines.clone());
    }

    (
        fitness,
        per_block,
        honest_cost,
        wins,
        verified,
        decoded,
        decoded_cost,
        decoded_bl,
        verified_cost,
        verified_bl,
    )
}

/// Evaluate one individual on CPU.
/// Returns (fitness, per_block_tasm, honest_cost, wins, verified, decoded,
///          decoded_cost, decoded_baseline, verified_cost, verified_baseline).
fn eval_individual_cpu(
    individual: &trident::ir::tir::neural::evolve::Individual,
    cf: &CompiledFile,
) -> (
    i64,
    Vec<Vec<String>>,
    u64,
    usize,
    usize,
    usize,
    u64,
    u64,
    u64,
    u64,
) {
    use trident::cost::{scorer, stack_verifier};
    use trident::ir::tir::lower::decode_output;
    use trident::ir::tir::neural::model::NeuralModel;

    let mut model = NeuralModel::from_weight_vec(&individual.weights);
    let mut fitness = 0i64;
    let mut per_block: Vec<Vec<String>> = Vec::with_capacity(cf.blocks.len());
    let mut honest_cost = 0u64;
    let mut wins = 0usize;
    let mut verified = 0usize;
    let mut decoded = 0usize;
    let mut decoded_cost = 0u64;
    let mut decoded_bl = 0u64;
    let mut verified_cost = 0u64;
    let mut verified_bl = 0u64;

    for (i, block) in cf.blocks.iter().enumerate() {
        let baseline = cf.per_block_baselines[i];
        let baseline_tasm = &cf.per_block_tasm[i];

        if !cf.per_block_verifiable.get(i).copied().unwrap_or(false) {
            honest_cost += baseline;
            per_block.push(baseline_tasm.clone());
            continue;
        }

        let output = model.forward(block);
        if !output.is_empty() {
            let candidate = decode_output(&output);
            if !candidate.is_empty() && !baseline_tasm.is_empty() {
                decoded += 1;
                let profile =
                    scorer::profile_tasm(&candidate.iter().map(|s| s.as_str()).collect::<Vec<_>>());
                let cost = profile.cost();
                decoded_cost += cost;
                decoded_bl += baseline;

                if stack_verifier::verify_equivalent(baseline_tasm, &candidate, i as u64) {
                    verified += 1;
                    verified_cost += cost;
                    verified_bl += baseline;
                    honest_cost += cost;
                    if cost < baseline {
                        fitness += (baseline as i64) - (cost as i64);
                        wins += 1;
                    }
                    per_block.push(candidate);
                    continue;
                }
            }
        }
        honest_cost += baseline;
        per_block.push(baseline_tasm.clone());
    }

    (
        fitness,
        per_block,
        honest_cost,
        wins,
        verified,
        decoded,
        decoded_cost,
        decoded_bl,
        verified_cost,
        verified_bl,
    )
}
