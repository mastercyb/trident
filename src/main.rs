mod cli;

use clap::{Parser, Subcommand};

use cli::bench::BenchArgs;
use cli::build::BuildArgs;
use cli::check::CheckArgs;
use cli::deploy::DeployArgs;
use cli::deps::DepsAction;
use cli::doc::DocArgs;
use cli::fmt::FmtArgs;
use cli::generate::GenerateArgs;
use cli::hash::HashArgs;
use cli::init::InitArgs;
use cli::package::PackageArgs;
use cli::registry::RegistryAction;
use cli::store::StoreAction;
use cli::test::TestArgs;
use cli::verify::{EquivArgs, VerifyArgs};
use cli::view::ViewArgs;

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
    Init(InitArgs),
    /// Compile a .tri file (or project) to TASM
    Build(BuildArgs),
    /// Type-check without emitting TASM
    Check(CheckArgs),
    /// Format .tri source files
    Fmt(FmtArgs),
    /// Run #[test] functions
    Test(TestArgs),
    /// Generate documentation with cost annotations
    Doc(DocArgs),
    /// Verify assertions using symbolic execution + algebraic solver
    Verify(VerifyArgs),
    /// Show content hashes of functions (BLAKE3)
    Hash(HashArgs),
    /// Run benchmarks: compare Trident output vs hand-written TASM
    Bench(BenchArgs),
    /// Generate code scaffold from spec annotations
    Generate(GenerateArgs),
    /// View a function definition (pretty-printed from AST)
    View(ViewArgs),
    /// Hash-keyed definitions store
    Store {
        #[command(subcommand)]
        action: StoreAction,
    },
    /// Atlas — on-chain package registry: publish, pull, search definitions
    Atlas {
        #[command(subcommand)]
        action: RegistryAction,
    },
    /// Check semantic equivalence of two functions
    Equiv(EquivArgs),
    /// Manage project dependencies
    Deps {
        #[command(subcommand)]
        action: DepsAction,
    },
    /// Build, hash, and produce a self-contained artifact (.deploy/ directory)
    Package(PackageArgs),
    /// Deploy a program to a registry server or blockchain node
    Deploy(DeployArgs),
    /// Start the Language Server Protocol server
    Lsp,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Init(args) => cli::init::cmd_init(args),
        Command::Build(args) => cli::build::cmd_build(args),
        Command::Check(args) => cli::check::cmd_check(args),
        Command::Fmt(args) => cli::fmt::cmd_fmt(args),
        Command::Test(args) => cli::test::cmd_test(args),
        Command::Doc(args) => cli::doc::cmd_doc(args),
        Command::Verify(args) => cli::verify::cmd_verify(args),
        Command::Hash(args) => cli::hash::cmd_hash(args),
        Command::Bench(args) => cli::bench::cmd_bench(args),
        Command::Generate(args) => cli::generate::cmd_generate(args),
        Command::View(args) => cli::view::cmd_view(args),
        Command::Store { action } => cli::store::cmd_store(action),
        Command::Atlas { action } => cli::registry::cmd_registry(action),
        Command::Equiv(args) => cli::verify::cmd_equiv(args),
        Command::Deps { action } => cli::deps::cmd_deps(action),
        Command::Package(args) => cli::package::cmd_package(args),
        Command::Deploy(args) => cli::deploy::cmd_deploy(args),
        Command::Lsp => cmd_lsp(),
    }
}

fn cmd_lsp() {
    let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
    rt.block_on(trident::lsp::run_server());
}
