use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::process;

#[derive(Parser)]
#[command(
    name = "trident",
    version,
    about = "Trident compiler — Correct. Bounded. Provable."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Initialize a new Trident project
    Init {
        /// Project name (defaults to current directory name)
        name: Option<String>,
    },
    /// Compile a .tri file (or project) to TASM
    Build {
        /// Input .tri file or directory with trident.toml
        input: PathBuf,
        /// Output .tasm file (default: <input>.tasm)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Print cost analysis report
        #[arg(long)]
        costs: bool,
        /// Show top cost contributors (implies --costs)
        #[arg(long)]
        hotspots: bool,
        /// Show optimization hints (H0001-H0004)
        #[arg(long)]
        hints: bool,
        /// Compilation target (debug or release)
        #[arg(long, default_value = "debug")]
        target: String,
    },
    /// Type-check without emitting TASM
    Check {
        /// Input .tri file or directory with trident.toml
        input: PathBuf,
        /// Print cost analysis report
        #[arg(long)]
        costs: bool,
        /// Compilation target (debug or release)
        #[arg(long, default_value = "debug")]
        target: String,
    },
    /// Format .tri source files
    Fmt {
        /// Input .tri file or directory (defaults to current directory)
        input: Option<PathBuf>,
        /// Check formatting without modifying (exit 1 if unformatted)
        #[arg(long)]
        check: bool,
    },
    /// Start the Language Server Protocol server
    Lsp,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Init { name } => cmd_init(name),
        Command::Build {
            input,
            output,
            costs,
            hotspots,
            hints,
            target,
        } => cmd_build(input, output, costs, hotspots, hints, &target),
        Command::Check {
            input,
            costs,
            target,
        } => cmd_check(input, costs, &target),
        Command::Fmt { input, check } => cmd_fmt(input, check),
        Command::Lsp => cmd_lsp(),
    }
}

// --- trident init ---

fn cmd_init(name: Option<String>) {
    let (project_dir, project_name) = if let Some(ref name) = name {
        let dir = PathBuf::from(name);
        (dir, name.clone())
    } else {
        let dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let name = dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("my_project")
            .to_string();
        (dir, name)
    };

    // Create directory if name was provided
    if name.is_some() {
        if let Err(e) = std::fs::create_dir_all(&project_dir) {
            eprintln!(
                "error: cannot create directory '{}': {}",
                project_dir.display(),
                e
            );
            process::exit(1);
        }
    }

    let toml_path = project_dir.join("trident.toml");
    if toml_path.exists() {
        eprintln!("error: '{}' already exists", toml_path.display());
        process::exit(1);
    }

    let toml_content = format!(
        "[project]\nname = \"{}\"\nversion = \"0.1.0\"\nentry = \"main.tri\"\n",
        project_name
    );

    let main_content = format!(
        "program {}\n\nfn main() {{\n    let x: Field = pub_read()\n    pub_write(x)\n}}\n",
        project_name
    );

    if let Err(e) = std::fs::write(&toml_path, &toml_content) {
        eprintln!("error: cannot write '{}': {}", toml_path.display(), e);
        process::exit(1);
    }

    let main_path = project_dir.join("main.tri");
    if let Err(e) = std::fs::write(&main_path, &main_content) {
        eprintln!("error: cannot write '{}': {}", main_path.display(), e);
        process::exit(1);
    }

    eprintln!(
        "Created project '{}' in {}",
        project_name,
        project_dir.display()
    );
    eprintln!("  {}", toml_path.display());
    eprintln!("  {}", main_path.display());
}

// --- trident build ---

/// Resolve a target name to CompileOptions, using project targets if available.
fn resolve_target(
    target: &str,
    project: Option<&trident::project::Project>,
) -> trident::CompileOptions {
    if let Some(proj) = project {
        if let Some(flags) = proj.targets.get(target) {
            return trident::CompileOptions {
                target: target.to_string(),
                cfg_flags: flags.iter().cloned().collect(),
            };
        }
    }
    // Built-in targets: the target name is itself the single cfg flag
    trident::CompileOptions::for_target(target)
}

