use std::collections::BTreeMap;

use super::model::{create_cost_model, CostModel, TableCost};
use super::visit::next_power_of_two;
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
    pub estimated_proving_ns: u64,
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
    pub(crate) cost_model: &'a dyn CostModel,
    /// Function bodies indexed by name (for resolving calls).
    pub(crate) fn_bodies: BTreeMap<String, FnDef>,
    /// Cached function costs to avoid recomputation.
    fn_costs: BTreeMap<String, TableCost>,
    /// Recursion guard to prevent infinite loops in cost computation.
    in_progress: Vec<String>,
    /// H0004: collected loop bound waste entries (fn_name, end_value, bound).
    pub(crate) loop_bound_waste: Vec<(String, u64, u64)>,
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
            fn_bodies: BTreeMap::new(),
            fn_costs: BTreeMap::new(),
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

        // Proving time estimate: padded_height * columns * log2(ph) * 3 nanoseconds per field op
        let columns = self.cost_model.trace_column_count();
        let log2_padded = 64 - padded_height.leading_zeros() as u64;
        let estimated_proving_ns = padded_height * columns * log2_padded * 3;

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
            estimated_proving_ns,
            loop_bound_waste: std::mem::take(&mut self.loop_bound_waste),
        }
    }

    pub(crate) fn cost_fn(&mut self, func: &FnDef) -> TableCost {
        if let Some(cached) = self.fn_costs.get(&func.name.node) {
            return *cached;
        }

        // Recursion guard: if this function is already being analyzed,
        // return ZERO to break the cycle.
        if self.in_progress.contains(&func.name.node) {
            return TableCost::ZERO;
        }

        let depth_before = self.in_progress.len();
        self.in_progress.push(func.name.node.clone());

        let cost = if let Some(body) = &func.body {
            self.cost_block(&body.node)
        } else {
            TableCost::ZERO
        };

        self.in_progress.pop();

        // Only cache if we're at the top-level call (no recursion in flight).
        // Costs computed during active recursion are underestimates because
        // recursive calls are costed as ZERO.
        if depth_before == 0 {
            self.fn_costs.insert(func.name.node.clone(), cost);
        }
        cost
    }

    pub(crate) fn cost_block(&mut self, block: &Block) -> TableCost {
        let mut cost = TableCost::ZERO;
        for stmt in &block.stmts {
            cost = cost.add(&self.cost_stmt(&stmt.node));
        }
        if let Some(tail) = &block.tail_expr {
            cost = cost.add(&self.cost_expr(&tail.node));
        }
        cost
    }

    pub(crate) fn cost_stmt(&mut self, stmt: &Stmt) -> TableCost {
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
                    // Non-constant loop bound with no `bounded` annotation.
                    // Default to 1 iteration but record as a warning so
                    // report.rs can flag it via H0004.
                    self.loop_bound_waste.push((
                        self.in_progress.last().cloned().unwrap_or_default(),
                        1, // assumed iterations
                        0, // no declared bound (0 signals "unknown")
                    ));
                    1
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
                // All non-wildcard arms need comparison overhead
                let num_checked_arms = arms
                    .iter()
                    .filter(|a| !matches!(a.pattern.node, MatchPattern::Wildcard))
                    .count() as u64;
                let check_cost = arm_overhead.scale(num_checked_arms);
                // Worst-case body: max across all arms
                let max_body = arms
                    .iter()
                    .map(|a| self.cost_block(&a.body.node))
                    .fold(TableCost::ZERO, |acc, c| acc.max(&c));
                scrutinee_cost.add(&check_cost).add(&max_body)
            }
            Stmt::Seal { fields, .. } => {
                // push tag + field exprs + padding pushes + hash + write_io 5
                // Hash rate is 10 (tag + up to 9 fields); excess fields need extra hashes.
                let mut cost = stack_op.clone(); // push tag
                for (_name, val) in fields {
                    cost = cost.add(&self.cost_expr(&val.node));
                }
                let padding = 9usize.saturating_sub(fields.len());
                for _ in 0..padding {
                    cost = cost.add(&stack_op); // push 0 padding
                }
                // hash (one per 10 elements; extra hashes if >9 fields)
                let hash_count = (1 + fields.len()).div_ceil(10);
                for _ in 0..hash_count {
                    cost = cost.add(&self.cost_model.builtin_cost("hash"));
                }
                // write_io 5
                cost = cost.add(&self.cost_model.builtin_cost("pub_write5"));
                cost
            }
        }
    }
}
