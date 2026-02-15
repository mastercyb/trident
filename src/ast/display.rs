//! Pretty-printing utilities for AST nodes.
//!
//! This module is the single source of truth for converting AST types,
//! function signatures, and constant values to display strings.

use super::{Expr, File, FileKind, FnDef, Item, Literal, Type};
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

// ─── Canonical formatting helpers ──────────────────────────────────

/// Format an AST type for display (documentation, diagnostics, hover).
pub fn format_ast_type(ty: &Type) -> String {
    match ty {
        Type::Field => "Field".to_string(),
        Type::XField => "XField".to_string(),
        Type::Bool => "Bool".to_string(),
        Type::U32 => "U32".to_string(),
        Type::Digest => "Digest".to_string(),
        Type::Array(inner, size) => format!("[{}; {}]", format_ast_type(inner), size),
        Type::Tuple(elems) => {
            let parts: Vec<_> = elems.iter().map(format_ast_type).collect();
            format!("({})", parts.join(", "))
        }
        Type::Named(path) => path.as_dotted(),
    }
}

/// Format a function signature for display (documentation, diagnostics).
///
/// Includes type parameters, parameter names and types, and return type.
pub fn format_fn_signature(func: &FnDef) -> String {
    let mut sig = String::from("fn ");
    sig.push_str(&func.name.node);

    if !func.type_params.is_empty() {
        let params: Vec<_> = func.type_params.iter().map(|p| p.node.clone()).collect();
        sig.push_str(&format!("<{}>", params.join(", ")));
    }

    sig.push('(');
    let params: Vec<String> = func
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name.node, format_ast_type(&p.ty.node)))
        .collect();
    sig.push_str(&params.join(", "));
    sig.push(')');

    if let Some(ref ret) = func.return_ty {
        sig.push_str(&format!(" -> {}", format_ast_type(&ret.node)));
    }

    sig
}

/// Format a constant value expression for display (documentation).
pub fn format_const_value(expr: &Expr) -> String {
    match expr {
        Expr::Literal(Literal::Integer(n)) => n.to_string(),
        Expr::Literal(Literal::Bool(b)) => b.to_string(),
        _ => "...".to_string(),
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
}
