use crate::syntax::span::Span;

/// Token type indices (must match mod.rs constants).
const TT_KEYWORD: u32 = 0;
const TT_NUMBER: u32 = 6;
const TT_COMMENT: u32 = 7;
const TT_NAMESPACE: u32 = 9;

/// Modifier for builtin/stdlib items — distinguishes asm instructions
/// from language keywords visually.
const MOD_DEFAULT_LIBRARY: u32 = 1 << 3;

/// Known Triton VM assembly instructions.
const TRITON_INSTRUCTIONS: &[&str] = &[
    // Stack manipulation
    "push",
    "pop",
    "dup",
    "swap",
    // Arithmetic
    "add",
    "mul",
    "eq",
    "lt",
    // Bitwise
    "and",
    "or",
    "xor",
    // Math
    "div_mod",
    "invert",
    "split",
    "pow",
    "log_2_floor",
    "pop_count",
    // I/O
    "read_io",
    "write_io",
    "divine",
    // Memory
    "read_mem",
    "write_mem",
    // Hash / sponge
    "hash",
    "sponge_init",
    "sponge_absorb",
    "sponge_squeeze",
    "sponge_absorb_mem",
    // Merkle
    "merkle_step",
    "merkle_step_mem",
    // Assertions
    "assert",
    "assert_vector",
    // Control flow
    "call",
    "return",
    "recurse",
    "halt",
    "skiz",
    "nop",
    // Extension field
    "xb_mul",
    "x_invert",
    // Folding
    "xx_dot_step",
    "xb_dot_step",
];

/// Expand an AsmBlock token into sub-tokens for instruction-level highlighting.
///
/// Returns `(span, token_type, modifiers)` tuples that replace the single
/// AsmBlock token in the semantic token stream.
pub(super) fn expand_asm_tokens(
    source: &str,
    block_span: Span,
    _body: &str,
    _effect: i32,
    _target: &Option<String>,
) -> Vec<(Span, u32, u32)> {
    let mut tokens = Vec::new();
    let src = source.as_bytes();
    let start = block_span.start as usize;
    let end = block_span.end as usize;
    if start >= source.len() || end > source.len() {
        return tokens;
    }

    let region = &src[start..end];
    let mut pos = 0;

    // 1. `asm` keyword (always first 3 bytes)
    if region.len() >= 3 && &region[..3] == b"asm" {
        tokens.push((
            Span::new(0, block_span.start, block_span.start + 3),
            TT_KEYWORD,
            0,
        ));
        pos = 3;
    }

    // Skip whitespace
    while pos < region.len() && region[pos].is_ascii_whitespace() {
        pos += 1;
    }

    // 2. Optional annotation: `(target, +N)` or `(+N)` or `(target)`
    if pos < region.len() && region[pos] == b'(' {
        pos += 1; // skip '('
        skip_ws(region, &mut pos);

        // Check for identifier (target) vs effect number
        if pos < region.len() && region[pos].is_ascii_alphabetic() {
            let word_start = pos;
            while pos < region.len() && is_ident_char(region[pos]) {
                pos += 1;
            }
            tokens.push((
                span_at(block_span.start, start + word_start, start + pos),
                TT_NAMESPACE,
                0,
            ));
            skip_ws(region, &mut pos);

            // Optional `, +N`
            if pos < region.len() && region[pos] == b',' {
                pos += 1;
                skip_ws(region, &mut pos);
                scan_effect_token(region, &mut pos, block_span.start, start, &mut tokens);
            }
        } else {
            // Effect number directly
            scan_effect_token(region, &mut pos, block_span.start, start, &mut tokens);
        }

        // Skip to closing ')'
        while pos < region.len() && region[pos] != b')' {
            pos += 1;
        }
        if pos < region.len() {
            pos += 1; // skip ')'
        }
        skip_ws(region, &mut pos);
    }

    // 3. Body: everything between `{` and `}`
    if pos < region.len() && region[pos] == b'{' {
        pos += 1; // skip '{'

        // Find the matching closing brace (last byte before end)
        let body_end = if region.len() > 0 && region[region.len() - 1] == b'}' {
            region.len() - 1
        } else {
            region.len()
        };

        // Tokenize the body
        tokenize_asm_body(region, pos, body_end, block_span.start, start, &mut tokens);
    }

    tokens
}

/// Tokenize the asm body content between braces.
fn tokenize_asm_body(
    region: &[u8],
    mut pos: usize,
    end: usize,
    span_base: u32,
    abs_start: usize,
    tokens: &mut Vec<(Span, u32, u32)>,
) {
    while pos < end {
        // Skip whitespace
        if region[pos].is_ascii_whitespace() {
            pos += 1;
            continue;
        }

        // Line comment
        if pos + 1 < end && region[pos] == b'/' && region[pos + 1] == b'/' {
            let comment_start = pos;
            while pos < end && region[pos] != b'\n' {
                pos += 1;
            }
            tokens.push((
                span_at(span_base, abs_start + comment_start, abs_start + pos),
                TT_COMMENT,
                0,
            ));
            continue;
        }

        // Integer literal (possibly negative)
        if region[pos].is_ascii_digit()
            || (region[pos] == b'-' && pos + 1 < end && region[pos + 1].is_ascii_digit())
        {
            let num_start = pos;
            if region[pos] == b'-' {
                pos += 1;
            }
            while pos < end && region[pos].is_ascii_digit() {
                pos += 1;
            }
            tokens.push((
                span_at(span_base, abs_start + num_start, abs_start + pos),
                TT_NUMBER,
                0,
            ));
            continue;
        }

        // Identifier or instruction
        if region[pos].is_ascii_alphabetic() || region[pos] == b'_' {
            let word_start = pos;
            while pos < end && is_ident_char(region[pos]) {
                pos += 1;
            }
            let word = std::str::from_utf8(&region[word_start..pos]).unwrap_or("");
            let (tt, mods) = if TRITON_INSTRUCTIONS.contains(&word) {
                (TT_KEYWORD, MOD_DEFAULT_LIBRARY)
            } else {
                (TT_KEYWORD, MOD_DEFAULT_LIBRARY)
            };
            tokens.push((
                span_at(span_base, abs_start + word_start, abs_start + pos),
                tt,
                mods,
            ));
            continue;
        }

        // Skip unknown characters
        pos += 1;
    }
}

fn scan_effect_token(
    region: &[u8],
    pos: &mut usize,
    _span_base: u32,
    abs_start: usize,
    tokens: &mut Vec<(Span, u32, u32)>,
) {
    if *pos >= region.len() {
        return;
    }
    let eff_start = *pos;
    if region[*pos] == b'+' || region[*pos] == b'-' {
        *pos += 1;
    }
    while *pos < region.len() && region[*pos].is_ascii_digit() {
        *pos += 1;
    }
    if *pos > eff_start {
        tokens.push((
            Span::new(0, (abs_start + eff_start) as u32, (abs_start + *pos) as u32),
            TT_NUMBER,
            0,
        ));
    }
}

fn skip_ws(region: &[u8], pos: &mut usize) {
    while *pos < region.len() && region[*pos].is_ascii_whitespace() {
        *pos += 1;
    }
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn span_at(_span_base: u32, abs_start: usize, abs_end: usize) -> Span {
    Span::new(0, abs_start as u32, abs_end as u32)
}
