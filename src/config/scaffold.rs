/// Spec-driven code scaffolding.
///
/// A spec file is a `.tri` file with function signatures, spec annotations
/// (`#[requires(...)]` and `#[ensures(...)]`), and empty or stub bodies.
/// `trident generate` fills in scaffolding: TODO comments, placeholder
/// return values, and assertion stubs that mirror the spec annotations.
use crate::ast::{File, FnDef, Item, Param, Type};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Generate a complete scaffold file from a parsed spec file.
///
/// For every function that carries `requires`/`ensures` annotations (or has
/// an empty body), the scaffold replaces the body with:
///   - a comment block summarising the specification,
///   - TODO markers explaining what needs to be implemented,
///   - assertion stubs that mirror the `ensures` clauses,
///   - a placeholder return value matching the declared return type.
///
/// Non-function items (structs, constants, events) and the file header
/// (`program`/`module` declaration + `use` statements) are reproduced
/// verbatim so the output is a valid Trident source file.
pub fn generate_scaffold(file: &File) -> String {
    let mut out = String::new();

    // File header
    let kind = match file.kind {
        crate::ast::FileKind::Program => "program",
        crate::ast::FileKind::Module => "module",
    };
    out.push_str(&format!("{} {}\n", kind, file.name.node));

    // Use declarations
    for u in &file.uses {
        out.push_str(&format!("\nuse {}", u.node));
    }

    // Items
    for item in &file.items {
        out.push('\n');
        match &item.node {
            Item::Fn(func) => {
                out.push_str(&scaffold_function(func));
            }
            Item::Const(c) => {
                if c.is_pub {
                    out.push_str("pub ");
                }
                out.push_str(&format!(
                    "const {}: {} = {}\n",
                    c.name.node,
                    format_type(&c.ty.node),
                    format_const_expr(&c.value.node),
                ));
            }
            Item::Struct(s) => {
                if s.is_pub {
                    out.push_str("pub ");
                }
                out.push_str(&format!("struct {} {{\n", s.name.node));
                for field in &s.fields {
                    if field.is_pub {
                        out.push_str("    pub ");
                    } else {
                        out.push_str("    ");
                    }
                    out.push_str(&format!(
                        "{}: {},\n",
                        field.name.node,
                        format_type(&field.ty.node),
                    ));
                }
                out.push_str("}\n");
            }
            Item::Event(e) => {
                out.push_str(&format!("event {} {{\n", e.name.node));
                for field in &e.fields {
                    out.push_str(&format!(
                        "    {}: {},\n",
                        field.name.node,
                        format_type(&field.ty.node),
                    ));
                }
                out.push_str("}\n");
            }
        }
    }

    out
}

// ---------------------------------------------------------------------------
// Per-function scaffolding
// ---------------------------------------------------------------------------

