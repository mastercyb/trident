pub mod bench;
pub mod build;
pub mod check;
pub mod deploy;
pub mod deps;
pub mod doc;
pub mod fmt;
pub mod generate;
pub mod hash;
pub mod init;
pub mod package;
pub mod registry;
pub mod store;
pub mod test;
pub mod tree_sitter;
pub mod verify;
pub mod view;

use std::path::{Path, PathBuf};
use std::process;

/// Resolved input: entry file and optional project.
pub struct ResolvedInput {
    pub entry: PathBuf,
    pub project: Option<trident::project::Project>,
}

fn load_project(toml_path: &Path) -> trident::project::Project {
    match trident::project::Project::load(toml_path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {}", e.message);
            process::exit(1);
        }
    }
}

/// Resolve an input path (file or project directory) to an entry file and optional project.
pub fn resolve_input(input: &Path) -> ResolvedInput {
    if input.is_dir() {
        let toml_path = input.join("trident.toml");
        if !toml_path.exists() {
            eprintln!("error: no trident.toml found in '{}'", input.display());
            process::exit(1);
        }
        let project = load_project(&toml_path);
        let entry = project.entry.clone();
        return ResolvedInput {
            entry,
            project: Some(project),
        };
    }

    if !input.extension().is_some_and(|e| e == "tri") {
        eprintln!("error: input must be a .tri file or project directory");
        process::exit(1);
    }

    let toml_path = trident::project::Project::find(input.parent().unwrap_or(Path::new(".")));
    match toml_path {
        Some(p) => {
            let project = load_project(&p);
            let entry = project.entry.clone();
            ResolvedInput {
                entry,
                project: Some(project),
            }
        }
        None => ResolvedInput {
            entry: input.to_path_buf(),
            project: None,
        },
    }
}

/// Resolve a VM target + profile to CompileOptions.
pub fn resolve_options(
    target: &str,
    profile: &str,
    project: Option<&trident::project::Project>,
) -> trident::CompileOptions {
    // Backward compat: --target debug/release → treat as profile
    let (vm_target, actual_profile) = match target {
        "debug" | "release" => {
            eprintln!(
                "warning: --target {} is deprecated; use --profile {} --target triton",
                target, target
            );
            ("triton", target)
        }
        _ => (target, profile),
    };

    // Project may override the default "triton" target
    let effective_target = match (vm_target, project) {
        ("triton", Some(proj)) if proj.target.is_some() => {
            proj.target.as_deref().expect("guarded by is_some() check")
        }
        _ => vm_target,
    };

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

    let cfg_flags = project
        .and_then(|proj| proj.targets.get(actual_profile))
        .map(|flags| flags.iter().cloned().collect())
        .unwrap_or_else(|| std::collections::BTreeSet::from([actual_profile.to_string()]));

    trident::CompileOptions {
        profile: actual_profile.to_string(),
        cfg_flags,
        target_config,
        dep_dirs: Vec::new(),
    }
}

/// Result of the shared compile → analyze → parse → verify pipeline.
pub struct PreparedArtifact {
    pub project: Option<trident::project::Project>,
    pub entry: PathBuf,
    pub tasm: String,
    pub cost: trident::cost::ProgramCost,
    pub file: trident::ast::File,
    pub name: String,
    pub version: String,
    pub resolved: trident::target::ResolvedTarget,
}

