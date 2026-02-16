use std::collections::BTreeMap;
use std::path::Path;

use tower_lsp::lsp_types::*;

use crate::ast::{Block, File, Item, Stmt};
use crate::syntax::lexeme::Lexeme;
use crate::syntax::lexer::Lexer;
use crate::syntax::span::Span;

use super::builtins::builtin_completions;

// Token type indices — must match TOKEN_TYPES order.
const TT_KEYWORD: u32 = 0;
const TT_TYPE: u32 = 1;
const TT_FUNCTION: u32 = 2;
const TT_VARIABLE: u32 = 3;
const TT_PARAMETER: u32 = 4;
const TT_PROPERTY: u32 = 5;
const TT_NUMBER: u32 = 6;
const TT_COMMENT: u32 = 7;
const TT_OPERATOR: u32 = 8;
#[allow(dead_code)] // reserved for use-path highlighting
const TT_NAMESPACE: u32 = 9;
const TT_EVENT: u32 = 10;
const TT_MACRO: u32 = 11;
const TT_ENUM_MEMBER: u32 = 12;

// Modifier bit flags — must match TOKEN_MODIFIERS order.
const MOD_DECLARATION: u32 = 1 << 0;
const MOD_DEFINITION: u32 = 1 << 1;
const MOD_READONLY: u32 = 1 << 2;
const MOD_DEFAULT_LIBRARY: u32 = 1 << 3;

pub fn token_legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: vec![
            SemanticTokenType::KEYWORD,     // 0
            SemanticTokenType::TYPE,        // 1
            SemanticTokenType::FUNCTION,    // 2
            SemanticTokenType::VARIABLE,    // 3
            SemanticTokenType::PARAMETER,   // 4
            SemanticTokenType::PROPERTY,    // 5
            SemanticTokenType::NUMBER,      // 6
            SemanticTokenType::COMMENT,     // 7
            SemanticTokenType::OPERATOR,    // 8
            SemanticTokenType::NAMESPACE,   // 9
            SemanticTokenType::EVENT,       // 10
            SemanticTokenType::MACRO,       // 11
            SemanticTokenType::ENUM_MEMBER, // 12
        ],
        token_modifiers: vec![
            SemanticTokenModifier::DECLARATION,     // bit 0
            SemanticTokenModifier::DEFINITION,      // bit 1
            SemanticTokenModifier::READONLY,        // bit 2
            SemanticTokenModifier::DEFAULT_LIBRARY, // bit 3
        ],
    }
}

#[derive(Clone, Copy)]
enum NameKind {
    Function,
    Type,
    Parameter,
    Variable,
    Constant,
    EventName,
    Property,
}

pub fn semantic_tokens(source: &str, _file_path: &Path) -> Vec<SemanticToken> {
    let (tokens, comments, _) = Lexer::new(source, 0).tokenize();

    // Try parsing to get name classification
    let name_kinds = match crate::parse_source_silent(source, "") {
        Ok(file) => build_name_kinds(&file),
        Err(_) => BTreeMap::new(),
    };

    let builtin_names: std::collections::HashSet<String> =
        builtin_completions().into_iter().map(|(n, _)| n).collect();

    // Merge lexeme tokens and comments by byte position
    let mut raw: Vec<(Span, u32, u32)> = Vec::new();

    for tok in &tokens {
        if let Some((tt, mods)) = classify_lexeme(&tok.node, &name_kinds, &builtin_names) {
            raw.push((tok.span, tt, mods));
        }
    }

    for comment in &comments {
        raw.push((comment.span, TT_COMMENT, 0));
    }

    // Sort by byte position
    raw.sort_by_key(|(span, _, _)| span.start);

    // Encode as delta positions
    encode_deltas(source, &raw)
}

