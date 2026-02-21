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
    /// Train the neural optimizer for N epochs (implies --neural)
    #[arg(long, value_name = "EPOCHS")]
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
    train_epochs: Option<u64>,
) {
    use trident::ir::tir::neural::report::{OptimizerReport, OptimizerStatus};
    use trident::neural::model::composite::NeuralCompilerConfig;
    use trident::neural::model::vocab::Vocab;
    use trident::neural::training::supervised;

    // Build TIR for neural analysis
    let ir = match build_tir(entry, options) {
        Some(ir) => ir,
        None => {
            eprintln!("warning: could not build TIR for neural analysis");
            return;
        }
    };

    // Compute classical baseline
    let lowering = trident::ir::tir::lower::create_stack_lowering(&options.target_config.name);
    let baseline_tasm = lowering.lower(&ir);
    let baseline_profile = trident::cost::scorer::profile_tasm_str(&baseline_tasm.join("\n"));
    let baseline_cost = baseline_profile.cost();

    let vocab = Vocab::new();

    // Training mode (--train N): run N epochs of supervised training
    if let Some(epochs) = train_epochs {
        use burn::backend::Autodiff;
        use burn::backend::NdArray;

        type TrainBackend = Autodiff<NdArray>;
        let device = Default::default();

        let config = NeuralCompilerConfig::new();
        let model = config.init::<TrainBackend>(&device);

        let blocks = vec![(
            ir.clone(),
            baseline_tasm.clone(),
            entry.to_string_lossy().to_string(),
            baseline_cost,
        )];
        let pairs = trident::neural::data::pairs::extract_pairs(&blocks, &vocab);
        if pairs.is_empty() {
            eprintln!("No training pairs extracted.");
            return;
        }

        let sup_config = supervised::SupervisedConfig::default();
        let mut optimizer = supervised::create_optimizer::<TrainBackend>(&sup_config);
        let lr = sup_config.lr;

        let start = std::time::Instant::now();
        let mut model = model;
        let mut best_loss = f32::INFINITY;

        eprintln!(
            "Training v2 neural optimizer: {} pairs, {} epochs, ~{}M params",
            pairs.len(),
            epochs,
            config.param_estimate() / 1_000_000,
        );

        for epoch in 0..epochs {
            let (updated, result) =
                supervised::train_epoch(model, &pairs, &mut optimizer, lr, &device);
            model = updated;
            let improved = result.avg_loss < best_loss;
            if improved {
                best_loss = result.avg_loss;
            }
            let marker = if improved { " *" } else { "" };
            eprintln!(
                "  epoch {}/{} | loss: {:.4}{}",
                epoch + 1,
                epochs,
                result.avg_loss,
                marker,
            );
        }

        let elapsed = start.elapsed();
        eprintln!(
            "\n{}",
            OptimizerReport::format_training(
                0,
                epochs,
                elapsed.as_micros() as u64,
                baseline_cost,
                baseline_cost,
                &OptimizerStatus::Improving
            ),
        );
        return;
    }

    // Analysis mode (--neural without --train): per-function beam search with trained checkpoint.
    // Load model once, then compile each function independently.
    use burn::backend::wgpu::{Wgpu, WgpuDevice};
    use trident::neural::data::pairs::split_tir_by_function;

    let wgpu_device = WgpuDevice::default();
    let model = match trident::neural::load_model::<Wgpu>(&wgpu_device) {
        Some(m) => m,
        None => {
            eprintln!("\nNeural v2: no trained checkpoint found. Run `trident train` first.");
            return;
        }
    };

    let functions = split_tir_by_function(&ir);
    let lowering_fn = trident::ir::tir::lower::create_stack_lowering(&options.target_config.name);

    let mut neural_wins = 0usize;
    let mut neural_fallbacks = 0usize;
    let mut total_neural_cost = 0u64;
    let mut total_compiler_cost = 0u64;

    eprintln!("\nNeural v2 per-function compilation:");
    eprintln!(
        "  {:<50} {:>10} {:>10} {:>8}",
        "Function", "Compiler", "Neural", "Result"
    );
    eprintln!("  {}", "-".repeat(82));

    for (fn_name, fn_tir) in &functions {
        if fn_name.starts_with("__") || fn_tir.is_empty() {
            continue;
        }

        // Lower this function's TIR to compiler TASM baseline
        let fn_baseline = lowering_fn.lower(fn_tir);
        let fn_baseline: Vec<String> = fn_baseline
            .into_iter()
            .filter(|l| {
                let t = l.trim();
                !t.is_empty() && !t.ends_with(':') && !t.starts_with("//")
            })
            .map(|l| l.trim().to_string())
            .collect();

        if fn_baseline.is_empty() {
            continue;
        }

        let compiler_cost = trident::cost::scorer::profile_tasm(
            &fn_baseline.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        )
        .cost()
        .max(1);

        total_compiler_cost += compiler_cost;

        match trident::neural::compile_with_model(fn_tir, &fn_baseline, &model, &wgpu_device) {
            Ok(result) if result.neural && result.cost <= compiler_cost => {
                neural_wins += 1;
                total_neural_cost += result.cost;
                let ratio = result.cost as f64 / compiler_cost as f64;
                eprintln!(
                    "  {:<50} {:>10} {:>10} {:>8}",
                    fn_name,
                    compiler_cost,
                    format!("{} ({:.2}x)", result.cost, ratio),
                    "neural",
                );
            }
            Ok(result) if result.neural => {
                neural_fallbacks += 1;
                total_neural_cost += compiler_cost;
                eprintln!(
                    "  {:<50} {:>10} {:>10} {:>8}",
                    fn_name, compiler_cost, result.cost, "compiler",
                );
            }
            _ => {
                neural_fallbacks += 1;
                total_neural_cost += compiler_cost;
                eprintln!(
                    "  {:<50} {:>10} {:>10} {:>8}",
                    fn_name, compiler_cost, "-", "fallback",
                );
            }
        }
    }

    eprintln!("  {}", "-".repeat(82));
    let total_fns = neural_wins + neural_fallbacks;
    eprintln!(
        "  Total: {}/{} functions neural, compiler cost: {}, neural cost: {}",
        neural_wins, total_fns, total_compiler_cost, total_neural_cost,
    );
    if total_compiler_cost > 0 {
        let ratio = total_neural_cost as f64 / total_compiler_cost as f64;
        eprintln!("  Overall ratio: {:.4}x", ratio);
    }
}

/// Build TIR from a source entry point (for neural analysis).
/// Uses full project resolution so imports (use vm.*, std.*) work.
fn build_tir(
    entry: &std::path::Path,
    options: &trident::CompileOptions,
) -> Option<Vec<trident::tir::TIROp>> {
    trident::build_tir_project(entry, options).ok()
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
