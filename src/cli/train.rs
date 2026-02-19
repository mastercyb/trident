use std::path::Path;
use std::process;

use clap::Args;

#[derive(Args)]
pub struct TrainArgs {
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

/// Pre-compiled file data — TIR + blocks + baselines, computed once.
struct CompiledFile {
    path: String,
    blocks: Vec<trident::ir::tir::encode::TIRBlock>,
    per_block_baselines: Vec<u64>,
    per_block_tasm: Vec<Vec<String>>,
    baseline_cost: u64,
}

pub fn cmd_train(args: TrainArgs) {
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

    eprintln!(
        "  corpus    {} files ({} trainable, {} blocks)",
        corpus.len(),
        compiled.len(),
        total_blocks
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
    let mut prev_epoch_avg = 0u64;
    let mut epoch_history: Vec<u64> = Vec::new();
    // EMA of per-epoch improvement rate + volatility (alpha=0.3 — smooths over ~3 epochs)
    // Initialized to None — seeded from first observed values, not 0.0.
    let mut ema_rate: Option<f64> = None;
    let mut ema_volatility: Option<f64> = None;
    const EMA_ALPHA: f64 = 0.3;

    for epoch in 0..args.epochs {
        // Shuffle file indices each epoch
        let mut indices: Vec<usize> = (0..compiled.len()).collect();
        shuffle(&mut indices, gen_start + epoch);

        let epoch_start = std::time::Instant::now();
        let mut epoch_costs: Vec<(usize, u64)> = Vec::new();

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

            let cost = train_one_compiled(cf, args.generations, &mut gpu_accel);
            epoch_costs.push((file_idx, cost));
            total_trained += 1;
        }

        let epoch_elapsed = epoch_start.elapsed();
        let epoch_cost: u64 = epoch_costs.iter().map(|(_, c)| c).sum();
        let avg_cost = epoch_cost / compiled.len().max(1) as u64;

        let trend = if epoch == 0 {
            String::new()
        } else if avg_cost < prev_epoch_avg {
            format!(" (-{} vs prev)", prev_epoch_avg - avg_cost)
        } else if avg_cost > prev_epoch_avg {
            format!(" (+{} vs prev)", avg_cost - prev_epoch_avg)
        } else {
            " (=)".into()
        };
        prev_epoch_avg = avg_cost;
        epoch_history.push(epoch_cost);

        let ratio = epoch_cost as f64 / total_baseline.max(1) as f64;

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

        let reduction_pct = (1.0 - ratio) * 100.0;
        eprintln!(
            "\r  epoch {}/{} | cost {}/{} ({:.2}x) | {:.1}% reduction | {:.1}s{}{}",
            epoch + 1,
            args.epochs,
            epoch_cost,
            total_baseline,
            ratio,
            reduction_pct,
            epoch_elapsed.as_secs_f64(),
            trend,
            conv_info,
        );

        // Per-file breakdown on first and last epoch
        if epoch == 0 || epoch + 1 == args.epochs {
            let mut sorted: Vec<_> = epoch_costs
                .iter()
                .map(|&(idx, cost)| {
                    let cf = &compiled[idx];
                    (cf.path.as_str(), cf.blocks.len(), cost, cf.baseline_cost)
                })
                .collect();
            sorted.sort_by(|a, b| {
                let ra = a.2 as f64 / a.3.max(1) as f64;
                let rb = b.2 as f64 / b.3.max(1) as f64;
                ra.partial_cmp(&rb).unwrap()
            });
            let label = if epoch == 0 { "initial" } else { "final" };
            eprintln!("    {} per-file breakdown:", label);
            for (path, blocks, cost, baseline) in &sorted {
                let r = *cost as f64 / (*baseline).max(1) as f64;
                eprintln!(
                    "      {:<45} {:>3} blk  {:>6} / {:<6} ({:.2}x) {}",
                    path,
                    blocks,
                    cost,
                    baseline,
                    r,
                    cost_bar(r),
                );
            }
            eprintln!();
        }
    }

    let elapsed = start.elapsed();
    let meta = weights::load_best_meta().ok();
    let gen_end = meta.as_ref().map_or(0, |m| m.generation);

    // Corpus-level final cost (last epoch)
    let final_cost = prev_epoch_avg * compiled.len() as u64;
    let final_ratio = final_cost as f64 / total_baseline.max(1) as f64;

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
            best_score: final_cost,
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
    eprintln!(
        "  corpus cost  {} / {} baseline ({:.2}x) — {:.1}% reduction",
        final_cost,
        total_baseline,
        final_ratio,
        (1.0 - final_ratio) * 100.0,
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

fn cost_bar(ratio: f64) -> &'static str {
    if ratio <= 0.25 {
        ">>>>"
    } else if ratio <= 0.5 {
        ">>>"
    } else if ratio <= 0.75 {
        ">>"
    } else if ratio < 1.0 {
        ">"
    } else {
        "="
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
        let baseline_tasm = lowering.lower(&ir);
        let baseline_profile = trident::cost::scorer::profile_tasm_str(&baseline_tasm.join("\n"));
        let baseline_cost = baseline_profile.cost();

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

        compiled.push(CompiledFile {
            path: short_path(file),
            blocks,
            per_block_baselines,
            per_block_tasm,
            baseline_cost,
        });
    }

    compiled
}

/// Train on a pre-compiled file. Returns the cost after training.
fn train_one_compiled(
    cf: &CompiledFile,
    generations: u64,
    gpu_accel: &mut Option<trident::gpu::neural_accel::NeuralAccelerator>,
) -> u64 {
    use trident::ir::tir::lower::decode_output;
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
    for gen in 0..generations {
        if let Some(ref accel) = gpu_accel {
            let weight_vecs: Vec<Vec<trident::field::fixed::Fixed>> = pop
                .individuals
                .iter()
                .map(|ind| ind.weights.clone())
                .collect();
            let gpu_outputs = accel.batch_forward(&weight_vecs);
            // Score in parallel — one thread per individual
            let baselines = &cf.per_block_baselines;
            let block_tasm = &cf.per_block_tasm;
            let fitnesses: Vec<i64> = std::thread::scope(|s| {
                let handles: Vec<_> = gpu_outputs
                    .iter()
                    .map(|ind_outputs| {
                        s.spawn(move || {
                            let mut total = 0i64;
                            for (b, block_out) in ind_outputs.iter().enumerate() {
                                total -= score_neural_output(
                                    block_out,
                                    baselines[b],
                                    &block_tasm[b],
                                    b as u64,
                                ) as i64;
                            }
                            total
                        })
                    })
                    .collect();
                handles.into_iter().map(|h| h.join().unwrap()).collect()
            });
            for (i, ind) in pop.individuals.iter_mut().enumerate() {
                ind.fitness = fitnesses[i];
            }
            pop.update_best();
        } else {
            // CPU path
            let fitnesses: Vec<i64> = std::thread::scope(|s| {
                let handles: Vec<_> = pop
                    .individuals
                    .iter()
                    .map(|individual| {
                        s.spawn(move || {
                            use trident::cost::stack_verifier;
                            let mut model = NeuralModel::from_weight_vec(&individual.weights);
                            let mut total = 0i64;
                            for (i, block) in cf.blocks.iter().enumerate() {
                                let baseline = cf.per_block_baselines[i];
                                let output = model.forward(block);
                                if output.is_empty() {
                                    total -= baseline as i64;
                                    continue;
                                }
                                let candidate_lines = decode_output(&output);
                                if candidate_lines.is_empty() {
                                    total -= baseline as i64;
                                    continue;
                                }
                                // Correctness check
                                let baseline_tasm = &cf.per_block_tasm[i];
                                if !baseline_tasm.is_empty()
                                    && !stack_verifier::verify_equivalent(
                                        baseline_tasm,
                                        &candidate_lines,
                                        i as u64,
                                    )
                                {
                                    total -= baseline as i64;
                                    continue;
                                }
                                let profile = trident::cost::scorer::profile_tasm(
                                    &candidate_lines
                                        .iter()
                                        .map(|s| s.as_str())
                                        .collect::<Vec<_>>(),
                                );
                                total -= profile.cost().min(baseline) as i64;
                            }
                            total
                        })
                    })
                    .collect();
                handles
                    .into_iter()
                    .map(|h| h.join().expect("evaluate thread panicked"))
                    .collect()
            });
            for (i, ind) in pop.individuals.iter_mut().enumerate() {
                ind.fitness = fitnesses[i];
            }
            pop.update_best();
        }

        let gen_best = pop
            .individuals
            .iter()
            .map(|i| i.fitness)
            .max()
            .unwrap_or(i64::MIN);
        if gen_best > best_seen {
            best_seen = gen_best;
        }
        pop.evolve(gen_start.wrapping_add(gen));
    }

    let best = pop.best_weights();
    let score_after = if best_seen > i64::MIN {
        (-best_seen) as u64
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

    score_after
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

fn score_neural_output(
    raw_codes: &[u32],
    block_baseline: u64,
    baseline_tasm: &[String],
    block_seed: u64,
) -> u64 {
    use trident::cost::stack_verifier;
    use trident::ir::tir::lower::decode_output;
    let codes: Vec<u64> = raw_codes
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u64)
        .collect();
    if codes.is_empty() {
        return block_baseline;
    }
    let candidate_lines = decode_output(&codes);
    if candidate_lines.is_empty() {
        return block_baseline;
    }
    // Correctness check: candidate must produce same stack as baseline
    if !baseline_tasm.is_empty()
        && !stack_verifier::verify_equivalent(baseline_tasm, &candidate_lines, block_seed)
    {
        return block_baseline; // incorrect — reject
    }
    let profile = trident::cost::scorer::profile_tasm(
        &candidate_lines
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>(),
    );
    profile.cost().min(block_baseline)
}
