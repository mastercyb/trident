use std::path::PathBuf;
use std::process;

use clap::Args;

use super::{resolve_input, resolve_options};

#[derive(Args)]
pub struct TestArgs {
    /// Input .tri file or directory with trident.toml
    pub input: PathBuf,
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

pub fn cmd_test(args: TestArgs) {
    let TestArgs {
        input,
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

    let options = resolve_options(&target, &profile, ri.project.as_ref());
    let result = trident::run_tests(&ri.entry, &options);

    match result {
        Ok(report) => {
            eprintln!("{}", report);
        }
        Err(_) => {
            process::exit(1);
        }
    }
}