/// Shared pipeline for package and deploy.
pub fn prepare_artifact(
    input: &Path,
    target: &str,
    profile: &str,
    verify: bool,
) -> PreparedArtifact {
    let ri = resolve_input(input);
    let project = ri.project;
    let entry = ri.entry;

    let resolved = match trident::target::ResolvedTarget::resolve(target) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {}", e.message);
            process::exit(1);
        }
    };

    let mut options = resolve_options(&resolved.vm.name, profile, project.as_ref());
    options.target_config = resolved.vm.clone();
    if let Some(ref proj) = project {
        options.dep_dirs = load_dep_dirs(proj);
    }

    eprintln!("Compiling {}...", entry.display());
    let tasm = match trident::compile_project_with_options(&entry, &options) {
        Ok(t) => t,
        Err(_) => {
            eprintln!("error: compilation failed");
            process::exit(1);
        }
    };

    let cost = trident::analyze_costs_project(&entry, &options).unwrap_or_else(|_| {
        eprintln!("warning: cost analysis failed, using zeros");
        trident::cost::ProgramCost {
            program_name: String::new(),
            functions: Vec::new(),
            total: trident::cost::TableCost::ZERO,
            table_names: Vec::new(),
            table_short_names: Vec::new(),
            attestation_hash_rows: 0,
            padded_height: 0,
            estimated_proving_ns: 0,
            loop_bound_waste: Vec::new(),
        }
    });

    let (_, file) = load_and_parse(&entry);

    let (name, version) = match project {
        Some(ref proj) => (proj.name.clone(), proj.version.clone()),
        None => {
            let stem = entry
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("program");
            (stem.to_string(), "0.1.0".to_string())
        }
    };

    if verify {
        verify_or_exit(&entry);
    }

    PreparedArtifact {
        project,
        entry,
        tasm,
        cost,
        file,
        name,
        version,
        resolved,
    }
}

fn verify_or_exit(entry: &Path) {
    eprintln!("Verifying {}...", entry.display());
    match trident::verify_project(entry) {
        Ok(report) if report.is_safe() => eprintln!("Verification: OK"),
        Ok(report) => {
            eprintln!("error: verification failed\n{}", report.format_report());
            process::exit(1);
        }
        Err(_) => {
            eprintln!("error: verification failed");
            process::exit(1);
        }
    }
}

/// Try to load and parse a .tri file, returning None on error (prints diagnostics).
pub fn try_load_and_parse(path: &Path) -> Option<(String, trident::ast::File)> {
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read '{}': {}", path.display(), e);
            return None;
        }
    };
    let filename = path.to_string_lossy().to_string();
    match trident::parse_source_silent(&source, &filename) {
        Ok(f) => Some((source, f)),
        Err(_) => {
            eprintln!("error: parse errors in '{}'", path.display());
            None
        }
    }
}

/// Load and parse a .tri file, exiting on error.
pub fn load_and_parse(path: &Path) -> (String, trident::ast::File) {
    match try_load_and_parse(path) {
        Some(result) => result,
        None => process::exit(1),
    }
}

/// Open the codebase store, exiting on error.
pub fn open_codebase() -> trident::store::Codebase {
    match trident::store::Codebase::open() {
        Ok(cb) => cb,
        Err(e) => {
            eprintln!("error: cannot open codebase: {}", e);
            process::exit(1);
        }
    }
}

/// Create a registry client with health check, exiting on error.
pub fn registry_client(url: Option<String>) -> trident::registry::RegistryClient {
    let url = url.unwrap_or_else(trident::registry::RegistryClient::default_url);
    let client = trident::registry::RegistryClient::new(&url);
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
    client
}

/// Resolve a registry URL to its default if None.
pub fn registry_url(url: Option<String>) -> String {
    url.unwrap_or_else(trident::registry::RegistryClient::default_url)
}

/// Load dependency search directories from a project's lockfile (if present).
pub fn load_dep_dirs(project: &trident::project::Project) -> Vec<PathBuf> {
    let lock_path = project.root_dir.join("trident.lock");
    if !lock_path.exists() {
        return Vec::new();
    }
    match trident::manifest::load_lockfile(&lock_path) {
        Ok(lockfile) => trident::manifest::dependency_search_paths(&project.root_dir, &lockfile),
        Err(_) => Vec::new(),
    }
}

pub fn find_program_source(input: &Path) -> Option<PathBuf> {
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

/// Truncate a hash string to a short prefix for display.
pub fn short_hash(hash: &str) -> &str {
    &hash[..hash.len().min(16)]
}

/// Resolve an input path to a list of .tri files (file or directory), exiting on error.
pub fn resolve_tri_files(input: &Path) -> Vec<PathBuf> {
    if input.is_dir() {
        collect_tri_files(input)
    } else if input.extension().is_some_and(|e| e == "tri") {
        vec![input.to_path_buf()]
    } else {
        eprintln!("error: input must be a .tri file or directory");
        process::exit(1);
    }
}

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
