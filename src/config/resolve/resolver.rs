use std::collections::BTreeMap;

use super::*;

pub(crate) struct ModuleResolver {
    /// Root directory of the project.
    pub(crate) root_dir: PathBuf,
    /// Standard library directory (if found).
    pub(crate) stdlib_dir: Option<PathBuf>,
    /// OS library directory — OS-specific extension code (if found).
    pub(crate) os_dir: Option<PathBuf>,
    /// Additional directories to search for modules (from locked dependencies).
    pub(crate) dep_dirs: Vec<PathBuf>,
    /// All discovered modules by name.
    pub(crate) modules: BTreeMap<String, ModuleInfo>,
    /// Queue of modules to process.
    queue: Vec<String>,
    /// Diagnostics.
    diagnostics: Vec<Diagnostic>,
}

impl ModuleResolver {
    pub(crate) fn new(entry_path: &Path) -> Result<Self, Vec<Diagnostic>> {
        let root_dir = entry_path.parent().unwrap_or(Path::new(".")).to_path_buf();

        let source = std::fs::read_to_string(entry_path).map_err(|e| {
            vec![Diagnostic::error(
                format!("cannot read '{}': {}", entry_path.display(), e),
                Span::dummy(),
            )
            .with_help("check that the file exists and is readable".to_string())]
        })?;

        // Quick-parse the entry file to get its name and dependencies
        let (name, deps) = scan_module_header(&source);
        let entry_name = name.unwrap_or_else(|| "main".to_string());

        let info = ModuleInfo {
            name: entry_name.clone(),
            file_path: entry_path.to_path_buf(),
            source,
            dependencies: deps.clone(),
        };

        let mut modules = BTreeMap::new();
        modules.insert(entry_name.clone(), info);

        Ok(Self {
            root_dir,
            stdlib_dir: find_stdlib_dir(),
            os_dir: find_os_dir(),
            dep_dirs: Vec::new(),
            modules,
            queue: deps,
            diagnostics: Vec::new(),
        })
    }

    /// Find the VM intrinsic library directory.
    /// Mirrors stdlib_dir search but looks for `vm/` sibling to `std/`.
    pub(crate) fn find_vm_dir(&self) -> Option<PathBuf> {
        // If we have a stdlib_dir, look for vm/ as a sibling
        if let Some(ref stdlib_dir) = self.stdlib_dir {
            if let Some(parent) = stdlib_dir.parent() {
                let vm_dir = parent.join("vm");
                if vm_dir.is_dir() {
                    return Some(vm_dir);
                }
            }
        }
        // Fallback: vm/ in current working directory
        let cwd_vm = PathBuf::from("vm");
        if cwd_vm.is_dir() {
            return Some(cwd_vm);
        }
        None
    }

    pub(crate) fn discover_all(&mut self) -> Result<(), Vec<Diagnostic>> {
        while let Some(module_name) = self.queue.pop() {
            if self.modules.contains_key(&module_name) {
                continue;
            }

            // Resolve module name to file path
            let file_path = self.resolve_path(&module_name);
            let source = match std::fs::read_to_string(&file_path) {
                Ok(s) => s,
                Err(e) => {
                    self.diagnostics.push(
                        Diagnostic::error(
                            format!(
                                "cannot find module '{}' (looked at '{}'): {}",
                                module_name,
                                file_path.display(),
                                e
                            ),
                            Span::dummy(),
                        )
                        .with_help(format!(
                            "create the file '{}' or check the module name in the `use` statement",
                            file_path.display()
                        )),
                    );
                    continue;
                }
            };

            let (_name, deps) = scan_module_header(&source);

            // Queue newly discovered dependencies
            for dep in &deps {
                if !self.modules.contains_key(dep) {
                    self.queue.push(dep.clone());
                }
            }

            self.modules.insert(
                module_name.clone(),
                ModuleInfo {
                    name: module_name,
                    file_path,
                    source,
                    dependencies: deps,
                },
            );
        }

        if self.diagnostics.is_empty() {
            Ok(())
        } else {
            Err(self.diagnostics.clone())
        }
    }

