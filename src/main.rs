use clap::{Parser, Subcommand};
use std::path::PathBuf;
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
    },
    /// Type-check without emitting TASM
    Check {
        /// Input .tri file
        input: PathBuf,
        /// Print cost analysis report
        #[arg(long)]
        costs: bool,
    },
    /// Format a .tri file
    Fmt {
        /// Input .tri file
        input: PathBuf,
        /// Check formatting without modifying (exit 1 if unformatted)
        #[arg(long)]
        check: bool,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Build {
            input,
            output,
            costs,
            hotspots,
        } => {
            // Check if input is a directory or has a trident.toml nearby
            let (tasm, default_output) = if input.is_dir() {
                // Project mode: look for trident.toml in directory
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
                let tasm = match trident::compile_project(&project.entry) {
                    Ok(t) => t,
                    Err(_) => process::exit(1),
                };
                let out = input.join(format!("{}.tasm", project.name));
                (tasm, out)
            } else if input.extension().is_some_and(|e| e == "tri") {
                // Check for trident.toml in parent directories
                if let Some(toml_path) = trident::project::Project::find(
                    input.parent().unwrap_or(std::path::Path::new(".")),
                ) {
                    let project = match trident::project::Project::load(&toml_path) {
                        Ok(p) => p,
                        Err(e) => {
                            eprintln!("error: {}", e.message);
                            process::exit(1);
                        }
                    };
                    let tasm = match trident::compile_project(&project.entry) {
                        Ok(t) => t,
                        Err(_) => process::exit(1),
                    };
                    let out = project.root_dir.join(format!("{}.tasm", project.name));
                    (tasm, out)
                } else {
                    // Single-file mode (also resolves std.* imports)
                    let tasm = match trident::compile_project(&input) {
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

            // Cost analysis (single-file only for now)
            if costs || hotspots {
                if let Some(source_path) = find_program_source(&input) {
                    let source = std::fs::read_to_string(&source_path).unwrap_or_default();
                    let filename = source_path.to_string_lossy().to_string();
                    if let Ok(program_cost) = trident::analyze_costs(&source, &filename) {
                        eprintln!("\n{}", program_cost.format_report());
                        if hotspots {
                            eprintln!("{}", program_cost.format_hotspots(5));
                        }
                    }
                }
            }
        }
        Command::Fmt { input, check } => {
            let source = match std::fs::read_to_string(&input) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("error: cannot read '{}': {}", input.display(), e);
                    process::exit(1);
                }
            };
            let filename = input.to_string_lossy().to_string();
            match trident::format_source(&source, &filename) {
                Ok(formatted) => {
                    if check {
                        if formatted != source {
                            eprintln!("would reformat: {}", input.display());
                            process::exit(1);
                        }
                        eprintln!("OK: {}", input.display());
                    } else {
                        if formatted != source {
                            if let Err(e) = std::fs::write(&input, &formatted) {
                                eprintln!("error: cannot write '{}': {}", input.display(), e);
                                process::exit(1);
                            }
                            eprintln!("Formatted: {}", input.display());
                        } else {
                            eprintln!("Already formatted: {}", input.display());
                        }
                    }
                }
                Err(_) => {
                    eprintln!("error: cannot format '{}' (parse errors)", input.display());
                    process::exit(1);
                }
            }
        }
        Command::Check { input, costs } => {
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
                if let Some(toml_path) = trident::project::Project::find(
                    input.parent().unwrap_or(std::path::Path::new(".")),
                ) {
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
    }
}

/// Find the program source file for cost analysis.
fn find_program_source(input: &std::path::Path) -> Option<PathBuf> {
    if input.is_file() && input.extension().is_some_and(|e| e == "tri") {
        return Some(input.to_path_buf());
    }
    if input.is_dir() {
        // Look for main.tri in the directory
        let main_tri = input.join("main.tri");
        if main_tri.exists() {
            return Some(main_tri);
        }
    }
    None
}
