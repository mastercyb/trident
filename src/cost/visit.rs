use super::analyzer::CostAnalyzer;
use super::model::TableCost;
use crate::ast::*;

// --- Per-function cost result ---

/// Cost analysis result for a single function.

impl<'a> CostAnalyzer<'a> {
    pub(crate) fn cost_expr(&mut self, expr: &Expr) -> TableCost {
        let stack_op = self.cost_model.stack_op();
        match expr {
            Expr::Literal(_) => {
                // push instruction: 1 cc, 1 opstack.
                stack_op
            }
            Expr::Var(_) => {
                // dup instruction: 1 cc, 1 opstack.
                stack_op
            }
            Expr::BinOp { op, lhs, rhs } => {
                let lhs_cost = self.cost_expr(&lhs.node);
                let rhs_cost = self.cost_expr(&rhs.node);
                lhs_cost.add(&rhs_cost).add(&self.cost_model.binop_cost(op))
            }
            Expr::Call { path, args, .. } => {
                let fn_name = path.node.as_dotted();
                let args_cost = args
                    .iter()
                    .fold(TableCost::ZERO, |acc, a| acc.add(&self.cost_expr(&a.node)));

                // Check if it's a builtin — try full name first, then short name
                // to handle cross-module calls like "hash.tip5" → "tip5" → "hash"
                let base_name = fn_name.rsplit('.').next().unwrap_or(&fn_name);
                let fn_cost = {
                    let c = self.cost_model.builtin_cost(&fn_name);
                    if c.is_nonzero() {
                        c
                    } else {
                        self.cost_model.builtin_cost(base_name)
                    }
                };
                if fn_cost.is_nonzero() {
                    // Builtin: use the cost table.
                    args_cost.add(&fn_cost)
                } else {
                    // User-defined: look up body cost + call overhead.
                    let body_cost = if let Some(func) = self.fn_bodies.get(base_name).cloned() {
                        self.cost_fn(&func)
                    } else {
                        TableCost::ZERO
                    };
                    args_cost
                        .add(&body_cost)
                        .add(&self.cost_model.call_overhead())
                }
            }
            Expr::FieldAccess { expr: inner, .. } => {
                // Evaluate inner struct + dup field elements.
                self.cost_expr(&inner.node).add(&stack_op)
            }
            Expr::Index { expr: inner, .. } => {
                // Evaluate inner array + dup indexed element.
                self.cost_expr(&inner.node).add(&stack_op)
            }
            Expr::StructInit { fields, .. } => {
                fields.iter().fold(TableCost::ZERO, |acc, (_, val)| {
                    acc.add(&self.cost_expr(&val.node))
                })
            }
            Expr::ArrayInit(elems) => elems
                .iter()
                .fold(TableCost::ZERO, |acc, e| acc.add(&self.cost_expr(&e.node))),
            Expr::Tuple(elems) => elems
                .iter()
                .fold(TableCost::ZERO, |acc, e| acc.add(&self.cost_expr(&e.node))),
        }
    }

    /// Find the first loop in a function and return its per-iteration cost + bound.
    pub(crate) fn find_loop_iteration_cost(&mut self, func: &FnDef) -> Option<(TableCost, u64)> {
        if let Some(body) = &func.body {
            for stmt in &body.node.stmts {
                if let Stmt::For {
                    bound,
                    body: loop_body,
                    end,
                    ..
                } = &stmt.node
                {
                    let body_cost = self.cost_block(&loop_body.node);
                    let per_iter = body_cost.add(&self.cost_model.loop_overhead());
                    let iterations = if let Some(b) = bound {
                        *b
                    } else if let Expr::Literal(Literal::Integer(n)) = &end.node {
                        *n
                    } else {
                        1
                    };
                    return Some((per_iter, iterations));
                }
            }
        }
        None
    }

