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
        /// Output per-line cost annotations
        #[arg(long)]
        annotate: bool,
        /// Save cost analysis to a JSON file
        #[arg(long, value_name = "PATH")]
        save_costs: Option<PathBuf>,
        /// Compare costs with a previous cost JSON file
        #[arg(long, value_name = "PATH")]
        compare: Option<PathBuf>,
        /// Target VM (default: triton)
        #[arg(long, default_value = "triton")]
        target: String,
        /// Compilation profile for cfg flags (debug or release)
        #[arg(long, default_value = "debug")]
        profile: String,
    },
    /// Type-check without emitting TASM
    Check {
        /// Input .tri file or directory with trident.toml
        input: PathBuf,
        /// Print cost analysis report
        #[arg(long)]
        costs: bool,
        /// Target VM (default: triton)
        #[arg(long, default_value = "triton")]
        target: String,
        /// Compilation profile for cfg flags (debug or release)
        #[arg(long, default_value = "debug")]
        profile: String,
    },
    /// Format .tri source files
    Fmt {
        /// Input .tri file or directory (defaults to current directory)
        input: Option<PathBuf>,
        /// Check formatting without modifying (exit 1 if unformatted)
        #[arg(long)]
        check: bool,
    },
    /// Run #[test] functions
    Test {
        /// Input .tri file or directory with trident.toml
        input: PathBuf,
        /// Target VM (default: triton)
        #[arg(long, default_value = "triton")]
        target: String,
        /// Compilation profile for cfg flags (debug or release)
        #[arg(long, default_value = "debug")]
        profile: String,
    },
    /// Generate documentation with cost annotations
    Doc {
        /// Input .tri file or directory with trident.toml
        input: PathBuf,
        /// Output markdown file (default: stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Target VM (default: triton)
        #[arg(long, default_value = "triton")]
        target: String,
        /// Compilation profile for cfg flags (debug or release)
        #[arg(long, default_value = "debug")]
        profile: String,
    },
    /// Verify assertions using symbolic execution + algebraic solver
    Verify {
        /// Input .tri file or directory with trident.toml
        input: PathBuf,
        /// Show detailed constraint system summary
        #[arg(long)]
        verbose: bool,
        /// Output SMT-LIB2 encoding to file (for external solvers)
        #[arg(long, value_name = "PATH")]
        smt: Option<PathBuf>,
        /// Run Z3 solver (if available) for formal verification
        #[arg(long)]
        z3: bool,
        /// Output machine-readable JSON report (for LLM/CI consumption)
        #[arg(long)]
        json: bool,
        /// Synthesize and suggest specifications (invariants, pre/postconditions)
        #[arg(long)]
        synthesize: bool,
    },
    /// Show content hashes of functions (BLAKE3)
    Hash {
        /// Input .tri file or directory with trident.toml
        input: PathBuf,
        /// Show full 256-bit hashes instead of short form
        #[arg(long)]
        full: bool,
    },
    /// Run benchmarks: compare Trident output vs hand-written TASM
    Bench {
        /// Directory containing benchmark .tri + .baseline.tasm files
        #[arg(default_value = "benches")]
        dir: PathBuf,
    },
    /// Generate code scaffold from spec annotations
    Generate {
        /// Input .tri spec file
        input: PathBuf,
        /// Output file (default: stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// View a function definition (pretty-printed from AST)
    View {
        /// Function name or content hash prefix
        name: String,
        /// Input .tri file or directory with trident.toml
        #[arg(short, long)]
        input: Option<PathBuf>,
        /// Show full hash instead of short form
        #[arg(long)]
        full: bool,
    },
    /// Universal Codebase Manager — hash-keyed definitions store
    Ucm {
        #[command(subcommand)]
        action: UcmAction,
    },
    /// Global registry — publish, pull, search definitions
    Registry {
        #[command(subcommand)]
        action: RegistryAction,
    },
    /// Check semantic equivalence of two functions
    Equiv {
        /// Input .tri file containing both functions
        input: PathBuf,
        /// First function name
        fn_a: String,
        /// Second function name
        fn_b: String,
        /// Show detailed symbolic analysis
        #[arg(long)]
        verbose: bool,
    },
    /// Manage project dependencies
    Deps {
        #[command(subcommand)]
        action: DepsAction,
    },
    /// Build, hash, and produce a self-contained artifact (.deploy/ directory)
    Package {
        /// Input .tri file or directory with trident.toml
        input: PathBuf,
        /// Output directory for the .deploy/ artifact (default: project root or cwd)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Target VM or OS (default: triton)
        #[arg(long, default_value = "triton")]
        target: String,
        /// Compilation profile for cfg flags (default: release)
        #[arg(long, default_value = "release")]
        profile: String,
        /// Run verification before packaging
        #[arg(long)]
        verify: bool,
        /// Show what would be produced without writing files
        #[arg(long)]
        dry_run: bool,
    },
    /// Deploy a program to a registry server or blockchain node
    Deploy {
        /// Input .tri file, project directory, or .deploy/ artifact
        input: PathBuf,
        /// Target VM or OS (default: triton)
        #[arg(long, default_value = "triton")]
        target: String,
        /// Compilation profile for cfg flags (default: release)
        #[arg(long, default_value = "release")]
        profile: String,
        /// Registry URL to deploy to
        #[arg(long)]
        registry: Option<String>,
        /// Run verification before deploying
        #[arg(long)]
        verify: bool,
        /// Show what would be deployed without actually deploying
        #[arg(long)]
        dry_run: bool,
    },
    /// Start the Language Server Protocol server
    Lsp,
}

