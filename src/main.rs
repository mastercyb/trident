mod cli;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

use cli::deps::DepsAction;
use cli::registry::RegistryAction;
use cli::ucm::UcmAction;

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

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Init { name } => cli::init::cmd_init(name),
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
        } => cli::build::cmd_build(
            input, output, costs, hotspots, hints, annotate, save_costs, compare, &target, &profile,
        ),
        Command::Check {
            input,
            costs,
            target,
            profile,
        } => cli::check::cmd_check(input, costs, &target, &profile),
        Command::Fmt { input, check } => cli::fmt::cmd_fmt(input, check),
        Command::Test {
            input,
            target,
            profile,
        } => cli::test::cmd_test(input, &target, &profile),
        Command::Doc {
            input,
            output,
            target,
            profile,
        } => cli::doc::cmd_doc(input, output, &target, &profile),
        Command::Verify {
            input,
            verbose,
            smt,
            z3,
            json,
            synthesize,
        } => cli::verify::cmd_verify(input, verbose, smt, z3, json, synthesize),
        Command::Hash { input, full } => cli::hash::cmd_hash(input, full),
        Command::Bench { dir } => cli::bench::cmd_bench(dir),
        Command::Generate { input, output } => cli::generate::cmd_generate(input, output),
        Command::View { name, input, full } => cli::view::cmd_view(name, input, full),
        Command::Ucm { action } => cli::ucm::cmd_ucm(action),
        Command::Registry { action } => cli::registry::cmd_registry(action),
        Command::Equiv {
            input,
            fn_a,
            fn_b,
            verbose,
        } => cli::verify::cmd_equiv(input, &fn_a, &fn_b, verbose),
        Command::Deps { action } => cli::deps::cmd_deps(action),
        Command::Package {
            input,
            output,
            target,
            profile,
            verify,
            dry_run,
        } => cli::package::cmd_package(input, output, &target, &profile, verify, dry_run),
        Command::Deploy {
            input,
            target,
            profile,
            registry,
            verify,
            dry_run,
        } => cli::deploy::cmd_deploy(input, &target, &profile, registry, verify, dry_run),
        Command::Lsp => cmd_lsp(),
    }
}

fn cmd_lsp() {
    let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
    rt.block_on(trident::lsp::run_server());
}