    /// Collect per-statement costs mapped to line numbers.
    ///
    /// Walks every function body and records the cost of each statement
    /// along with the 1-based line number derived from the statement's span.
    /// Also records function definition lines with call overhead.
    pub(crate) fn stmt_costs(&mut self, file: &File, source: &str) -> Vec<(u32, TableCost)> {
        // Build line offset table: line_starts[i] = byte offset of line i+1
        let line_starts: Vec<u32> = std::iter::once(0)
            .chain(source.bytes().enumerate().filter_map(|(i, b)| {
                if b == b'\n' {
                    Some((i + 1) as u32)
                } else {
                    None
                }
            }))
            .collect();

        let byte_to_line = |offset: u32| -> u32 {
            match line_starts.binary_search(&offset) {
                Ok(i) => (i + 1) as u32,
                Err(i) => i as u32,
            }
        };

        // Ensure fn_bodies are populated (analyze_file does this, but in case
        // stmt_costs is called standalone)
        for item in &file.items {
            if let Item::Fn(func) = &item.node {
                self.fn_bodies
                    .entry(func.name.node.clone())
                    .or_insert_with(|| func.clone());
            }
        }

        let mut result: Vec<(u32, TableCost)> = Vec::new();

        for item in &file.items {
            if let Item::Fn(func) = &item.node {
                // Record function header with call overhead
                let fn_line = byte_to_line(item.span.start);
                result.push((fn_line, self.cost_model.call_overhead()));

                if let Some(body) = &func.body {
                    self.collect_block_costs(&body.node, &byte_to_line, &mut result);
                }
            }
        }

        result.sort_by_key(|(line, _)| *line);
        result
    }

    /// Recursively collect costs for all statements in a block.
    pub(crate) fn collect_block_costs(
        &mut self,
        block: &Block,
        byte_to_line: &dyn Fn(u32) -> u32,
        result: &mut Vec<(u32, TableCost)>,
    ) {
        for stmt in &block.stmts {
            let line = byte_to_line(stmt.span.start);
            let cost = self.cost_stmt(&stmt.node);
            result.push((line, cost));

            // Recurse into nested blocks
            match &stmt.node {
                Stmt::If {
                    then_block,
                    else_block,
                    ..
                } => {
                    self.collect_block_costs(&then_block.node, byte_to_line, result);
                    if let Some(eb) = else_block {
                        self.collect_block_costs(&eb.node, byte_to_line, result);
                    }
                }
                Stmt::For { body, .. } => {
                    self.collect_block_costs(&body.node, byte_to_line, result);
                }
                Stmt::Match { arms, .. } => {
                    for arm in arms {
                        self.collect_block_costs(&arm.body.node, byte_to_line, result);
                    }
                }
                _ => {}
            }
        }

        if let Some(tail) = &block.tail_expr {
            let line = byte_to_line(tail.span.start);
            let cost = self.cost_expr(&tail.node);
            result.push((line, cost));
        }
    }

    /// H0004: scan a block for loops where declared bound >> constant end value.
    pub(crate) fn scan_loop_bound_waste(&mut self, fn_name: &str, block: &Block) {
        for stmt in &block.stmts {
            if let Stmt::For {
                end, bound, body, ..
            } = &stmt.node
            {
                // Check if end is a constant and bound is declared
                if let (Some(declared_bound), Expr::Literal(Literal::Integer(end_val))) =
                    (bound, &end.node)
                {
                    if *declared_bound > *end_val * 4 && *declared_bound > 8 {
                        self.loop_bound_waste.push((
                            fn_name.to_string(),
                            *end_val,
                            *declared_bound,
                        ));
                    }
                }
                // Recurse into loop body
                self.scan_loop_bound_waste(fn_name, &body.node);
            }
            // Recurse into if/else blocks
            if let Stmt::If {
                then_block,
                else_block,
                ..
            } = &stmt.node
            {
                self.scan_loop_bound_waste(fn_name, &then_block.node);
                if let Some(eb) = else_block {
                    self.scan_loop_bound_waste(fn_name, &eb.node);
                }
            }
            // Recurse into match arms
            if let Stmt::Match { arms, .. } = &stmt.node {
                for arm in arms {
                    self.scan_loop_bound_waste(fn_name, &arm.body.node);
                }
            }
        }
    }
}

/// Smallest power of 2 >= n.
///
/// Delegates to `field::proof::padded_height` — same formula, different name.
pub(crate) fn next_power_of_two(n: u64) -> u64 {
    crate::field::proof::padded_height(n)
}
