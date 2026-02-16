//! Static analysis: recursion detection, call graph collection, used-module tracking.

use std::collections::{BTreeMap, BTreeSet};

use crate::ast::*;

use super::TypeChecker;

impl TypeChecker {
    /// Build a call graph from the file's functions and report any cycles.
    pub(super) fn detect_recursion(&mut self, file: &File) {
        // Build adjacency list: fn_name -> set of called fn_names
        let mut call_graph: BTreeMap<String, Vec<String>> = BTreeMap::new();

        for item in &file.items {
            if let Item::Fn(func) = &item.node {
                if let Some(body) = &func.body {
                    let mut callees = Vec::new();
                    Self::collect_calls_block(&body.node, &mut callees);
                    call_graph.insert(func.name.node.clone(), callees);
                }
            }
        }

        // DFS cycle detection
        let fn_names: Vec<String> = call_graph.keys().cloned().collect();
        let mut visited = BTreeMap::new(); // 0=unvisited, 1=in-stack, 2=done

        for name in &fn_names {
            visited.insert(name.clone(), 0u8);
        }

        for name in &fn_names {
            if visited[name] == 0 {
                let mut path = Vec::new();
                if self.dfs_cycle(name, &call_graph, &mut visited, &mut path) {
                    // Find the span for the function that starts the cycle
                    let cycle_fn = &path[0];
                    let span = file
                        .items
                        .iter()
                        .find_map(|item| {
                            if let Item::Fn(func) = &item.node {
                                if func.name.node == *cycle_fn {
                                    return Some(func.name.span);
                                }
                            }
                            None
                        })
                        .unwrap_or(file.name.span);
                    self.error_with_help(
                        format!("recursive call cycle detected: {}", path.join(" -> ")),
                        span,
                        "stack-machine targets do not support recursion; use loops (`for`) or iterative algorithms instead".to_string(),
                    );
                }
            }
        }
    }

    fn dfs_cycle(
        &self,
        node: &str,
        graph: &BTreeMap<String, Vec<String>>,
        visited: &mut BTreeMap<String, u8>,
        path: &mut Vec<String>,
    ) -> bool {
        visited.insert(node.to_string(), 1); // in-stack
        path.push(node.to_string());

        if let Some(callees) = graph.get(node) {
            for callee in callees {
                // Only check local functions (those in our graph)
                let state = visited.get(callee).copied().unwrap_or(2);
                if state == 1 {
                    // Back-edge: cycle found
                    path.push(callee.clone());
                    return true;
                }
                if state == 0 && self.dfs_cycle(callee, graph, visited, path) {
                    return true;
                }
            }
        }

        path.pop();
        visited.insert(node.to_string(), 2); // done
        false
    }

    /// Collect all function call names from a block.
    pub(super) fn collect_calls_block(block: &Block, calls: &mut Vec<String>) {
        for stmt in &block.stmts {
            Self::collect_calls_stmt(&stmt.node, calls);
        }
        if let Some(tail) = &block.tail_expr {
            Self::collect_calls_expr(&tail.node, calls);
        }
    }

    fn collect_calls_stmt(stmt: &Stmt, calls: &mut Vec<String>) {
        match stmt {
            Stmt::Let { init, .. } => Self::collect_calls_expr(&init.node, calls),
            Stmt::Assign { value, .. } => Self::collect_calls_expr(&value.node, calls),
            Stmt::If {
                cond,
                then_block,
                else_block,
            } => {
                Self::collect_calls_expr(&cond.node, calls);
                Self::collect_calls_block(&then_block.node, calls);
                if let Some(eb) = else_block {
                    Self::collect_calls_block(&eb.node, calls);
                }
            }
            Stmt::For {
                start, end, body, ..
            } => {
                Self::collect_calls_expr(&start.node, calls);
                Self::collect_calls_expr(&end.node, calls);
                Self::collect_calls_block(&body.node, calls);
            }
            Stmt::TupleAssign { value, .. } => Self::collect_calls_expr(&value.node, calls),
            Stmt::Expr(expr) => Self::collect_calls_expr(&expr.node, calls),
            Stmt::Return(Some(val)) => Self::collect_calls_expr(&val.node, calls),
            Stmt::Return(None) => {}
            Stmt::Reveal { fields, .. } | Stmt::Seal { fields, .. } => {
                for (_, val) in fields {
                    Self::collect_calls_expr(&val.node, calls);
                }
            }
            Stmt::Asm { .. } => {}
            Stmt::Match { expr, arms } => {
                Self::collect_calls_expr(&expr.node, calls);
                for arm in arms {
                    Self::collect_calls_block(&arm.body.node, calls);
                }
            }
        }
    }

