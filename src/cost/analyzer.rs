use std::collections::HashMap;

use super::model::{create_cost_model, CostModel, TableCost};
use crate::ast::*;

// --- Per-function cost result ---

/// Cost analysis result for a single function.
#[derive(Clone, Debug)]
pub struct FunctionCost {
    pub name: String,
    pub cost: TableCost,
    /// If this function contains a loop, per-iteration cost.
    pub per_iteration: Option<(TableCost, u64)>,
}

/// Cost analysis result for the full program.
#[derive(Clone, Debug)]
pub struct ProgramCost {
    pub program_name: String,
    pub functions: Vec<FunctionCost>,
    pub total: TableCost,
    /// Table names from the CostModel (e.g. ["processor", "hash", ...]).
    pub table_names: Vec<String>,
    /// Short display names (e.g. ["cc", "hash", ...]).
    pub table_short_names: Vec<String>,
    /// Program attestation adds ceil(instruction_count / 10) * 6 hash rows.
    pub attestation_hash_rows: u64,
    pub padded_height: u64,
    pub estimated_proving_secs: f64,
    /// H0004: loops where declared bound >> actual constant end.
    pub loop_bound_waste: Vec<(String, u64, u64)>, // (fn_name, end_value, bound)
}

impl ProgramCost {
    /// Short names as str slice refs (for passing to TableCost methods).
    pub fn short_names(&self) -> Vec<&str> {
        self.table_short_names.iter().map(|s| s.as_str()).collect()
    }

    /// Long names as str slice refs.
    pub fn long_names(&self) -> Vec<&str> {
        self.table_names.iter().map(|s| s.as_str()).collect()
    }
}

// --- Cost analyzer ---

/// Computes static cost by walking the AST.
///
/// The analyzer is parameterized by a `CostModel` that provides all
/// target-specific cost constants.
pub(crate) struct CostAnalyzer<'a> {
    /// Target-specific cost model.
    cost_model: &'a dyn CostModel,
    /// Function bodies indexed by name (for resolving calls).
    fn_bodies: HashMap<String, FnDef>,
    /// Cached function costs to avoid recomputation.
    fn_costs: HashMap<String, TableCost>,
    /// Recursion guard to prevent infinite loops in cost computation.
    in_progress: Vec<String>,
    /// H0004: collected loop bound waste entries (fn_name, end_value, bound).
    loop_bound_waste: Vec<(String, u64, u64)>,
}

impl Default for CostAnalyzer<'_> {
    fn default() -> Self {
        Self::for_target("triton")
    }
}

impl<'a> CostAnalyzer<'a> {
    /// Create an analyzer for the named target.
    pub(crate) fn for_target(target_name: &str) -> Self {
        Self::with_cost_model(create_cost_model(target_name))
    }

    /// Create an analyzer with a specific cost model.
    pub(crate) fn with_cost_model(cost_model: &'a dyn CostModel) -> Self {
        Self {
            cost_model,
            fn_bodies: HashMap::new(),
            fn_costs: HashMap::new(),
            in_progress: Vec::new(),
            loop_bound_waste: Vec::new(),
        }
    }