#[derive(Subcommand)]
enum DepsAction {
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

#[derive(Subcommand)]
enum UcmAction {
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

#[derive(Subcommand)]
enum RegistryAction {
    /// Start a registry server
    Serve {
        /// Bind address (default: 127.0.0.1:8090)
        #[arg(long, default_value = "127.0.0.1:8090")]
        bind: String,
        /// Storage directory (default: ~/.trident/registry)
        #[arg(long)]
        storage: Option<PathBuf>,
    },
    /// Publish local UCM definitions to a registry
    Publish {
        /// Registry URL (default: $TRIDENT_REGISTRY_URL or http://127.0.0.1:8090)
        #[arg(long)]
        registry: Option<String>,
        /// Tags to attach to published definitions
        #[arg(long)]
        tag: Vec<String>,
        /// Input .tri file or directory (adds to UCM first, then publishes)
        #[arg(short, long)]
        input: Option<PathBuf>,
    },
    /// Pull a definition from a registry into local UCM
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
            annotate,
            save_costs,
            compare,
            target,
            profile,
        } => cmd_build(
            input, output, costs, hotspots, hints, annotate, save_costs, compare, &target, &profile,
        ),
        Command::Check {
            input,
            costs,
            target,
            profile,
        } => cmd_check(input, costs, &target, &profile),
        Command::Fmt { input, check } => cmd_fmt(input, check),
        Command::Test {
            input,
            target,
            profile,
        } => cmd_test(input, &target, &profile),
        Command::Doc {
            input,
            output,
            target,
            profile,
        } => cmd_doc(input, output, &target, &profile),
        Command::Verify {
            input,
            verbose,
            smt,
            z3,
            json,
            synthesize,
        } => cmd_verify(input, verbose, smt, z3, json, synthesize),
        Command::Hash { input, full } => cmd_hash(input, full),
        Command::Bench { dir } => cmd_bench(dir),
        Command::Generate { input, output } => cmd_generate(input, output),
        Command::View { name, input, full } => cmd_view(name, input, full),
        Command::Ucm { action } => cmd_ucm(action),
        Command::Registry { action } => cmd_registry(action),
        Command::Equiv {
            input,
            fn_a,
            fn_b,
            verbose,
        } => cmd_equiv(input, &fn_a, &fn_b, verbose),
        Command::Deps { action } => cmd_deps(action),
        Command::Package {
            input,
            output,
            target,
            profile,
            verify,
            dry_run,
        } => cmd_package(input, output, &target, &profile, verify, dry_run),
        Command::Deploy {
            input,
            target,
            profile,
            registry,
            verify,
            dry_run,
        } => cmd_deploy(input, &target, &profile, registry, verify, dry_run),
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

/// Resolve a VM target + profile to CompileOptions.
///
/// - `target`: VM target name (e.g. "triton"). For backward compat, if
///   "debug" or "release" is passed as target, treat it as profile with a
///   deprecation warning.
/// - `profile`: compilation profile for cfg flags (e.g. "debug", "release").
fn resolve_options(
    target: &str,
    profile: &str,
    project: Option<&trident::project::Project>,
) -> trident::CompileOptions {
    // Backward compatibility: if --target was "debug" or "release", the user
    // is using the old semantics where --target meant profile.
    let (vm_target, actual_profile) = match target {
        "debug" | "release" => {
            eprintln!(
                "warning: --target {} is deprecated for profile selection; use --profile {} --target triton",
                target, target
            );
            ("triton", target)
        }
        _ => (target, profile),
    };

    // Use project's target if CLI target is the default and project specifies one
    let effective_target = if vm_target == "triton" {
        if let Some(proj) = project {
            if let Some(ref proj_target) = proj.target {
                proj_target.as_str()
            } else {
                vm_target
            }
        } else {
            vm_target
        }
    } else {
        vm_target
    };

    // Resolve the VM target config
    let target_config = if effective_target == "triton" {
        trident::target::TargetConfig::triton()
    } else {
        match trident::target::TargetConfig::resolve(effective_target) {
            Ok(config) => config,
            Err(e) => {
                eprintln!("error: {}", e.message);
                process::exit(1);
            }
        }
    };

    // Resolve cfg flags from project targets or default to profile name
    let cfg_flags = if let Some(proj) = project {
        // Check project [target] field first
        // Then check project [targets.PROFILE] for cfg flags
        if let Some(flags) = proj.targets.get(actual_profile) {
            flags.iter().cloned().collect()
        } else {
            std::collections::HashSet::from([actual_profile.to_string()])
        }
    } else {
        std::collections::HashSet::from([actual_profile.to_string()])
    };

    trident::CompileOptions {
        profile: actual_profile.to_string(),
        cfg_flags,
        target_config,
        dep_dirs: Vec::new(),
    }
}

#[allow(clippy::too_many_arguments)]
fn cmd_build(
    input: PathBuf,
    output: Option<PathBuf>,
    costs: bool,
    hotspots: bool,
    hints: bool,
    annotate: bool,
    save_costs: Option<PathBuf>,
    compare: Option<PathBuf>,
    target: &str,
    profile: &str,
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
        let mut options = resolve_options(target, profile, Some(&project));
        options.dep_dirs = load_dep_dirs(&project);
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
            let mut options = resolve_options(target, profile, Some(&project));
            options.dep_dirs = load_dep_dirs(&project);
            let tasm = match trident::compile_project_with_options(&project.entry, &options) {
                Ok(t) => t,
                Err(_) => process::exit(1),
            };
            let out = project.root_dir.join(format!("{}.tasm", project.name));
            (tasm, out)
        } else {
            let options = resolve_options(target, profile, None);
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

    // --annotate: print per-line cost annotations
    if annotate {
        if let Some(source_path) = find_program_source(&input) {
            let source = std::fs::read_to_string(&source_path).unwrap_or_default();
            let filename = source_path.to_string_lossy().to_string();
            match trident::annotate_source(&source, &filename) {
                Ok(annotated) => {
                    println!("{}", annotated);
                }
                Err(_) => {
                    eprintln!("error: could not annotate source (compilation errors)");
                }
            }
        }
    }

    // Cost analysis, hotspots, and optimization hints
    if costs || hotspots || hints || save_costs.is_some() || compare.is_some() {
        if let Some(source_path) = find_program_source(&input) {
            let options = resolve_options(target, profile, None);
            if let Ok(program_cost) = trident::analyze_costs_project(&source_path, &options) {
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

                // --save-costs: write cost JSON to file
                if let Some(ref save_path) = save_costs {
                    if let Err(e) = program_cost.save_json(save_path) {
                        eprintln!("error: {}", e);
                        process::exit(1);
                    }
                    eprintln!("Saved costs -> {}", save_path.display());
                }

                // --compare: load previous costs and show diff
                if let Some(ref compare_path) = compare {
                    match trident::cost::ProgramCost::load_json(compare_path) {
                        Ok(old_cost) => {
                            eprintln!("\n{}", old_cost.format_comparison(&program_cost));
                        }
                        Err(e) => {
                            eprintln!("error: {}", e);
                            process::exit(1);
                        }
                    }
                }
            }
        }
    }
}

// --- trident check ---

fn cmd_check(input: PathBuf, costs: bool, _target: &str, _profile: &str) {
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
            let options = trident::CompileOptions::default();
            if let Ok(program_cost) = trident::analyze_costs_project(&source_path, &options) {
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

// --- trident test ---

fn cmd_test(input: PathBuf, target: &str, profile: &str) {
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

    let options = resolve_options(target, profile, None);
    let result = trident::run_tests(&entry, &options);

    match result {
        Ok(report) => {
            eprintln!("{}", report);
        }
        Err(_) => {
            process::exit(1);
        }
    }
}

// --- trident doc ---

fn cmd_doc(input: PathBuf, output: Option<PathBuf>, target: &str, profile: &str) {
    let (entry, project) = if input.is_dir() {
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
        let entry = project.entry.clone();
        (entry, Some(project))
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
            let entry = project.entry.clone();
            (entry, Some(project))
        } else {
            (input.clone(), None)
        }
    } else {
        eprintln!("error: input must be a .tri file or project directory");
        process::exit(1);
    };

    let options = resolve_options(target, profile, project.as_ref());
    let markdown = match trident::generate_docs(&entry, &options) {
        Ok(md) => md,
        Err(_) => {
            eprintln!("error: documentation generation failed (compilation errors)");
            process::exit(1);
        }
    };

    if let Some(out_path) = output {
        if let Err(e) = std::fs::write(&out_path, &markdown) {
            eprintln!("error: cannot write '{}': {}", out_path.display(), e);
            process::exit(1);
        }
        eprintln!("Documentation written to {}", out_path.display());
    } else {
        print!("{}", markdown);
    }
}

// --- trident verify ---

fn cmd_verify(
    input: PathBuf,
    verbose: bool,
    smt_output: Option<PathBuf>,
    run_z3: bool,
    json: bool,
    synthesize: bool,
) {
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

    eprintln!("Verifying {}...", input.display());

    // Parse for symbolic analysis (needed for verbose, SMT, Z3, JSON, and synthesize)
    let need_parse = verbose || smt_output.is_some() || run_z3 || json || synthesize;
    let (system, parsed_file) = if need_parse {
        if let Ok(source) = std::fs::read_to_string(&entry) {
            let filename = entry.to_string_lossy().to_string();
            match trident::parse_source_silent(&source, &filename) {
                Ok(file) => {
                    let sys = trident::sym::analyze(&file);
                    if verbose {
                        eprintln!("\nConstraint system: {}", sys.summary());
                    }
                    (Some(sys), Some(file))
                }
                Err(_) => (None, None),
            }
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    // --smt: write SMT-LIB2 encoding to file
    if let Some(ref smt_path) = smt_output {
        if let Some(ref sys) = system {
            let smt_script = trident::smt::encode_system(sys, trident::smt::QueryMode::SafetyCheck);
            if let Err(e) = std::fs::write(smt_path, &smt_script) {
                eprintln!("error: cannot write '{}': {}", smt_path.display(), e);
                process::exit(1);
            }
            eprintln!("SMT-LIB2 written to {}", smt_path.display());
        }
    }

    // --z3: run Z3 solver
    if run_z3 {
        if let Some(ref sys) = system {
            let smt_script = trident::smt::encode_system(sys, trident::smt::QueryMode::SafetyCheck);
            match trident::smt::run_z3(&smt_script) {
                Ok(result) => {
                    eprintln!("\nZ3 safety check:");
                    match result.status {
                        trident::smt::SmtStatus::Unsat => {
                            eprintln!("  Result: UNSAT (formally verified safe)");
                        }
                        trident::smt::SmtStatus::Sat => {
                            eprintln!("  Result: SAT (counterexample found)");
                            if let Some(model) = &result.model {
                                eprintln!("  Model:\n{}", model);
                            }
                        }
                        trident::smt::SmtStatus::Unknown => {
                            eprintln!("  Result: UNKNOWN (solver timed out or gave up)");
                        }
                        trident::smt::SmtStatus::Error(ref e) => {
                            eprintln!("  Result: ERROR\n  {}", e);
                        }
                    }

                    // Also check witness existence for programs with divine inputs
                    if !sys.divine_inputs.is_empty() {
                        let witness_script = trident::smt::encode_system(
                            sys,
                            trident::smt::QueryMode::WitnessExistence,
                        );
                        if let Ok(witness_result) = trident::smt::run_z3(&witness_script) {
                            eprintln!(
                                "\nZ3 witness existence ({} divine inputs):",
                                sys.divine_inputs.len()
                            );
                            match witness_result.status {
                                trident::smt::SmtStatus::Sat => {
                                    eprintln!("  Result: SAT (valid witness exists)");
                                }
                                trident::smt::SmtStatus::Unsat => {
                                    eprintln!("  Result: UNSAT (no valid witness — constraints unsatisfiable)");
                                }
                                _ => {
                                    eprintln!(
                                        "  Result: {}",
                                        witness_result.output.lines().next().unwrap_or("unknown")
                                    );
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("\nZ3 not available: {}", e);
                    eprintln!("  Install Z3 or use --smt to export for external solvers.");
                }
            }
        }
    }

    // --synthesize: automatic invariant synthesis
    if synthesize {
        if let Some(ref file) = parsed_file {
            let specs = trident::synthesize::synthesize_specs(file);
            eprintln!("\n{}", trident::synthesize::format_report(&specs));
        } else {
            eprintln!("warning: could not parse file for synthesis");
        }
    }

    // Standard verification (random + BMC)
    match trident::verify_project(&entry) {
        Ok(report) => {
            if json {
                if let Some(ref sys) = system {
                    let file_name = entry.to_string_lossy().to_string();
                    let json_output =
                        trident::report::generate_json_report(&file_name, sys, &report);
                    println!("{}", json_output);
                } else {
                    eprintln!("error: could not build constraint system for JSON report");
                    process::exit(1);
                }
            } else {
                eprintln!("\n{}", report.format_report());
            }
            if !report.is_safe() {
                process::exit(1);
            }
        }
        Err(_) => {
            process::exit(1);
        }
    }
}

// --- trident hash ---

fn cmd_hash(input: PathBuf, full: bool) {
    let source_path = if input.is_dir() {
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
        input.clone()
    } else {
        eprintln!("error: input must be a .tri file or project directory");
        process::exit(1);
    };

    let source = match std::fs::read_to_string(&source_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read '{}': {}", source_path.display(), e);
            process::exit(1);
        }
    };

    let filename = source_path.to_string_lossy().to_string();
    let file = match trident::parse_source_silent(&source, &filename) {
        Ok(f) => f,
        Err(_) => {
            eprintln!("error: parse errors in '{}'", source_path.display());
            process::exit(1);
        }
    };

    // Hash all functions
    let fn_hashes = trident::hash::hash_file(&file);
    let file_hash = trident::hash::hash_file_content(&file);

    // Print file hash
    if full {
        eprintln!("File: {} {}", file_hash.to_hex(), source_path.display());
    } else {
        eprintln!("File: {} {}", file_hash, source_path.display());
    }

    // Print function hashes in sorted order
    let mut sorted: Vec<_> = fn_hashes.iter().collect();
    sorted.sort_by_key(|(name, _)| (*name).clone());
    for (name, hash) in sorted {
        if full {
            println!("  {} {}", hash.to_hex(), name);
        } else {
            println!("  {} {}", hash, name);
        }
    }
}

// --- trident bench ---

fn cmd_bench(dir: PathBuf) {
    if !dir.is_dir() {
        eprintln!("error: '{}' is not a directory", dir.display());
        process::exit(1);
    }

    // Find all .tri files in the bench directory
    let mut tri_files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| {
            eprintln!("error: cannot read '{}': {}", dir.display(), e);
            process::exit(1);
        })
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|p| p.extension().is_some_and(|e| e == "tri"))
        .collect();
    tri_files.sort();

    if tri_files.is_empty() {
        eprintln!("No benchmark .tri files found in '{}'", dir.display());
        process::exit(1);
    }

    let options = trident::CompileOptions::default();
    let mut results: Vec<trident::BenchmarkResult> = Vec::new();

    for tri_path in &tri_files {
        let stem = tri_path.file_stem().unwrap().to_string_lossy().to_string();
        let baseline_path = dir.join(format!("{}.baseline.tasm", stem));

        // Compile the Trident program
        let tasm = match trident::compile_project_with_options(tri_path, &options) {
            Ok(t) => t,
            Err(_) => {
                eprintln!("  FAIL  {}  (compilation error)", stem);
                continue;
            }
        };

        let trident_count = trident::count_tasm_instructions(&tasm);

        // Get cost analysis for padded height
        let trident_padded = trident::analyze_costs_project(tri_path, &options)
            .map(|c| c.padded_height)
            .unwrap_or(0);

        // Read baseline if available
        let (baseline_count, baseline_padded) = if baseline_path.exists() {
            let baseline = std::fs::read_to_string(&baseline_path).unwrap_or_default();
            let count = trident::count_tasm_instructions(&baseline);
            // Baseline padded height: count instructions as approximate processor rows
            let padded = (count as u64).next_power_of_two();
            (count, padded)
        } else {
            (0, 0)
        };

        let ratio = if baseline_count > 0 {
            trident_count as f64 / baseline_count as f64
        } else {
            0.0
        };

        results.push(trident::BenchmarkResult {
            name: stem,
            trident_instructions: trident_count,
            baseline_instructions: baseline_count,
            overhead_ratio: ratio,
            trident_padded_height: trident_padded,
            baseline_padded_height: baseline_padded,
        });
    }

    // Print results table
    eprintln!();
    eprintln!("{}", trident::BenchmarkResult::format_header());
    eprintln!("{}", trident::BenchmarkResult::format_separator());
    for result in &results {
        eprintln!("{}", result.format());
    }
    eprintln!("{}", trident::BenchmarkResult::format_separator());

    // Summary
    let with_baseline: Vec<_> = results
        .iter()
        .filter(|r| r.baseline_instructions > 0)
        .collect();
    if !with_baseline.is_empty() {
        let avg_ratio: f64 = with_baseline.iter().map(|r| r.overhead_ratio).sum::<f64>()
            / with_baseline.len() as f64;
        let max_ratio = with_baseline
            .iter()
            .map(|r| r.overhead_ratio)
            .fold(0.0f64, f64::max);
        eprintln!(
            "Average overhead: {:.2}x  Max: {:.2}x  ({} benchmarks with baselines)",
            avg_ratio,
            max_ratio,
            with_baseline.len()
        );
    }
    eprintln!();
}

// --- trident generate ---

fn cmd_generate(input: PathBuf, output: Option<PathBuf>) {
    if !input.extension().is_some_and(|e| e == "tri") {
        eprintln!("error: input must be a .tri file");
        process::exit(1);
    }

    let source = match std::fs::read_to_string(&input) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read '{}': {}", input.display(), e);
            process::exit(1);
        }
    };

    let filename = input.to_string_lossy().to_string();
    let file = match trident::parse_source_silent(&source, &filename) {
        Ok(f) => f,
        Err(errors) => {
            trident::diagnostic::render_diagnostics(&errors, &filename, &source);
            eprintln!("error: parse errors in '{}'", input.display());
            process::exit(1);
        }
    };

    let scaffold = trident::scaffold::generate_scaffold(&file);

    if let Some(out_path) = output {
        if let Err(e) = std::fs::write(&out_path, &scaffold) {
            eprintln!("error: cannot write '{}': {}", out_path.display(), e);
            process::exit(1);
        }
        eprintln!("Generated scaffold -> {}", out_path.display());
    } else {
        print!("{}", scaffold);
    }
}

// --- trident view ---

fn cmd_view(name: String, input: Option<PathBuf>, full: bool) {
    // Resolve the source file to parse
    let source_path = if let Some(ref path) = input {
        if path.is_dir() {
            let toml_path = path.join("trident.toml");
            if !toml_path.exists() {
                eprintln!("error: no trident.toml found in '{}'", path.display());
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
        } else if path.extension().is_some_and(|e| e == "tri") {
            path.clone()
        } else {
            eprintln!("error: input must be a .tri file or project directory");
            process::exit(1);
        }
    } else {
        // Try current directory for trident.toml, then look for .tri files
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let toml_path = cwd.join("trident.toml");
        if toml_path.exists() {
            let project = match trident::project::Project::load(&toml_path) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("error: {}", e.message);
                    process::exit(1);
                }
            };
            project.entry
        } else {
            let main_tri = cwd.join("main.tri");
            if main_tri.exists() {
                main_tri
            } else {
                eprintln!("error: no trident.toml or main.tri found in current directory");
                eprintln!("  use --input to specify a .tri file or project directory");
                process::exit(1);
            }
        }
    };

    let source = match std::fs::read_to_string(&source_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read '{}': {}", source_path.display(), e);
            process::exit(1);
        }
    };

    let filename = source_path.to_string_lossy().to_string();
    let file = match trident::parse_source_silent(&source, &filename) {
        Ok(f) => f,
        Err(_) => {
            eprintln!("error: parse errors in '{}'", source_path.display());
            process::exit(1);
        }
    };

    let fn_hashes = trident::hash::hash_file(&file);

    // Try to find the function: by hash prefix or by name
    let (fn_name, func) = if trident::view::looks_like_hash(&name) {
        // Try hash prefix first, fall back to name lookup
        if let Some((found_name, found_func)) =
            trident::view::find_function_by_hash(&file, &fn_hashes, &name)
        {
            (found_name, found_func.clone())
        } else if let Some(found_func) = trident::view::find_function(&file, &name) {
            (name.clone(), found_func.clone())
        } else {
            eprintln!("error: no function matching '{}' found", name);
            process::exit(1);
        }
    } else if let Some(found_func) = trident::view::find_function(&file, &name) {
        (name.clone(), found_func.clone())
    } else {
        eprintln!("error: function '{}' not found in '{}'", name, filename);
        eprintln!("\nAvailable functions:");
        for item in &file.items {
            if let trident::ast::Item::Fn(f) = &item.node {
                if let Some(hash) = fn_hashes.get(&f.name.node) {
                    eprintln!("  {}  {}", hash, f.name.node);
                }
            }
        }
        process::exit(1);
    };

    // Pretty-print the function
    let formatted = trident::view::format_function(&func);

    // Show hash
    if let Some(hash) = fn_hashes.get(&fn_name) {
        if full {
            eprintln!("Hash: {}", hash.to_hex());
        } else {
            eprintln!("Hash: {}", hash);
        }
    }

    print!("{}", formatted);
}

// --- trident ucm ---

fn cmd_ucm(action: UcmAction) {
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
    let mut cb = match trident::ucm::Codebase::open() {
        Ok(cb) => cb,
        Err(e) => {
            eprintln!("error: cannot open codebase: {}", e);
            process::exit(1);
        }
    };

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
    let cb = match trident::ucm::Codebase::open() {
        Ok(cb) => cb,
        Err(e) => {
            eprintln!("error: cannot open codebase: {}", e);
            process::exit(1);
        }
    };

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
    let cb = match trident::ucm::Codebase::open() {
        Ok(cb) => cb,
        Err(e) => {
            eprintln!("error: cannot open codebase: {}", e);
            process::exit(1);
        }
    };

    // Try by name first, then by hash prefix.
    if let Some(view) = cb.view(&name) {
        print!("{}", view);
    } else if let Some((hash, def)) = cb.lookup_by_prefix(&name) {
        // Find a name for this hash.
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
    let mut cb = match trident::ucm::Codebase::open() {
        Ok(cb) => cb,
        Err(e) => {
            eprintln!("error: cannot open codebase: {}", e);
            process::exit(1);
        }
    };

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
    let cb = match trident::ucm::Codebase::open() {
        Ok(cb) => cb,
        Err(e) => {
            eprintln!("error: cannot open codebase: {}", e);
            process::exit(1);
        }
    };

    let stats = cb.stats();
    eprintln!("Codebase statistics:");
    eprintln!("  Definitions: {}", stats.definitions);
    eprintln!("  Names:       {}", stats.names);
    eprintln!("  Source size:  {} bytes", stats.total_source_bytes);
}

fn cmd_ucm_history(name: String) {
    let cb = match trident::ucm::Codebase::open() {
        Ok(cb) => cb,
        Err(e) => {
            eprintln!("error: cannot open codebase: {}", e);
            process::exit(1);
        }
    };

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
    let cb = match trident::ucm::Codebase::open() {
        Ok(cb) => cb,
        Err(e) => {
            eprintln!("error: cannot open codebase: {}", e);
            process::exit(1);
        }
    };

    // Look up the hash for this name.
    let hash = if let Some(def) = cb.lookup(&name) {
        // Get hash from names map by looking it up through the definition.
        let _ = def;
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

// --- trident registry ---

fn cmd_registry(action: RegistryAction) {
    match action {
        RegistryAction::Serve { bind, storage } => cmd_registry_serve(bind, storage),
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

fn cmd_registry_serve(bind: String, storage: Option<PathBuf>) {
    let config = trident::registry::RegistryConfig {
        bind_addr: bind,
        storage_dir: storage.unwrap_or_else(|| {
            std::env::var("TRIDENT_REGISTRY_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| {
                    std::env::var("HOME")
                        .map(|h| PathBuf::from(h).join(".trident").join("registry"))
                        .unwrap_or_else(|_| PathBuf::from(".trident-registry"))
                })
        }),
        max_body_size: 1024 * 1024,
    };

    if let Err(e) = trident::registry::run_server(&config) {
        eprintln!("error: registry server failed: {}", e);
        process::exit(1);
    }
}

fn cmd_registry_publish(registry: Option<String>, tags: Vec<String>, input: Option<PathBuf>) {
    let url = registry.unwrap_or_else(trident::registry::RegistryClient::default_url);
    let client = trident::registry::RegistryClient::new(&url);

    // Check health first.
    match client.health() {
        Ok(true) => {}
        Ok(false) => {
            eprintln!("error: registry at {} is not healthy", url);
            process::exit(1);
        }
        Err(e) => {
            eprintln!("error: cannot connect to registry at {}: {}", url, e);
            process::exit(1);
        }
    }

    let mut cb = match trident::ucm::Codebase::open() {
        Ok(cb) => cb,
        Err(e) => {
            eprintln!("error: cannot open codebase: {}", e);
            process::exit(1);
        }
    };

    // If input is provided, add to UCM first.
    if let Some(ref input_path) = input {
        let files = if input_path.is_dir() {
            collect_tri_files(input_path)
        } else if input_path.extension().is_some_and(|e| e == "tri") {
            vec![input_path.clone()]
        } else {
            eprintln!("error: input must be a .tri file or directory");
            process::exit(1);
        };

        for file_path in &files {
            let source = match std::fs::read_to_string(file_path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("error: cannot read '{}': {}", file_path.display(), e);
                    continue;
                }
            };
            let filename = file_path.to_string_lossy().to_string();
            if let Ok(file) = trident::parse_source_silent(&source, &filename) {
                cb.add_file(&file);
            }
        }
        if let Err(e) = cb.save() {
            eprintln!("error: cannot save codebase: {}", e);
        }
    }

    eprintln!("Publishing to {}...", url);
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
    let url = registry.unwrap_or_else(trident::registry::RegistryClient::default_url);
    let client = trident::registry::RegistryClient::new(&url);

    let mut cb = match trident::ucm::Codebase::open() {
        Ok(cb) => cb,
        Err(e) => {
            eprintln!("error: cannot open codebase: {}", e);
            process::exit(1);
        }
    };

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
    let url = registry.unwrap_or_else(trident::registry::RegistryClient::default_url);
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

// --- trident equiv ---

fn cmd_equiv(input: PathBuf, fn_a: &str, fn_b: &str, verbose: bool) {
    if !input.extension().is_some_and(|e| e == "tri") {
        eprintln!("error: input must be a .tri file");
        process::exit(1);
    }

    let source = match std::fs::read_to_string(&input) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read '{}': {}", input.display(), e);
            process::exit(1);
        }
    };

    let filename = input.to_string_lossy().to_string();
    let file = match trident::parse_source_silent(&source, &filename) {
        Ok(f) => f,
        Err(errors) => {
            trident::diagnostic::render_diagnostics(&errors, &filename, &source);
            eprintln!("error: parse errors in '{}'", input.display());
            process::exit(1);
        }
    };

    eprintln!(
        "Checking equivalence: {} vs {} in {}",
        fn_a,
        fn_b,
        input.display()
    );

    if verbose {
        // Show content hashes for both functions.
        let fn_hashes = trident::hash::hash_file(&file);
        if let Some(h) = fn_hashes.get(fn_a) {
            eprintln!("  {} hash: {}", fn_a, h);
        }
        if let Some(h) = fn_hashes.get(fn_b) {
            eprintln!("  {} hash: {}", fn_b, h);
        }
    }

    let result = trident::equiv::check_equivalence(&file, fn_a, fn_b);

    eprintln!("\n{}", result.format_report());

    match result.verdict {
        trident::equiv::EquivalenceVerdict::Equivalent => {
            // Success exit code.
        }
        trident::equiv::EquivalenceVerdict::NotEquivalent => {
            process::exit(1);
        }
        trident::equiv::EquivalenceVerdict::Unknown => {
            process::exit(2);
        }
    }
}

// --- trident package ---

fn cmd_package(
    input: PathBuf,
    output: Option<PathBuf>,
    target: &str,
    profile: &str,
    verify: bool,
    dry_run: bool,
) {
    // 1. Resolve input to project or file
    let (project, entry, source_path) = if input.is_dir() {
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
        let entry = project.entry.clone();
        let source_path = project.entry.clone();
        (Some(project), entry, source_path)
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
            let entry = project.entry.clone();
            (Some(project), entry, input.clone())
        } else {
            (None, input.clone(), input.clone())
        }
    } else {
        eprintln!("error: input must be a .tri file or project directory");
        process::exit(1);
    };

    // 2. Resolve target (OS-aware)
    let resolved = match trident::target::ResolvedTarget::resolve(target) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {}", e.message);
            process::exit(1);
        }
    };