fn classify_lexeme(
    lexeme: &Lexeme,
    name_kinds: &BTreeMap<String, (NameKind, u32)>,
    builtins: &std::collections::HashSet<String>,
) -> Option<(u32, u32)> {
    match lexeme {
        // Keywords
        Lexeme::Program
        | Lexeme::Module
        | Lexeme::Use
        | Lexeme::Fn
        | Lexeme::Pub
        | Lexeme::Sec
        | Lexeme::Let
        | Lexeme::Mut
        | Lexeme::Const
        | Lexeme::Struct
        | Lexeme::If
        | Lexeme::Else
        | Lexeme::For
        | Lexeme::In
        | Lexeme::Bounded
        | Lexeme::Return
        | Lexeme::Event
        | Lexeme::Reveal
        | Lexeme::Seal
        | Lexeme::Match => Some((TT_KEYWORD, 0)),
        Lexeme::True | Lexeme::False => Some((TT_ENUM_MEMBER, 0)),

        // Type keywords
        Lexeme::FieldTy | Lexeme::XFieldTy | Lexeme::BoolTy | Lexeme::U32Ty | Lexeme::DigestTy => {
            Some((TT_TYPE, MOD_DEFAULT_LIBRARY))
        }

        // Literals
        Lexeme::Integer(_) => Some((TT_NUMBER, 0)),

        // Identifiers — classify by name
        Lexeme::Ident(name) => {
            if let Some((kind, mods)) = name_kinds.get(name.as_str()) {
                let tt = match kind {
                    NameKind::Function => TT_FUNCTION,
                    NameKind::Type => TT_TYPE,
                    NameKind::Parameter => TT_PARAMETER,
                    NameKind::Variable => TT_VARIABLE,
                    NameKind::Constant => TT_VARIABLE,
                    NameKind::EventName => TT_EVENT,
                    NameKind::Property => TT_PROPERTY,
                };
                Some((tt, *mods))
            } else if builtins.contains(name) {
                Some((TT_FUNCTION, MOD_DEFAULT_LIBRARY))
            } else {
                Some((TT_VARIABLE, 0))
            }
        }

        // Operators
        Lexeme::Plus
        | Lexeme::Star
        | Lexeme::StarDot
        | Lexeme::EqEq
        | Lexeme::Lt
        | Lexeme::Gt
        | Lexeme::Amp
        | Lexeme::Caret
        | Lexeme::SlashPercent
        | Lexeme::Eq
        | Lexeme::Arrow
        | Lexeme::FatArrow
        | Lexeme::DotDot => Some((TT_OPERATOR, 0)),

        // Hash (attribute marker)
        Lexeme::Hash => Some((TT_MACRO, 0)),

        // Asm block keyword
        Lexeme::AsmBlock { .. } => Some((TT_KEYWORD, 0)),

        // Punctuation/delimiters — skip (editor handles these)
        Lexeme::LParen
        | Lexeme::RParen
        | Lexeme::LBrace
        | Lexeme::RBrace
        | Lexeme::LBracket
        | Lexeme::RBracket
        | Lexeme::Comma
        | Lexeme::Colon
        | Lexeme::Semicolon
        | Lexeme::Dot
        | Lexeme::Underscore
        | Lexeme::Eof => None,
    }
}

fn build_name_kinds(file: &File) -> BTreeMap<String, (NameKind, u32)> {
    let mut kinds = BTreeMap::new();

    // Module/use names → namespace
    for use_path in &file.uses {
        for seg in &use_path.node.0 {
            kinds.insert(seg.clone(), (NameKind::Variable, 0));
        }
    }

    for item in &file.items {
        match &item.node {
            Item::Fn(f) => {
                kinds.insert(f.name.node.clone(), (NameKind::Function, MOD_DECLARATION));
                for p in &f.params {
                    kinds.insert(p.name.node.clone(), (NameKind::Parameter, 0));
                }
                if let Some(body) = &f.body {
                    collect_block_names(&body.node, &mut kinds);
                }
            }
            Item::Struct(s) => {
                kinds.insert(s.name.node.clone(), (NameKind::Type, MOD_DECLARATION));
                for field in &s.fields {
                    kinds.insert(field.name.node.clone(), (NameKind::Property, 0));
                }
            }
            Item::Event(e) => {
                kinds.insert(e.name.node.clone(), (NameKind::EventName, MOD_DECLARATION));
                for field in &e.fields {
                    kinds.insert(field.name.node.clone(), (NameKind::Property, 0));
                }
            }
            Item::Const(c) => {
                kinds.insert(
                    c.name.node.clone(),
                    (NameKind::Constant, MOD_DECLARATION | MOD_READONLY),
                );
            }
        }
    }

    kinds
}