    /// Analyze a complete file and return the program cost.
    pub(crate) fn analyze_file(&mut self, file: &File) -> ProgramCost {
        // Collect all function definitions.
        for item in &file.items {
            if let Item::Fn(func) = &item.node {
                self.fn_bodies.insert(func.name.node.clone(), func.clone());
            }
        }

        // Compute cost for each function.
        let mut functions = Vec::new();
        let fn_names: Vec<String> = self.fn_bodies.keys().cloned().collect();
        for name in &fn_names {
            let func = self.fn_bodies.get(name).unwrap().clone();
            let cost = self.cost_fn(&func);
            let per_iteration = self.find_loop_iteration_cost(&func);
            functions.push(FunctionCost {
                name: name.clone(),
                cost,
                per_iteration,
            });
        }

        // Total cost: start from main if it exists, otherwise sum all.
        let total = if let Some(main_cost) = self.fn_costs.get("main") {
            main_cost.add(&self.cost_model.call_overhead()) // call main + halt
        } else {
            functions
                .iter()
                .fold(TableCost::ZERO, |acc, f| acc.add(&f.cost))
        };

        // Estimate program instruction count for attestation.
        // Rough heuristic: total first-table value (processor cycles) ≈ instruction count.
        let instruction_count = total.get(0).max(10);
        let hash_rows = self.cost_model.hash_rows_per_permutation();
        let attestation_hash_rows = instruction_count.div_ceil(10) * hash_rows;

        // Padded height includes attestation.
        let max_height = total.max_height().max(attestation_hash_rows);
        let padded_height = next_power_of_two(max_height);

        // Proving time estimate: padded_height * 300 columns * log2(ph) * 3ns field op
        let log_ph = (padded_height as f64).log2();
        let estimated_proving_secs = (padded_height as f64) * 300.0 * log_ph * 3e-9;

        // H0004: scan for loop bound waste (bound >> constant end)
        for item in &file.items {
            if let Item::Fn(func) = &item.node {
                if let Some(body) = &func.body {
                    self.scan_loop_bound_waste(&func.name.node, &body.node);
                }
            }
        }

        ProgramCost {
            program_name: file.name.node.clone(),
            functions,
            total,
            table_names: self
                .cost_model
                .table_names()
                .iter()
                .map(|s| s.to_string())
                .collect(),
            table_short_names: self
                .cost_model
                .table_short_names()
                .iter()
                .map(|s| s.to_string())
                .collect(),
            attestation_hash_rows,
            padded_height,
            estimated_proving_secs,
            loop_bound_waste: std::mem::take(&mut self.loop_bound_waste),
        }
    }

    fn cost_fn(&mut self, func: &FnDef) -> TableCost {
        if let Some(cached) = self.fn_costs.get(&func.name.node) {
            return cached.clone();
        }

        // Recursion guard.
        if self.in_progress.contains(&func.name.node) {
            return TableCost::ZERO;
        }
        self.in_progress.push(func.name.node.clone());

        let cost = if let Some(body) = &func.body {
            self.cost_block(&body.node)
        } else {
            TableCost::ZERO
        };

        self.in_progress.pop();
        self.fn_costs.insert(func.name.node.clone(), cost.clone());
        cost
    }

    fn cost_block(&mut self, block: &Block) -> TableCost {
        let mut cost = TableCost::ZERO;
        for stmt in &block.stmts {
            cost = cost.add(&self.cost_stmt(&stmt.node));
        }
        if let Some(tail) = &block.tail_expr {
            cost = cost.add(&self.cost_expr(&tail.node));
        }
        cost
    }

