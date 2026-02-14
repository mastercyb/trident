use std::path::PathBuf;
use std::process;

use clap::Subcommand;

#[derive(Subcommand)]
pub enum DepsAction {
    /// Show declared dependencies and lock status
    List,
    /// Resolve and fetch all dependencies
    Fetch {
        /// Registry URL (default: http://127.0.0.1:8090)
        #[arg(long, default_value = "http://127.0.0.1:8090")]
        registry: String,
    },
    /// Verify all locked dependencies are cached and valid
    Check,
}

pub fn cmd_deps(action: DepsAction) {
    // Find project root
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let toml_path = match trident::project::Project::find(&cwd) {
        Some(p) => p,
        None => {
            eprintln!("error: no trident.toml found (run from project root)");
            process::exit(1);
        }
    };
    let project = match trident::project::Project::load(&toml_path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {}", e.message);
            process::exit(1);
        }
    };

    match action {
        DepsAction::List => {
            let deps = &project.dependencies.dependencies;
            if deps.is_empty() {
                println!("No dependencies declared in trident.toml.");
                return;
            }
            println!("Dependencies ({}):", deps.len());
            let mut names: Vec<_> = deps.keys().collect();
            names.sort();
            for name in names {
                let dep = &deps[name];
                match dep {
                    trident::manifest::Dependency::Hash { hash } => {
                        println!("  {} = {} (hash)", name, &hash[..16]);
                    }
                    trident::manifest::Dependency::Registry {
                        name: reg_name,
                        registry,
                    } => {
                        println!("  {} = {} @ {} (registry)", name, reg_name, registry);
                    }
                    trident::manifest::Dependency::Path { path } => {
                        println!("  {} = {} (path)", name, path.display());
                    }
                }
            }
            // Check lockfile
            let lock_path = project.root_dir.join("trident.lock");
            if lock_path.exists() {
                match trident::manifest::load_lockfile(&lock_path) {
                    Ok(lock) => println!("\nLocked: {} dependencies", lock.locked.len()),
                    Err(e) => println!("\nLockfile error: {}", e),
                }
            } else {
                println!("\nNo lockfile. Run `trident deps fetch` to resolve.");
            }
        }
        DepsAction::Fetch { registry } => {
            let deps = &project.dependencies;
            if deps.dependencies.is_empty() {
                println!("No dependencies to fetch.");
                return;
            }
            // Load existing lockfile if present
            let lock_path = project.root_dir.join("trident.lock");
            let existing_lock = if lock_path.exists() {
                trident::manifest::load_lockfile(&lock_path).ok()
            } else {
                None
            };
            match trident::manifest::resolve_dependencies(
                &project.root_dir,
                deps,
                &existing_lock,
                &registry,
            ) {
                Ok(lockfile) => {
                    if let Err(e) = trident::manifest::save_lockfile(&lock_path, &lockfile) {
                        eprintln!("error writing lockfile: {}", e);
                        process::exit(1);
                    }
                    println!(
                        "Resolved {} dependencies. Lockfile written to trident.lock.",
                        lockfile.locked.len()
                    );
                }
                Err(e) => {
                    eprintln!("error resolving dependencies: {}", e);
                    process::exit(1);
                }
            }
        }
        DepsAction::Check => {
            let lock_path = project.root_dir.join("trident.lock");
            if !lock_path.exists() {
                eprintln!("error: no trident.lock found. Run `trident deps fetch` first.");
                process::exit(1);
            }
            let lockfile = match trident::manifest::load_lockfile(&lock_path) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("error: {}", e);
                    process::exit(1);
                }
            };
            let mut ok = true;
            for (name, locked) in &lockfile.locked {
                let cached = trident::manifest::dep_source_path(&project.root_dir, &locked.hash);
                if cached.exists() {
                    println!("  OK  {} ({})", name, &locked.hash[..16]);
                } else {
                    println!("  MISSING  {} ({})", name, &locked.hash[..16]);
                    ok = false;
                }
            }
            if ok {
                println!("\nAll dependencies cached.");
            } else {
                println!("\nSome dependencies missing. Run `trident deps fetch`.");
                process::exit(1);
            }
        }
    }
}