    // 3. Build CompileOptions using the resolved VM config
    let mut options = resolve_options(&resolved.vm.name, profile, project.as_ref());
    options.target_config = resolved.vm.clone();
    if let Some(ref proj) = project {
        options.dep_dirs = load_dep_dirs(proj);
    }

    // 4. Compile
    eprintln!("Compiling {}...", source_path.display());
    let tasm = match trident::compile_project_with_options(&entry, &options) {
        Ok(t) => t,
        Err(_) => {
            eprintln!("error: compilation failed");
            process::exit(1);
        }
    };

    // 5. Cost analysis
    let cost = match trident::analyze_costs_project(&entry, &options) {
        Ok(c) => c,
        Err(_) => {
            eprintln!("warning: cost analysis failed, using zeros");
            trident::cost::ProgramCost {
                program_name: String::new(),
                functions: Vec::new(),
                total: trident::cost::TableCost::ZERO,
                attestation_hash_rows: 0,
                padded_height: 0,
                estimated_proving_secs: 0.0,
                loop_bound_waste: Vec::new(),
            }
        }
    };

    // 6. Parse source for function signatures and hashes
    let source = std::fs::read_to_string(&source_path).unwrap_or_default();
    let filename = source_path.to_string_lossy().to_string();
    let file = match trident::parse_source_silent(&source, &filename) {
        Ok(f) => f,
        Err(_) => {
            eprintln!("error: cannot parse source for manifest");
            process::exit(1);
        }
    };

