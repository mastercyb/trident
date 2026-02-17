use std::path::{Path, PathBuf};
use std::process;

use clap::Args;

use super::{open_codebase, prepare_artifact, registry_client, try_load_and_parse};

#[derive(Args)]
pub struct DeployArgs {
    /// Input .tri file, project directory, or .deploy/ artifact
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
    /// Compilation profile for cfg flags (default: release)
    #[arg(long, default_value = "release")]
    pub profile: String,
    /// Registry URL to deploy to
    #[arg(long)]
    pub registry: Option<String>,
    /// Run formal audit before deploying
    #[arg(long)]
    pub audit: bool,
    /// Show what would be deployed without actually deploying
    #[arg(long)]
    pub dry_run: bool,
}

pub fn cmd_deploy(args: DeployArgs) {
    let DeployArgs {
        input,
        target,
        engine,
        terrain,
        network,
        union_flag,
        vimputer,
        state,
        profile,
        registry,
        audit,
        dry_run,
    } = args;
    let bf = super::resolve_battlefield(
        &target,
        &engine,
        &terrain,
        &network,
        &union_flag,
        &vimputer,
        &state,
    );
    let target = bf.target;
    let state_selection = bf.state;

    // Handle pre-packaged .deploy/ artifact directory
    if input.is_dir() && input.join("manifest.json").exists() && input.join("program.tasm").exists()
    {
        let manifest_json = match std::fs::read_to_string(input.join("manifest.json")) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: cannot read manifest.json: {}", e);
                process::exit(1);
            }
        };

        if dry_run {
            eprintln!("Dry run — would deploy artifact:");
            eprintln!("  Artifact:  {}", input.display());
            for line in manifest_json.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("\"name\"") || trimmed.starts_with("\"program_digest\"") {
                    eprintln!("  {}", trimmed.trim_end_matches(','));
                }
            }
            return;
        }

        let client = registry_client(registry);
        deploy_to_registry(&input, &client);
        return;
    }

    // Build from source
    let art = prepare_artifact(&input, &target, &profile, audit);
    let output_base = art.entry.parent().unwrap_or(Path::new(".")).to_path_buf();

    // Resolve state config if specified
    let state_config = if let Some(ref state_name) = state_selection {
        if let Some(ref os) = art.resolved.os {
            match trident::target::StateConfig::resolve(&os.name, state_name) {
                Ok(Some(sc)) => Some(sc),
                Ok(None) => {
                    eprintln!(
                        "error: unknown state '{}' for union '{}'",
                        state_name, os.name
                    );
                    let available = trident::target::StateConfig::list_states(&os.name);
                    if !available.is_empty() {
                        eprintln!("  available: {}", available.join(", "));
                    }
                    process::exit(1);
                }
                Err(e) => {
                    eprintln!("error: {}", e.message);
                    process::exit(1);
                }
            }
        } else {
            eprintln!(
                "error: --state requires a union target, not bare terrain '{}'",
                target
            );
            process::exit(1);
        }
    } else {
        None
    };

    let target_display = if let Some(ref os) = art.resolved.os {
        if let Some(ref sc) = state_config {
            format!("{} {} ({})", os.name, sc.display_name, art.resolved.vm.name)
        } else {
            format!("{} ({})", os.name, art.resolved.vm.name)
        }
    } else {
        art.resolved.vm.name.clone()
    };

    if dry_run {
        let program_digest =
            trident::hash::ContentHash(trident::poseidon2::hash_bytes(art.tasm.as_bytes()));
        eprintln!("Dry run — would deploy:");
        eprintln!("  Name:            {}", art.name);
        eprintln!("  Version:         {}", art.version);
        eprintln!("  Target:          {}", target_display);
        if let Some(ref sc) = state_config {
            eprintln!("  State:           {} (chain_id: {})", sc.name, sc.chain_id);
            if !sc.rpc_url.is_empty() {
                eprintln!("  RPC:             {}", sc.rpc_url);
            }
        }
        eprintln!("  Program digest:  {}", program_digest.to_hex());
        eprintln!("  Padded height:   {}", art.cost.padded_height);
        return;
    }

    let result = match trident::deploy::generate_artifact(
        &art.name,
        &art.version,
        &art.tasm,
        &art.file,
        &art.cost,
        &art.resolved.vm,
        art.resolved.os.as_ref(),
        &output_base,
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {}", e);
            process::exit(1);
        }
    };

    eprintln!("Packaged -> {}", result.artifact_dir.display());
    eprintln!("  digest: {}", result.manifest.program_digest);

    let client = registry_client(registry);
    deploy_to_registry(&result.artifact_dir, &client);
}

/// Deploy a validated artifact directory (must contain manifest.json + program.tasm).
fn deploy_to_registry(artifact_dir: &Path, client: &trident::registry::RegistryClient) {
    eprintln!("Deploying...");

    // Try to find and add source to codebase
    let source_path = artifact_dir.parent().and_then(|parent| {
        let stem = artifact_dir
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .trim_end_matches(".deploy");
        let tri_file = parent.join(format!("{}.tri", stem));
        tri_file.exists().then_some(tri_file)
    });

    let mut cb = open_codebase();
    if let Some(source_file) = source_path {
        if let Some((_, file)) = try_load_and_parse(&source_file) {
            cb.add_file(&file);
            if let Err(e) = cb.save() {
                eprintln!("error: cannot save codebase: {}", e);
                process::exit(1);
            }
        }
    }

    match trident::registry::publish_codebase(&cb, client, &[]) {
        Ok(results) => {
            let created = results.iter().filter(|r| r.created).count();
            eprintln!("Deployed: {} definitions ({} new)", results.len(), created);
        }
        Err(e) => {
            eprintln!("error: deploy failed: {}", e);
            process::exit(1);
        }
    }
}
