use std::path::Path;
use std::process;

use clap::Args;

#[derive(Args)]
pub struct TrainArgs {
    /// Input .tri file or directory (trains on all .tri files found)
    pub input: std::path::PathBuf,
    /// Generations per file (default: 100)
    #[arg(short, long, default_value = "100")]
    pub generations: u64,
    /// Use GPU acceleration (default: CPU parallel)
    #[arg(long)]
    pub gpu: bool,
}

pub fn cmd_train(args: TrainArgs) {
    let files = super::resolve_tri_files(&args.input);
    if files.is_empty() {
        eprintln!("error: no .tri files found");
        process::exit(1);
    }

    use trident::ir::tir::neural::weights;

    let meta = weights::load_best_meta().ok();
    let gen_start = meta.as_ref().map_or(0, |m| m.generation);

    eprintln!(
        "Training neural optimizer on {} file(s), {} generations each",
        files.len(),
        args.generations,
    );
    if gen_start > 0 {
        eprintln!("  resuming from generation {}", gen_start);
    }
    eprintln!();

    let mut trained = 0u64;
    let mut skipped = 0u64;
    let total = files.len();
    let start = std::time::Instant::now();

    for (i, file) in files.iter().enumerate() {
        eprint!("[{}/{}] {} ", i + 1, total, file.display());

        match train_one(file, args.generations, args.gpu) {
            TrainResult::Trained { blocks, score } => {
                eprintln!("  {} blocks, cost {}", blocks, score);
                trained += 1;
            }
            TrainResult::NoBlocks => {
                eprintln!("  (no trainable blocks)");
                skipped += 1;
            }
            TrainResult::Failed(msg) => {
                eprintln!("  FAILED: {}", msg);
                skipped += 1;
            }
        }
    }

    let elapsed = start.elapsed();
    let meta = weights::load_best_meta().ok();
    let gen_end = meta.as_ref().map_or(0, |m| m.generation);

    eprintln!();
    eprintln!(
        "Done: {} trained, {} skipped, {} total generations ({:.1}s)",
        trained,
        skipped,
        gen_end - gen_start,
        elapsed.as_secs_f64(),
    );

    if let Some(meta) = meta {
        eprintln!(
            "  model: gen {}, score {}, status: {}",
            meta.generation, meta.best_score, meta.status,
        );
    }
}

enum TrainResult {
    Trained { blocks: usize, score: u64 },
    NoBlocks,
    Failed(String),
}

