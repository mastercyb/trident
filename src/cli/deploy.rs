use std::path::{Path, PathBuf};
use std::process;

use super::prepare_artifact;

pub fn cmd_deploy(
    input: PathBuf,
    target: &str,
    profile: &str,
    registry: Option<String>,
    verify: bool,
    dry_run: bool,
) {
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
        let url = registry.unwrap_or_else(trident::registry::RegistryClient::default_url);

        if dry_run {
            eprintln!("Dry run — would deploy artifact:");
            eprintln!("  Artifact:  {}", input.display());
            eprintln!("  Registry:  {}", url);
            for line in manifest_json.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("\"name\"") {
                    eprintln!("  {}", trimmed.trim_end_matches(','));
                }
                if trimmed.starts_with("\"program_digest\"") {
                    eprintln!("  {}", trimmed.trim_end_matches(','));
                }
            }
            return;
        }

        eprintln!("Deploying artifact {} to {}...", input.display(), url);
        deploy_to_registry(&input, &url);
        return;
    }

    // Build from source
    let art = prepare_artifact(&input, target, profile, verify);

    let output_base = art.entry.parent().unwrap_or(Path::new(".")).to_path_buf();

    let target_display = if let Some(ref os) = art.resolved.os {
        format!("{} ({})", os.name, art.resolved.vm.name)
    } else {
        art.resolved.vm.name.clone()
    };

    let url = registry.unwrap_or_else(trident::registry::RegistryClient::default_url);

    if dry_run {
        let program_digest =
            trident::hash::ContentHash(trident::poseidon2::hash_bytes(art.tasm.as_bytes()));
        eprintln!("Dry run — would deploy:");
        eprintln!("  Name:            {}", art.name);
        eprintln!("  Version:         {}", art.version);
        eprintln!("  Target:          {}", target_display);
        eprintln!("  Program digest:  {}", program_digest.to_hex());
        eprintln!("  Padded height:   {}", art.cost.padded_height);
        eprintln!("  Registry:        {}", url);
        return;
    }

    // Generate artifact
    let result = match trident::artifact::generate_artifact(
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

    // Deploy to registry
    deploy_to_registry(&result.artifact_dir, &url);
}

/// Deploy a packaged artifact directory to a registry server.
fn deploy_to_registry(artifact_dir: &Path, url: &str) {
    eprintln!("Deploying to {}...", url);
    let client = trident::registry::RegistryClient::new(url);
    match client.health() {
        Ok(true) => {}
        Ok(false) => {
            eprintln!("error: registry at {} is not healthy", url);
            process::exit(1);
        }
        Err(e) => {
            eprintln!("error: cannot reach registry at {}: {}", url, e);
            process::exit(1);
        }
    }

    let manifest_path = artifact_dir.join("manifest.json");
    let tasm_path = artifact_dir.join("program.tasm");

    if !manifest_path.exists() || !tasm_path.exists() {
        eprintln!(
            "error: artifact directory '{}' missing manifest.json or program.tasm",
            artifact_dir.display()
        );
        process::exit(1);
    }

    let tasm = match std::fs::read_to_string(&tasm_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read program.tasm: {}", e);
            process::exit(1);
        }
    };

    let source_path = artifact_dir.parent().and_then(|parent| {
        let stem = artifact_dir
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .trim_end_matches(".deploy");
        let tri_file = parent.join(format!("{}.tri", stem));
        if tri_file.exists() {
            Some(tri_file)
        } else {
            None
        }
    });

    if let Some(source_file) = source_path {
        let source = match std::fs::read_to_string(&source_file) {
            Ok(s) => s,
            Err(_) => {
                eprintln!("warning: cannot read source file, publishing artifact only");
                publish_artifact_only(&client, &tasm);
                return;
            }
        };
        let filename = source_file.to_string_lossy().to_string();
        match trident::parse_source_silent(&source, &filename) {
            Ok(file) => {
                let mut cb = match trident::ucm::Codebase::open() {
                    Ok(cb) => cb,
                    Err(e) => {
                        eprintln!("error: cannot open codebase: {}", e);
                        process::exit(1);
                    }
                };
                cb.add_file(&file);
                if let Err(e) = cb.save() {
                    eprintln!("error: cannot save codebase: {}", e);
                    process::exit(1);
                }
                match trident::registry::publish_codebase(&cb, &client, &[]) {
                    Ok(results) => {
                        let created = results.iter().filter(|r| r.created).count();
                        eprintln!(
                            "Deployed: {} definitions ({} new) to {}",
                            results.len(),
                            created,
                            url
                        );
                    }
                    Err(e) => {
                        eprintln!("error: deploy failed: {}", e);
                        process::exit(1);
                    }
                }
            }
            Err(_) => {
                eprintln!("warning: cannot parse source, publishing artifact only");
                publish_artifact_only(&client, &tasm);
            }
        }
    } else {
        publish_artifact_only(&client, &tasm);
    }
}

/// Publish just the compiled TASM when source is unavailable.
fn publish_artifact_only(client: &trident::registry::RegistryClient, tasm: &str) {
    let hash = trident::hash::ContentHash(trident::poseidon2::hash_bytes(tasm.as_bytes()));
    eprintln!("Publishing artifact (digest: {})...", hash.to_hex());
    let cb = match trident::ucm::Codebase::open() {
        Ok(cb) => cb,
        Err(e) => {
            eprintln!("error: cannot open codebase: {}", e);
            process::exit(1);
        }
    };
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