    fn collect_calls_expr(expr: &Expr, calls: &mut Vec<String>) {
        match expr {
            Expr::Call { path, args, .. } => {
                // Extract the function name (last segment for cross-module calls)
                let dotted = path.node.as_dotted();
                let fn_name = dotted.rsplit('.').next().unwrap_or(&dotted);
                calls.push(fn_name.to_string());
                for arg in args {
                    Self::collect_calls_expr(&arg.node, calls);
                }
            }
            Expr::BinOp { lhs, rhs, .. } => {
                Self::collect_calls_expr(&lhs.node, calls);
                Self::collect_calls_expr(&rhs.node, calls);
            }
            Expr::Tuple(elems) | Expr::ArrayInit(elems) => {
                for e in elems {
                    Self::collect_calls_expr(&e.node, calls);
                }
            }
            Expr::FieldAccess { expr: inner, .. } | Expr::Index { expr: inner, .. } => {
                Self::collect_calls_expr(&inner.node, calls);
            }
            Expr::StructInit { fields, .. } => {
                for (_, val) in fields {
                    Self::collect_calls_expr(&val.node, calls);
                }
            }
            Expr::Literal(_) | Expr::Var(_) => {}
        }
    }

    /// Collect module prefixes used in calls and variable access within a block.
    pub(super) fn collect_used_modules_block(block: &Block, used: &mut BTreeSet<String>) {
        for stmt in &block.stmts {
            Self::collect_used_modules_stmt(&stmt.node, used);
        }
        if let Some(tail) = &block.tail_expr {
            Self::collect_used_modules_expr(&tail.node, used);
        }
    }

    fn collect_used_modules_stmt(stmt: &Stmt, used: &mut BTreeSet<String>) {
        match stmt {
            Stmt::Let { init, .. } => Self::collect_used_modules_expr(&init.node, used),
            Stmt::Assign { value, .. } => Self::collect_used_modules_expr(&value.node, used),
            Stmt::If {
                cond,
                then_block,
                else_block,
            } => {
                Self::collect_used_modules_expr(&cond.node, used);
                Self::collect_used_modules_block(&then_block.node, used);
                if let Some(eb) = else_block {
                    Self::collect_used_modules_block(&eb.node, used);
                }
            }
            Stmt::For {
                start, end, body, ..
            } => {
                Self::collect_used_modules_expr(&start.node, used);
                Self::collect_used_modules_expr(&end.node, used);
                Self::collect_used_modules_block(&body.node, used);
            }
            Stmt::TupleAssign { value, .. } => Self::collect_used_modules_expr(&value.node, used),
            Stmt::Expr(expr) => Self::collect_used_modules_expr(&expr.node, used),
            Stmt::Return(Some(val)) => Self::collect_used_modules_expr(&val.node, used),
            Stmt::Return(None) => {}
            Stmt::Reveal { fields, .. } | Stmt::Seal { fields, .. } => {
                for (_, val) in fields {
                    Self::collect_used_modules_expr(&val.node, used);
                }
            }
            Stmt::Asm { .. } => {}
            Stmt::Match { expr, arms } => {
                Self::collect_used_modules_expr(&expr.node, used);
                for arm in arms {
                    Self::collect_used_modules_block(&arm.body.node, used);
                }
            }
        }
    }

    fn collect_used_modules_expr(expr: &Expr, used: &mut BTreeSet<String>) {
        match expr {
            Expr::Call { path, args, .. } => {
                let dotted = path.node.as_dotted();
                // "module.func" -> module is used
                if let Some(dot_pos) = dotted.rfind('.') {
                    let prefix = &dotted[..dot_pos];
                    used.insert(prefix.to_string());
                }
                for arg in args {
                    Self::collect_used_modules_expr(&arg.node, used);
                }
            }
            Expr::Var(name) => {
                // "module.CONST" -> module is used
                if let Some(dot_pos) = name.rfind('.') {
                    let prefix = &name[..dot_pos];
                    used.insert(prefix.to_string());
                }
            }
            Expr::BinOp { lhs, rhs, .. } => {
                Self::collect_used_modules_expr(&lhs.node, used);
                Self::collect_used_modules_expr(&rhs.node, used);
            }
            Expr::Tuple(elems) | Expr::ArrayInit(elems) => {
                for e in elems {
                    Self::collect_used_modules_expr(&e.node, used);
                }
            }
            Expr::FieldAccess { expr: inner, .. } | Expr::Index { expr: inner, .. } => {
                Self::collect_used_modules_expr(&inner.node, used);
            }
            Expr::StructInit { path, fields } => {
                let dotted = path.node.as_dotted();
                if let Some(dot_pos) = dotted.rfind('.') {
                    let prefix = &dotted[..dot_pos];
                    used.insert(prefix.to_string());
                }
                for (_, val) in fields {
                    Self::collect_used_modules_expr(&val.node, used);
                }
            }
            Expr::Literal(_) => {}
        }
    }
}
