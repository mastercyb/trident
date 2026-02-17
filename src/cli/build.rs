use std::path::PathBuf;
use std::process;

use clap::Args;

use super::{find_program_source, load_dep_dirs, resolve_input, resolve_options};

#[derive(Args)]
pub struct BuildArgs {
    /// Input .tri file or directory with trident.toml
    pub input: PathBuf,
    /// Output .tasm file (default: <input>.tasm)
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    /// Print cost analysis report
    #[arg(long)]
    pub costs: bool,
    /// Show top cost contributors (implies --costs)
    #[arg(long)]
    pub hotspots: bool,
    /// Show optimization hints (H0001-H0004)
    #[arg(long)]
    pub hints: bool,
    /// Output per-line cost annotations
    #[arg(long)]
    pub annotate: bool,
    /// Save cost analysis to a JSON file
    #[arg(long, value_name = "PATH")]
    pub save_costs: Option<PathBuf>,
    /// Compare costs with a previous cost JSON file
    #[arg(long, value_name = "PATH")]
    pub compare: Option<PathBuf>,
    /// Target VM (default: triton)
    #[arg(long, default_value = "triton")]
    pub target: String,
    /// Engine (geeky for terrain/VM)
    #[arg(long, conflicts_with_all = ["terrain", "network", "union_flag"])]
    pub engine: Option<String>,
    /// Terrain (gamy for engine/VM)
    #[arg(long, conflicts_with_all = ["engine", "network", "union_flag"])]
    pub terrain: Option<String>,
    /// Network (geeky for union/OS)
    #[arg(long, conflicts_with_all = ["engine", "terrain", "union_flag"])]
    pub network: Option<String>,
    /// Union (gamy for network/OS)
    #[arg(long = "union", conflicts_with_all = ["engine", "terrain", "network"])]
    pub union_flag: Option<String>,
    /// Compilation profile for cfg flags (debug or release)
    #[arg(long, default_value = "debug")]
    pub profile: String,
    /// Run neural optimizer analysis (shows per-block decisions)
    #[arg(long)]
    pub neural: bool,
    /// Train the neural optimizer for N generations (implies --neural)
    #[arg(long, value_name = "GENERATIONS")]
    pub train: Option<u64>,
}

pub fn cmd_build(args: BuildArgs) {
    let BuildArgs {
        input,
        output,
        costs,
        hotspots,
        hints,
        annotate,
        save_costs,
        compare,
        target,
        engine,
        terrain,
        network,
        union_flag,
        profile,
        neural,
        train,
    } = args;
    let bf = super::resolve_battlefield_compile(&target, &engine, &terrain, &network, &union_flag);
    let target = bf.target;
    let ri = resolve_input(&input);

    let mut options = resolve_options(&target, &profile, ri.project.as_ref());
    if let Some(ref proj) = ri.project {
        options.dep_dirs = load_dep_dirs(proj);
    }

    let tasm = match trident::compile_project_with_options(&ri.entry, &options) {
        Ok(t) => t,
        Err(_) => process::exit(1),
    };

    let default_output = if let Some(ref proj) = ri.project {
        proj.root_dir.join(format!("{}.tasm", proj.name))
    } else {
        input.with_extension("tasm")
    };

    let out_path = output.unwrap_or(default_output);
    if let Err(e) = std::fs::write(&out_path, &tasm) {
        eprintln!("error: cannot write '{}': {}", out_path.display(), e);
        process::exit(1);
    }
    eprintln!("Compiled -> {}", out_path.display());

    // Neural optimizer analysis
    let use_neural = neural || train.is_some();
    if use_neural {
        run_neural_analysis(&ri.entry, &options, train);
    }

    if annotate {
        if let Some(source_path) = find_program_source(&input) {
            let source = std::fs::read_to_string(&source_path).unwrap_or_default();
            let filename = source_path.to_string_lossy().to_string();
            match trident::annotate_source_with_target(&source, &filename, &target) {
                Ok(annotated) => println!("{}", annotated),
                Err(_) => eprintln!("error: could not annotate source (compilation errors)"),
            }
        }
    }

    let need_costs = costs || hotspots || hints || save_costs.is_some() || compare.is_some();
    if !need_costs {
        return;
    }
    let source_path = match find_program_source(&input) {
        Some(p) => p,
        None => return,
    };
    let cost_options = resolve_options(&target, &profile, None);
    let program_cost = match trident::analyze_costs_project(&source_path, &cost_options) {
        Ok(c) => c,
        Err(_) => return,
    };

    if costs || hotspots {
        eprintln!("\n{}", program_cost.format_report());
        if hotspots {
            eprintln!("{}", program_cost.format_hotspots(5));
        }
    }
    if hints {
        print_hints(&program_cost);
    }
    if let Some(ref save_path) = save_costs {
        if let Err(e) = program_cost.save_json(save_path) {
            eprintln!("error: {}", e);
            process::exit(1);
        }
        eprintln!("Saved costs -> {}", save_path.display());
    }
    if let Some(ref compare_path) = compare {
        match trident::cost::ProgramCost::load_json(compare_path) {
            Ok(old_cost) => eprintln!("\n{}", old_cost.format_comparison(&program_cost)),
            Err(e) => {
                eprintln!("error: {}", e);
                process::exit(1);
            }
        }
    }
}