/// Generate a function scaffold including annotations, signature, and body.
fn scaffold_function(func: &FnDef) -> String {
    let mut out = String::new();

    let requires: Vec<&str> = func.requires.iter().map(|s| s.node.as_str()).collect();
    let ensures: Vec<&str> = func.ensures.iter().map(|s| s.node.as_str()).collect();

    // Emit annotation attributes
    for r in &requires {
        out.push_str(&format!("#[requires({})]\n", r));
    }
    for e in &ensures {
        out.push_str(&format!("#[ensures({})]\n", e));
    }

    // Visibility + test markers
    if func.is_pub {
        out.push_str("pub ");
    }
    if func.is_test {
        out.push_str("#[test]\n");
    }

    // Signature
    out.push_str(&format!("fn {}", func.name.node));
    if !func.type_params.is_empty() {
        let tp: Vec<&str> = func.type_params.iter().map(|p| p.node.as_str()).collect();
        out.push_str(&format!("<{}>", tp.join(", ")));
    }
    out.push('(');
    let params: Vec<String> = func
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name.node, format_type(&p.ty.node)))
        .collect();
    out.push_str(&params.join(", "));
    out.push(')');

    if let Some(ref ret) = func.return_ty {
        out.push_str(&format!(" -> {}", format_type(&ret.node)));
    }

    // Body
    out.push_str(" {\n");

    let has_specs = !requires.is_empty() || !ensures.is_empty();

    if has_specs {
        // Specification comment block
        out.push_str(&spec_comment(&requires, &ensures));
        out.push('\n');
    }

    // Function name for the TODO comment
    let fn_name = &func.name.node;

    if let Some(ref ret_ty) = func.return_ty {
        // Function returns a value
        out.push_str(&format!("    // TODO: Implement {} logic\n", fn_name));

        if !ensures.is_empty() {
            // Generate a helpful comment about what the result must satisfy
            out.push_str(&format!(
                "    // The result must satisfy: {}\n",
                ensures.join(", "),
            ));
        }

        // Try to synthesise a meaningful result expression from ensures
        let result_expr = synthesise_result_expr(&ensures, &func.params, &ret_ty.node);
        out.push_str(&format!(
            "    let result: {} = {}\n",
            format_type(&ret_ty.node),
            result_expr,
        ));

        // Assertion stubs from ensures
        for e in &ensures {
            let assertion = ensures_to_assertion(e, &ret_ty.node);
            out.push_str(&format!(
                "\n    // Verify postcondition\n    {}\n",
                assertion
            ));
        }

        out.push_str("\n    result\n");
    } else {
        // Void function
        out.push_str(&format!("    // TODO: Implement {} logic\n", fn_name));

        // For void functions, emit assertions from requires + ensures
        for r in &requires {
            out.push_str(&format!("    assert({})\n", r));
        }
        for e in &ensures {
            if *e != "true" {
                out.push_str(&format!("    assert({})\n", e));
            }
        }
    }

    out.push_str("}\n");
    out
}

// ---------------------------------------------------------------------------
// Specification comment block
// ---------------------------------------------------------------------------

/// Generate a comment block explaining the specification.
fn spec_comment(requires: &[&str], ensures: &[&str]) -> String {
    let mut out = String::new();
    out.push_str("    // Specification:\n");
    for r in requires {
        out.push_str(&format!("    //   requires: {}\n", r));
    }
    for e in ensures {
        out.push_str(&format!("    //   ensures: {}\n", e));
    }
    out
}

// ---------------------------------------------------------------------------
// Default values
// ---------------------------------------------------------------------------

