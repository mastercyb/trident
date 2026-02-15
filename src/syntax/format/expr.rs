use crate::ast::*;

/// Format a type to string.
pub(crate) fn format_type(ty: &Type) -> String {
    match ty {
        Type::Field => "Field".to_string(),
        Type::XField => "XField".to_string(),
        Type::Bool => "Bool".to_string(),
        Type::U32 => "U32".to_string(),
        Type::Digest => "Digest".to_string(),
        Type::Array(inner, size) => format!("[{}; {}]", format_type(inner), size),
        Type::Tuple(elems) => {
            let inner: Vec<String> = elems.iter().map(format_type).collect();
            format!("({})", inner.join(", "))
        }
        Type::Named(path) => path.as_dotted(),
    }
}

/// Format an expression to a single-line string.
pub(crate) fn format_expr(expr: &Expr) -> String {
    match expr {
        Expr::Literal(lit) => match lit {
            Literal::Integer(n) => n.to_string(),
            Literal::Bool(b) => b.to_string(),
        },
        Expr::Var(name) => name.clone(),
        Expr::BinOp { op, lhs, rhs } => {
            let l = format_expr_precedence(&lhs.node, op, true);
            let r = format_expr_precedence(&rhs.node, op, false);
            format!("{} {} {}", l, op.as_str(), r)
        }
        Expr::Call {
            path,
            args,
            generic_args,
        } => {
            let args_str: Vec<String> = args.iter().map(|a| format_expr(&a.node)).collect();
            if generic_args.is_empty() {
                format!("{}({})", path.node.as_dotted(), args_str.join(", "))
            } else {
                let ga: Vec<String> = generic_args.iter().map(|a| a.node.to_string()).collect();
                format!(
                    "{}<{}>({})",
                    path.node.as_dotted(),
                    ga.join(", "),
                    args_str.join(", ")
                )
            }
        }
        Expr::FieldAccess { expr, field } => {
            format!("{}.{}", format_expr(&expr.node), field.node)
        }
        Expr::Index { expr, index } => {
            format!("{}[{}]", format_expr(&expr.node), format_expr(&index.node))
        }
        Expr::StructInit { path, fields } => {
            let fields_str: Vec<String> = fields
                .iter()
                .map(|(name, expr)| format!("{}: {}", name.node, format_expr(&expr.node)))
                .collect();
            format!("{} {{ {} }}", path.node.as_dotted(), fields_str.join(", "))
        }
        Expr::ArrayInit(elems) => {
            let inner: Vec<String> = elems.iter().map(|e| format_expr(&e.node)).collect();
            format!("[{}]", inner.join(", "))
        }
        Expr::Tuple(elems) => {
            let inner: Vec<String> = elems.iter().map(|e| format_expr(&e.node)).collect();
            format!("({})", inner.join(", "))
        }
    }
}

/// Format an expression with parentheses if needed for precedence.
fn format_expr_precedence(expr: &Expr, parent_op: &BinOp, _is_left: bool) -> String {
    if let Expr::BinOp { op, .. } = expr {
        if op.binding_power().0 < parent_op.binding_power().0 {
            return format!("({})", format_expr(expr));
        }
    }
    format_expr(expr)
}

/// Format a place (l-value) to string.
pub(crate) fn format_place(place: &Place) -> String {
    match place {
        Place::Var(name) => name.clone(),
        Place::FieldAccess(inner, field) => {
            format!("{}.{}", format_place(&inner.node), field.node)
        }
        Place::Index(inner, index) => {
            format!(
                "{}[{}]",
                format_place(&inner.node),
                format_expr(&index.node)
            )
        }
    }
}
