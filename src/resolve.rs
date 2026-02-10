use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::diagnostic::Diagnostic;
use crate::span::Span;

/// Information about a discovered module.
#[derive(Clone, Debug)]
pub struct ModuleInfo {
    /// Dotted module name (e.g. "crypto.sponge").
    pub name: String,
    /// Filesystem path to the .tri file.
    pub file_path: PathBuf,
    /// Source code.
    pub source: String,
    /// Modules this module depends on (from `use` statements).
    pub dependencies: Vec<String>,
}

/// Resolve all modules reachable from an entry point.
/// Returns modules in topological order (dependencies first).
pub fn resolve_modules(entry_path: &Path) -> Result<Vec<ModuleInfo>, Vec<Diagnostic>> {
    let mut resolver = ModuleResolver::new(entry_path)?;
    resolver.discover_all()?;
    resolver.topological_sort()
}

/// Find the standard library directory.
/// Search order:
///   1. TRIDENT_STDLIB environment variable
///   2. `std/` relative to the compiler binary
///   3. `std/` in the repository root (development)
pub fn find_stdlib_dir() -> Option<PathBuf> {
    // 1. Environment variable
    if let Ok(p) = std::env::var("TRIDENT_STDLIB") {
        let path = PathBuf::from(p);
        if path.is_dir() {
            return Some(path);
        }
    }

    // 2. Relative to the compiler binary
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let path = dir.join("std");
            if path.is_dir() {
                return Some(path);
            }
            // Also check one level up (e.g. target/debug/../std)
            if let Some(parent) = dir.parent() {
                let path = parent.join("std");
                if path.is_dir() {
                    return Some(path);
                }
                // Two levels up for target/debug/../../std
                if let Some(grandparent) = parent.parent() {
                    let path = grandparent.join("std");
                    if path.is_dir() {
                        return Some(path);
                    }
                }
            }
        }
    }

    // 3. Current working directory
    let cwd_std = PathBuf::from("std");
    if cwd_std.is_dir() {
        return Some(cwd_std);
    }

    None
}

struct ModuleResolver {
    /// Root directory of the project.
    root_dir: PathBuf,
    /// Standard library directory (if found).
    stdlib_dir: Option<PathBuf>,
    /// All discovered modules by name.
    modules: HashMap<String, ModuleInfo>,
    /// Queue of modules to process.
    queue: Vec<String>,
    /// Diagnostics.
    diagnostics: Vec<Diagnostic>,
}

impl ModuleResolver {
    fn new(entry_path: &Path) -> Result<Self, Vec<Diagnostic>> {
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

        let mut modules = HashMap::new();
        modules.insert(entry_name.clone(), info);

        Ok(Self {
            root_dir,
            stdlib_dir: find_stdlib_dir(),
            modules,
            queue: deps,
            diagnostics: Vec::new(),
        })
    }

    fn discover_all(&mut self) -> Result<(), Vec<Diagnostic>> {
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
    /// "std.hash" → stdlib_dir/hash.tri
    /// "crypto.sponge" → root_dir/crypto/sponge.tri
    /// "merkle" → root_dir/merkle.tri
    fn resolve_path(&self, module_name: &str) -> PathBuf {
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

        // Standard library modules resolve from stdlib_dir
        if let Some(rest) = module_name.strip_prefix("std.") {
            if let Some(ref stdlib_dir) = self.stdlib_dir {
                let parts: Vec<&str> = rest.split('.').collect();
                let mut path = stdlib_dir.clone();
                for part in &parts {
                    path = path.join(part);
                }
                return path.with_extension("tri");
            }
        }

        let parts: Vec<&str> = module_name.split('.').collect();
        let mut path = self.root_dir.clone();
        for part in &parts {
            path = path.join(part);
        }
        path.with_extension("tri")
    }

    /// Topological sort of the module DAG. Returns Err if circular.
    fn topological_sort(&self) -> Result<Vec<ModuleInfo>, Vec<Diagnostic>> {
        let mut visited: HashSet<String> = HashSet::new();
        let mut in_progress: HashSet<String> = HashSet::new();
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
        visited: &mut HashSet<String>,
        in_progress: &mut HashSet<String>,
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
fn scan_module_header(source: &str) -> (Option<String>, Vec<String>) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_module_header_program() {
        let (name, deps) =
            scan_module_header("program my_app\n\nuse merkle\nuse crypto.sponge\n\nfn main() {}");
        assert_eq!(name, Some("my_app".to_string()));
        assert_eq!(deps, vec!["merkle", "crypto.sponge"]);
    }

    #[test]
    fn test_scan_module_header_module() {
        let (name, deps) =
            scan_module_header("module merkle\n\nuse std.convert\n\npub fn verify() {}");
        assert_eq!(name, Some("merkle".to_string()));
        assert_eq!(deps, vec!["std.convert"]);
    }

    #[test]
    fn test_scan_module_header_no_deps() {
        let (name, deps) = scan_module_header("program simple\n\nfn main() {}");
        assert_eq!(name, Some("simple".to_string()));
        assert!(deps.is_empty());
    }

    // --- Error path tests ---

    #[test]
    fn test_error_missing_entry_file() {
        let result = resolve_modules(Path::new("/nonexistent/path/to/file.tri"));
        assert!(result.is_err(), "should error on missing entry file");
        let diags = result.unwrap_err();
        assert!(
            diags[0].message.contains("cannot read"),
            "should report file read error, got: {}",
            diags[0].message
        );
        assert!(
            diags[0].help.is_some(),
            "file-not-found error should have help text"
        );
    }

    #[test]
    fn test_error_module_not_found_has_path() {
        // Create a temp file that uses a nonexistent module
        let dir = std::env::temp_dir().join("trident_test_resolve");
        let _ = std::fs::create_dir_all(&dir);
        let entry = dir.join("test_missing.tri");
        std::fs::write(
            &entry,
            "program test_missing\nuse nonexistent_module\nfn main() {}\n",
        )
        .unwrap();

        let result = resolve_modules(&entry);
        assert!(result.is_err(), "should error on missing module");
        let diags = result.unwrap_err();
        let has_not_found = diags.iter().any(|d| {
            d.message
                .contains("cannot find module 'nonexistent_module'")
        });
        assert!(
            has_not_found,
            "should report module not found with name, got: {:?}",
            diags.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
        // Check that it says where it looked
        let has_path = diags.iter().any(|d| d.message.contains("looked at"));
        assert!(has_path, "should say where it looked for the module");
        // Check help text
        let has_help = diags.iter().any(|d| d.help.is_some());
        assert!(has_help, "module-not-found error should have help text");

        // Cleanup
        let _ = std::fs::remove_file(&entry);
    }

    #[test]
    fn test_path_traversal_rejected() {
        // A module name with ".." should not escape the project directory
        let dir = std::env::temp_dir().join("trident_test_traversal");
        let _ = std::fs::create_dir_all(&dir);
        let entry = dir.join("test_traversal.tri");
        std::fs::write(
            &entry,
            "program test_traversal\nuse ....etc.passwd\nfn main() {}\n",
        )
        .unwrap();

        let result = resolve_modules(&entry);
        assert!(result.is_err(), "path traversal module should fail");
        let diags = result.unwrap_err();
        // Should get a "cannot find module" error, NOT actually read outside project
        let has_error = diags
            .iter()
            .any(|d| d.message.contains("cannot find module"));
        assert!(
            has_error,
            "should report module not found, got: {:?}",
            diags.iter().map(|d| &d.message).collect::<Vec<_>>()
        );

        let _ = std::fs::remove_file(&entry);
    }
}
