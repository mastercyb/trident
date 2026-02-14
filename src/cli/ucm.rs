use std::path::PathBuf;
use std::process;

use clap::Subcommand;

use super::{collect_tri_files, open_codebase};

#[derive(Subcommand)]
pub enum UcmAction {
    /// Add a file to the codebase
    Add {
        /// Input .tri file or directory
        input: PathBuf,
    },
    /// List all named definitions
    List,
    /// View a definition by name or hash prefix
    View {
        /// Name or hash prefix
        name: String,
    },
    /// Rename a definition
    Rename {
        /// Current name
        from: String,
        /// New name
        to: String,
    },
    /// Show codebase statistics
    Stats,
    /// Show history of a name
    History {
        /// Name to show history for
        name: String,
    },
    /// Show dependencies of a definition
    Deps {
        /// Name or hash prefix
        name: String,
    },
}

pub fn cmd_ucm(action: UcmAction) {
    match action {
        UcmAction::Add { input } => cmd_ucm_add(input),
        UcmAction::List => cmd_ucm_list(),
        UcmAction::View { name } => cmd_ucm_view(name),
        UcmAction::Rename { from, to } => cmd_ucm_rename(from, to),
        UcmAction::Stats => cmd_ucm_stats(),
        UcmAction::History { name } => cmd_ucm_history(name),
        UcmAction::Deps { name } => cmd_ucm_deps(name),
    }
}

fn cmd_ucm_add(input: PathBuf) {
    let mut cb = open_codebase();

    let files = if input.is_dir() {
        collect_tri_files(&input)
    } else if input.extension().is_some_and(|e| e == "tri") {
        vec![input.clone()]
    } else {
        eprintln!("error: input must be a .tri file or directory");
        process::exit(1);
    };

    if files.is_empty() {
        eprintln!("No .tri files found in '{}'", input.display());
        return;
    }

    let mut total_added = 0usize;
    let mut total_updated = 0usize;
    let mut total_unchanged = 0usize;

    for file_path in &files {
        let source = match std::fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: cannot read '{}': {}", file_path.display(), e);
                continue;
            }
        };
        let filename = file_path.to_string_lossy().to_string();
        let file = match trident::parse_source_silent(&source, &filename) {
            Ok(f) => f,
            Err(_) => {
                eprintln!("error: parse errors in '{}'", file_path.display());
                continue;
            }
        };
        let result = cb.add_file(&file);
        total_added += result.added;
        total_updated += result.updated;
        total_unchanged += result.unchanged;
        eprintln!(
            "  {} +{} ~{} ={} {}",
            if result.added > 0 || result.updated > 0 {
                "OK"
            } else {
                "  "
            },
            result.added,
            result.updated,
            result.unchanged,
            file_path.display()
        );
    }

    if let Err(e) = cb.save() {
        eprintln!("error: cannot save codebase: {}", e);
        process::exit(1);
    }

    eprintln!(
        "\nCodebase: {} added, {} updated, {} unchanged",
        total_added, total_updated, total_unchanged
    );
}

fn cmd_ucm_list() {
    let cb = open_codebase();
    let names = cb.list_names();
    if names.is_empty() {
        eprintln!("Codebase is empty. Use `trident ucm add <file>` to add definitions.");
        return;
    }
    for (name, hash) in &names {
        println!("  {}  {}", hash, name);
    }
    eprintln!("\n{} definitions", names.len());
}

fn cmd_ucm_view(name: String) {
    let cb = open_codebase();
    if let Some(view) = cb.view(&name) {
        print!("{}", view);
    } else if let Some((hash, def)) = cb.lookup_by_prefix(&name) {
        let names = cb.names_for_hash(hash);
        let display_name = names.first().copied().unwrap_or("<unnamed>");
        println!("-- {} {}", display_name, hash);
        println!("{}", def.source);
    } else {
        eprintln!("error: '{}' not found in codebase", name);
        process::exit(1);
    }
}

fn cmd_ucm_rename(from: String, to: String) {
    let mut cb = open_codebase();
    if let Err(e) = cb.rename(&from, &to) {
        eprintln!("error: {}", e);
        process::exit(1);
    }
    if let Err(e) = cb.save() {
        eprintln!("error: cannot save codebase: {}", e);
        process::exit(1);
    }
    eprintln!("Renamed '{}' -> '{}'", from, to);
}

fn cmd_ucm_stats() {
    let cb = open_codebase();
    let stats = cb.stats();
    eprintln!("Codebase statistics:");
    eprintln!("  Definitions: {}", stats.definitions);
    eprintln!("  Names:       {}", stats.names);
    eprintln!("  Source size:  {} bytes", stats.total_source_bytes);
}

fn cmd_ucm_history(name: String) {
    let cb = open_codebase();
    let history = cb.name_history(&name);
    if history.is_empty() {
        eprintln!("No history for '{}'", name);
        return;
    }
    eprintln!("History of '{}':", name);
    for (hash, timestamp) in &history {
        println!("  {} at {}", hash, timestamp);
    }
}

fn cmd_ucm_deps(name: String) {
    let cb = open_codebase();

    let hash = if let Some(_def) = cb.lookup(&name) {
        match cb.list_names().iter().find(|(n, _)| *n == name.as_str()) {
            Some((_, h)) => **h,
            None => {
                eprintln!("error: '{}' not found", name);
                process::exit(1);
            }
        }
    } else if let Some((h, _)) = cb.lookup_by_prefix(&name) {
        *h
    } else {
        eprintln!("error: '{}' not found in codebase", name);
        process::exit(1);
    };

    let deps = cb.dependencies(&hash);
    if deps.is_empty() {
        eprintln!("'{}' has no dependencies", name);
    } else {
        eprintln!("Dependencies of '{}':", name);
        for (dep_name, dep_hash) in &deps {
            println!("  {}  {}", dep_hash, dep_name);
        }
    }

    let dependents = cb.dependents(&hash);
    if !dependents.is_empty() {
        eprintln!("\nUsed by:");
        for (dep_name, dep_hash) in &dependents {
            println!("  {}  {}", dep_hash, dep_name);
        }
    }
}