fn cmd_build(
    input: PathBuf,
    output: Option<PathBuf>,
    costs: bool,
    hotspots: bool,
    hints: bool,
    target: &str,
) {
    let (tasm, default_output) = if input.is_dir() {
        let toml_path = input.join("trident.toml");
        if !toml_path.exists() {
            eprintln!("error: no trident.toml found in '{}'", input.display());
            process::exit(1);
        }
        let project = match trident::project::Project::load(&toml_path) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("error: {}", e.message);
                process::exit(1);
            }
        };
        let options = resolve_target(target, Some(&project));
        let tasm = match trident::compile_project_with_options(&project.entry, &options) {
            Ok(t) => t,
            Err(_) => process::exit(1),
        };
        let out = input.join(format!("{}.tasm", project.name));
        (tasm, out)
    } else if input.extension().is_some_and(|e| e == "tri") {
        if let Some(toml_path) =
            trident::project::Project::find(input.parent().unwrap_or(Path::new(".")))
        {
            let project = match trident::project::Project::load(&toml_path) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("error: {}", e.message);
                    process::exit(1);
                }
            };
            let options = resolve_target(target, Some(&project));
            let tasm = match trident::compile_project_with_options(&project.entry, &options) {
                Ok(t) => t,
                Err(_) => process::exit(1),
            };
            let out = project.root_dir.join(format!("{}.tasm", project.name));
            (tasm, out)
        } else {
            let options = resolve_target(target, None);
            let tasm = match trident::compile_project_with_options(&input, &options) {
                Ok(t) => t,
                Err(_) => process::exit(1),
            };
            let out = input.with_extension("tasm");
            (tasm, out)
        }
    } else {
        eprintln!("error: input must be a .tri file or project directory");
        process::exit(1);
    };

    let out_path = output.unwrap_or(default_output);
    if let Err(e) = std::fs::write(&out_path, &tasm) {
        eprintln!("error: cannot write '{}': {}", out_path.display(), e);
        process::exit(1);
    }
    eprintln!("Compiled -> {}", out_path.display());

    // Cost analysis, hotspots, and optimization hints
    if costs || hotspots || hints {
        if let Some(source_path) = find_program_source(&input) {
            let source = std::fs::read_to_string(&source_path).unwrap_or_default();
            let filename = source_path.to_string_lossy().to_string();
            if let Ok(program_cost) = trident::analyze_costs(&source, &filename) {
                if costs || hotspots {
                    eprintln!("\n{}", program_cost.format_report());
                    if hotspots {
                        eprintln!("{}", program_cost.format_hotspots(5));
                    }
                }
                if hints {
                    let opt_hints = program_cost.optimization_hints();
                    let boundary = program_cost.boundary_warnings();
                    let all_hints: Vec<_> = opt_hints.into_iter().chain(boundary).collect();
                    if all_hints.is_empty() {
                        eprintln!("\nNo optimization hints.");
                    } else {
                        eprintln!("\nOptimization hints:");
                        for hint in &all_hints {
                            eprintln!("  {}", hint.message);
                            for note in &hint.notes {
                                eprintln!("    note: {}", note);
                            }
                            if let Some(help) = &hint.help {
                                eprintln!("    help: {}", help);
                            }
                        }
                    }
                }
            }
        }
    }
}

// --- trident check ---