    /// Resolve a dotted module name to a file path.
    ///
    /// Four-tier namespace:
    /// "vm.core.field"       → vm_dir/core/field.tri     (VM intrinsics)
    /// "std.crypto.sha256"   → stdlib_dir/crypto/sha256.tri (real libraries)
    /// "os.neptune.kernel"   → os_dir/neptune/kernel.tri (OS-specific)
    /// "crypto.sponge"       → root_dir/crypto/sponge.tri (local)
    ///
    /// Legacy backward compatibility still supported:
    /// "neptune.ext.kernel"  → os_dir/neptune/kernel.tri
    /// "ext.neptune.kernel"  → os_dir/neptune/kernel.tri
    /// "std.crypto.hash"     → vm_dir/crypto/hash.tri (intrinsics moved)
    /// "std.hash"            → vm_dir/crypto/hash.tri (flat → layered → vm)
    pub(crate) fn resolve_path(&self, module_name: &str) -> PathBuf {
        // Validate: reject path traversal components
        let raw_parts: Vec<&str> = module_name.split('.').collect();
        for part in &raw_parts {
            if part.is_empty()
                || *part == ".."
                || part.starts_with('.')
                || part.contains('/')
                || part.contains('\\')
            {
                return self.root_dir.join("<invalid-module-name>");
            }
        }

        // OS-specific extension modules: os.<os>.<module> → os/<os>/<module>.tri
        // Distinguishes os.neptune.kernel (extension) from os.neuron (portable).
        // An OS name is recognized if os/<os_name>/ exists as a directory.
        if let Some(rest) = module_name.strip_prefix("os.") {
            if let Some(ref os_dir) = self.os_dir {
                let parts: Vec<&str> = rest.split('.').collect();
                if parts.len() >= 2 {
                    let os_name = parts[0];
                    let target_dir = os_dir.join(os_name);
                    if target_dir.is_dir() {
                        let mut path = target_dir;
                        for part in &parts[1..] {
                            path = path.join(part);
                        }
                        return path.with_extension("tri");
                    }
                }
            }
        }

        // VM intrinsic modules: vm.* → vm/<rest>.tri
        if let Some(rest) = module_name.strip_prefix("vm.") {
            // Look for vm/ directory using same search strategy as stdlib
            if let Some(ref vm_dir) = self.find_vm_dir() {
                let parts: Vec<&str> = rest.split('.').collect();
                let mut path = vm_dir.clone();
                for part in &parts {
                    path = path.join(part);
                }
                return path.with_extension("tri");
            }
        }

        // Legacy: <os>.ext.<module> → os/<os>/<module>.tri
        if let Some(ext_pos) = raw_parts.iter().position(|&p| p == "ext") {
            if ext_pos > 0 && ext_pos + 1 < raw_parts.len() {
                if let Some(ref os_dir) = self.os_dir {
                    let os_name = &raw_parts[..ext_pos].join("/");
                    let rest = &raw_parts[ext_pos + 1..];
                    let mut path = os_dir.join(os_name);
                    for part in rest {
                        path = path.join(part);
                    }
                    return path.with_extension("tri");
                }
            }
        }

        // Legacy: ext.<os>.<module> → os/<os>/<module>.tri
        if let Some(rest) = module_name.strip_prefix("ext.") {
            if let Some(ref os_dir) = self.os_dir {
                let parts: Vec<&str> = rest.split('.').collect();
                let mut path = os_dir.clone();
                for part in &parts {
                    path = path.join(part);
                }
                return path.with_extension("tri");
            }
        }

        // Standard library modules resolve from stdlib_dir
        if let Some(rest) = module_name.strip_prefix("std.") {
            if let Some(ref stdlib_dir) = self.stdlib_dir {
                let parts: Vec<&str> = rest.split('.').collect();
                let mut path = stdlib_dir.clone();
                for part in &parts {
                    path = path.join(part);
                }
                let candidate = path.with_extension("tri");
                // If the layered path exists, use it
                if candidate.exists() {
                    return candidate;
                }
                // Legacy fallback: try remapped path for old flat names
                if let Some(new_name) = legacy_stdlib_fallback(module_name) {
                    return self.resolve_path(new_name);
                }
                // Return the original candidate (will fail with good error)
                return candidate;
            }
        }

        // Check dependency cache directories
        for dep_dir in &self.dep_dirs {
            let parts: Vec<&str> = module_name.split('.').collect();
            let mut path = dep_dir.clone();
            for part in &parts {
                path = path.join(part);
            }
            let candidate = path.with_extension("tri");
            if candidate.exists() {
                return candidate;
            }
            // Also check for main.tri inside a directory matching the name
            if path.is_dir() {
                let main_tri = path.join("main.tri");
                if main_tri.exists() {
                    return main_tri;
                }
            }
        }

        // Default: local project path
        let parts: Vec<&str> = module_name.split('.').collect();
        let mut path = self.root_dir.clone();
        for part in &parts {
            path = path.join(part);
        }
        path.with_extension("tri")
    }

