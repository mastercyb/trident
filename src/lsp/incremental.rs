//! Incremental lexing: re-lex only the dirty byte region after an edit,
//! then splice the new tokens into the cached token list.

use crate::syntax::lexeme::Lexeme;
use crate::syntax::lexer::{Comment, Lexer};
use crate::syntax::span::{Span, Spanned};

/// Result of an incremental lex operation.
pub(super) struct IncrementalLexResult {
    pub tokens: Vec<Spanned<Lexeme>>,
    pub comments: Vec<Comment>,
}

/// Re-lex only the changed region and splice into the old token list.
///
/// - `source`: the NEW source text (edit already applied)
/// - `old_tokens`: previous token list (sorted by span.start)
/// - `old_comments`: previous comment list
/// - `edit_start`: byte offset where the edit begins (in the OLD source)
/// - `old_end`: byte offset where the edit ends (in the OLD source)
/// - `new_end`: byte offset where the edit ends (in the NEW source)
pub(super) fn incremental_lex(
    source: &str,
    old_tokens: &[Spanned<Lexeme>],
    old_comments: &[Comment],
    edit_start: usize,
    old_end: usize,
    new_end: usize,
) -> IncrementalLexResult {
    let delta: i64 = new_end as i64 - old_end as i64;

    // 1. Find first affected token: first whose span.end > edit_start
    let first_dirty = old_tokens.partition_point(|t| (t.span.end as usize) <= edit_start);

    // 2. Find resync search start: first token whose span.start >= old_end
    let resync_search_start = old_tokens.partition_point(|t| (t.span.start as usize) < old_end);

    // 3. Start re-lexing from the earlier of first_dirty's start or edit_start
    let relex_start = if first_dirty < old_tokens.len() {
        (old_tokens[first_dirty].span.start as usize).min(edit_start)
    } else {
        edit_start
    };

    // 4. Lex from relex_start in the new source
    let mut lexer = Lexer::new_from_offset(source, 0, relex_start);
    let mut new_tokens_mid: Vec<Spanned<Lexeme>> = Vec::new();
    let mut resync_idx: Option<usize> = None;

    loop {
        let tok = lexer.next_spanned();
        let is_eof = tok.node == Lexeme::Eof;

        if !is_eof {
            // Try to resynchronize against old tokens past the edit region
            if let Some(idx) = find_resync(&tok, old_tokens, resync_search_start, delta) {
                resync_idx = Some(idx);
                // Don't push the resync token — the suffix starts at idx
                // (the shifted old token is identical to this new one)
                break;
            }
        }

        new_tokens_mid.push(tok);
        if is_eof {
            break;
        }
    }

    let mid_comments = lexer.take_comments();

    // 5. Assemble: prefix + middle + suffix
    let mut result_tokens = Vec::with_capacity(old_tokens.len());
    let mut result_comments = Vec::new();

    // Prefix: tokens entirely before the edit (exclude Eof)
    for t in &old_tokens[..first_dirty] {
        if t.node != Lexeme::Eof {
            result_tokens.push(t.clone());
        }
    }

    // Prefix comments: before edit_start
    for c in old_comments {
        if (c.span.end as usize) <= edit_start {
            result_comments.push(c.clone());
        }
    }

    // Middle: newly lexed tokens + comments
    result_tokens.extend(new_tokens_mid);
    result_comments.extend(mid_comments);

    if let Some(ri) = resync_idx {
        // Suffix: shift remaining old tokens by delta
        for t in &old_tokens[ri..] {
            result_tokens.push(shift_token(t, delta));
        }
        // Suffix comments: after old_end, shifted
        for c in old_comments {
            if (c.span.start as usize) >= old_end {
                result_comments.push(shift_comment(c, delta));
            }
        }
    }
    // If no resync, the middle already includes Eof — no suffix needed.

    // Sort comments by position (prefix + middle + suffix may interleave)
    result_comments.sort_by_key(|c| c.span.start);

    IncrementalLexResult {
        tokens: result_tokens,
        comments: result_comments,
    }
}

/// Maximum tokens to scan past the expected resync point.
const RESYNC_WINDOW: usize = 8;

/// Check if `new_tok` matches an old token (after offset shift) past the edit.
fn find_resync(
    new_tok: &Spanned<Lexeme>,
    old_tokens: &[Spanned<Lexeme>],
    search_start: usize,
    delta: i64,
) -> Option<usize> {
    let limit = (search_start + RESYNC_WINDOW).min(old_tokens.len());
    for i in search_start..limit {
        let old = &old_tokens[i];
        let shifted_start = (old.span.start as i64 + delta) as u32;
        let shifted_end = (old.span.end as i64 + delta) as u32;
        if new_tok.span.start == shifted_start
            && new_tok.span.end == shifted_end
            && new_tok.node == old.node
        {
            return Some(i);
        }
    }
    None
}

