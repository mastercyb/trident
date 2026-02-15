//! Pretty-printing utilities for AST function definitions.

use super::{File, FileKind, FnDef, Item};
use crate::format;
use crate::span::Spanned;

/// Pretty-print a single function definition by wrapping it in a
/// minimal synthetic `File` and running the canonical formatter.
pub fn format_function(func: &FnDef) -> String {
    // Build a minimal File containing only this function.
    let file = File {
        kind: FileKind::Program,
        name: Spanned::dummy("_view".to_string()),
        uses: Vec::new(),
        declarations: Vec::new(),
        items: vec![Spanned::dummy(Item::Fn(func.clone()))],
    };

    let formatted = format::format_file(&file, &[]);

    // The formatter emits "program _view\n\n<fn>\n".
    // Strip the synthetic header to isolate the function text.
    strip_synthetic_header(&formatted)
}

/// Pretty-print a function with an optional cost annotation appended
/// as a trailing comment on the signature line.
pub fn format_function_with_cost(func: &FnDef, cost: Option<&str>) -> String {
    let base = format_function(func);
    match cost {
        Some(c) => {
            // Insert cost comment after the opening brace of the function
            if let Some(brace_pos) = base.find('{') {
                let (before, after) = base.split_at(brace_pos + 1);
                format!("{} // cost: {}{}", before.trim_end(), c, after)
            } else {
                // No body (intrinsic) — append at end of first line
                let mut lines: Vec<&str> = base.lines().collect();
                if let Some(first) = lines.first_mut() {
                    return format!("{} // cost: {}", first, c);
                }
                base
            }
        }
        None => base,
    }
}

/// Strip the synthetic "program _view\n\n" header produced by the
/// formatter when we wrap a single function in a dummy File.
fn strip_synthetic_header(formatted: &str) -> String {
    // The formatter produces: "program _view\n\n<items>\n"
    // Find the first blank line and take everything after it.
    if let Some(pos) = formatted.find("\n\n") {
        let rest = &formatted[pos + 2..];
        rest.to_string()
    } else {
        formatted.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::navigate::find_function;

    fn parse_file(source: &str) -> File {
        crate::parse_source_silent(source, "test.tri").unwrap()
    }

    #[test]
    fn test_format_function_produces_valid_source() {
        let source = "program test\n\nfn add(a: Field, b: Field) -> Field {\n    a + b\n}\n";
        let file = parse_file(source);
        let func = find_function(&file, "add").expect("add function should exist");
        let formatted = format_function(func);

        assert!(formatted.contains("fn add("));
        assert!(formatted.contains("a: Field, b: Field"));
        assert!(formatted.contains("-> Field"));
        assert!(formatted.contains("a + b"));
    }

    #[test]
    fn test_format_function_with_annotations() {
        let source = "program test\n\n#[requires(a + b < 1000)]\n#[ensures(result == a + b)]\nfn add(a: Field, b: Field) -> Field {\n    a + b\n}\n";
        let file = parse_file(source);
        let func = find_function(&file, "add").expect("add function should exist");
        let formatted = format_function(func);

        assert!(formatted.contains("#[requires("));
        assert!(formatted.contains("#[ensures("));
        assert!(formatted.contains("fn add("));
    }

    #[test]
    fn test_format_function_pub() {
        let source = "module test\n\npub fn helper(x: Field) -> Field {\n    x + 1\n}\n";
        let file = parse_file(source);
        let func = find_function(&file, "helper").expect("helper function should exist");
        let formatted = format_function(func);

        assert!(formatted.contains("pub fn helper("));
    }

    #[test]
    fn test_format_function_with_cost() {
        let source = "program test\n\nfn add(a: Field, b: Field) -> Field {\n    a + b\n}\n";
        let file = parse_file(source);
        let func = find_function(&file, "add").expect("add function should exist");
        let formatted = format_function_with_cost(func, Some("cc=5, hash=0"));

        assert!(formatted.contains("// cost: cc=5, hash=0"));
    }
}