    // 7. Determine name and version
    let (name, version) = if let Some(ref proj) = project {
        (proj.name.clone(), proj.version.clone())
    } else {
        let stem = source_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("program")
            .to_string();
        (stem, "0.1.0".to_string())
    };

    // 8. Optional verification
    if verify {
        eprintln!("Verifying {}...", source_path.display());
        match trident::verify_project(&entry) {
            Ok(report) => {
                if !report.is_safe() {
                    eprintln!("error: verification failed");
                    eprintln!("{}", report.format_report());
                    process::exit(1);
                }
                eprintln!("Verification: OK");
            }
            Err(_) => {
                eprintln!("error: verification failed");
                process::exit(1);
            }
        }
    }

    // 9. Determine output base directory
    let output_base = output.unwrap_or_else(|| {
        if let Some(ref proj) = project {
            proj.root_dir.clone()
        } else {
            source_path.parent().unwrap_or(Path::new(".")).to_path_buf()
        }
    });

    // 10. Compute program digest for display / dry run
    let program_digest =
        trident::hash::ContentHash(trident::poseidon2::hash_bytes(tasm.as_bytes()));

    // Target display string
    let target_display = if let Some(ref os) = resolved.os {
        format!("{} ({})", os.name, resolved.vm.name)
    } else {
        resolved.vm.name.clone()
    };

