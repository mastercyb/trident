//! Find references, rename, and document highlight.
//!
//! All three features share the same foundation: lex the source and
//! collect all `Ident(name)` tokens matching a target name.

use std::path::PathBuf;

use tower_lsp::lsp_types::*;

use crate::syntax::lexeme::Lexeme;
use crate::syntax::lexer::Lexer;

use super::project::find_project_entry;
use super::util::{position_to_byte_offset, span_to_range, word_at_position};
use super::TridentLsp;

/// Find all occurrences of `target` as an identifier in `source`.
fn find_references_in_source(source: &str, target: &str) -> Vec<Range> {
    let (tokens, _, _) = Lexer::new(source, 0).tokenize();
    tokens
        .iter()
        .filter_map(|tok| {
            if let Lexeme::Ident(name) = &tok.node {
                if name == target {
                    return Some(span_to_range(source, tok.span));
                }
            }
            None
        })
        .collect()
}

/// Find all references to `target` across all project modules.
fn find_references_in_project(file_path: &std::path::Path, target: &str) -> Vec<Location> {
    let entry = find_project_entry(file_path);
    let modules = match crate::resolve::resolve_modules(&entry) {
        Ok(m) => m,
        Err(_) => return Vec::new(),
    };

    let mut locations = Vec::new();
    for module in &modules {
        let uri = Url::from_file_path(&module.file_path).unwrap_or_else(|_| {
            Url::parse(&format!("file://{}", module.file_path.display())).unwrap()
        });
        for range in find_references_in_source(&module.source, target) {
            locations.push(Location {
                uri: uri.clone(),
                range,
            });
        }
    }
    locations
}

/// Validate that the position is on an identifier and return its range + text.
fn prepare_rename_at(source: &str, pos: Position) -> Option<(Range, String)> {
    let offset = position_to_byte_offset(source, pos)?;
    let bytes = source.as_bytes();

    // Find identifier boundaries (bare name only, no dots)
    let mut start = offset;
    while start > 0 && is_ident_byte(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = offset;
    while end < bytes.len() && is_ident_byte(bytes[end]) {
        end += 1;
    }
    if start == end {
        return None;
    }

    let name = source[start..end].to_string();
    // Verify it's actually an identifier token (not a keyword)
    let (tokens, _, _) = Lexer::new(&name, 0).tokenize();
    if tokens.iter().any(|t| matches!(&t.node, Lexeme::Ident(_))) {
        let start_pos = super::util::byte_offset_to_position(source, start);
        let end_pos = super::util::byte_offset_to_position(source, end);
        Some((Range::new(start_pos, end_pos), name))
    } else {
        None
    }
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

impl TridentLsp {
    pub(super) fn do_references(&self, uri: &Url, pos: Position) -> Vec<Location> {
        let source = match self.documents.lock().unwrap().get(uri) {
            Some(doc) => doc.source.clone(),
            None => return Vec::new(),
        };

        let word = word_at_position(&source, pos);
        // Use bare name (after last dot) for reference search
        let target = word.rsplit('.').next().unwrap_or(&word);
        if target.is_empty() {
            return Vec::new();
        }

        let file_path = PathBuf::from(uri.path());
        find_references_in_project(&file_path, target)
    }

    pub(super) fn do_document_highlight(&self, uri: &Url, pos: Position) -> Vec<DocumentHighlight> {
        let source = match self.documents.lock().unwrap().get(uri) {
            Some(doc) => doc.source.clone(),
            None => return Vec::new(),
        };

        let word = word_at_position(&source, pos);
        let target = word.rsplit('.').next().unwrap_or(&word);
        if target.is_empty() {
            return Vec::new();
        }

        // Use name_kinds to distinguish definition (Write) from use (Read)
        let name_kinds = self
            .documents
            .lock()
            .unwrap()
            .get(uri)
            .map(|d| d.name_kinds.clone())
            .unwrap_or_default();

        let is_definition_site = name_kinds
            .get(target)
            .map(|(_, mods)| mods & super::semantic::MOD_DECLARATION != 0)
            .unwrap_or(false);

        find_references_in_source(&source, target)
            .into_iter()
            .map(|range| {
                // First occurrence at definition site gets Write kind
                let kind = if is_definition_site {
                    Some(DocumentHighlightKind::WRITE)
                } else {
                    Some(DocumentHighlightKind::READ)
                };
                DocumentHighlight { range, kind }
            })
            .collect()
    }

    pub(super) fn do_prepare_rename(
        &self,
        uri: &Url,
        pos: Position,
    ) -> Option<PrepareRenameResponse> {
        let source = match self.documents.lock().unwrap().get(uri) {
            Some(doc) => doc.source.clone(),
            None => return None,
        };

        let (range, name) = prepare_rename_at(&source, pos)?;
        Some(PrepareRenameResponse::RangeWithPlaceholder {
            range,
            placeholder: name,
        })
    }

    pub(super) fn do_rename(
        &self,
        uri: &Url,
        pos: Position,
        new_name: &str,
    ) -> Option<WorkspaceEdit> {
        let source = match self.documents.lock().unwrap().get(uri) {
            Some(doc) => doc.source.clone(),
            None => return None,
        };

        let word = word_at_position(&source, pos);
        let old_name = word.rsplit('.').next().unwrap_or(&word);
        if old_name.is_empty() {
            return None;
        }

        let file_path = PathBuf::from(uri.path());
        let locations = find_references_in_project(&file_path, old_name);

        let mut changes: std::collections::BTreeMap<Url, Vec<TextEdit>> =
            std::collections::BTreeMap::new();
        for loc in locations {
            changes.entry(loc.uri).or_default().push(TextEdit {
                range: loc.range,
                new_text: new_name.to_string(),
            });
        }

        // Convert BTreeMap to HashMap for WorkspaceEdit
        let changes: std::collections::HashMap<Url, Vec<TextEdit>> = changes.into_iter().collect();

        Some(WorkspaceEdit {
            changes: Some(changes),
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_refs_in_source_finds_all_uses() {
        let source = "program test\nfn foo() {\n  let x: Field = 1\n  let y: Field = x + x\n}\n";
        let refs = find_references_in_source(source, "x");
        assert_eq!(refs.len(), 3); // let x, x + x
    }

    #[test]
    fn find_refs_ignores_keywords() {
        let source = "program test\nfn main() {\n  let val: Field = 1\n}\n";
        let refs = find_references_in_source(source, "fn");
        assert_eq!(refs.len(), 0);
    }

    #[test]
    fn prepare_rename_on_identifier() {
        let source = "program test\nfn foo() {}\n";
        // Position on "foo" (line 1, col 3)
        let result = prepare_rename_at(source, Position::new(1, 4));
        assert!(result.is_some());
        let (_, name) = result.unwrap();
        assert_eq!(name, "foo");
    }

    #[test]
    fn prepare_rename_on_keyword_fails() {
        let source = "program test\nfn foo() {}\n";
        // Position on "fn" (line 1, col 0)
        let result = prepare_rename_at(source, Position::new(1, 0));
        assert!(result.is_none());
    }
}
