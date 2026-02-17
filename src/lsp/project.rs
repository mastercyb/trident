//! Project-level helpers: symbol index, exports, function costs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use tower_lsp::lsp_types::*;

use crate::ast::Item;
use crate::resolve::resolve_modules;
use crate::typecheck::{ModuleExports, TypeChecker};

use super::document::DocumentData;
use super::util::{format_fn_signature, span_to_range};
use super::TridentLsp;

/// Find the project entry point for a given file.
pub(super) fn find_project_entry(file_path: &Path) -> PathBuf {
    let dir = file_path.parent().unwrap_or(Path::new("."));
    match crate::project::Project::find(dir) {
        Some(toml_path) => match crate::project::Project::load(&toml_path) {
            Ok(p) => p.entry,
            Err(_) => file_path.to_path_buf(),
        },
        None => file_path.to_path_buf(),
    }
}

impl TridentLsp {
    /// Build a symbol index mapping names to (uri, range) for go-to-definition.
    pub(super) fn build_symbol_index(&self, file_path: &Path) -> BTreeMap<String, (Url, Range)> {
        let mut index = BTreeMap::new();
        let entry = find_project_entry(file_path);

        let modules = match resolve_modules(&entry) {
            Ok(m) => m,
            Err(_) => return index,
        };

        for module in &modules {
            let parsed = match crate::parse_source_silent(
                &module.source,
                &module.file_path.to_string_lossy(),
            ) {
                Ok(f) => f,
                Err(_) => continue,
            };

            let mod_uri = match Url::from_file_path(&module.file_path) {
                Ok(u) => u,
                Err(_) => match Url::parse(&format!("file://{}", module.file_path.display())) {
                    Ok(u) => u,
                    Err(_) => continue,
                },
            };
            let mod_short = module.name.rsplit('.').next().unwrap_or(&module.name);

            for item in &parsed.items {
                let (name, name_span) = match &item.node {
                    Item::Fn(f) => (f.name.node.clone(), f.name.span),
                    Item::Struct(s) => (s.name.node.clone(), s.name.span),
                    Item::Const(c) => (c.name.node.clone(), c.name.span),
                    Item::Event(e) => (e.name.node.clone(), e.name.span),
                };

                let range = span_to_range(&module.source, name_span);
                let qualified = format!("{}.{}", mod_short, name);
                let full_qualified = format!("{}.{}", module.name, name);

                index.insert(name.clone(), (mod_uri.clone(), range));
                index.insert(qualified, (mod_uri.clone(), range));
                if full_qualified != format!("{}.{}", mod_short, name) {
                    index.insert(full_qualified, (mod_uri.clone(), range));
                }
            }
        }

        index
    }

    /// Collect type-checked exports from all project modules.
    pub(super) fn collect_project_exports(&self, file_path: &Path) -> Vec<ModuleExports> {
        let entry = find_project_entry(file_path);

        let modules = match resolve_modules(&entry) {
            Ok(m) => m,
            Err(_) => return Vec::new(),
        };

        let mut all_exports = Vec::new();
        for module in &modules {
            let parsed = match crate::parse_source_silent(
                &module.source,
                &module.file_path.to_string_lossy(),
            ) {
                Ok(f) => f,
                Err(_) => continue,
            };

            let mut tc = TypeChecker::new();
            for exports in &all_exports {
                tc.import_module(exports);
            }

            match tc.check_file(&parsed) {
                Ok(exports) => all_exports.push(exports),
                Err(_) => continue,
            }
        }

        all_exports
    }

    /// Compute cost for a specific user-defined function by name.
    pub(super) fn compute_function_cost(
        &self,
        file_path: &Path,
        fn_name: &str,
    ) -> Option<crate::cost::TableCost> {
        let entry = find_project_entry(file_path);
        let modules = resolve_modules(&entry).ok()?;

        for module in &modules {
            let parsed =
                crate::parse_source_silent(&module.source, &module.file_path.to_string_lossy())
                    .ok()?;

            let has_fn = parsed.items.iter().any(|item| {
                if let Item::Fn(f) = &item.node {
                    f.name.node == fn_name
                } else {
                    false
                }
            });

            if has_fn {
                let mut analyzer = crate::cost::CostAnalyzer::default();
                let program_cost = analyzer.analyze_file(&parsed);
                for fc in &program_cost.functions {
                    if fc.name == fn_name {
                        return Some(fc.cost.clone());
                    }
                }
            }
        }

        None
    }

    /// Collect workspace symbols from all open documents, filtered by query.
    pub(super) fn workspace_symbols(
        &self,
        query: &str,
        docs: &std::collections::BTreeMap<Url, DocumentData>,
    ) -> Vec<SymbolInformation> {
        let query_lower = query.to_lowercase();
        let mut symbols = Vec::new();

        for (uri, doc) in docs.iter() {
            let file = match crate::parse_source_silent(&doc.source, uri.path()) {
                Ok(f) => f,
                Err(_) => continue,
            };

            for item in &file.items {
                let (name, kind, name_span) = match &item.node {
                    Item::Fn(f) => (f.name.node.clone(), SymbolKind::FUNCTION, f.name.span),
                    Item::Struct(s) => (s.name.node.clone(), SymbolKind::STRUCT, s.name.span),
                    Item::Const(c) => (c.name.node.clone(), SymbolKind::CONSTANT, c.name.span),
                    Item::Event(e) => (e.name.node.clone(), SymbolKind::EVENT, e.name.span),
                };

                if !query_lower.is_empty() && !name.to_lowercase().contains(&query_lower) {
                    continue;
                }

                #[allow(deprecated)]
                symbols.push(SymbolInformation {
                    name,
                    kind,
                    tags: None,
                    deprecated: None,
                    location: Location {
                        uri: uri.clone(),
                        range: span_to_range(&doc.source, name_span),
                    },
                    container_name: None,
                });
            }
        }

        symbols
    }

    /// Build document symbols for a single file.
    pub(super) fn document_symbols(
        &self,
        source: &str,
        file: &crate::ast::File,
    ) -> Vec<DocumentSymbol> {
        let mut symbols = Vec::new();
        for item in &file.items {
            let (name, kind, detail) = match &item.node {
                Item::Fn(f) => {
                    let sig = format_fn_signature(f);
                    (f.name.node.clone(), SymbolKind::FUNCTION, Some(sig))
                }
                Item::Struct(s) => (s.name.node.clone(), SymbolKind::STRUCT, None),
                Item::Const(c) => (c.name.node.clone(), SymbolKind::CONSTANT, None),
                Item::Event(e) => (e.name.node.clone(), SymbolKind::EVENT, None),
            };

            let range = span_to_range(source, item.span);
            let selection_range = match &item.node {
                Item::Fn(f) => span_to_range(source, f.name.span),
                Item::Struct(s) => span_to_range(source, s.name.span),
                Item::Const(c) => span_to_range(source, c.name.span),
                Item::Event(e) => span_to_range(source, e.name.span),
            };

            #[allow(deprecated)]
            symbols.push(DocumentSymbol {
                name,
                detail,
                kind,
                tags: None,
                deprecated: None,
                range,
                selection_range,
                children: None,
            });
        }
        symbols
    }
}
