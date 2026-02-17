use std::path::PathBuf;

use clap::Args;

#[derive(Args)]
pub struct VerifyProofArgs {
    /// Path to the proof file
    pub proof: PathBuf,
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
}

pub fn cmd_verify_proof(args: VerifyProofArgs) {
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
            args.proof.display().to_string(),
            "--target".to_string(),
            target.clone(),
        ];
        if let Some(ref state_name) = state_for_warrior {
            extra.push("--state".to_string());
            extra.push(state_name.clone());
        }
        let refs: Vec<&str> = extra.iter().map(|s| s.as_str()).collect();
        super::delegate_to_warrior(&warrior_bin, "verify", &refs);
        return;
    }

    eprintln!("No verification warrior found for target '{}'.", target);
    eprintln!("Warriors handle proof verification using target-specific verifiers.");
    eprintln!();
    eprintln!("Install a warrior for this target:");
    eprintln!("  cargo install trisha   # Triton VM + Neptune");
}
