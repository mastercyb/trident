//! LSP utility functions: position/offset conversion, word extraction,
//! type formatting, and call context analysis.

use tower_lsp::lsp_types::*;

use crate::span::Span;

// Re-export canonical formatters so lsp/mod.rs can import them via util::.
pub use crate::ast::display::{format_ast_type, format_fn_signature};

pub fn to_lsp_diagnostic(diag: &crate::diagnostic::Diagnostic, source: &str) -> Diagnostic {
    let start = byte_offset_to_position(source, diag.span.start as usize);
    let end = byte_offset_to_position(source, diag.span.end as usize);

    let severity = match diag.severity {
        crate::diagnostic::Severity::Error => DiagnosticSeverity::ERROR,
        crate::diagnostic::Severity::Warning => DiagnosticSeverity::WARNING,
    };

    let mut message = diag.message.clone();
    for note in &diag.notes {
        message.push_str("\nnote: ");
        message.push_str(note);
    }
    if let Some(help) = &diag.help {
        message.push_str("\nhelp: ");
        message.push_str(help);
    }

    Diagnostic {
        range: Range::new(start, end),
        severity: Some(severity),
        source: Some("trident".to_string()),
        message,
        ..Default::default()
    }
}

pub fn byte_offset_to_position(source: &str, offset: usize) -> Position {
    let offset = offset.min(source.len());
    let mut line = 0u32;
    let mut col = 0u32;
    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += ch.len_utf16() as u32;
        }
    }
    Position::new(line, col)
}

pub fn span_to_range(source: &str, span: Span) -> Range {
    Range::new(
        byte_offset_to_position(source, span.start as usize),
        byte_offset_to_position(source, span.end as usize),
    )
}

/// Extract the word (identifier) at a given cursor position.
pub fn word_at_position(source: &str, pos: Position) -> String {
    let Some(offset) = position_to_byte_offset(source, pos) else {
        return String::new();
    };

    let bytes = source.as_bytes();
    let mut start = offset;
    while start > 0 && is_ident_char(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = offset;
    while end < bytes.len() && is_ident_char(bytes[end]) {
        end += 1;
    }

    // Include dot for qualified names like "hash.tip5"
    if start > 0 && bytes[start - 1] == b'.' {
        let mut dot_start = start - 1;
        while dot_start > 0 && is_ident_char(bytes[dot_start - 1]) {
            dot_start -= 1;
        }
        source[dot_start..end].to_string()
    } else if end < bytes.len() && bytes[end] == b'.' {
        let mut dot_end = end + 1;
        while dot_end < bytes.len() && is_ident_char(bytes[dot_end]) {
            dot_end += 1;
        }
        source[start..dot_end].to_string()
    } else {
        source[start..end].to_string()
    }
}

/// Check if there's a dot before the cursor and return the module prefix.
pub fn text_before_dot(source: &str, pos: Position) -> Option<String> {
    let offset = position_to_byte_offset(source, pos)?;
    let bytes = source.as_bytes();

    let mut i = offset;
    while i > 0 && is_ident_char(bytes[i - 1]) {
        i -= 1;
    }
    if i > 0 && bytes[i - 1] == b'.' {
        let dot_pos = i - 1;
        let mut start = dot_pos;
        while start > 0 && is_ident_char(bytes[start - 1]) {
            start -= 1;
        }
        if start < dot_pos {
            return Some(source[start..dot_pos].to_string());
        }
    }
    None
}

pub fn position_to_byte_offset(source: &str, pos: Position) -> Option<usize> {
    let mut line = 0u32;
    let mut col = 0u32;
    for (i, ch) in source.char_indices() {
        if line == pos.line && col == pos.character {
            return Some(i);
        }
        if ch == '\n' {
            if line == pos.line {
                return Some(i);
            }
            line += 1;
            col = 0;
        } else {
            col += ch.len_utf16() as u32;
        }
    }
    if line == pos.line {
        Some(source.len())
    } else {
        None
    }
}

pub fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Format a `TableCost` as a compact inline string for hover display.
pub fn format_cost_inline(cost: &crate::cost::TableCost) -> String {
    let model = crate::cost::create_cost_model("triton");
    let short_names = model.table_short_names();
    let n = cost.count as usize;
    let mut parts = Vec::new();
    for i in 0..n.min(short_names.len()) {
        if i == 0 || cost.values[i] > 0 {
            parts.push(format!("{}={}", short_names[i], cost.values[i]));
        }
    }
    format!(
        "{} | dominant: {}",
        parts.join(", "),
        cost.dominant_table(&short_names[..n.min(short_names.len())])
    )
}

/// Find the function name and active parameter index at a given position.
pub fn find_call_context(source: &str, pos: Position) -> Option<(String, u32)> {
    let offset = position_to_byte_offset(source, pos)?;
    let bytes = source.as_bytes();

    let mut depth = 0i32;
    let mut comma_count = 0u32;
    let mut i = offset;
    while i > 0 {
        i -= 1;
        match bytes[i] {
            b')' => depth += 1,
            b'(' => {
                if depth == 0 {
                    let mut name_end = i;
                    while name_end > 0 && bytes[name_end - 1] == b' ' {
                        name_end -= 1;
                    }
                    let mut name_start = name_end;
                    while name_start > 0
                        && (is_ident_char(bytes[name_start - 1]) || bytes[name_start - 1] == b'.')
                    {
                        name_start -= 1;
                    }
                    if name_start < name_end {
                        let name = source[name_start..name_end].to_string();
                        return Some((name, comma_count));
                    }
                    return None;
                }
                depth -= 1;
            }
            b',' if depth == 0 => comma_count += 1,
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests;
