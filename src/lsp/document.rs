//! Per-document cached state for incremental LSP operations.

use std::collections::BTreeMap;

use tower_lsp::lsp_types::SemanticToken;

use crate::syntax::lexeme::Lexeme;
use crate::syntax::lexer::{Comment, Lexer};
use crate::syntax::span::Spanned;

/// Name classification for identifier highlighting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NameKind {
    Function,
    Type,
    Parameter,
    Variable,
    Constant,
    EventName,
    Property,
}

/// Cached state for a single open document.
pub(super) struct DocumentState {
    /// Current full source text.
    pub source: String,
    /// Cached lexer output: tokens sorted by byte offset.
    pub tokens: Vec<Spanned<Lexeme>>,
    /// Cached comments from last lex.
    pub comments: Vec<Comment>,
    /// Precomputed line start byte offsets.
    pub line_starts: Vec<usize>,
    /// Classified name kinds from last successful parse.
    pub name_kinds: BTreeMap<String, (NameKind, u32)>,
    /// Last emitted semantic token array (for delta computation).
    pub last_semantic_tokens: Vec<SemanticToken>,
    /// Monotonically increasing result ID for delta tracking.
    pub result_version: u64,
}

impl DocumentState {
    pub fn new(source: String) -> Self {
        let line_starts = compute_line_starts(&source);
        let (tokens, comments, _diagnostics) = Lexer::new(&source, 0).tokenize();
        Self {
            source,
            tokens,
            comments,
            line_starts,
            name_kinds: BTreeMap::new(),
            last_semantic_tokens: Vec::new(),
            result_version: 0,
        }
    }

    /// Current result_id as a string for LSP protocol.
    pub fn result_id(&self) -> String {
        self.result_version.to_string()
    }
}

/// Compute byte offsets of each line start in the source.
pub(super) fn compute_line_starts(source: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (i, b) in source.bytes().enumerate() {
        if b == b'\n' {
            starts.push(i + 1);
        }
    }
    starts
}
