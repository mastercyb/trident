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