fn collect_block_names(block: &Block, kinds: &mut BTreeMap<String, (NameKind, u32)>) {
    for stmt in &block.stmts {
        match &stmt.node {
            Stmt::Let { pattern, .. } => match pattern {
                crate::ast::Pattern::Name(name) => {
                    kinds.insert(name.node.clone(), (NameKind::Variable, MOD_DEFINITION));
                }
                crate::ast::Pattern::Tuple(names) => {
                    for name in names {
                        kinds.insert(name.node.clone(), (NameKind::Variable, MOD_DEFINITION));
                    }
                }
            },
            Stmt::If {
                then_block,
                else_block,
                ..
            } => {
                collect_block_names(&then_block.node, kinds);
                if let Some(eb) = else_block {
                    collect_block_names(&eb.node, kinds);
                }
            }
            Stmt::For { var, body, .. } => {
                kinds.insert(var.node.clone(), (NameKind::Variable, MOD_DEFINITION));
                collect_block_names(&body.node, kinds);
            }
            Stmt::Match { arms, .. } => {
                for arm in arms {
                    collect_block_names(&arm.body.node, kinds);
                }
            }
            _ => {}
        }
    }
}

fn encode_deltas(source: &str, raw: &[(Span, u32, u32)]) -> Vec<SemanticToken> {
    let line_starts = compute_line_starts(source);
    let mut result = Vec::with_capacity(raw.len());
    let mut prev_line: u32 = 0;
    let mut prev_col: u32 = 0;

    for &(span, token_type, modifiers) in raw {
        let start = span.start as usize;
        let end = span.end as usize;
        if start >= source.len() || end > source.len() || start >= end {
            continue;
        }

        let line = line_starts
            .partition_point(|&offset| offset <= start)
            .saturating_sub(1) as u32;
        let line_start = line_starts[line as usize];
        let col = (start - line_start) as u32;
        let length = (end - start) as u32;

        let delta_line = line - prev_line;
        let delta_start = if delta_line == 0 { col - prev_col } else { col };

        result.push(SemanticToken {
            delta_line,
            delta_start,
            length,
            token_type,
            token_modifiers_bitset: modifiers,
        });

        prev_line = line;
        prev_col = col;
    }

    result
}

fn compute_line_starts(source: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (i, b) in source.bytes().enumerate() {
        if b == b'\n' {
            starts.push(i + 1);
        }
    }
    starts
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn legend_has_all_types() {
        let legend = token_legend();
        assert_eq!(legend.token_types.len(), 13);
        assert_eq!(legend.token_modifiers.len(), 4);
    }

    #[test]
    fn simple_program_tokens() {
        let source = "program test\nfn main() {\n  let x: Field = 42\n}\n";
        let tokens = semantic_tokens(source, &PathBuf::from("test.tri"));
        assert!(!tokens.is_empty());
        // First token: "program" keyword
        assert_eq!(tokens[0].token_type, TT_KEYWORD);
        assert_eq!(tokens[0].delta_line, 0);
        assert_eq!(tokens[0].delta_start, 0);
        assert_eq!(tokens[0].length, 7);
    }

    #[test]
    fn builtin_classified_as_function() {
        let source = "program test\nfn main() {\n  let x: Field = pub_read()\n}\n";
        let tokens = semantic_tokens(source, &PathBuf::from("test.tri"));
        let pub_read = tokens.iter().find(|t| t.length == 8);
        assert!(pub_read.is_some());
        let pr = pub_read.unwrap();
        assert_eq!(pr.token_type, TT_FUNCTION);
        assert_ne!(pr.token_modifiers_bitset & MOD_DEFAULT_LIBRARY, 0);
    }

    #[test]
    fn struct_name_classified_as_type() {
        let source = "module std.test\npub struct Point {\n  x: Field,\n  y: Field,\n}\n";
        let tokens = semantic_tokens(source, &PathBuf::from("test.tri"));
        let point = tokens
            .iter()
            .find(|t| t.length == 5 && t.token_type == TT_TYPE);
        assert!(point.is_some());
    }

    #[test]
    fn comments_included() {
        let source = "program test\n// this is a comment\nfn main() {}\n";
        let tokens = semantic_tokens(source, &PathBuf::from("test.tri"));
        let comment = tokens.iter().find(|t| t.token_type == TT_COMMENT);
        assert!(comment.is_some());
    }
}
