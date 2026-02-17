use std::path::PathBuf;
use std::process;

use clap::Args;

use super::resolve_input;

#[derive(Args)]
pub struct RunArgs {
    /// Input .tri file or directory with trident.toml
    pub input: PathBuf,
    /// Target VM or OS (default: triton)
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
    /// Vimputer (geeky for state/chain instance)
    #[arg(long, conflicts_with = "state")]
    pub vimputer: Option<String>,
    /// State (gamy for vimputer/chain instance)
    #[arg(long, conflicts_with = "vimputer")]
    pub state: Option<String>,
    /// Compilation profile (debug or release)
    #[arg(long, default_value = "debug")]
    pub profile: String,
    /// Public input values (comma-separated field elements)
    #[arg(long, value_delimiter = ',')]
    pub input_values: Option<Vec<u64>>,
    /// Secret/divine input values (comma-separated field elements)
    #[arg(long, value_delimiter = ',')]
    pub secret: Option<Vec<u64>>,
}

pub fn cmd_run(args: RunArgs) {
    let ri = resolve_input(&args.input);
    let bf = super::resolve_battlefield(
        &args.target,
        &args.engine,
        &args.terrain,
        &args.network,
        &args.union_flag,
        &args.vimputer,
        &args.state,
    );
    let target = bf.target;
    let state_for_warrior = bf.state;

    if let Some(warrior_bin) = super::find_warrior(&target) {
        let mut extra: Vec<String> = vec![
            args.input.display().to_string(),
            "--target".to_string(),
            target.clone(),
            "--profile".to_string(),
            args.profile.clone(),
        ];
        if let Some(ref vals) = args.input_values {
            extra.push("--input-values".to_string());
            let s: Vec<String> = vals.iter().map(|v| v.to_string()).collect();
            extra.push(s.join(","));
        }
        if let Some(ref vals) = args.secret {
            extra.push("--secret".to_string());
            let s: Vec<String> = vals.iter().map(|v| v.to_string()).collect();
            extra.push(s.join(","));
        }
        if let Some(ref state_name) = state_for_warrior {
            extra.push("--state".to_string());
            extra.push(state_name.clone());
        }
        let refs: Vec<&str> = extra.iter().map(|s| s.as_str()).collect();
        super::delegate_to_warrior(&warrior_bin, "run", &refs);
        return;
    }

    let options = super::resolve_options(&target, &args.profile, ri.project.as_ref());
    match trident::compile_to_bundle(&ri.entry, &options) {
        Ok(bundle) => {
            let op_count = bundle.assembly.lines().count();
            eprintln!("Compiled {} ({} ops)", bundle.name, op_count);
            eprintln!();
            eprintln!("No runtime warrior found for target '{}'.", target);
            eprintln!("Warriors handle execution, proving, and deployment.");
            eprintln!();
            eprintln!("Install a warrior for this target:");
            eprintln!("  cargo install trident-trisha   # Triton VM + Neptune");
            eprintln!();
            eprintln!("Or use 'trident build' to produce TASM output directly.");
        }
        Err(_) => {
            eprintln!("error: compilation failed");
            process::exit(1);
        }
    }
}