    /// Topological sort of the module DAG. Returns Err if circular.
    pub(crate) fn topological_sort(&self) -> Result<Vec<ModuleInfo>, Vec<Diagnostic>> {
        let mut visited: BTreeSet<String> = BTreeSet::new();
        let mut in_progress: BTreeSet<String> = BTreeSet::new();
        let mut order: Vec<String> = Vec::new();
        let mut diagnostics: Vec<Diagnostic> = Vec::new();

        for name in self.modules.keys() {
            if !visited.contains(name) {
                self.dfs(
                    name,
                    &mut visited,
                    &mut in_progress,
                    &mut order,
                    &mut diagnostics,
                );
            }
        }

        if !diagnostics.is_empty() {
            return Err(diagnostics);
        }

        let result: Vec<ModuleInfo> = order
            .into_iter()
            .filter_map(|name| self.modules.get(&name).cloned())
            .collect();

        Ok(result)
    }

    fn dfs(
        &self,
        name: &str,
        visited: &mut BTreeSet<String>,
        in_progress: &mut BTreeSet<String>,
        order: &mut Vec<String>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if visited.contains(name) {
            return;
        }
        if in_progress.contains(name) {
            diagnostics.push(
                Diagnostic::error(
                    format!("circular dependency detected involving module '{}'", name),
                    Span::dummy(),
                )
                .with_help(
                    "break the cycle by extracting shared definitions into a separate module"
                        .to_string(),
                ),
            );
            return;
        }

        in_progress.insert(name.to_string());

        if let Some(info) = self.modules.get(name) {
            for dep in &info.dependencies {
                self.dfs(dep, visited, in_progress, order, diagnostics);
            }
        }

        in_progress.remove(name);
        visited.insert(name.to_string());
        order.push(name.to_string());
    }
}

/// Quick scan of a source file to extract module name and `use` dependencies.
/// Does not fully parse — just looks for `program X` / `module X` and `use Y` lines.
pub(crate) fn scan_module_header(source: &str) -> (Option<String>, Vec<String>) {
    let mut name = None;
    let mut deps = Vec::new();

    for line in source.lines() {
        let trimmed = line.trim();

        // Skip comments and empty lines
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("program ") {
            name = Some(rest.trim().to_string());
        } else if let Some(rest) = trimmed.strip_prefix("module ") {
            name = Some(rest.trim().to_string());
        } else if let Some(rest) = trimmed.strip_prefix("use ") {
            let dep = rest.trim().to_string();
            deps.push(dep);
        } else {
            // Once we hit a non-header line, stop scanning for use statements
            // (use must come before items per the grammar)
            if trimmed.starts_with("fn ")
                || trimmed.starts_with("pub ")
                || trimmed.starts_with("const ")
                || trimmed.starts_with("struct ")
            {
                break;
            }
        }
    }

    (name, deps)
}
