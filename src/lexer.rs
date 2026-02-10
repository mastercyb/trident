use crate::diagnostic::Diagnostic;
use crate::lexeme::Lexeme;
use crate::span::{Span, Spanned};

/// A source comment preserved for the formatter.
#[derive(Clone, Debug)]
pub struct Comment {
    pub text: String, // includes the "//" prefix
    pub span: Span,
    pub trailing: bool, // true if a token appeared earlier on the same line
}

pub struct Lexer<'src> {
    source: &'src [u8],
    file_id: u16,
    pos: usize,
    diagnostics: Vec<Diagnostic>,
    comments: Vec<Comment>,
    /// Whether we've seen a non-whitespace token on the current line.
    token_on_line: bool,
}

impl<'src> Lexer<'src> {
    pub fn new(source: &'src str, file_id: u16) -> Self {
        Self {
            source: source.as_bytes(),
            file_id,
            pos: 0,
            diagnostics: Vec::new(),
            comments: Vec::new(),
            token_on_line: false,
        }
    }

    pub fn tokenize(mut self) -> (Vec<Spanned<Lexeme>>, Vec<Comment>, Vec<Diagnostic>) {
        let mut tokens = Vec::new();
        loop {
            let tok = self.next_token();
            let is_eof = tok.node == Lexeme::Eof;
            tokens.push(tok);
            if is_eof {
                break;
            }
        }
        (tokens, self.comments, self.diagnostics)
    }