fn shift_token(tok: &Spanned<Lexeme>, delta: i64) -> Spanned<Lexeme> {
    let new_start = tok.span.start as i64 + delta;
    let new_end = tok.span.end as i64 + delta;
    debug_assert!(new_start >= 0, "shift_token: start went negative");
    debug_assert!(new_end >= 0, "shift_token: end went negative");
    Spanned::new(
        tok.node.clone(),
        Span::new(tok.span.file_id, new_start as u32, new_end as u32),
    )
}

fn shift_comment(comment: &Comment, delta: i64) -> Comment {
    let new_start = comment.span.start as i64 + delta;
    let new_end = comment.span.end as i64 + delta;
    debug_assert!(new_start >= 0, "shift_comment: start went negative");
    debug_assert!(new_end >= 0, "shift_comment: end went negative");
    Comment {
        text: comment.text.clone(),
        span: Span::new(comment.span.file_id, new_start as u32, new_end as u32),
        trailing: comment.trailing,
    }
}

/// Result of classifying whether an edit is contained within a single
/// top-level item (function, struct, event, const) or crosses boundaries.
#[derive(Debug, PartialEq, Eq)]
#[allow(dead_code)] // infrastructure for Phase 2 incremental parsing
pub(super) enum EditScope {
    /// Edit falls entirely within a single top-level item body.
    /// The pair gives the token indices `[start, end)` of that item.
    SingleItem(usize, usize),
    /// Edit crosses item boundaries or is in the file header.
    Full,
}

