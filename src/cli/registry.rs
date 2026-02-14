use std::path::PathBuf;
use std::process;

use clap::Subcommand;

use super::{open_codebase, registry_client, registry_url, resolve_tri_files, try_load_and_parse};

#[derive(Subcommand)]
pub enum RegistryAction {
    /// Publish local store definitions to a registry
    Publish {
        /// Registry URL (default: $TRIDENT_REGISTRY_URL or http://127.0.0.1:8090)
        #[arg(long)]
        registry: Option<String>,
        /// Tags to attach to published definitions
        #[arg(long)]
        tag: Vec<String>,
        /// Input .tri file or directory (adds to store first, then publishes)
        #[arg(short, long)]
        input: Option<PathBuf>,
    },
    /// Pull a definition from a registry into local store
    Pull {
        /// Name or content hash to pull
        name: String,
        /// Registry URL
        #[arg(long)]
        registry: Option<String>,
    },
    /// Search a registry for definitions
    Search {
        /// Search query (name, module, or type signature)
        query: String,
        /// Registry URL
        #[arg(long)]
        registry: Option<String>,
        /// Search by type signature instead of name
        #[arg(long)]
        r#type: bool,
        /// Search by tag
        #[arg(long)]
        tag: bool,
        /// Only show verified definitions
        #[arg(long)]
        verified: bool,
    },
}

pub fn cmd_registry(action: RegistryAction) {
    match action {
        RegistryAction::Publish {
            registry,
            tag,
            input,
        } => cmd_registry_publish(registry, tag, input),
        RegistryAction::Pull { name, registry } => cmd_registry_pull(name, registry),
        RegistryAction::Search {
            query,
            registry,
            r#type,
            tag,
            verified: _,
        } => cmd_registry_search(query, registry, r#type, tag),
    }
}

fn cmd_registry_publish(registry: Option<String>, tags: Vec<String>, input: Option<PathBuf>) {
    let client = registry_client(registry);
    let mut cb = open_codebase();

    if let Some(ref input_path) = input {
        let files = resolve_tri_files(input_path);
        for file_path in &files {
            if let Some((_, file)) = try_load_and_parse(file_path) {
                cb.add_file(&file);
            }
        }
        if let Err(e) = cb.save() {
            eprintln!("error: cannot save codebase: {}", e);
        }
    }

    eprintln!("Publishing...");
    match trident::registry::publish_codebase(&cb, &client, &tags) {
        Ok(results) => {
            let created = results.iter().filter(|r| r.created).count();
            let existing = results.len() - created;
            let named = results.iter().filter(|r| r.name_bound).count();
            eprintln!(
                "Published: {} new, {} existing, {} names bound",
                created, existing, named
            );
        }
        Err(e) => {
            eprintln!("error: publish failed: {}", e);
            process::exit(1);
        }
    }
}

fn cmd_registry_pull(name: String, registry: Option<String>) {
    let url = registry_url(registry);
    let client = trident::registry::RegistryClient::new(&url);
    let mut cb = open_codebase();

    eprintln!("Pulling '{}' from {}...", name, url);
    match trident::registry::pull_into_codebase(&mut cb, &client, &name) {
        Ok(result) => {
            eprintln!("Pulled: {} ({})", name, &result.hash[..16]);
            eprintln!("  Module: {}", result.module);
            if !result.params.is_empty() {
                let params: Vec<String> = result
                    .params
                    .iter()
                    .map(|(n, t)| format!("{}: {}", n, t))
                    .collect();
                eprintln!("  Params: {}", params.join(", "));
            }
            if let Some(ref ret) = result.return_ty {
                eprintln!("  Returns: {}", ret);
            }
            if !result.dependencies.is_empty() {
                eprintln!("  Dependencies: {}", result.dependencies.len());
            }
        }
        Err(e) => {
            eprintln!("error: {}", e);
            process::exit(1);
        }
    }
}

fn cmd_registry_search(query: String, registry: Option<String>, by_type: bool, by_tag: bool) {
    let url = registry_url(registry);
    let client = trident::registry::RegistryClient::new(&url);

    let results = if by_type {
        client.search_by_type(&query)
    } else if by_tag {
        client.search_by_tag(&query)
    } else {
        client.search(&query)
    };

    match results {
        Ok(results) => {
            if results.is_empty() {
                eprintln!("No results for '{}'", query);
                return;
            }
            for r in &results {
                let verified = if r.verified { " [verified]" } else { "" };
                let tags = if r.tags.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", r.tags.join(", "))
                };
                println!(
                    "  {}  {}  {}{}{}",
                    &r.hash[..16],
                    r.name,
                    r.signature,
                    verified,
                    tags
                );
            }
            eprintln!("\n{} results", results.len());
        }
        Err(e) => {
            eprintln!("error: {}", e);
            process::exit(1);
        }
    }
}