    fn cost_stmt(&mut self, stmt: &Stmt) -> TableCost {
        let stack_op = self.cost_model.stack_op();
        match stmt {
            Stmt::Let { init, .. } => {
                // Cost of evaluating the init expression + stack placement.
                self.cost_expr(&init.node).add(&stack_op)
            }
            Stmt::Assign { value, .. } => {
                // Cost of evaluating value + swap to replace old value.
                self.cost_expr(&value.node).add(&stack_op).add(&stack_op)
            }
            Stmt::TupleAssign { names, value } => {
                let mut cost = self.cost_expr(&value.node);
                // One swap+pop per element.
                for _ in names {
                    cost = cost.add(&stack_op).add(&stack_op);
                }
                cost
            }
            Stmt::If {
                cond,
                then_block,
                else_block,
            } => {
                let cond_cost = self.cost_expr(&cond.node);
                let then_cost = self.cost_block(&then_block.node);
                let else_cost = if let Some(eb) = else_block {
                    self.cost_block(&eb.node)
                } else {
                    TableCost::ZERO
                };
                // Worst case: max of then/else branches.
                cond_cost
                    .add(&then_cost.max(&else_cost))
                    .add(&self.cost_model.if_overhead())
            }
            Stmt::For {
                end, bound, body, ..
            } => {
                let end_cost = self.cost_expr(&end.node);
                let body_cost = self.cost_block(&body.node);
                // Use declared bound if available, otherwise use end expr as literal.
                let iterations = if let Some(b) = bound {
                    *b
                } else if let Expr::Literal(Literal::Integer(n)) = &end.node {
                    *n
                } else {
                    1 // unknown, conservative fallback
                };
                // Per-iteration: body + loop overhead (dup, check, decrement, recurse).
                let per_iter = body_cost.add(&self.cost_model.loop_overhead());
                end_cost.add(&per_iter.scale(iterations))
            }
            Stmt::Expr(expr) => self.cost_expr(&expr.node),
            Stmt::Return(val) => {
                if let Some(v) = val {
                    self.cost_expr(&v.node)
                } else {
                    TableCost::ZERO
                }
            }
            Stmt::Reveal { fields, .. } => {
                // push tag + write_io 1 + (field expr + write_io 1) per field
                let io_cost = self.cost_model.builtin_cost("pub_write");
                let mut cost = stack_op.clone(); // push tag
                cost = cost.add(&io_cost); // write_io 1 for tag
                for (_name, val) in fields {
                    cost = cost.add(&self.cost_expr(&val.node));
                    cost = cost.add(&io_cost); // write_io 1
                }
                cost
            }
            Stmt::Asm { body, .. } => {
                // Conservative estimate: count non-empty, non-comment lines as stack ops
                let line_count = body
                    .lines()
                    .filter(|l| {
                        let t = l.trim();
                        !t.is_empty() && !t.starts_with("//")
                    })
                    .count() as u64;
                stack_op.scale(line_count)
            }
            Stmt::Match { expr, arms } => {
                let scrutinee_cost = self.cost_expr(&expr.node);
                // Per arm: dup + push + eq + skiz/call overhead = ~5 rows
                let arm_overhead = stack_op.scale(3).add(&self.cost_model.if_overhead());
                let num_literal_arms = arms
                    .iter()
                    .filter(|a| matches!(a.pattern.node, MatchPattern::Literal(_)))
                    .count() as u64;
                let check_cost = arm_overhead.scale(num_literal_arms);
                // Worst-case body: max across all arms
                let max_body = arms
                    .iter()
                    .map(|a| self.cost_block(&a.body.node))
                    .fold(TableCost::ZERO, |acc, c| acc.max(&c));
                scrutinee_cost.add(&check_cost).add(&max_body)
            }
            Stmt::Seal { fields, .. } => {
                // push tag + field exprs + padding pushes + hash + write_io 5
                let mut cost = stack_op.clone(); // push tag
                for (_name, val) in fields {
                    cost = cost.add(&self.cost_expr(&val.node));
                }
                let padding = 10 - 1 - fields.len();
                for _ in 0..padding {
                    cost = cost.add(&stack_op); // push 0 padding
                }
                // hash
                cost = cost.add(&self.cost_model.builtin_cost("hash"));
                // write_io 5
                cost = cost.add(&self.cost_model.builtin_cost("pub_write5"));
                cost
            }
        }
    }

    fn cost_expr(&mut self, expr: &Expr) -> TableCost {
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
    fn find_loop_iteration_cost(&mut self, func: &FnDef) -> Option<(TableCost, u64)> {
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
    fn collect_block_costs(
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
    fn scan_loop_bound_waste(&mut self, fn_name: &str, block: &Block) {
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

/// Find the index of the matching closing brace for a `{` at position `start`.
pub fn find_matching_brace(s: &str, start: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    if bytes.get(start) != Some(&b'{') {
        return None;
    }
    let mut depth = 0i32;
    for (i, &b) in bytes[start..].iter().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(start + i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Smallest power of 2 >= n.
pub(crate) fn next_power_of_two(n: u64) -> u64 {
    if n <= 1 {
        return 1;
    }
    1u64 << (64 - (n - 1).leading_zeros())
}