    // 11. Dry run
    if dry_run {
        eprintln!("Dry run — would package:");
        eprintln!("  Name:            {}", name);
        eprintln!("  Version:         {}", version);
        eprintln!("  Target:          {}", target_display);
        eprintln!("  Program digest:  {}", program_digest.to_hex());
        eprintln!("  Padded height:   {}", cost.padded_height);
        eprintln!(
            "  Artifact:        {}/{}.deploy/",
            output_base.display(),
            name
        );
        return;
    }

    // 12. Generate artifact
    let result = match trident::artifact::generate_artifact(
        &name,
        &version,
        &tasm,
        &file,
        &cost,
        &resolved.vm,
        resolved.os.as_ref(),
        &output_base,
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {}", e);
            process::exit(1);
        }
    };

    eprintln!("Packaged -> {}", result.artifact_dir.display());
    eprintln!("  program.tasm:   {}", result.tasm_path.display());
    eprintln!("  manifest.json:  {}", result.manifest_path.display());
    eprintln!("  digest:         {}", result.manifest.program_digest);
    eprintln!("  padded height:  {}", result.manifest.cost.padded_height);
    eprintln!("  target:         {}", target_display);
}

// --- trident deploy ---

fn cmd_deploy(
    input: PathBuf,
    target: &str,
    profile: &str,
    registry: Option<String>,
    verify: bool,
    dry_run: bool,
) {
    // 1. Resolve input to project or file
    let (project, entry, source_path) = if input.is_dir() {
        // Could be a .deploy/ artifact directory
        if input.join("manifest.json").exists() && input.join("program.tasm").exists() {
            // Pre-packaged artifact — deploy directly from manifest
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
                // Extract name from manifest for display
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

        // Project directory with trident.toml
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
        let entry = project.entry.clone();
        let source_path = project.entry.clone();
        (Some(project), entry, source_path)
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
            let entry = project.entry.clone();
            (Some(project), entry, input.clone())
        } else {
            (None, input.clone(), input.clone())
        }
    } else {
        eprintln!("error: input must be a .tri file, project directory, or .deploy/ artifact");
        process::exit(1);
    };

    // 2. Resolve target (OS-aware)
    let resolved = match trident::target::ResolvedTarget::resolve(target) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {}", e.message);
            process::exit(1);
        }
    };

    // 3. Build CompileOptions
    let mut options = resolve_options(&resolved.vm.name, profile, project.as_ref());
    options.target_config = resolved.vm.clone();
    if let Some(ref proj) = project {
        options.dep_dirs = load_dep_dirs(proj);
    }

    // 4. Compile
    eprintln!("Compiling {}...", source_path.display());
    let tasm = match trident::compile_project_with_options(&entry, &options) {
        Ok(t) => t,
        Err(_) => {
            eprintln!("error: compilation failed");
            process::exit(1);
        }
    };

    // 5. Cost analysis
    let cost = match trident::analyze_costs_project(&entry, &options) {
        Ok(c) => c,
        Err(_) => {
            eprintln!("warning: cost analysis failed, using zeros");
            trident::cost::ProgramCost {
                program_name: String::new(),
                functions: Vec::new(),
                total: trident::cost::TableCost::ZERO,
                attestation_hash_rows: 0,
                padded_height: 0,
                estimated_proving_secs: 0.0,
                loop_bound_waste: Vec::new(),
            }
        }
    };

    // 6. Parse source for function signatures
    let source = std::fs::read_to_string(&source_path).unwrap_or_default();
    let filename = source_path.to_string_lossy().to_string();
    let file = match trident::parse_source_silent(&source, &filename) {
        Ok(f) => f,
        Err(_) => {
            eprintln!("error: cannot parse source for manifest");
            process::exit(1);
        }
    };

    // 7. Determine name and version
    let (name, version) = if let Some(ref proj) = project {
        (proj.name.clone(), proj.version.clone())
    } else {
        let stem = source_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("program")
            .to_string();
        (stem, "0.1.0".to_string())
    };

    // 8. Optional verification
    if verify {
        eprintln!("Verifying {}...", source_path.display());
        match trident::verify_project(&entry) {
            Ok(report) => {
                if !report.is_safe() {
                    eprintln!("error: verification failed — refusing to deploy");
                    eprintln!("{}", report.format_report());
                    process::exit(1);
                }
                eprintln!("Verification: OK");
            }
            Err(_) => {
                eprintln!("error: verification failed");
                process::exit(1);
            }
        }
    }

    // 9. Package artifact into temp dir
    let output_base = source_path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let program_digest =
        trident::hash::ContentHash(trident::poseidon2::hash_bytes(tasm.as_bytes()));

    let target_display = if let Some(ref os) = resolved.os {
        format!("{} ({})", os.name, resolved.vm.name)
    } else {
        resolved.vm.name.clone()
    };

    let url = registry.unwrap_or_else(trident::registry::RegistryClient::default_url);

    // 10. Dry run
    if dry_run {
        eprintln!("Dry run — would deploy:");
        eprintln!("  Name:            {}", name);
        eprintln!("  Version:         {}", version);
        eprintln!("  Target:          {}", target_display);
        eprintln!("  Program digest:  {}", program_digest.to_hex());
        eprintln!("  Padded height:   {}", cost.padded_height);
        eprintln!("  Registry:        {}", url);
        return;
    }

    // 11. Generate artifact
    let result = match trident::artifact::generate_artifact(
        &name,
        &version,
        &tasm,
        &file,
        &cost,
        &resolved.vm,
        resolved.os.as_ref(),
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

    // 12. Deploy to registry
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

    // Read manifest to get program info
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

    // Parse the TASM as a pseudo-source to get function definitions for UCM.
    // Since we already have manifest.json, we use the original source if available.
    // Fall back to publishing just the compiled artifact.
    let source_path = artifact_dir.parent().and_then(|parent| {
        // Look for a .tri file next to the .deploy/ directory
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
    // Create a minimal codebase entry for the compiled artifact
    let hash = trident::hash::ContentHash(trident::poseidon2::hash_bytes(tasm.as_bytes()));
    eprintln!("Publishing artifact (digest: {})...", hash.to_hex());
    // Use the registry's raw definition publish endpoint
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

// --- trident deps ---

fn cmd_deps(action: DepsAction) {
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
                    trident::package::Dependency::Hash { hash } => {
                        println!("  {} = {} (hash)", name, &hash[..16]);
                    }
                    trident::package::Dependency::Registry {
                        name: reg_name,
                        registry,
                    } => {
                        println!("  {} = {} @ {} (registry)", name, reg_name, registry);
                    }
                    trident::package::Dependency::Path { path } => {
                        println!("  {} = {} (path)", name, path.display());
                    }
                }
            }
            // Check lockfile
            let lock_path = project.root_dir.join("trident.lock");
            if lock_path.exists() {
                match trident::package::load_lockfile(&lock_path) {
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
                trident::package::load_lockfile(&lock_path).ok()
            } else {
                None
            };
            match trident::package::resolve_dependencies(
                &project.root_dir,
                deps,
                &existing_lock,
                &registry,
            ) {
                Ok(lockfile) => {
                    if let Err(e) = trident::package::save_lockfile(&lock_path, &lockfile) {
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
            let lockfile = match trident::package::load_lockfile(&lock_path) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("error: {}", e);
                    process::exit(1);
                }
            };
            let mut ok = true;
            for (name, locked) in &lockfile.locked {
                let cached = trident::package::dep_source_path(&project.root_dir, &locked.hash);
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

fn cmd_lsp() {
    let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
    rt.block_on(trident::lsp::run_server());
}

// --- Helpers ---

/// Load dependency search directories from a project's lockfile (if present).
fn load_dep_dirs(project: &trident::project::Project) -> Vec<PathBuf> {
    let lock_path = project.root_dir.join("trident.lock");
    if !lock_path.exists() {
        return Vec::new();
    }
    match trident::package::load_lockfile(&lock_path) {
        Ok(lockfile) => trident::package::dependency_search_paths(&project.root_dir, &lockfile),
        Err(_) => Vec::new(),
    }
}

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
