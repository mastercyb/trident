use crate::diagnostic::Diagnostic;
use crate::lexeme::Lexeme;
use crate::span::{Span, Spanned};

/// A source comment preserved for the formatter.
#[derive(Clone, Debug)]
pub(crate) struct Comment {
    pub(crate) text: String, // includes the "//" prefix
    pub(crate) span: Span,
    pub(crate) trailing: bool, // true if a token appeared earlier on the same line
}

pub(crate) struct Lexer<'src> {
    source: &'src [u8],
    file_id: u16,
    pos: usize,
    diagnostics: Vec<Diagnostic>,
    comments: Vec<Comment>,
    /// Whether we've seen a non-whitespace token on the current line.
    token_on_line: bool,
}

impl<'src> Lexer<'src> {
    pub(crate) fn new(source: &'src str, file_id: u16) -> Self {
        Self {
            source: source.as_bytes(),
            file_id,
            pos: 0,
            diagnostics: Vec::new(),
            comments: Vec::new(),
            token_on_line: false,
        }
    }

    /// Create a lexer that starts at the given byte offset.
    /// Used for incremental re-lexing of dirty regions.
    pub(crate) fn new_from_offset(source: &'src str, file_id: u16, offset: usize) -> Self {
        Self {
            source: source.as_bytes(),
            file_id,
            pos: offset,
            diagnostics: Vec::new(),
            comments: Vec::new(),
            token_on_line: false,
        }
    }

    /// Lex a single token and advance. Exposed for incremental lexing.
    pub(crate) fn next_spanned(&mut self) -> Spanned<Lexeme> {
        self.next_token()
    }

    /// Drain accumulated comments from the lexer.
    pub(crate) fn take_comments(&mut self) -> Vec<Comment> {
        std::mem::take(&mut self.comments)
    }

    /// Drain accumulated diagnostics from the lexer.
    #[allow(dead_code)] // reserved for incremental diagnostic support
    pub(crate) fn take_diagnostics(&mut self) -> Vec<Diagnostic> {
        std::mem::take(&mut self.diagnostics)
    }

    pub(crate) fn tokenize(mut self) -> (Vec<Spanned<Lexeme>>, Vec<Comment>, Vec<Diagnostic>) {
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

    /// Scan an inline asm block. Supported forms:
    /// - `asm { ... }` — bare block, no target or effect
    /// - `asm(+N) { ... }` / `asm(-N) { ... }` — stack effect annotation
    /// - `asm(triton) { ... }` — target-tagged block
    /// - `asm(triton, +N) { ... }` — target tag + stack effect
    fn scan_asm_block(&mut self, start: usize) -> Spanned<Lexeme> {
        // Skip whitespace
        while self.pos < self.source.len() && self.source[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }

        // Optional parenthesized annotation: target tag and/or stack effect
        let mut effect: i32 = 0;
        let mut target: Option<String> = None;
        if self.pos < self.source.len() && self.source[self.pos] == b'(' {
            self.pos += 1; // skip '('

            // Skip whitespace inside parens
            while self.pos < self.source.len() && self.source[self.pos].is_ascii_whitespace() {
                self.pos += 1;
            }

            // Determine what's inside: identifier (target) or +/-N (effect)
            if self.pos < self.source.len() && self.source[self.pos].is_ascii_alphabetic() {
                // Target tag: scan identifier
                let tag_start = self.pos;
                while self.pos < self.source.len() && is_ident_continue(self.source[self.pos]) {
                    self.pos += 1;
                }
                let tag = std::str::from_utf8(&self.source[tag_start..self.pos])
                    .unwrap()
                    .to_string();
                target = Some(tag);

                // Skip whitespace
                while self.pos < self.source.len() && self.source[self.pos].is_ascii_whitespace() {
                    self.pos += 1;
                }

                // Optional comma + effect after target
                if self.pos < self.source.len() && self.source[self.pos] == b',' {
                    self.pos += 1; // skip ','
                    while self.pos < self.source.len()
                        && self.source[self.pos].is_ascii_whitespace()
                    {
                        self.pos += 1;
                    }
                    effect = self.scan_effect_number();
                }
            } else {
                // Stack effect: +N or -N
                effect = self.scan_effect_number();
            }

            // Expect ')'
            while self.pos < self.source.len() && self.source[self.pos].is_ascii_whitespace() {
                self.pos += 1;
            }
            if self.pos < self.source.len() && self.source[self.pos] == b')' {
                self.pos += 1;
            } else {
                self.diagnostics.push(
                    Diagnostic::error(
                        "expected ')' after asm annotation".to_string(),
                        Span::new(self.file_id, self.pos as u32, self.pos as u32),
                    )
                    .with_help(
                        "asm annotations: `asm(+1) { ... }`, `asm(triton) { ... }`, or `asm(triton, +1) { ... }`"
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
            ).with_help("inline assembly syntax is `asm { instructions }` or `asm(triton) { instructions }`".to_string()));
            return self.make_token(
                Lexeme::AsmBlock {
                    body: String::new(),
                    effect,
                    target,
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

        self.make_token(
            Lexeme::AsmBlock {
                body,
                effect,
                target,
            },
            start,
            self.pos,
        )
    }

    /// Parse a stack effect number: `+N`, `-N`, or just `N`.
    fn scan_effect_number(&mut self) -> i32 {
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
        if neg {
            -n
        } else {
            n
        }
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
mod tests;