/// Classify an edit by scanning tokens for top-level brace pairs.
///
/// Walks the token list at brace depth 0 to find item boundaries. If the
/// edit region is entirely inside one item's brace pair, returns
/// `SingleItem`; otherwise returns `Full`.
#[allow(dead_code)] // infrastructure for Phase 2 incremental parsing
pub(super) fn classify_edit_scope(
    tokens: &[Spanned<Lexeme>],
    edit_start: usize,
    edit_new_end: usize,
) -> EditScope {
    // Find top-level items by tracking brace depth.
    // An item starts at the token before a depth-0 LBrace and ends at
    // the matching RBrace (when depth returns to 0).
    let mut depth: u32 = 0;
    let mut item_start: Option<usize> = None;
    let mut brace_open_byte: usize = 0;

    // Walk backwards from the first LBrace to find the item's leading keyword.
    // For simplicity, track the token index where depth goes from 0→1.
    for (i, tok) in tokens.iter().enumerate() {
        match &tok.node {
            Lexeme::LBrace => {
                if depth == 0 {
                    // Find the item start: scan backwards past identifiers,
                    // keywords, parens, types, attributes to find the leading
                    // keyword (fn, struct, event).
                    let mut start_idx = i;
                    while start_idx > 0 {
                        let prev = &tokens[start_idx - 1].node;
                        match prev {
                            Lexeme::Fn
                            | Lexeme::Struct
                            | Lexeme::Event
                            | Lexeme::Pub
                            | Lexeme::Sec
                            | Lexeme::Const
                            | Lexeme::Mut
                            | Lexeme::Hash
                            | Lexeme::Ident(_)
                            | Lexeme::LParen
                            | Lexeme::RParen
                            | Lexeme::Comma
                            | Lexeme::Colon
                            | Lexeme::Arrow
                            | Lexeme::FieldTy
                            | Lexeme::XFieldTy
                            | Lexeme::BoolTy
                            | Lexeme::U32Ty
                            | Lexeme::DigestTy
                            | Lexeme::LBracket
                            | Lexeme::RBracket
                            | Lexeme::Semicolon
                            | Lexeme::Integer(_) => {
                                start_idx -= 1;
                            }
                            _ => break,
                        }
                    }
                    item_start = Some(start_idx);
                    brace_open_byte = tok.span.start as usize;
                }
                depth += 1;
            }
            Lexeme::RBrace => {
                if depth > 0 {
                    depth -= 1;
                }
                if depth == 0 {
                    if let Some(si) = item_start {
                        let brace_close_byte = tok.span.end as usize;
                        // Check if edit is entirely within this item's braces
                        if edit_start > brace_open_byte && edit_new_end < brace_close_byte {
                            return EditScope::SingleItem(si, i + 1);
                        }
                    }
                    item_start = None;
                }
            }
            _ => {}
        }
    }

    EditScope::Full
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full_lex(source: &str) -> (Vec<Spanned<Lexeme>>, Vec<Comment>) {
        let (tokens, comments, _) = Lexer::new(source, 0).tokenize();
        (tokens, comments)
    }

    fn token_kinds(tokens: &[Spanned<Lexeme>]) -> Vec<&Lexeme> {
        tokens.iter().map(|t| &t.node).collect()
    }

    #[test]
    fn insert_single_char() {
        let old_source = "fn main() {}";
        let new_source = "fn  main() {}"; // inserted a space
        let (old_tokens, old_comments) = full_lex(old_source);
        let (expected_tokens, _) = full_lex(new_source);

        // Edit: insert ' ' at position 2 (after "fn")
        let result = incremental_lex(new_source, &old_tokens, &old_comments, 2, 2, 3);

        assert_eq!(token_kinds(&result.tokens), token_kinds(&expected_tokens));
        // Verify spans match
        for (got, want) in result.tokens.iter().zip(expected_tokens.iter()) {
            assert_eq!(got.span.start, want.span.start);
            assert_eq!(got.span.end, want.span.end);
        }
    }

    #[test]
    fn delete_token() {
        let old_source = "fn main() { let x: Field = 42 }";
        let new_source = "fn main() { let x: Field = }";
        let (old_tokens, old_comments) = full_lex(old_source);
        let (expected_tokens, _) = full_lex(new_source);

        // Edit: delete "42 " at positions 27..30 → becomes 27..27
        let result = incremental_lex(new_source, &old_tokens, &old_comments, 27, 30, 27);

        assert_eq!(token_kinds(&result.tokens), token_kinds(&expected_tokens));
    }

    #[test]
    fn edit_at_start() {
        let old_source = "fn main() {}";
        let new_source = "pub fn main() {}";
        let (old_tokens, old_comments) = full_lex(old_source);
        let (expected_tokens, _) = full_lex(new_source);

        // Edit: insert "pub " at position 0
        let result = incremental_lex(new_source, &old_tokens, &old_comments, 0, 0, 4);

        assert_eq!(token_kinds(&result.tokens), token_kinds(&expected_tokens));
    }

    #[test]
    fn edit_at_end() {
        let old_source = "fn main() {}";
        let new_source = "fn main() {}\n";
        let (old_tokens, old_comments) = full_lex(old_source);
        let (expected_tokens, _) = full_lex(new_source);

        // Edit: insert newline at position 12
        let result = incremental_lex(new_source, &old_tokens, &old_comments, 12, 12, 13);

        assert_eq!(token_kinds(&result.tokens), token_kinds(&expected_tokens));
    }

    #[test]
    fn multiline_edit() {
        let old_source = "fn main() {\n  let x: Field = 42\n  let y: Field = 7\n}";
        let new_source = "fn main() {\n  let z: Field = 99\n}";
        let (old_tokens, old_comments) = full_lex(old_source);
        let (expected_tokens, _) = full_lex(new_source);

        // Edit: replace "let x: Field = 42\n  let y: Field = 7" with "let z: Field = 99"
        let result = incremental_lex(new_source, &old_tokens, &old_comments, 14, 49, 32);

        assert_eq!(token_kinds(&result.tokens), token_kinds(&expected_tokens));
    }

    #[test]
    fn comment_preserved() {
        let old_source = "// header\nfn main() {}";
        let new_source = "// header\nfn  main() {}"; // insert space
        let (old_tokens, old_comments) = full_lex(old_source);

        let result = incremental_lex(new_source, &old_tokens, &old_comments, 12, 12, 13);

        assert_eq!(result.comments.len(), 1);
        assert_eq!(result.comments[0].text, "// header");
    }

    #[test]
    fn edit_scope_within_function_body() {
        let source = "program test\nfn main() {\n    let x: Field = 42\n}\n";
        let (tokens, _) = full_lex(source);
        // Edit inside the function body (e.g., changing "42" to "99")
        let scope = classify_edit_scope(&tokens, 40, 42);
        assert!(
            matches!(scope, EditScope::SingleItem(_, _)),
            "edit inside fn body should be SingleItem, got {:?}",
            scope
        );
    }

    #[test]
    fn edit_scope_in_file_header() {
        let source = "program test\nuse vm.io\nfn main() {}\n";
        let (tokens, _) = full_lex(source);
        // Edit in the program declaration (changing "test")
        let scope = classify_edit_scope(&tokens, 8, 12);
        assert_eq!(scope, EditScope::Full);
    }

    #[test]
    fn edit_scope_crossing_functions() {
        let source = "program test\nfn foo() {\n    let x: Field = 1\n}\nfn bar() {\n    let y: Field = 2\n}\n";
        let (tokens, _) = full_lex(source);
        // Edit spanning from inside foo to inside bar
        let scope = classify_edit_scope(&tokens, 30, 70);
        assert_eq!(scope, EditScope::Full);
    }
}
