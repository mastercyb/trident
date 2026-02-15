use std::collections::BTreeMap;

use crate::ast::{self, Expr, Stmt};
use crate::hash::ContentHash;

// ─── Dependency Extraction ─────────────────────────────────────────

/// Extract dependencies from a function body by walking for Call expressions.
pub(super) fn extract_dependencies(
    func: &ast::FnDef,
    fn_hashes: &BTreeMap<String, ContentHash>,
) -> Vec<ContentHash> {
    let mut deps = Vec::new();
    let mut seen = std::collections::HashSet::new();

    if let Some(ref body) = func.body {
        walk_block_for_calls(&body.node, fn_hashes, &func.name.node, &mut deps, &mut seen);
    }

    deps
}

fn walk_block_for_calls(
    block: &ast::Block,
    fn_hashes: &BTreeMap<String, ContentHash>,
    self_name: &str,
    deps: &mut Vec<ContentHash>,
    seen: &mut std::collections::HashSet<ContentHash>,
) {
    for stmt in &block.stmts {
        walk_stmt_for_calls(&stmt.node, fn_hashes, self_name, deps, seen);
    }
    if let Some(ref tail) = block.tail_expr {
        walk_expr_for_calls(&tail.node, fn_hashes, self_name, deps, seen);
    }
}

fn walk_stmt_for_calls(
    stmt: &Stmt,
    fn_hashes: &BTreeMap<String, ContentHash>,
    self_name: &str,
    deps: &mut Vec<ContentHash>,
    seen: &mut std::collections::HashSet<ContentHash>,
) {
    match stmt {
        Stmt::Let { init, .. } => {
            walk_expr_for_calls(&init.node, fn_hashes, self_name, deps, seen);
        }
        Stmt::Assign { value, .. } => {
            walk_expr_for_calls(&value.node, fn_hashes, self_name, deps, seen);
        }
        Stmt::TupleAssign { value, .. } => {
            walk_expr_for_calls(&value.node, fn_hashes, self_name, deps, seen);
        }
        Stmt::If {
            cond,
            then_block,
            else_block,
        } => {
            walk_expr_for_calls(&cond.node, fn_hashes, self_name, deps, seen);
            walk_block_for_calls(&then_block.node, fn_hashes, self_name, deps, seen);
            if let Some(ref else_blk) = else_block {
                walk_block_for_calls(&else_blk.node, fn_hashes, self_name, deps, seen);
            }
        }
        Stmt::For {
            start, end, body, ..
        } => {
            walk_expr_for_calls(&start.node, fn_hashes, self_name, deps, seen);
            walk_expr_for_calls(&end.node, fn_hashes, self_name, deps, seen);
            walk_block_for_calls(&body.node, fn_hashes, self_name, deps, seen);
        }
        Stmt::Expr(expr) => {
            walk_expr_for_calls(&expr.node, fn_hashes, self_name, deps, seen);
        }
        Stmt::Return(Some(expr)) => {
            walk_expr_for_calls(&expr.node, fn_hashes, self_name, deps, seen);
        }
        Stmt::Return(None) | Stmt::Asm { .. } => {}
        Stmt::Reveal { fields, .. } | Stmt::Seal { fields, .. } => {
            for (_, val) in fields {
                walk_expr_for_calls(&val.node, fn_hashes, self_name, deps, seen);
            }
        }
        Stmt::Match { expr, arms } => {
            walk_expr_for_calls(&expr.node, fn_hashes, self_name, deps, seen);
            for arm in arms {
                walk_block_for_calls(&arm.body.node, fn_hashes, self_name, deps, seen);
            }
        }
    }
}

fn walk_expr_for_calls(
    expr: &Expr,
    fn_hashes: &BTreeMap<String, ContentHash>,
    self_name: &str,
    deps: &mut Vec<ContentHash>,
    seen: &mut std::collections::HashSet<ContentHash>,
) {
    match expr {
        Expr::Call { path, args, .. } => {
            let name = path.node.as_dotted();
            let short = path.node.0.last().map(|s| s.as_str()).unwrap_or("");
            // Don't add self as a dependency.
            if name != self_name && short != self_name {
                // Try full name first, then short name.
                let hash = fn_hashes.get(&name).or_else(|| fn_hashes.get(short));
                if let Some(h) = hash {
                    if seen.insert(*h) {
                        deps.push(*h);
                    }
                }
            }
            for arg in args {
                walk_expr_for_calls(&arg.node, fn_hashes, self_name, deps, seen);
            }
        }
        Expr::BinOp { lhs, rhs, .. } => {
            walk_expr_for_calls(&lhs.node, fn_hashes, self_name, deps, seen);
            walk_expr_for_calls(&rhs.node, fn_hashes, self_name, deps, seen);
        }
        Expr::FieldAccess { expr, .. } => {
            walk_expr_for_calls(&expr.node, fn_hashes, self_name, deps, seen);
        }
        Expr::Index { expr, index } => {
            walk_expr_for_calls(&expr.node, fn_hashes, self_name, deps, seen);
            walk_expr_for_calls(&index.node, fn_hashes, self_name, deps, seen);
        }
        Expr::StructInit { fields, .. } => {
            for (_, val) in fields {
                walk_expr_for_calls(&val.node, fn_hashes, self_name, deps, seen);
            }
        }
        Expr::ArrayInit(elems) | Expr::Tuple(elems) => {
            for elem in elems {
                walk_expr_for_calls(&elem.node, fn_hashes, self_name, deps, seen);
            }
        }
        Expr::Literal(_) | Expr::Var(_) => {}
    }
}