fn run_neural_analysis(
    entry: &std::path::Path,
    options: &trident::CompileOptions,
    train_generations: Option<u64>,
) {
    use trident::field::PrimeField;
    use trident::ir::tir::encode;
    use trident::ir::tir::lower::{create_speculative_lowering, decode_output, StackLowering};
    use trident::ir::tir::neural::evolve::Population;
    use trident::ir::tir::neural::model::NeuralModel;
    use trident::ir::tir::neural::report::OptimizerReport;
    use trident::ir::tir::neural::weights::{self, OptimizerMeta, OptimizerStatus};

    // Determine project root for weight storage
    let project_root = entry.parent().unwrap_or(std::path::Path::new("."));
    let weights_path = weights::weights_path(project_root);
    let meta_path = weights::meta_path(project_root);

    // Load existing weights or start fresh
    let (model, meta) = match weights::load_weights(&weights_path) {
        Ok(w) => {
            let meta = weights::load_meta(&meta_path).unwrap_or(OptimizerMeta {
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

    // Build TIR for neural analysis
    let ir = match build_tir(entry, options) {
        Some(ir) => ir,
        None => {
            eprintln!("warning: could not build TIR for neural analysis");
            return;
        }
    };

    // Training mode
    if let Some(generations) = train_generations {
        let blocks = encode::encode_blocks(&ir);
        if blocks.is_empty() {
            eprintln!("No blocks to train on.");
            return;
        }

        let start_time = std::time::Instant::now();
        let gen_start = meta.generation;

        // Create population from current weights
        let current_weights = model.to_weight_vec();
        let mut pop = if current_weights.iter().all(|w| w.to_f64() == 0.0) {
            Population::new_random(gen_start.wrapping_add(42))
        } else {
            Population::from_weights(&current_weights, gen_start.wrapping_add(42))
        };

        // Compute baseline cost (classical lowering)
        let lowering = trident::ir::tir::lower::create_stack_lowering(&options.target_config.name);
        let baseline_tasm = lowering.lower(&ir);
        let baseline_profile = trident::cost::scorer::profile_tasm_str(&baseline_tasm.join("\n"));
        let baseline_cost = baseline_profile.cost();

        let score_before = if meta.best_score > 0 {
            meta.best_score
        } else {
            baseline_cost
        };

        eprintln!(
            "Training neural optimizer on {} blocks ({} weights), baseline cost: {}",
            blocks.len(),
            pop.individuals[0].weights.len(),
            baseline_cost,
        );

        let per_block_baseline = baseline_cost / blocks.len().max(1) as u64;

        // Try GPU acceleration
        let gpu_accel = trident::gpu::neural_accel::NeuralAccelerator::try_new(
            &blocks,
            trident::ir::tir::neural::evolve::POP_SIZE as u32,
        );
        if gpu_accel.is_some() {
            eprintln!("  using GPU acceleration");
        }

        // Train for N generations with live progress
        let mut best_seen = i64::MIN;
        for gen in 0..generations {
            if let Some(ref accel) = gpu_accel {
                // GPU path: batch all forward passes in one dispatch
                let weight_vecs: Vec<Vec<u64>> = pop
                    .individuals
                    .iter()
                    .map(|ind| ind.weights.iter().map(|w| w.raw().to_u64()).collect())
                    .collect();
                let gpu_outputs = accel.batch_forward(&weight_vecs);

                // Score outputs on CPU
                for (i, ind) in pop.individuals.iter_mut().enumerate() {
                    let mut total = 0i64;
                    for (b, _block) in blocks.iter().enumerate() {
                        let codes: Vec<u64> = gpu_outputs[i][b]
                            .iter()
                            .take_while(|&&c| c != 0)
                            .map(|&c| c as u64)
                            .collect();
                        if codes.is_empty() {
                            total -= per_block_baseline as i64;
                        } else {
                            let candidate_lines = decode_output(&codes);
                            if candidate_lines.is_empty() {
                                total -= per_block_baseline as i64;
                            } else {
                                let profile = trident::cost::scorer::profile_tasm(
                                    &candidate_lines
                                        .iter()
                                        .map(|s| s.as_str())
                                        .collect::<Vec<_>>(),
                                );
                                total -= profile.cost() as i64;
                            }
                        }
                    }
                    ind.fitness = total;
                }
                pop.update_best();
            } else {
                // CPU fallback
                pop.evaluate(
                    &blocks,
                    |m: &mut NeuralModel, block: &trident::ir::tir::encode::TIRBlock| {
                        let output = m.forward(block);
                        if output.is_empty() {
                            return -(per_block_baseline as i64);
                        }
                        let candidate_lines = decode_output(&output);
                        if candidate_lines.is_empty() {
                            return -(per_block_baseline as i64);
                        }
                        let profile = trident::cost::scorer::profile_tasm(
                            &candidate_lines
                                .iter()
                                .map(|s| s.as_str())
                                .collect::<Vec<_>>(),
                        );
                        -(profile.cost() as i64)
                    },
                );
            }

            let gen_best = pop
                .individuals
                .iter()
                .map(|i| i.fitness)
                .max()
                .unwrap_or(i64::MIN);
            let improved = gen_best > best_seen;
            if improved {
                best_seen = gen_best;
            }

            // Print progress: every gen for <=20, every 5 for <=100, every 10 otherwise
            let print_interval = if generations <= 20 {
                1
            } else if generations <= 100 {
                5
            } else {
                10
            };
            let is_last = gen + 1 == generations;
            if gen % print_interval == 0 || is_last || improved {
                let elapsed_so_far = start_time.elapsed();
                let cost_est = (-gen_best) as u64;
                let marker = if improved { " *" } else { "" };
                // Use \r to overwrite + pad with spaces to clear previous content
                eprint!(
                    "\r  gen {}/{}  cost: {}  ({:.1}s){}          ",
                    gen_start + gen + 1,
                    gen_start + generations,
                    cost_est,
                    elapsed_so_far.as_secs_f64(),
                    marker,
                );
                use std::io::Write;
                let _ = std::io::stderr().flush();
            }

            pop.evolve(gen_start.wrapping_add(gen));
        }
        eprintln!(); // newline after progress line

        let elapsed = start_time.elapsed();
        let best = pop.best_weights();
        let best_model = NeuralModel::from_weight_vec(best);

        // Evaluate best model via speculative lowering on the real IR
        let spec = create_speculative_lowering(
            &options.target_config.name,
            Some(best_model),
            gen_start + generations,
            String::new(),
            OptimizerStatus::Improving,
        );
        let _ = spec.lower(&ir);
        let report = spec.report();

        // Use the best evolutionary cost as the score (more accurate than report total)
        let best_evo_cost = if best_seen > i64::MIN {
            (-best_seen) as u64
        } else {
            baseline_cost
        };
        let score_after =
            if report.total_neural_cost > 0 && report.total_neural_cost < best_evo_cost {
                report.total_neural_cost
            } else {
                best_evo_cost
            };

        // Save weights
        let weight_hash = weights::hash_weights(best);
        if let Err(e) = weights::save_weights(best, &weights_path) {
            eprintln!("warning: could not save weights: {}", e);
        }

        let mut tracker = weights::ConvergenceTracker::new();
        // Feed in the score
        let status = tracker.record(score_after);

        let new_meta = OptimizerMeta {
            generation: gen_start + generations,
            weight_hash: weight_hash.clone(),
            best_score: score_after,
            prev_score: score_before,
            baseline_score: baseline_cost,
            status: status.clone(),
        };
        if let Err(e) = weights::save_meta(&new_meta, &meta_path) {
            eprintln!("warning: could not save meta: {}", e);
        }

        // Display training progress
        eprintln!(
            "\n{}",
            OptimizerReport::format_training(
                gen_start,
                gen_start + generations,
                elapsed.as_micros() as u64,
                score_before,
                score_after,
                &status,
            )
        );
        eprintln!("  weights: {} -> {}", weights_path.display(), weight_hash);
        return;
    }

    // Analysis mode (--neural without --train): run speculative lowering and show report
    let spec = create_speculative_lowering(
        &options.target_config.name,
        Some(model),
        meta.generation,
        meta.weight_hash.clone(),
        meta.status.clone(),
    );
    let _ = spec.lower(&ir);
    let report = spec.report();
    eprintln!("\n{}", report.format_report());
}

/// Build TIR from a source entry point (for neural analysis).
fn build_tir(
    entry: &std::path::Path,
    options: &trident::CompileOptions,
) -> Option<Vec<trident::tir::TIROp>> {
    let source = std::fs::read_to_string(entry).ok()?;
    let filename = entry.to_string_lossy().to_string();
    trident::build_tir(&source, &filename, options).ok()
}

fn print_hints(cost: &trident::cost::ProgramCost) {
    let all: Vec<_> = cost
        .optimization_hints()
        .into_iter()
        .chain(cost.boundary_warnings())
        .collect();
    if all.is_empty() {
        eprintln!("\nNo optimization hints.");
        return;
    }
    eprintln!("\nOptimization hints:");
    for hint in &all {
        eprintln!("  {}", hint.message);
        for note in &hint.notes {
            eprintln!("    note: {}", note);
        }
        if let Some(help) = &hint.help {
            eprintln!("    help: {}", help);
        }
    }
}