    fn next_token(&mut self) -> Spanned<Lexeme> {
        loop {
            self.skip_whitespace_and_comments();

            if self.pos >= self.source.len() {
                return self.make_token(Lexeme::Eof, self.pos, self.pos);
            }

            let start = self.pos;
            let ch = self.source[self.pos];

            self.token_on_line = true;

            // Identifiers and keywords
            if is_ident_start(ch) {
                return self.scan_ident_or_keyword();
            }

            // Integer literals
            if ch.is_ascii_digit() {
                return self.scan_number();
            }

            // Symbols
            if let Some(tok) = self.scan_symbol(start) {
                return tok;
            }
            // scan_symbol returned None → error was recorded, try again
        }
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            // Skip whitespace, tracking newlines
            while self.pos < self.source.len() && self.source[self.pos].is_ascii_whitespace() {
                if self.source[self.pos] == b'\n' {
                    self.token_on_line = false;
                }
                self.pos += 1;
            }

            // Collect line comments
            if self.pos + 1 < self.source.len()
                && self.source[self.pos] == b'/'
                && self.source[self.pos + 1] == b'/'
            {
                let start = self.pos;
                while self.pos < self.source.len() && self.source[self.pos] != b'\n' {
                    self.pos += 1;
                }
                let text = std::str::from_utf8(&self.source[start..self.pos])
                    .unwrap()
                    .to_string();
                self.comments.push(Comment {
                    text,
                    span: Span::new(self.file_id, start as u32, self.pos as u32),
                    trailing: self.token_on_line,
                });
                continue;
            }

            break;
        }
    }

    fn scan_ident_or_keyword(&mut self) -> Spanned<Lexeme> {
        let start = self.pos;
        while self.pos < self.source.len() && is_ident_continue(self.source[self.pos]) {
            self.pos += 1;
        }
        let text = std::str::from_utf8(&self.source[start..self.pos]).unwrap();
        if text == "asm" {
            return self.scan_asm_block(start);
        }
        let token = Lexeme::from_keyword(text).unwrap_or_else(|| Lexeme::Ident(text.to_string()));
        self.make_token(token, start, self.pos)
    }

    /// Scan an inline asm block: `asm { ... }` or `asm(+N) { ... }` or `asm(-N) { ... }`.
    /// Collects the raw body between braces as a single token.
    fn scan_asm_block(&mut self, start: usize) -> Spanned<Lexeme> {
        // Skip whitespace
        while self.pos < self.source.len() && self.source[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }

        // Optional effect annotation: (+N) or (-N)
        let mut effect: i32 = 0;
        if self.pos < self.source.len() && self.source[self.pos] == b'(' {
            self.pos += 1; // skip '('
                           // Parse sign and digits
            let neg = if self.pos < self.source.len() && self.source[self.pos] == b'-' {
                self.pos += 1;
                true
            } else {
                if self.pos < self.source.len() && self.source[self.pos] == b'+' {
                    self.pos += 1;
                }
                false
            };
            let num_start = self.pos;
            while self.pos < self.source.len() && self.source[self.pos].is_ascii_digit() {
                self.pos += 1;
            }
            let num_text = std::str::from_utf8(&self.source[num_start..self.pos]).unwrap();
            let n: i32 = num_text.parse().unwrap_or(0);
            effect = if neg { -n } else { n };
            // Expect ')'
            if self.pos < self.source.len() && self.source[self.pos] == b')' {
                self.pos += 1;
            } else {
                self.diagnostics.push(
                    Diagnostic::error(
                        "expected ')' after asm stack effect annotation".to_string(),
                        Span::new(self.file_id, self.pos as u32, self.pos as u32),
                    )
                    .with_help(
                        "asm effect annotations look like `asm(+1) { ... }` or `asm(-2) { ... }`"
                            .to_string(),
                    ),
                );
            }
            // Skip whitespace after annotation
            while self.pos < self.source.len() && self.source[self.pos].is_ascii_whitespace() {
                self.pos += 1;
            }
        }

        // Expect '{'
        if self.pos >= self.source.len() || self.source[self.pos] != b'{' {
            self.diagnostics.push(Diagnostic::error(
                "expected '{' after `asm` keyword".to_string(),
                Span::new(self.file_id, self.pos as u32, self.pos as u32),
            ).with_help("inline assembly syntax is `asm { instructions }` or `asm(+N) { instructions }`".to_string()));
            return self.make_token(
                Lexeme::AsmBlock {
                    body: String::new(),
                    effect,
                },
                start,
                self.pos,
            );
        }
        self.pos += 1; // skip '{'

        // Collect raw bytes until matching '}', tracking brace depth
        let body_start = self.pos;
        let mut depth = 1u32;
        while self.pos < self.source.len() && depth > 0 {
            match self.source[self.pos] {
                b'{' => depth += 1,
                b'}' => depth -= 1,
                _ => {}
            }
            if depth > 0 {
                self.pos += 1;
            }
        }
        let body = std::str::from_utf8(&self.source[body_start..self.pos])
            .unwrap()
            .trim()
            .to_string();

        if self.pos < self.source.len() {
            self.pos += 1; // skip closing '}'
        } else {
            self.diagnostics.push(
                Diagnostic::error(
                    "unterminated asm block: missing closing '}'".to_string(),
                    Span::new(self.file_id, start as u32, self.pos as u32),
                )
                .with_help(
                    "every `asm { ... }` block must have a matching closing brace".to_string(),
                ),
            );
        }

        self.make_token(Lexeme::AsmBlock { body, effect }, start, self.pos)
    }

    fn scan_number(&mut self) -> Spanned<Lexeme> {
        let start = self.pos;
        while self.pos < self.source.len() && self.source[self.pos].is_ascii_digit() {
            self.pos += 1;
        }
        let text = std::str::from_utf8(&self.source[start..self.pos]).unwrap();
        match text.parse::<u64>() {
            Ok(n) => self.make_token(Lexeme::Integer(n), start, self.pos),
            Err(_) => {
                self.diagnostics.push(
                    Diagnostic::error(
                        format!("integer literal '{}' is too large", text),
                        Span::new(self.file_id, start as u32, self.pos as u32),
                    )
                    .with_help(format!("maximum integer value is {}", u64::MAX)),
                );
                self.make_token(Lexeme::Integer(0), start, self.pos)
            }
        }
    }

    fn scan_symbol(&mut self, start: usize) -> Option<Spanned<Lexeme>> {
        let ch = self.source[self.pos];
        self.pos += 1;

        let token = match ch {
            b'(' => Lexeme::LParen,
            b')' => Lexeme::RParen,
            b'{' => Lexeme::LBrace,
            b'}' => Lexeme::RBrace,
            b'[' => Lexeme::LBracket,
            b']' => Lexeme::RBracket,
            b',' => Lexeme::Comma,
            b':' => Lexeme::Colon,
            b';' => Lexeme::Semicolon,
            b'+' => Lexeme::Plus,
            b'<' => Lexeme::Lt,
            b'>' => Lexeme::Gt,
            b'&' => Lexeme::Amp,
            b'^' => Lexeme::Caret,
            b'#' => Lexeme::Hash,
            b'.' => {
                if self.peek() == Some(b'.') {
                    self.pos += 1;
                    Lexeme::DotDot
                } else {
                    Lexeme::Dot
                }
            }
            b'-' => {
                if self.peek() == Some(b'>') {
                    self.pos += 1;
                    Lexeme::Arrow
                } else {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "unexpected '-'; Trident has no subtraction operator".to_string(),
                            Span::new(self.file_id, start as u32, self.pos as u32),
                        )
                        .with_help("use the `sub(a, b)` function instead of `a - b`".to_string()),
                    );
                    return None;
                }
            }
            b'=' => {
                if self.peek() == Some(b'=') {
                    self.pos += 1;
                    Lexeme::EqEq
                } else if self.peek() == Some(b'>') {
                    self.pos += 1;
                    Lexeme::FatArrow
                } else {
                    Lexeme::Eq
                }
            }
            b'*' => {
                if self.peek() == Some(b'.') {
                    self.pos += 1;
                    Lexeme::StarDot
                } else {
                    Lexeme::Star
                }
            }
            b'/' => {
                if self.peek() == Some(b'%') {
                    self.pos += 1;
                    Lexeme::SlashPercent
                } else {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "unexpected '/'; Trident has no division operator".to_string(),
                            Span::new(self.file_id, start as u32, self.pos as u32),
                        )
                        .with_help(
                            "use the `/% (divmod)` operator instead: `let (quot, rem) = a /% b`"
                                .to_string(),
                        ),
                    );
                    return None;
                }
            }
            b'_' => {
                // Could be start of identifier like _foo, or standalone underscore
                if self.pos < self.source.len() && is_ident_continue(self.source[self.pos]) {
                    // Back up and scan as identifier
                    self.pos = start;
                    return Some(self.scan_ident_or_keyword());
                }
                Lexeme::Underscore
            }
            _ => {
                self.diagnostics.push(
                    Diagnostic::error(
                        format!("unexpected character '{}' (U+{:04X})", ch as char, ch),
                        Span::new(self.file_id, start as u32, self.pos as u32),
                    )
                    .with_help(
                        "this character is not recognized as part of Trident syntax".to_string(),
                    ),
                );
                return None;
            }
        };

        Some(self.make_token(token, start, self.pos))
    }

    fn peek(&self) -> Option<u8> {
        if self.pos < self.source.len() {
            Some(self.source[self.pos])
        } else {
            None
        }
    }

    fn make_token(&self, token: Lexeme, start: usize, end: usize) -> Spanned<Lexeme> {
        Spanned::new(token, Span::new(self.file_id, start as u32, end as u32))
    }
}