fn train_one(file: &Path, generations: u64, gpu: bool) -> TrainResult {
    use trident::field::PrimeField;
    use trident::ir::tir::encode;
    use trident::ir::tir::lower::decode_output;
    use trident::ir::tir::neural::evolve::Population;
    use trident::ir::tir::neural::model::NeuralModel;
    use trident::ir::tir::neural::weights::{self, OptimizerMeta, OptimizerStatus};

    let options = super::resolve_options("triton", "debug", None);

    // Build TIR
    let ir = match trident::build_tir_project(file, &options) {
        Ok(ir) => ir,
        Err(_) => return TrainResult::Failed("TIR build failed".into()),
    };

    let blocks = encode::encode_blocks(&ir);
    if blocks.is_empty() {
        return TrainResult::NoBlocks;
    }

    // Load current model
    let (model, meta) = match weights::load_best_weights() {
        Ok(w) => {
            let meta = weights::load_best_meta().unwrap_or(OptimizerMeta {
                generation: 0,
                weight_hash: weights::hash_weights(&w),
                best_score: 0,
                prev_score: 0,
                baseline_score: 0,
                status: OptimizerStatus::Improving,
            });
            (NeuralModel::from_weight_vec(&w), meta)
        }
        Err(_) => {
            let meta = OptimizerMeta {
                generation: 0,
                weight_hash: String::new(),
                best_score: 0,
                prev_score: 0,
                baseline_score: 0,
                status: OptimizerStatus::Improving,
            };
            (NeuralModel::zeros(), meta)
        }
    };

    let gen_start = meta.generation;
    let current_weights = model.to_weight_vec();
    let mut pop = if current_weights.iter().all(|w| w.to_f64() == 0.0) {
        Population::new_random(gen_start.wrapping_add(42))
    } else {
        Population::from_weights(&current_weights, gen_start.wrapping_add(42))
    };

    // Classical baselines
    let lowering = trident::ir::tir::lower::create_stack_lowering(&options.target_config.name);
    let baseline_tasm = lowering.lower(&ir);
    let baseline_profile = trident::cost::scorer::profile_tasm_str(&baseline_tasm.join("\n"));
    let baseline_cost = baseline_profile.cost();

    let score_before = if meta.best_score > 0 {
        meta.best_score
    } else {
        baseline_cost
    };

    let per_block_baselines: Vec<u64> = blocks
        .iter()
        .map(|block| {
            let block_ops = &ir[block.start_idx..block.end_idx];
            if block_ops.is_empty() {
                return 1;
            }
            let block_tasm = lowering.lower(block_ops);
            if block_tasm.is_empty() {
                return 1;
            }
            let profile = trident::cost::scorer::profile_tasm(
                &block_tasm.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            );
            profile.cost().max(1)
        })
        .collect();

    // GPU acceleration
    let gpu_accel = if gpu {
        trident::gpu::neural_accel::NeuralAccelerator::try_new(
            &blocks,
            trident::ir::tir::neural::evolve::POP_SIZE as u32,
        )
    } else {
        None
    };

    // Train
    let mut best_seen = i64::MIN;
    for gen in 0..generations {
        if let Some(ref accel) = gpu_accel {
            let weight_vecs: Vec<Vec<u64>> = pop
                .individuals
                .iter()
                .map(|ind| ind.weights.iter().map(|w| w.raw().to_u64()).collect())
                .collect();
            let gpu_outputs = accel.batch_forward(&weight_vecs);
            for (i, ind) in pop.individuals.iter_mut().enumerate() {
                let mut total = 0i64;
                for (b, _block) in blocks.iter().enumerate() {
                    total -= score_neural_output(&gpu_outputs[i][b], per_block_baselines[b]) as i64;
                }
                ind.fitness = total;
            }
            pop.update_best();
        } else {
            pop.evaluate_with_baselines(
                &blocks,
                &per_block_baselines,
                |m: &mut NeuralModel,
                 block: &trident::ir::tir::encode::TIRBlock,
                 block_baseline: u64| {
                    let output = m.forward(block);
                    if output.is_empty() {
                        return -(block_baseline as i64);
                    }
                    let candidate_lines = decode_output(&output);
                    if candidate_lines.is_empty() {
                        return -(block_baseline as i64);
                    }
                    let profile = trident::cost::scorer::profile_tasm(
                        &candidate_lines
                            .iter()
                            .map(|s| s.as_str())
                            .collect::<Vec<_>>(),
                    );
                    -(profile.cost().min(block_baseline) as i64)
                },
            );
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

    // Save
    let best = pop.best_weights();
    let score_after = if best_seen > i64::MIN {
        (-best_seen) as u64
    } else {
        baseline_cost
    };

    let weight_hash = weights::hash_weights(best);
    let project_root = file.parent().unwrap_or(Path::new("."));
    let _ = weights::save_weights(best, &weights::weights_path(project_root));

    let mut tracker = weights::ConvergenceTracker::new();
    let status = tracker.record(score_after);

    let new_meta = OptimizerMeta {
        generation: gen_start + generations,
        weight_hash,
        best_score: score_after,
        prev_score: score_before,
        baseline_score: baseline_cost,
        status,
    };
    let _ = weights::save_meta(&new_meta, &weights::meta_path(project_root));

    TrainResult::Trained {
        blocks: blocks.len(),
        score: score_after,
    }
}

fn score_neural_output(raw_codes: &[u32], block_baseline: u64) -> u64 {
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
    let profile = trident::cost::scorer::profile_tasm(
        &candidate_lines
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>(),
    );
    profile.cost().min(block_baseline)
}
