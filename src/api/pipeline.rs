//! Shared project preparation pipeline.
//!
//! Extracts the resolve → parse → typecheck loop that was duplicated across
//! many public API functions in `lib.rs`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::ast;
use crate::ast::FileKind;
use crate::diagnostic::{render_diagnostics, Diagnostic};
use crate::resolve::{resolve_modules, resolve_modules_with_deps};
use crate::typecheck::{ModuleExports, TypeChecker};
use crate::CompileOptions;

/// A single parsed module: path, source text, and parsed AST.
pub(crate) struct ParsedModule {
    pub file_path: PathBuf,
    pub source: String,
    pub file: ast::File,
}

/// A fully resolved, parsed, and type-checked project.
pub(crate) struct PreparedProject {
    pub modules: Vec<ParsedModule>,
    pub exports: Vec<ModuleExports>,
}

impl PreparedProject {
    /// Build a project from an entry path using the given compile options.
    ///
    /// This performs the resolve → parse → typecheck pipeline that is shared
    /// across `compile_project`, `run_tests`, `analyze_costs_project`,
    /// and `generate_docs`.
    pub fn build(entry_path: &Path, options: &CompileOptions) -> Result<Self, Vec<Diagnostic>> {
        let resolved = if options.dep_dirs.is_empty() {
            resolve_modules(entry_path)?
        } else {
            resolve_modules_with_deps(entry_path, options.dep_dirs.clone())?
        };

        let mut modules = Vec::new();
        for m in &resolved {
            let file = crate::parse_source(&m.source, &m.file_path.to_string_lossy())?;
            modules.push(ParsedModule {
                file_path: m.file_path.clone(),
                source: m.source.clone(),
                file,
            });
        }

        let mut exports: Vec<ModuleExports> = Vec::new();
        for pm in &modules {
            let mut tc = TypeChecker::with_target(options.target_config.clone())
                .with_cfg_flags(options.cfg_flags.clone());
            for e in &exports {
                tc.import_module(e);
            }
            match tc.check_file(&pm.file) {
                Ok(e) => {
                    if !e.warnings.is_empty() {
                        render_diagnostics(
                            &e.warnings,
                            &pm.file_path.to_string_lossy(),
                            &pm.source,
                        );
                    }
                    exports.push(e);
                }
                Err(errors) => {
                    render_diagnostics(&errors, &pm.file_path.to_string_lossy(), &pm.source);
                    return Err(errors);
                }
            }
        }

        Ok(PreparedProject { modules, exports })
    }


    /// Build a project with default options (Triton target, debug profile).
    ///
    /// Used by `check_project` and `verify_project` which don't need target options.
    pub fn build_default(entry_path: &Path) -> Result<Self, Vec<Diagnostic>> {
        Self::build(entry_path, &CompileOptions::default())
    }


    /// Return the program module (last in topological order, has `FileKind::Program`).
    pub fn program_module(&self) -> Option<&ParsedModule> {
        self.modules
            .iter()
            .find(|m| m.file.kind == FileKind::Program)
    }

    /// Return the last parsed file (the entry / program module).
    pub fn last_file(&self) -> Option<&ast::File> {
        self.modules.last().map(|m| &m.file)
    }

    /// Build a global intrinsic map from all modules.
    ///
    /// Maps function names (short, qualified, and short-alias qualified) to
    /// their `#[intrinsic(...)]` values.
    pub fn intrinsic_map(&self) -> BTreeMap<String, String> {
        let mut map = BTreeMap::new();
        for pm in &self.modules {
            for item in &pm.file.items {
                if let ast::Item::Fn(func) = &item.node {
                    if let Some(ref intrinsic) = func.intrinsic {
                        let intr_value = if let Some(start) = intrinsic.node.find('(') {
                            let end = intrinsic.node.rfind(')').unwrap_or(intrinsic.node.len());
                            intrinsic.node[start + 1..end].to_string()
                        } else {
                            intrinsic.node.clone()
                        };
                        // Short function name
                        map.insert(func.name.node.clone(), intr_value.clone());
                        // Qualified name (module.func)
                        let qualified = format!("{}.{}", pm.file.name.node, func.name.node);
                        map.insert(qualified, intr_value.clone());
                        // Short alias (hash.func for std.hash)
                        if let Some(short) = pm.file.name.node.rsplit('.').next() {
                            if short != pm.file.name.node {
                                let short_qualified = format!("{}.{}", short, func.name.node);
                                map.insert(short_qualified, intr_value.clone());
                            }
                        }
                    }
                }
            }
        }
        map
    }

    /// Build module alias map: short name -> full name for dotted modules.
    pub fn module_aliases(&self) -> BTreeMap<String, String> {
        let mut aliases = BTreeMap::new();
        for pm in &self.modules {
            let full_name = &pm.file.name.node;
            if let Some(short) = full_name.rsplit('.').next() {
                if short != full_name.as_str() {
                    aliases.insert(short.to_string(), full_name.clone());
                }
            }
        }
        aliases
    }

    /// Build external constants map from all module exports.
    pub fn external_constants(&self) -> BTreeMap<String, u64> {
        let mut constants = BTreeMap::new();
        for exp in &self.exports {
            let full = &exp.module_name;
            let short = full.rsplit('.').next().unwrap_or(full);
            let has_short = short != full;
            for (const_name, _ty, value) in &exp.constants {
                let qualified = format!("{}.{}", full, const_name);
                constants.insert(qualified, *value);
                if has_short {
                    let short_qualified = format!("{}.{}", short, const_name);
                    constants.insert(short_qualified, *value);
                }
            }
        }
        constants
    }
}