fn cmd_check(input: PathBuf, costs: bool, _target: &str) {
    let entry = if input.is_dir() {
        let toml_path = input.join("trident.toml");
        if !toml_path.exists() {
            eprintln!("error: no trident.toml found in '{}'", input.display());
            process::exit(1);
        }
        let project = match trident::project::Project::load(&toml_path) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("error: {}", e.message);
                process::exit(1);
            }
        };
        project.entry
    } else if input.extension().is_some_and(|e| e == "tri") {
        if let Some(toml_path) =
            trident::project::Project::find(input.parent().unwrap_or(Path::new(".")))
        {
            let project = match trident::project::Project::load(&toml_path) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("error: {}", e.message);
                    process::exit(1);
                }
            };
            project.entry
        } else {
            input.clone()
        }
    } else {
        eprintln!("error: input must be a .tri file or project directory");
        process::exit(1);
    };

    match trident::check_project(&entry) {
        Ok(()) => {
            eprintln!("OK: {}", input.display());
        }
        Err(_) => {
            process::exit(1);
        }
    }

    if costs {
        if let Some(source_path) = find_program_source(&input) {
            let source = std::fs::read_to_string(&source_path).unwrap_or_default();
            let filename = source_path.to_string_lossy().to_string();
            if let Ok(program_cost) = trident::analyze_costs(&source, &filename) {
                eprintln!("\n{}", program_cost.format_report());
            }
        }
    }
}

// --- trident fmt ---

fn cmd_fmt(input: Option<PathBuf>, check: bool) {
    let input = input.unwrap_or_else(|| PathBuf::from("."));

    if input.is_dir() {
        let files = collect_tri_files(&input);
        if files.is_empty() {
            eprintln!("No .tri files found in '{}'", input.display());
            return;
        }

        let mut any_unformatted = false;
        for file in &files {
            match format_single_file(file, check) {
                Ok(changed) => {
                    if changed {
                        any_unformatted = true;
                    }
                }
                Err(msg) => {
                    eprintln!("error: {}", msg);
                }
            }
        }

        if check && any_unformatted {
            process::exit(1);
        }
    } else if input.extension().is_some_and(|e| e == "tri") {
        match format_single_file(&input, check) {
            Ok(changed) => {
                if check && changed {
                    process::exit(1);
                }
            }
            Err(msg) => {
                eprintln!("error: {}", msg);
                process::exit(1);
            }
        }
    } else {
        eprintln!("error: input must be a .tri file or directory");
        process::exit(1);
    }
}

/// Format a single .tri file. Returns Ok(true) if the file was changed/would be changed.
fn format_single_file(path: &Path, check: bool) -> Result<bool, String> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read '{}': {}", path.display(), e))?;
    let filename = path.to_string_lossy().to_string();
    let formatted = trident::format_source(&source, &filename)
        .map_err(|_| format!("cannot format '{}' (parse errors)", path.display()))?;

    if formatted == source {
        if check {
            eprintln!("OK: {}", path.display());
        } else {
            eprintln!("Already formatted: {}", path.display());
        }
        return Ok(false);
    }

    if check {
        eprintln!("would reformat: {}", path.display());
        return Ok(true);
    }

    std::fs::write(path, &formatted)
        .map_err(|e| format!("cannot write '{}': {}", path.display(), e))?;
    eprintln!("Formatted: {}", path.display());
    Ok(true)
}

/// Recursively collect all .tri files in a directory, skipping hidden dirs and target/.
fn collect_tri_files(dir: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    collect_tri_files_recursive(dir, &mut result);
    result.sort();
    result
}

fn collect_tri_files_recursive(dir: &Path, result: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip hidden directories and target/
        if name_str.starts_with('.') || name_str == "target" {
            continue;
        }

        if path.is_dir() {
            collect_tri_files_recursive(&path, result);
        } else if path.extension().is_some_and(|e| e == "tri") {
            result.push(path);
        }
    }
}

// --- trident lsp ---

fn cmd_lsp() {
    let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
    rt.block_on(trident::lsp::run_server());
}

// --- Helpers ---

/// Find the program source file for cost analysis.
fn find_program_source(input: &Path) -> Option<PathBuf> {
    if input.is_file() && input.extension().is_some_and(|e| e == "tri") {
        return Some(input.to_path_buf());
    }
    if input.is_dir() {
        let main_tri = input.join("main.tri");
        if main_tri.exists() {
            return Some(main_tri);
        }
    }
    None
}