/// Generate a default value expression for a type.
pub fn default_value(ty: &Type) -> String {
    match ty {
        Type::Field | Type::XField | Type::U32 => "0".to_string(),
        Type::Bool => "false".to_string(),
        Type::Digest => "0".to_string(),
        Type::Array(inner, size) => {
            let elem = default_value(inner);
            if let Some(n) = size.as_literal() {
                let elems: Vec<String> = (0..n).map(|_| elem.clone()).collect();
                format!("[{}]", elems.join(", "))
            } else {
                // Cannot generate a concrete array for a generic/expression size
                format!("[{}; {}]", elem, size)
            }
        }
        Type::Tuple(elems) => {
            let parts: Vec<String> = elems.iter().map(default_value).collect();
            format!("({})", parts.join(", "))
        }
        Type::Named(_) => "0".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Variable extraction from spec expressions
// ---------------------------------------------------------------------------

/// Parse a spec expression to extract referenced variable names.
///
/// This performs a lightweight scan of the expression text: any sequence of
/// `[a-zA-Z_][a-zA-Z0-9_]*` that is not a keyword or literal is considered
/// a variable reference.
pub fn extract_variables(spec: &str) -> Vec<String> {
    let keywords: &[&str] = &[
        "true", "false", "old", "result", "let", "if", "else", "for", "fn", "return",
    ];
    let mut vars = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut chars = spec.char_indices().peekable();

    while let Some(&(i, c)) = chars.peek() {
        if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while let Some(&(_, nc)) = chars.peek() {
                if nc.is_ascii_alphanumeric() || nc == '_' {
                    chars.next();
                } else {
                    break;
                }
            }
            let end = chars.peek().map(|&(idx, _)| idx).unwrap_or(spec.len());
            let word = &spec[start..end];
            if !keywords.contains(&word) && !seen.contains(word) {
                seen.insert(word.to_string());
                vars.push(word.to_string());
            }
        } else {
            chars.next();
        }
    }

    vars
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Format a Type as Trident source text.
fn format_type(ty: &Type) -> String {
    match ty {
        Type::Field => "Field".to_string(),
        Type::XField => "XField".to_string(),
        Type::Bool => "Bool".to_string(),
        Type::U32 => "U32".to_string(),
        Type::Digest => "Digest".to_string(),
        Type::Array(inner, size) => format!("[{}; {}]", format_type(inner), size),
        Type::Tuple(elems) => {
            let parts: Vec<String> = elems.iter().map(|t| format_type(t)).collect();
            format!("({})", parts.join(", "))
        }
        Type::Named(path) => path.as_dotted(),
    }
}

/// Format a constant expression in a best-effort way.
fn format_const_expr(expr: &crate::ast::Expr) -> String {
    match expr {
        crate::ast::Expr::Literal(crate::ast::Literal::Integer(n)) => n.to_string(),
        crate::ast::Expr::Literal(crate::ast::Literal::Bool(b)) => b.to_string(),
        _ => "0".to_string(),
    }
}

/// Try to synthesise a result expression from `ensures` clauses.
///
/// Simple heuristic: if an ensures clause has the form `result == <expr>`
/// or `<name> == <expr>` where `<name>` is not a parameter, extract `<expr>`.
/// Otherwise fall back to the type's default value.
fn synthesise_result_expr(ensures: &[&str], params: &[Param], ret_ty: &Type) -> String {
    // Look for a clause like `result == ...` or `<ident> == <expr>`
    for clause in ensures {
        if let Some(expr) = try_extract_result_expr(clause, params) {
            return expr;
        }
    }
    default_value(ret_ty)
}

/// Try to extract `<rhs>` from `result == <rhs>` or `<name> == <rhs>`
/// where `<name>` is not one of the function parameters.
fn try_extract_result_expr(clause: &str, params: &[Param]) -> Option<String> {
    let param_names: Vec<&str> = params.iter().map(|p| p.name.node.as_str()).collect();

    // Try "result == <rhs>"
    if let Some(rhs) = clause
        .strip_prefix("result == ")
        .or_else(|| clause.strip_prefix("result =="))
    {
        let rhs = rhs.trim();
        if !rhs.is_empty() {
            return Some(rhs.to_string());
        }
    }

    // Try "<ident> == <rhs>" where ident is not a parameter
    if let Some(eq_pos) = clause.find(" == ") {
        let lhs = clause[..eq_pos].trim();
        let rhs = clause[eq_pos + 4..].trim();

        // If LHS is a simple identifier (not a param), treat RHS as the result expr
        if is_simple_ident(lhs) && !param_names.contains(&lhs) && !rhs.is_empty() {
            return Some(rhs.to_string());
        }
    }

    None
}

/// Check whether a string is a simple identifier.
fn is_simple_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Convert an `ensures` clause into an assertion statement.
///
/// If the ensures clause references `result`, replace it with `result`.
/// For clauses of the form `<ident> == <expr>` where ident is the
/// "result name", emit `assert(result == <expr>)`.
fn ensures_to_assertion(clause: &str, _ret_ty: &Type) -> String {
    // If the clause already uses "result", emit as-is
    if clause.contains("result") {
        return format!("assert({})", clause);
    }

    // For `<name> == <expr>` patterns, replace <name> with result
    if let Some(eq_pos) = clause.find(" == ") {
        let lhs = clause[..eq_pos].trim();
        let rhs = clause[eq_pos + 4..].trim();
        if is_simple_ident(lhs) {
            return format!("assert(result == {})", rhs);
        }
    }

    // Fallback: emit as-is
    format!("assert({})", clause)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::ArraySize;
    use crate::parse_source_silent;

    #[test]
    fn test_scaffold_with_requires_ensures() {
        let source = r#"program test

#[requires(amount > 0)]
#[ensures(balance == old_balance + amount)]
fn deposit(old_balance: Field, amount: Field) -> Field {
}
"#;
        let file = parse_source_silent(source, "test.tri").unwrap();
        let scaffold = generate_scaffold(&file);

        // Should contain the spec annotations
        assert!(scaffold.contains("#[requires(amount > 0)]"));
        assert!(scaffold.contains("#[ensures(balance == old_balance + amount)]"));

        // Should contain TODO comment
        assert!(scaffold.contains("// TODO: Implement deposit logic"));

        // Should contain spec comment block
        assert!(scaffold.contains("//   requires: amount > 0"));
        assert!(scaffold.contains("//   ensures: balance == old_balance + amount"));

        // Should contain a result binding
        assert!(scaffold.contains("let result: Field ="));

        // Should contain postcondition assertion
        assert!(scaffold.contains("assert(result == old_balance + amount)"));

        // Should return result
        assert!(scaffold.contains("    result\n"));
    }

    #[test]
    fn test_scaffold_without_annotations() {
        let source = r#"program test

fn add(x: Field, y: Field) -> Field {
}
"#;
        let file = parse_source_silent(source, "test.tri").unwrap();
        let scaffold = generate_scaffold(&file);

        // Should have a TODO
        assert!(scaffold.contains("// TODO: Implement add logic"));

        // Should have a default return value
        assert!(scaffold.contains("let result: Field = 0"));

        // Should return result
        assert!(scaffold.contains("    result\n"));

        // Should NOT contain spec comment
        assert!(!scaffold.contains("// Specification:"));
    }

    #[test]
    fn test_scaffold_void_function() {
        let source = r#"program test

#[requires(x > 0)]
#[ensures(true)]
fn validate(x: Field) {
}
"#;
        let file = parse_source_silent(source, "test.tri").unwrap();
        let scaffold = generate_scaffold(&file);

        // Should have TODO
        assert!(scaffold.contains("// TODO: Implement validate logic"));

        // Void function should assert requires
        assert!(scaffold.contains("assert(x > 0)"));

        // "true" in ensures should be skipped
        assert!(!scaffold.contains("assert(true)"));

        // Should NOT have result binding or return
        assert!(!scaffold.contains("let result"));
    }

    #[test]
    fn test_default_value_field() {
        assert_eq!(default_value(&Type::Field), "0");
    }

    #[test]
    fn test_default_value_bool() {
        assert_eq!(default_value(&Type::Bool), "false");
    }

    #[test]
    fn test_default_value_u32() {
        assert_eq!(default_value(&Type::U32), "0");
    }

    #[test]
    fn test_default_value_digest() {
        assert_eq!(default_value(&Type::Digest), "0");
    }

    #[test]
    fn test_default_value_array() {
        let ty = Type::Array(Box::new(Type::Field), ArraySize::Literal(3));
        assert_eq!(default_value(&ty), "[0, 0, 0]");
    }

    #[test]
    fn test_default_value_tuple() {
        let ty = Type::Tuple(vec![Type::Field, Type::Bool]);
        assert_eq!(default_value(&ty), "(0, false)");
    }

    #[test]
    fn test_default_value_xfield() {
        assert_eq!(default_value(&Type::XField), "0");
    }

    #[test]
    fn test_extract_variables_simple() {
        let vars = extract_variables("amount > 0");
        assert_eq!(vars, vec!["amount"]);
    }

    #[test]
    fn test_extract_variables_expression() {
        let vars = extract_variables("balance == old_balance + amount");
        assert!(vars.contains(&"balance".to_string()));
        assert!(vars.contains(&"old_balance".to_string()));
        assert!(vars.contains(&"amount".to_string()));
    }

    #[test]
    fn test_extract_variables_skips_keywords() {
        let vars = extract_variables("result == true");
        // "result" and "true" are keywords, so should be excluded
        assert!(vars.is_empty());
    }

    #[test]
    fn test_extract_variables_with_underscore() {
        let vars = extract_variables("_private_var > 0");
        assert_eq!(vars, vec!["_private_var"]);
    }

    #[test]
    fn test_scaffold_tuple_return() {
        let source = r#"program test

#[requires(amount > 0)]
#[ensures(new_sender == sub(sender_balance, amount))]
#[ensures(new_receiver == receiver_balance + amount)]
fn transfer(sender_balance: Field, receiver_balance: Field, amount: Field) -> (Field, Field) {
}
"#;
        let file = parse_source_silent(source, "test.tri").unwrap();
        let scaffold = generate_scaffold(&file);

        // Should contain annotations (parser spaces tokens inside attributes)
        assert!(scaffold.contains("#[requires(amount > 0)]"));
        assert!(scaffold.contains("#[ensures(new_sender == sub ( sender_balance , amount ))]"));
        assert!(scaffold.contains("#[ensures(new_receiver == receiver_balance + amount)]"));

        // Should have result binding with tuple type
        assert!(scaffold.contains("let result: (Field, Field) ="));

        // Should have postcondition assertions
        assert!(scaffold.contains("assert(result == sub ( sender_balance , amount ))"));
        assert!(scaffold.contains("assert(result == receiver_balance + amount)"));
    }

    #[test]
    fn test_scaffold_result_keyword_in_ensures() {
        let source = r#"program test

#[requires(amount > 0)]
#[ensures(result == balance + amount)]
fn deposit(balance: Field, amount: Field) -> Field {
}
"#;
        let file = parse_source_silent(source, "test.tri").unwrap();
        let scaffold = generate_scaffold(&file);

        // When ensures uses "result", the synthesized expression comes from the RHS
        assert!(scaffold.contains("let result: Field = balance + amount"));

        // Assertion should use the clause as-is since it uses "result"
        assert!(scaffold.contains("assert(result == balance + amount)"));
    }

    #[test]
    fn test_scaffold_multiple_requires() {
        let source = r#"program test

#[requires(amount > 0)]
#[requires(balance > amount)]
#[ensures(result == sub(balance, amount))]
fn withdraw(balance: Field, amount: Field) -> Field {
}
"#;
        let file = parse_source_silent(source, "test.tri").unwrap();
        let scaffold = generate_scaffold(&file);

        assert!(scaffold.contains("#[requires(amount > 0)]"));
        assert!(scaffold.contains("#[requires(balance > amount)]"));
        assert!(scaffold.contains("//   requires: amount > 0"));
        assert!(scaffold.contains("//   requires: balance > amount"));
        // Parser spaces tokens inside attributes: sub(balance, amount) -> sub ( balance , amount )
        assert!(scaffold.contains("let result: Field = sub ( balance , amount )"));
    }

    #[test]
    fn test_scaffold_preserves_pub() {
        let source = r#"program test

#[requires(x > 0)]
pub fn check(x: Field) -> Field {
}
"#;
        let file = parse_source_silent(source, "test.tri").unwrap();
        let scaffold = generate_scaffold(&file);

        assert!(scaffold.contains("pub fn check"));
    }

    #[test]
    fn test_scaffold_preserves_program_header() {
        let source = r#"program my_app

fn main() {
}
"#;
        let file = parse_source_silent(source, "test.tri").unwrap();
        let scaffold = generate_scaffold(&file);

        assert!(scaffold.starts_with("program my_app\n"));
    }
}