fn is_ident_start(ch: u8) -> bool {
    ch.is_ascii_alphabetic() || ch == b'_'
}

fn is_ident_continue(ch: u8) -> bool {
    ch.is_ascii_alphanumeric() || ch == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(source: &str) -> Vec<Lexeme> {
        let (tokens, _comments, diags) = Lexer::new(source, 0).tokenize();
        assert!(diags.is_empty(), "unexpected errors: {:?}", diags);
        tokens.into_iter().map(|t| t.node).collect()
    }

    #[test]
    fn test_keywords() {
        let tokens = lex("program fn let mut pub if else for in bounded return");
        assert_eq!(
            tokens,
            vec![
                Lexeme::Program,
                Lexeme::Fn,
                Lexeme::Let,
                Lexeme::Mut,
                Lexeme::Pub,
                Lexeme::If,
                Lexeme::Else,
                Lexeme::For,
                Lexeme::In,
                Lexeme::Bounded,
                Lexeme::Return,
                Lexeme::Eof,
            ]
        );
    }

    #[test]
    fn test_types() {
        let tokens = lex("Field XField Bool U32 Digest");
        assert_eq!(
            tokens,
            vec![
                Lexeme::FieldTy,
                Lexeme::XFieldTy,
                Lexeme::BoolTy,
                Lexeme::U32Ty,
                Lexeme::DigestTy,
                Lexeme::Eof,
            ]
        );
    }

    #[test]
    fn test_symbols() {
        let tokens = lex("( ) { } [ ] , : ; . .. -> = == + * *. < & ^ /% #");
        assert_eq!(
            tokens,
            vec![
                Lexeme::LParen,
                Lexeme::RParen,
                Lexeme::LBrace,
                Lexeme::RBrace,
                Lexeme::LBracket,
                Lexeme::RBracket,
                Lexeme::Comma,
                Lexeme::Colon,
                Lexeme::Semicolon,
                Lexeme::Dot,
                Lexeme::DotDot,
                Lexeme::Arrow,
                Lexeme::Eq,
                Lexeme::EqEq,
                Lexeme::Plus,
                Lexeme::Star,
                Lexeme::StarDot,
                Lexeme::Lt,
                Lexeme::Amp,
                Lexeme::Caret,
                Lexeme::SlashPercent,
                Lexeme::Hash,
                Lexeme::Eof,
            ]
        );
    }

    #[test]
    fn test_integers() {
        let tokens = lex("0 1 42 18446744073709551615");
        assert_eq!(
            tokens,
            vec![
                Lexeme::Integer(0),
                Lexeme::Integer(1),
                Lexeme::Integer(42),
                Lexeme::Integer(u64::MAX),
                Lexeme::Eof,
            ]
        );
    }

    #[test]
    fn test_identifiers() {
        let tokens = lex("foo bar_baz x1 _underscore");
        assert_eq!(
            tokens,
            vec![
                Lexeme::Ident("foo".into()),
                Lexeme::Ident("bar_baz".into()),
                Lexeme::Ident("x1".into()),
                Lexeme::Ident("_underscore".into()),
                Lexeme::Eof,
            ]
        );
    }

    #[test]
    fn test_comments() {
        let tokens = lex("foo // this is a comment\nbar");
        assert_eq!(
            tokens,
            vec![
                Lexeme::Ident("foo".into()),
                Lexeme::Ident("bar".into()),
                Lexeme::Eof,
            ]
        );
    }

    #[test]
    fn test_simple_program() {
        let tokens = lex("program test\n\nfn main() {\n    let a: Field = pub_read()\n}");
        assert_eq!(tokens[0], Lexeme::Program);
        assert_eq!(tokens[1], Lexeme::Ident("test".into()));
        assert_eq!(tokens[2], Lexeme::Fn);
        assert_eq!(tokens[3], Lexeme::Ident("main".into()));
    }

    #[test]
    fn test_event_keywords() {
        let tokens = lex("event emit seal");
        assert_eq!(
            tokens,
            vec![Lexeme::Event, Lexeme::Emit, Lexeme::Seal, Lexeme::Eof,]
        );
    }

    #[test]
    fn test_asm_block_basic() {
        let tokens = lex("asm { push 1\nadd }");
        assert_eq!(
            tokens,
            vec![
                Lexeme::AsmBlock {
                    body: "push 1\nadd".to_string(),
                    effect: 0,
                },
                Lexeme::Eof,
            ]
        );
    }

    #[test]
    fn test_asm_block_positive_effect() {
        let tokens = lex("asm(+1) { push 42 }");
        assert_eq!(
            tokens,
            vec![
                Lexeme::AsmBlock {
                    body: "push 42".to_string(),
                    effect: 1,
                },
                Lexeme::Eof,
            ]
        );
    }

    #[test]
    fn test_asm_block_negative_effect() {
        let tokens = lex("asm(-2) { pop 1\npop 1 }");
        assert_eq!(
            tokens,
            vec![
                Lexeme::AsmBlock {
                    body: "pop 1\npop 1".to_string(),
                    effect: -2,
                },
                Lexeme::Eof,
            ]
        );
    }

    #[test]
    fn test_asm_block_with_negative_literal() {
        // Raw TASM can contain `push -1` which is NOT valid Trident
        let tokens = lex("asm { push -1\nadd }");
        assert_eq!(
            tokens,
            vec![
                Lexeme::AsmBlock {
                    body: "push -1\nadd".to_string(),
                    effect: 0,
                },
                Lexeme::Eof,
            ]
        );
    }

    #[test]
    fn test_asm_block_in_function() {
        // fn main() { asm { ... } }
        // Tokens: Fn, Ident("main"), LParen, RParen, LBrace, AsmBlock, RBrace, Eof
        let tokens = lex("fn main() {\n    asm { dup 0\nadd }\n}");
        assert_eq!(tokens[0], Lexeme::Fn);
        assert!(matches!(tokens[5], Lexeme::AsmBlock { .. }));
        assert_eq!(tokens[6], Lexeme::RBrace);
    }

    #[test]
    fn test_match_keyword() {
        let tokens = lex("match x { 0 => { } _ => { } }");
        assert_eq!(tokens[0], Lexeme::Match);
        assert_eq!(tokens[1], Lexeme::Ident("x".into()));
        assert_eq!(tokens[2], Lexeme::LBrace);
        assert_eq!(tokens[3], Lexeme::Integer(0));
        assert_eq!(tokens[4], Lexeme::FatArrow);
        assert_eq!(tokens[5], Lexeme::LBrace);
        assert_eq!(tokens[6], Lexeme::RBrace);
        assert_eq!(tokens[7], Lexeme::Ident("_".into()));
        assert_eq!(tokens[8], Lexeme::FatArrow);
    }

    #[test]
    fn test_fat_arrow_vs_eq() {
        let tokens = lex("= => ==");
        assert_eq!(
            tokens,
            vec![Lexeme::Eq, Lexeme::FatArrow, Lexeme::EqEq, Lexeme::Eof]
        );
    }

    // --- Error path tests ---

    fn lex_with_errors(source: &str) -> (Vec<Lexeme>, Vec<Diagnostic>) {
        let (tokens, _comments, diags) = Lexer::new(source, 0).tokenize();
        let lexemes = tokens.into_iter().map(|t| t.node).collect();
        (lexemes, diags)
    }

    #[test]
    fn test_error_unexpected_character() {
        let (_tokens, diags) = lex_with_errors("@");
        assert!(!diags.is_empty(), "should produce an error for '@'");
        assert!(
            diags[0].message.contains("unexpected character '@'"),
            "error should name the character, got: {}",
            diags[0].message
        );
        assert!(
            diags[0].help.is_some(),
            "unexpected character error should have help text"
        );
    }

    #[test]
    fn test_error_subtraction_operator() {
        let (_tokens, diags) = lex_with_errors("a - b");
        assert!(!diags.is_empty(), "should produce an error for '-'");
        assert!(
            diags[0].message.contains("no subtraction operator"),
            "should explain there is no subtraction, got: {}",
            diags[0].message
        );
        assert!(
            diags[0].help.as_deref().unwrap().contains("sub(a, b)"),
            "help should suggest sub() function"
        );
    }

    #[test]
    fn test_error_division_operator() {
        let (_tokens, diags) = lex_with_errors("a / b");
        assert!(!diags.is_empty(), "should produce an error for '/'");
        assert!(
            diags[0].message.contains("no division operator"),
            "should explain there is no division, got: {}",
            diags[0].message
        );
        assert!(
            diags[0].help.as_deref().unwrap().contains("/%"),
            "help should suggest /% operator"
        );
    }

    #[test]
    fn test_error_integer_too_large() {
        let (_tokens, diags) = lex_with_errors("99999999999999999999999");
        assert!(
            !diags.is_empty(),
            "should produce an error for huge integer"
        );
        assert!(
            diags[0].message.contains("too large"),
            "should say the integer is too large, got: {}",
            diags[0].message
        );
        assert!(
            diags[0].help.is_some(),
            "integer overflow error should have help text"
        );
    }

    #[test]
    fn test_error_unterminated_asm_block() {
        let (_tokens, diags) = lex_with_errors("asm { push 1");
        assert!(
            !diags.is_empty(),
            "should produce an error for unterminated asm"
        );
        assert!(
            diags[0].message.contains("unterminated asm block"),
            "should report unterminated asm, got: {}",
            diags[0].message
        );
        assert!(
            diags[0].help.is_some(),
            "unterminated asm error should have help text"
        );
    }
}
