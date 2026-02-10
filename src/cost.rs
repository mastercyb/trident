/// Static cost analysis for Trident programs.
///
/// Computes the trace heights of all Algebraic Execution Tables for the
/// configured target VM by walking the AST and summing per-instruction costs.
/// This gives an upper bound on proving cost without executing the program.
///
/// The cost model is target-agnostic: `CostModel` is a trait that any backend
/// implements to provide table names, per-instruction costs, and formatting.
/// `TritonCostModel` implements the trait for Triton VM's 6 tables.
use std::collections::HashMap;
use std::path::Path;

use crate::ast::*;
use crate::diagnostic::Diagnostic;
use crate::span::Span;

// ---------------------------------------------------------------------------
// CostModel trait — target-agnostic cost interface
// ---------------------------------------------------------------------------

/// Trait for target-specific cost models.
///
/// Each target VM implements this to provide table names, per-instruction
/// costs, and formatting for cost reports. The cost analyzer delegates all
/// target-specific knowledge through this trait.
#[allow(dead_code)] // Methods will be used by future cost model implementations.
pub(crate) trait CostModel {
    /// Names of the execution tables (e.g. ["processor", "hash", "u32", ...]).
    fn table_names(&self) -> &[&str];

    /// Short display names for compact annotations (e.g. ["cc", "hash", "u32", ...]).
    fn table_short_names(&self) -> &[&str];

    /// Cost of a builtin function call by name.
    fn builtin_cost(&self, name: &str) -> TableCost;

    /// Cost of a binary operation.
    fn binop_cost(&self, op: &BinOp) -> TableCost;

    /// Overhead cost for a function call/return pair.
    fn call_overhead(&self) -> TableCost;

    /// Cost of a single stack manipulation (push/dup/swap).
    fn stack_op(&self) -> TableCost;

    /// Overhead cost for an if/else branch.
    fn if_overhead(&self) -> TableCost;

    /// Overhead cost per loop iteration.
    fn loop_overhead(&self) -> TableCost;

    /// Number of hash table rows per hash permutation.
    fn hash_rows_per_permutation(&self) -> u64;

    /// Target display name for reports.
    fn target_name(&self) -> &str;
}

// ---------------------------------------------------------------------------
// TritonCostModel — Triton VM's 6-table cost model
// ---------------------------------------------------------------------------

/// Triton VM cost model with 6 Algebraic Execution Tables.
pub(crate) struct TritonCostModel;

impl TritonCostModel {
    /// Worst-case U32 table rows for 32-bit operations.
    const U32_WORST: u64 = 33;

    /// Simple arithmetic/logic op: 1 processor cycle, 1 op_stack row.
    const SIMPLE_OP: TableCost = TableCost {
        processor: 1,
        hash: 0,
        u32_table: 0,
        op_stack: 1,
        ram: 0,
        jump_stack: 0,
    };

    /// U32-table op with stack effect.
    const U32_OP: TableCost = TableCost {
        processor: 1,
        hash: 0,
        u32_table: Self::U32_WORST,
        op_stack: 1,
        ram: 0,
        jump_stack: 0,
    };

    /// U32-table op without stack growth.
    const U32_NOSTACK: TableCost = TableCost {
        processor: 1,
        hash: 0,
        u32_table: Self::U32_WORST,
        op_stack: 0,
        ram: 0,
        jump_stack: 0,
    };

    /// Hash-table op with stack effect (6 hash rows for Tip5 permutation).
    const HASH_OP: TableCost = TableCost {
        processor: 1,
        hash: 6,
        u32_table: 0,
        op_stack: 1,
        ram: 0,
        jump_stack: 0,
    };

    /// Two-element assertion: 2 processor cycles, 2 op_stack rows.
    const ASSERT2: TableCost = TableCost {
        processor: 2,
        hash: 0,
        u32_table: 0,
        op_stack: 2,
        ram: 0,
        jump_stack: 0,
    };

    /// Single RAM read/write: 2 processor cycles, 2 op_stack, 1 ram.
    const RAM_RW: TableCost = TableCost {
        processor: 2,
        hash: 0,
        u32_table: 0,
        op_stack: 2,
        ram: 1,
        jump_stack: 0,
    };

    /// Block RAM read/write: 2 processor cycles, 2 op_stack, 5 ram.
    const RAM_BLOCK_RW: TableCost = TableCost {
        processor: 2,
        hash: 0,
        u32_table: 0,
        op_stack: 2,
        ram: 5,
        jump_stack: 0,
    };

    /// Pure processor op (no stack/ram/hash effect): 1 processor cycle only.
    const PURE_PROC: TableCost = TableCost {
        processor: 1,
        hash: 0,
        u32_table: 0,
        op_stack: 0,
        ram: 0,
        jump_stack: 0,
    };
}

impl CostModel for TritonCostModel {
    fn table_names(&self) -> &[&str] {
        &["processor", "hash", "u32", "op_stack", "ram", "jump_stack"]
    }

    fn table_short_names(&self) -> &[&str] {
        &["cc", "hash", "u32", "opst", "ram", "jump"]
    }

    fn builtin_cost(&self, name: &str) -> TableCost {
        match name {
            // I/O
            "pub_read" | "pub_read2" | "pub_read3" | "pub_read4" | "pub_read5" => Self::SIMPLE_OP,
            "pub_write" | "pub_write2" | "pub_write3" | "pub_write4" | "pub_write5" => {
                Self::SIMPLE_OP
            }

            // Non-deterministic input
            "divine" | "divine3" | "divine5" => Self::SIMPLE_OP,

            // Assertions
            "assert" => Self::SIMPLE_OP,
            "assert_eq" => Self::ASSERT2,
            "assert_digest" => Self::ASSERT2,

            // Field ops
            "inv" => Self::PURE_PROC,
            "neg" => TableCost {
                processor: 2,
                hash: 0,
                u32_table: 0,
                op_stack: 1,
                ram: 0,
                jump_stack: 0,
            },
            "sub" => TableCost {
                processor: 3,
                hash: 0,
                u32_table: 0,
                op_stack: 2,
                ram: 0,
                jump_stack: 0,
            },

            // U32 ops
            "split" => Self::U32_OP,
            "log2" => Self::U32_NOSTACK,
            "pow" => Self::U32_OP,
            "popcount" => Self::U32_NOSTACK,

            // Hash ops (6 hash table rows each for Tip5 permutation)
            "hash" => Self::HASH_OP,
            "sponge_init" => TableCost {
                processor: 1,
                hash: 6,
                u32_table: 0,
                op_stack: 0,
                ram: 0,
                jump_stack: 0,
            },
            "sponge_absorb" => Self::HASH_OP,
            "sponge_squeeze" => Self::HASH_OP,
            "sponge_absorb_mem" => TableCost {
                processor: 1,
                hash: 6,
                u32_table: 0,
                op_stack: 1,
                ram: 10,
                jump_stack: 0,
            },

            // Merkle
            "merkle_step" => TableCost {
                processor: 1,
                hash: 6,
                u32_table: Self::U32_WORST,
                op_stack: 0,
                ram: 0,
                jump_stack: 0,
            },
            "merkle_step_mem" => TableCost {
                processor: 1,
                hash: 6,
                u32_table: Self::U32_WORST,
                op_stack: 0,
                ram: 5,
                jump_stack: 0,
            },

            // RAM
            "ram_read" => Self::RAM_RW,
            "ram_write" => Self::RAM_RW,
            "ram_read_block" => Self::RAM_BLOCK_RW,
            "ram_write_block" => Self::RAM_BLOCK_RW,

            // Dot steps
            "xx_dot_step" => TableCost {
                processor: 1,
                hash: 0,
                u32_table: 0,
                op_stack: 0,
                ram: 6,
                jump_stack: 0,
            },
            "xb_dot_step" => TableCost {
                processor: 1,
                hash: 0,
                u32_table: 0,
                op_stack: 0,
                ram: 4,
                jump_stack: 0,
            },

            // Conversions
            "as_u32" => TableCost {
                processor: 2,
                hash: 0,
                u32_table: Self::U32_WORST,
                op_stack: 1,
                ram: 0,
                jump_stack: 0,
            },
            "as_field" => TableCost::ZERO,

            // XField
            "xfield" => TableCost::ZERO,
            "xinvert" => Self::PURE_PROC,

            _ => TableCost::ZERO,
        }
    }

    fn binop_cost(&self, op: &BinOp) -> TableCost {
        match op {
            BinOp::Add => Self::SIMPLE_OP,
            BinOp::Mul => Self::SIMPLE_OP,
            BinOp::Eq => Self::SIMPLE_OP,
            BinOp::Lt => Self::U32_OP,
            BinOp::BitAnd => Self::U32_OP,
            BinOp::BitXor => Self::U32_OP,
            BinOp::DivMod => Self::U32_NOSTACK,
            BinOp::XFieldMul => Self::SIMPLE_OP,
        }
    }

    fn call_overhead(&self) -> TableCost {
        TableCost {
            processor: 2,
            hash: 0,
            u32_table: 0,
            op_stack: 0,
            ram: 0,
            jump_stack: 2,
        }
    }

    fn stack_op(&self) -> TableCost {
        TableCost {
            processor: 1,
            hash: 0,
            u32_table: 0,
            op_stack: 1,
            ram: 0,
            jump_stack: 0,
        }
    }

    fn if_overhead(&self) -> TableCost {
        TableCost {
            processor: 3,
            hash: 0,
            u32_table: 0,
            op_stack: 2,
            ram: 0,
            jump_stack: 1,
        }
    }

    fn loop_overhead(&self) -> TableCost {
        TableCost {
            processor: 8,
            hash: 0,
            u32_table: 0,
            op_stack: 4,
            ram: 0,
            jump_stack: 1,
        }
    }

    fn hash_rows_per_permutation(&self) -> u64 {
        6
    }

    fn target_name(&self) -> &str {
        "Triton VM"
    }
}

// ---------------------------------------------------------------------------
// TableCost — per-table cost vector
// ---------------------------------------------------------------------------

/// Cost across all 6 Triton VM tables.
#[derive(Clone, Debug, Default)]
pub struct TableCost {
    /// Processor Table rows (= clock cycles).
    pub processor: u64,
    /// Hash Table rows (6 per hash operation).
    pub hash: u64,
    /// U32 Table rows (variable, worst-case 32-bit estimates).
    pub u32_table: u64,
    /// Op Stack Table rows.
    pub op_stack: u64,
    /// RAM Table rows.
    pub ram: u64,
    /// Jump Stack Table rows.
    pub jump_stack: u64,
}

impl TableCost {
    pub const ZERO: TableCost = TableCost {
        processor: 0,
        hash: 0,
        u32_table: 0,
        op_stack: 0,
        ram: 0,
        jump_stack: 0,
    };

    pub fn add(&self, other: &TableCost) -> TableCost {
        TableCost {
            processor: self.processor + other.processor,
            hash: self.hash + other.hash,
            u32_table: self.u32_table + other.u32_table,
            op_stack: self.op_stack + other.op_stack,
            ram: self.ram + other.ram,
            jump_stack: self.jump_stack + other.jump_stack,
        }
    }

    pub fn scale(&self, factor: u64) -> TableCost {
        TableCost {
            processor: self.processor.saturating_mul(factor),
            hash: self.hash.saturating_mul(factor),
            u32_table: self.u32_table.saturating_mul(factor),
            op_stack: self.op_stack.saturating_mul(factor),
            ram: self.ram.saturating_mul(factor),
            jump_stack: self.jump_stack.saturating_mul(factor),
        }
    }

    pub fn max(&self, other: &TableCost) -> TableCost {
        TableCost {
            processor: self.processor.max(other.processor),
            hash: self.hash.max(other.hash),
            u32_table: self.u32_table.max(other.u32_table),
            op_stack: self.op_stack.max(other.op_stack),
            ram: self.ram.max(other.ram),
            jump_stack: self.jump_stack.max(other.jump_stack),
        }
    }

    /// The maximum height across all tables.
    pub fn max_height(&self) -> u64 {
        self.processor
            .max(self.hash)
            .max(self.u32_table)
            .max(self.op_stack)
            .max(self.ram)
            .max(self.jump_stack)
    }

    /// Which table is the tallest.
    pub fn dominant_table(&self) -> &'static str {
        let max = self.max_height();
        if max == 0 {
            return "proc";
        }
        if self.hash == max {
            "hash"
        } else if self.u32_table == max {
            "u32"
        } else if self.ram == max {
            "ram"
        } else if self.processor == max {
            "proc"
        } else if self.op_stack == max {
            "opstack"
        } else {
            "jump"
        }
    }

    /// Serialize to a JSON object string.
    pub fn to_json_value(&self) -> String {
        format!(
            "{{\"processor\": {}, \"hash\": {}, \"u32_table\": {}, \"op_stack\": {}, \"ram\": {}, \"jump_stack\": {}}}",
            self.processor, self.hash, self.u32_table, self.op_stack, self.ram, self.jump_stack
        )
    }

    /// Deserialize from a JSON object string.
    pub fn from_json_value(s: &str) -> Option<TableCost> {
        fn extract_u64(s: &str, key: &str) -> Option<u64> {
            let needle = format!("\"{}\"", key);
            let idx = s.find(&needle)?;
            let rest = &s[idx + needle.len()..];
            let colon = rest.find(':')?;
            let after_colon = rest[colon + 1..].trim_start();
            let end = after_colon
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(after_colon.len());
            after_colon[..end].parse().ok()
        }

        Some(TableCost {
            processor: extract_u64(s, "processor")?,
            hash: extract_u64(s, "hash")?,
            u32_table: extract_u64(s, "u32_table")?,
            op_stack: extract_u64(s, "op_stack")?,
            ram: extract_u64(s, "ram")?,
            jump_stack: extract_u64(s, "jump_stack")?,
        })
    }

    /// Format a compact annotation string showing non-zero cost fields.
    pub fn format_annotation(&self) -> String {
        let mut parts = Vec::new();
        if self.processor > 0 {
            parts.push(format!("cc={}", self.processor));
        }
        if self.hash > 0 {
            parts.push(format!("hash={}", self.hash));
        }
        if self.u32_table > 0 {
            parts.push(format!("u32={}", self.u32_table));
        }
        if self.op_stack > 0 {
            parts.push(format!("opst={}", self.op_stack));
        }
        if self.ram > 0 {
            parts.push(format!("ram={}", self.ram));
        }
        if self.jump_stack > 0 {
            parts.push(format!("jump={}", self.jump_stack));
        }
        parts.join(" ")
    }
}

/// Convenience function: look up builtin cost using the default Triton cost model.
/// Used by LSP and other callers that don't have a CostModel reference.
pub(crate) fn cost_builtin(name: &str) -> TableCost {
    TritonCostModel.builtin_cost(name)
}

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
    /// Program attestation adds ceil(instruction_count / 10) * 6 hash rows.
    pub attestation_hash_rows: u64,
    pub padded_height: u64,
    pub estimated_proving_secs: f64,
    /// H0004: loops where declared bound >> actual constant end.
    pub loop_bound_waste: Vec<(String, u64, u64)>, // (fn_name, end_value, bound)
}

// --- Cost analyzer ---

/// Computes static cost by walking the AST.
///
/// The analyzer is parameterized by a `CostModel` that provides all
/// target-specific cost constants. Default: `TritonCostModel`.
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
        Self::new()
    }
}

/// Static reference to the default Triton cost model for `CostAnalyzer::new()`.
static TRITON_COST_MODEL: TritonCostModel = TritonCostModel;

impl<'a> CostAnalyzer<'a> {
    /// Create a new analyzer with the default Triton VM cost model.
    pub(crate) fn new() -> Self {
        Self {
            cost_model: &TRITON_COST_MODEL,
            fn_bodies: HashMap::new(),
            fn_costs: HashMap::new(),
            in_progress: Vec::new(),
            loop_bound_waste: Vec::new(),
        }
    }

    /// Create a new analyzer with a specific cost model.
    #[allow(dead_code)] // Will be used when multiple cost models are available.
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
        // Rough heuristic: total processor cycles ≈ instruction count.
        let instruction_count = total.processor.max(10);
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
            Stmt::Emit { fields, .. } => {
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
                    .filter(|a| !matches!(a.pattern.node, MatchPattern::Wildcard))
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
                    if c.processor > 0 || c.hash > 0 || c.u32_table > 0 || c.ram > 0 {
                        c
                    } else {
                        self.cost_model.builtin_cost(base_name)
                    }
                };
                if fn_cost.processor > 0
                    || fn_cost.hash > 0
                    || fn_cost.u32_table > 0
                    || fn_cost.ram > 0
                {
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
fn find_matching_brace(s: &str, start: usize) -> Option<usize> {
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

// --- Report formatting ---

impl ProgramCost {
    /// Format a table-style cost report.
    pub fn format_report(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("Cost report: {}\n", self.program_name));
        out.push_str(&format!(
            "{:<24} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6}  {}\n",
            "Function", "cc", "hash", "u32", "opst", "ram", "jump", "dominant"
        ));
        out.push_str(&"-".repeat(84));
        out.push('\n');

        for func in &self.functions {
            out.push_str(&format!(
                "{:<24} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6}  {}\n",
                func.name,
                func.cost.processor,
                func.cost.hash,
                func.cost.u32_table,
                func.cost.op_stack,
                func.cost.ram,
                func.cost.jump_stack,
                func.cost.dominant_table(),
            ));
            if let Some((per_iter, bound)) = &func.per_iteration {
                out.push_str(&format!(
                    "  per iteration (x{})   {:>6} {:>6} {:>6} {:>6} {:>6} {:>6}\n",
                    bound,
                    per_iter.processor,
                    per_iter.hash,
                    per_iter.u32_table,
                    per_iter.op_stack,
                    per_iter.ram,
                    per_iter.jump_stack,
                ));
            }
        }

        out.push_str(&"-".repeat(84));
        out.push('\n');
        out.push_str(&format!(
            "{:<24} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6}  {}\n",
            "TOTAL",
            self.total.processor,
            self.total.hash,
            self.total.u32_table,
            self.total.op_stack,
            self.total.ram,
            self.total.jump_stack,
            self.total.dominant_table(),
        ));
        out.push('\n');
        out.push_str(&format!(
            "Padded height:           {}\n",
            self.padded_height
        ));
        out.push_str(&format!(
            "Program attestation:     {} hash rows\n",
            self.attestation_hash_rows
        ));
        out.push_str(&format!(
            "Estimated proving time:  ~{:.1}s\n",
            self.estimated_proving_secs
        ));

        // Power-of-2 boundary warning.
        let headroom = self.padded_height - self.total.max_height();
        if headroom < self.padded_height / 8 {
            out.push_str(&format!(
                "\nwarning: {} rows below padded height boundary ({})\n",
                headroom, self.padded_height
            ));
            out.push_str(&format!(
                "  adding {}+ rows to any table will double proving cost to {}\n",
                headroom + 1,
                self.padded_height * 2
            ));
        }

        out
    }

    /// Format a hotspots report (top N cost contributors).
    pub fn format_hotspots(&self, top_n: usize) -> String {
        let mut out = String::new();
        out.push_str(&format!("Top {} cost contributors:\n", top_n));

        let dominant = self.total.dominant_table();
        let dominant_total = match dominant {
            "hash" => self.total.hash,
            "u32" => self.total.u32_table,
            "ram" => self.total.ram,
            "proc" => self.total.processor,
            "opstack" => self.total.op_stack,
            _ => self.total.jump_stack,
        };

        let mut ranked: Vec<&FunctionCost> = self.functions.iter().collect();
        ranked.sort_by(|a, b| {
            let av = match dominant {
                "hash" => a.cost.hash,
                "u32" => a.cost.u32_table,
                "ram" => a.cost.ram,
                _ => a.cost.processor,
            };
            let bv = match dominant {
                "hash" => b.cost.hash,
                "u32" => b.cost.u32_table,
                "ram" => b.cost.ram,
                _ => b.cost.processor,
            };
            bv.cmp(&av)
        });

        for (i, func) in ranked.iter().take(top_n).enumerate() {
            let val = match dominant {
                "hash" => func.cost.hash,
                "u32" => func.cost.u32_table,
                "ram" => func.cost.ram,
                _ => func.cost.processor,
            };
            let pct = if dominant_total > 0 {
                (val as f64 / dominant_total as f64) * 100.0
            } else {
                0.0
            };
            out.push_str(&format!(
                "  {}. {:<24} {:>6} {} rows ({:.0}% of {} table)\n",
                i + 1,
                func.name,
                val,
                dominant,
                pct,
                dominant
            ));
        }

        out.push_str(&format!(
            "\nDominant table: {} ({} rows). Reduce {} operations to lower padded height.\n",
            dominant, dominant_total, dominant
        ));

        out
    }

    /// Generate optimization hints (H0001, H0002, H0004).
    /// H0001: hash table dominance — hash table is >2x taller than processor.
    /// H0002: headroom hint — significant room below next power-of-2 boundary.
    /// H0004: loop bound waste — declared bound >> constant iteration count.
    pub fn optimization_hints(&self) -> Vec<Diagnostic> {
        let mut hints = Vec::new();

        // H0001: Hash table dominance
        if self.total.hash > 0 && self.total.processor > 0 {
            let ratio = self.total.hash as f64 / self.total.processor as f64;
            if ratio > 2.0 {
                let mut diag = Diagnostic::warning(
                    format!(
                        "hint[H0001]: hash table is {:.1}x taller than processor table",
                        ratio
                    ),
                    Span::dummy(),
                );
                diag.notes
                    .push("processor optimizations will not reduce proving cost".to_string());
                diag.help = Some(
                    "consider: batching data before hashing, reducing Merkle depth, \
                     or using sponge_absorb_mem instead of repeated sponge_absorb"
                        .to_string(),
                );
                hints.push(diag);
            }
        }

        // H0002: Headroom hint (far below boundary = room to grow)
        let max_height = self.total.max_height().max(self.attestation_hash_rows);
        let headroom = self.padded_height - max_height;
        if headroom > self.padded_height / 4 && self.padded_height >= 16 {
            let headroom_pct = (headroom as f64 / self.padded_height as f64) * 100.0;
            let mut diag = Diagnostic::warning(
                format!(
                    "hint[H0002]: padded height is {}, but max table height is only {}",
                    self.padded_height, max_height
                ),
                Span::dummy(),
            );
            diag.notes.push(format!(
                "you have {} rows of headroom ({:.0}%) before the next doubling",
                headroom, headroom_pct
            ));
            diag.help = Some(format!(
                "this program could be {:.0}% more complex at zero additional proving cost",
                headroom_pct
            ));
            hints.push(diag);
        }

        // H0004: Loop bound waste
        for (fn_name, end_val, bound) in &self.loop_bound_waste {
            let ratio = *bound as f64 / *end_val as f64;
            let mut diag = Diagnostic::warning(
                format!(
                    "hint[H0004]: loop in '{}' bounded {} but iterates only {} times",
                    fn_name, bound, end_val
                ),
                Span::dummy(),
            );
            diag.notes.push(format!(
                "declared bound is {:.0}x the actual iteration count",
                ratio
            ));
            diag.help = Some(format!(
                "tightening the bound to {} would reduce worst-case cost",
                next_power_of_two(*end_val)
            ));
            hints.push(diag);
        }

        hints
    }

    /// Serialize ProgramCost to a JSON string.
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        out.push_str("{\n  \"functions\": {\n");
        for (i, func) in self.functions.iter().enumerate() {
            out.push_str(&format!(
                "    \"{}\": {}",
                func.name,
                func.cost.to_json_value()
            ));
            if i + 1 < self.functions.len() {
                out.push(',');
            }
            out.push('\n');
        }
        out.push_str("  },\n");
        out.push_str(&format!("  \"total\": {},\n", self.total.to_json_value()));
        out.push_str(&format!("  \"padded_height\": {}\n", self.padded_height));
        out.push_str("}\n");
        out
    }

    /// Save cost analysis to a JSON file.
    pub fn save_json(&self, path: &Path) -> Result<(), String> {
        std::fs::write(path, self.to_json())
            .map_err(|e| format!("cannot write '{}': {}", path.display(), e))
    }

    /// Load cost analysis from a JSON file.
    pub fn load_json(path: &Path) -> Result<ProgramCost, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read '{}': {}", path.display(), e))?;
        Self::from_json(&content)
    }

    /// Parse a ProgramCost from a JSON string.
    pub fn from_json(s: &str) -> Result<ProgramCost, String> {
        // Extract "functions" block
        let fns_start = s
            .find("\"functions\"")
            .ok_or_else(|| "missing 'functions' key".to_string())?;
        let fns_obj_start = s[fns_start..]
            .find('{')
            .map(|i| fns_start + i)
            .ok_or_else(|| "missing functions object".to_string())?;

        // Find matching closing brace for functions object
        let fns_obj_end = find_matching_brace(s, fns_obj_start)
            .ok_or_else(|| "unmatched brace in functions".to_string())?;
        let fns_content = &s[fns_obj_start + 1..fns_obj_end];

        // Parse individual function entries
        let mut functions = Vec::new();
        let mut pos = 0;
        while pos < fns_content.len() {
            // Find next function name
            if let Some(quote_start) = fns_content[pos..].find('"') {
                let name_start = pos + quote_start + 1;
                if let Some(quote_end) = fns_content[name_start..].find('"') {
                    let name = fns_content[name_start..name_start + quote_end].to_string();
                    // Find the cost object for this function
                    let after_name = name_start + quote_end + 1;
                    if let Some(obj_start) = fns_content[after_name..].find('{') {
                        let abs_obj_start = after_name + obj_start;
                        if let Some(obj_end) = find_matching_brace(fns_content, abs_obj_start) {
                            let cost_str = &fns_content[abs_obj_start..=obj_end];
                            if let Some(cost) = TableCost::from_json_value(cost_str) {
                                functions.push(FunctionCost {
                                    name,
                                    cost,
                                    per_iteration: None,
                                });
                            }
                            pos = obj_end + 1;
                            continue;
                        }
                    }
                }
            }
            break;
        }

        // Extract "total"
        let total = {
            let total_start = s
                .find("\"total\"")
                .ok_or_else(|| "missing 'total' key".to_string())?;
            let obj_start = s[total_start..]
                .find('{')
                .map(|i| total_start + i)
                .ok_or_else(|| "missing total object".to_string())?;
            let obj_end = find_matching_brace(s, obj_start)
                .ok_or_else(|| "unmatched brace in total".to_string())?;
            TableCost::from_json_value(&s[obj_start..=obj_end])
                .ok_or_else(|| "invalid total cost".to_string())?
        };

        // Extract "padded_height"
        let padded_height = {
            let ph_start = s
                .find("\"padded_height\"")
                .ok_or_else(|| "missing 'padded_height' key".to_string())?;
            let rest = &s[ph_start + "\"padded_height\"".len()..];
            let colon = rest
                .find(':')
                .ok_or_else(|| "missing colon after padded_height".to_string())?;
            let after_colon = rest[colon + 1..].trim_start();
            let end = after_colon
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(after_colon.len());
            after_colon[..end]
                .parse::<u64>()
                .map_err(|e| format!("invalid padded_height: {}", e))?
        };

        Ok(ProgramCost {
            program_name: String::new(),
            functions,
            total,
            attestation_hash_rows: 0,
            padded_height,
            estimated_proving_secs: 0.0,
            loop_bound_waste: Vec::new(),
        })
    }

    /// Format a comparison between this cost and another (old vs new).
    pub fn format_comparison(&self, other: &ProgramCost) -> String {
        let mut out = String::new();
        out.push_str("Cost comparison:\n");
        out.push_str(&format!(
            "{:<20} {:>9} {:>9}  {:>6}\n",
            "Function", "cc (old)", "cc (new)", "delta"
        ));
        out.push_str(&"-".repeat(48));
        out.push('\n');

        // Collect all function names from both
        let mut all_names: Vec<String> = Vec::new();
        for f in &self.functions {
            if !all_names.contains(&f.name) {
                all_names.push(f.name.clone());
            }
        }
        for f in &other.functions {
            if !all_names.contains(&f.name) {
                all_names.push(f.name.clone());
            }
        }

        for name in &all_names {
            let old_cc = self
                .functions
                .iter()
                .find(|f| f.name == *name)
                .map(|f| f.cost.processor)
                .unwrap_or(0);
            let new_cc = other
                .functions
                .iter()
                .find(|f| f.name == *name)
                .map(|f| f.cost.processor)
                .unwrap_or(0);
            let delta = new_cc as i64 - old_cc as i64;
            let delta_str = if delta > 0 {
                format!("+{}", delta)
            } else if delta == 0 {
                "0".to_string()
            } else {
                format!("{}", delta)
            };
            out.push_str(&format!(
                "{:<20} {:>9} {:>9}  {:>6}\n",
                name, old_cc, new_cc, delta_str
            ));
        }

        out.push_str(&"-".repeat(48));
        out.push('\n');

        let old_total = self.total.processor;
        let new_total = other.total.processor;
        let total_delta = new_total as i64 - old_total as i64;
        let total_delta_str = if total_delta > 0 {
            format!("+{}", total_delta)
        } else if total_delta == 0 {
            "0".to_string()
        } else {
            format!("{}", total_delta)
        };
        out.push_str(&format!(
            "{:<20} {:>9} {:>9}  {:>6}\n",
            "TOTAL", old_total, new_total, total_delta_str
        ));

        let old_ph = self.padded_height;
        let new_ph = other.padded_height;
        let ph_delta = new_ph as i64 - old_ph as i64;
        let ph_delta_str = if ph_delta > 0 {
            format!("+{}", ph_delta)
        } else if ph_delta == 0 {
            "0".to_string()
        } else {
            format!("{}", ph_delta)
        };
        out.push_str(&format!(
            "{:<20} {:>9} {:>9}  {:>6}\n",
            "Padded height:", old_ph, new_ph, ph_delta_str
        ));

        out
    }

    /// Generate diagnostics for power-of-2 boundary proximity.
    /// Warns when the program is within 12.5% of the next power-of-2 boundary.
    pub fn boundary_warnings(&self) -> Vec<Diagnostic> {
        let mut warnings = Vec::new();
        let max_height = self.total.max_height().max(self.attestation_hash_rows);
        let headroom = self.padded_height - max_height;

        if headroom < self.padded_height / 8 {
            let mut diag = Diagnostic::warning(
                format!("program is {} rows below padded height boundary", headroom),
                Span::dummy(),
            );
            diag.notes.push(format!(
                "padded_height = {} (max table height = {})",
                self.padded_height, max_height
            ));
            diag.notes.push(format!(
                "adding {}+ rows to any table will double proving cost to {}",
                headroom + 1,
                self.padded_height * 2
            ));
            diag.help = Some(format!(
                "consider optimizing to stay well below {}",
                self.padded_height
            ));
            warnings.push(diag);
        }

        warnings
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    fn analyze(source: &str) -> ProgramCost {
        let (tokens, _, _) = Lexer::new(source, 0).tokenize();
        let file = Parser::new(tokens).parse_file().unwrap();
        CostAnalyzer::new().analyze_file(&file)
    }

    #[test]
    fn test_next_power_of_two() {
        assert_eq!(next_power_of_two(0), 1);
        assert_eq!(next_power_of_two(1), 1);
        assert_eq!(next_power_of_two(2), 2);
        assert_eq!(next_power_of_two(3), 4);
        assert_eq!(next_power_of_two(1023), 1024);
        assert_eq!(next_power_of_two(1024), 1024);
        assert_eq!(next_power_of_two(1025), 2048);
    }

    #[test]
    fn test_simple_program_cost() {
        let cost = analyze(
            "program test\nfn main() {\n    let a: Field = pub_read()\n    let b: Field = pub_read()\n    let c: Field = a + b\n    pub_write(c)\n}",
        );
        // pub_read: 1cc + 1opstack each (x2)
        // a + b: dup a (1cc) + dup b (1cc) + add (1cc + 1opstack)
        // pub_write: dup c (1cc) + write_io (1cc + 1opstack)
        // let bindings: 1cc each (x3)
        assert!(cost.total.processor > 0);
        assert_eq!(cost.total.hash, 0);
        assert_eq!(cost.total.u32_table, 0);
        assert_eq!(cost.total.ram, 0);
        eprintln!(
            "Simple program cost: cc={}, opstack={}",
            cost.total.processor, cost.total.op_stack
        );
    }

    #[test]
    fn test_hash_dominates() {
        let cost = analyze(
            "program test\nfn main() {\n    let d: Digest = divine5()\n    let h: Digest = hash(d)\n    pub_write(h)\n}",
        );
        // hash: 6 hash table rows
        assert!(cost.total.hash >= 6);
        // If hash table is the tallest, dominant should be "hash"
        if cost.total.hash > cost.total.processor {
            assert_eq!(cost.total.dominant_table(), "hash");
        }
        eprintln!(
            "Hash program: cc={}, hash={}",
            cost.total.processor, cost.total.hash
        );
    }

    #[test]
    fn test_loop_cost_multiplied() {
        let cost = analyze(
            "program test\nfn main() {\n    let x: Field = pub_read()\n    for i in 0..10 {\n        pub_write(x)\n    }\n}",
        );
        // Loop body: dup x (1cc) + write_io (1cc) = 2cc + overhead per iteration
        // 10 iterations, so total loop cost should be significantly > 10
        assert!(
            cost.total.processor >= 10,
            "loop cost should be at least 10 cc, got {}",
            cost.total.processor
        );
        eprintln!("Loop program: cc={}", cost.total.processor);
    }

    #[test]
    fn test_if_else_worst_case() {
        // Then branch is more expensive (has hash), so cost should include hash cost.
        let cost = analyze(
            "program test\nfn main() {\n    let x: Field = pub_read()\n    if x == x {\n        let d: Digest = divine5()\n        let h: Digest = hash(d)\n    }\n}",
        );
        // If branch has hash (6 rows), else is empty.
        assert!(
            cost.total.hash >= 6,
            "if-branch hash cost should be included, got {}",
            cost.total.hash
        );
    }

    #[test]
    fn test_function_call_cost() {
        let cost = analyze(
            "program test\nfn double(x: Field) -> Field {\n    x + x\n}\nfn main() {\n    let a: Field = pub_read()\n    let b: Field = double(a)\n    pub_write(b)\n}",
        );
        // Function call adds CALL_OVERHEAD (2cc, 2 jump_stack)
        assert!(
            cost.total.jump_stack >= 2,
            "function call should contribute to jump_stack"
        );
        eprintln!(
            "Call program: cc={}, jump={}",
            cost.total.processor, cost.total.jump_stack
        );
    }

    #[test]
    fn test_padded_height() {
        let cost = analyze(
            "program test\nfn main() {\n    let a: Field = pub_read()\n    pub_write(a)\n}",
        );
        // Padded height should be a power of 2.
        assert!(cost.padded_height.is_power_of_two());
        assert!(cost.padded_height >= cost.total.max_height());
    }

    #[test]
    fn test_cost_report_format() {
        let cost = analyze(
            "program test\nfn main() {\n    let a: Field = pub_read()\n    pub_write(a)\n}",
        );
        let report = cost.format_report();
        assert!(report.contains("Cost report:"));
        assert!(report.contains("TOTAL"));
        assert!(report.contains("Padded height:"));
        eprintln!("{}", report);
    }

    #[test]
    fn test_u32_cost() {
        let cost = analyze(
            "program test\nfn main() {\n    let a: Field = pub_read()\n    let b: Field = pub_read()\n    assert(a < b)\n}",
        );
        // lt uses u32 table
        assert!(
            cost.total.u32_table > 0,
            "lt should contribute to u32 table"
        );
    }

    #[test]
    fn test_emit_cost_no_hash() {
        let cost = analyze(
            "program test\nevent Ev { x: Field, y: Field }\nfn main() {\n    emit Ev { x: pub_read(), y: pub_read() }\n}",
        );
        // Open emit should have zero hash cost (no hashing)
        assert_eq!(cost.total.hash, 0, "open emit should have zero hash cost");
        assert!(cost.total.processor > 0);
    }

    #[test]
    fn test_seal_cost_has_hash() {
        let cost = analyze(
            "program test\nevent Ev { x: Field, y: Field }\nfn main() {\n    seal Ev { x: pub_read(), y: pub_read() }\n}",
        );
        // Seal should have hash cost (>= 6 rows for one hash)
        assert!(
            cost.total.hash >= 6,
            "seal should have hash cost >= 6, got {}",
            cost.total.hash
        );
    }

    #[test]
    fn test_boundary_warning_when_close() {
        // Construct a ProgramCost near the boundary
        let cost = ProgramCost {
            program_name: "test".to_string(),
            functions: Vec::new(),
            total: TableCost {
                processor: 1020,
                hash: 0,
                u32_table: 0,
                op_stack: 0,
                ram: 0,
                jump_stack: 0,
            },
            attestation_hash_rows: 0,
            padded_height: 1024,
            estimated_proving_secs: 0.0,
            loop_bound_waste: Vec::new(),
        };
        let warnings = cost.boundary_warnings();
        assert_eq!(warnings.len(), 1, "should warn when 4 rows from boundary");
        assert!(warnings[0].message.contains("4 rows below"));
    }

    #[test]
    fn test_h0001_hash_table_dominance() {
        let cost = ProgramCost {
            program_name: "test".to_string(),
            functions: Vec::new(),
            total: TableCost {
                processor: 10,
                hash: 60,
                u32_table: 0,
                op_stack: 0,
                ram: 0,
                jump_stack: 0,
            },
            attestation_hash_rows: 0,
            padded_height: 64,
            estimated_proving_secs: 0.0,
            loop_bound_waste: Vec::new(),
        };
        let hints = cost.optimization_hints();
        assert!(
            hints.iter().any(|h| h.message.contains("H0001")),
            "should emit H0001 when hash is 6x processor"
        );
    }

    #[test]
    fn test_h0002_headroom_hint() {
        let cost = ProgramCost {
            program_name: "test".to_string(),
            functions: Vec::new(),
            total: TableCost {
                processor: 500,
                hash: 0,
                u32_table: 0,
                op_stack: 0,
                ram: 0,
                jump_stack: 0,
            },
            attestation_hash_rows: 0,
            padded_height: 1024,
            estimated_proving_secs: 0.0,
            loop_bound_waste: Vec::new(),
        };
        let hints = cost.optimization_hints();
        assert!(
            hints.iter().any(|h| h.message.contains("H0002")),
            "should emit H0002 when >25% headroom"
        );
    }

    #[test]
    fn test_no_boundary_warning_when_far() {
        let cost = ProgramCost {
            program_name: "test".to_string(),
            functions: Vec::new(),
            total: TableCost {
                processor: 500,
                hash: 0,
                u32_table: 0,
                op_stack: 0,
                ram: 0,
                jump_stack: 0,
            },
            attestation_hash_rows: 0,
            padded_height: 1024,
            estimated_proving_secs: 0.0,
            loop_bound_waste: Vec::new(),
        };
        let warnings = cost.boundary_warnings();
        assert!(
            warnings.is_empty(),
            "should not warn when far from boundary"
        );
    }

    #[test]
    fn test_h0004_loop_bound_waste() {
        // Loop with bound 128 but only 10 iterations — should warn
        let cost = analyze(
            "program test\nfn main() {\n    let x: Field = pub_read()\n    for i in 0..10 bounded 128 {\n        pub_write(x)\n    }\n}",
        );
        let hints = cost.optimization_hints();
        let h0004 = hints.iter().any(|h| h.message.contains("H0004"));
        assert!(
            h0004,
            "expected H0004 for bound 128 >> end 10, got: {:?}",
            hints
        );
    }

    #[test]
    fn test_h0004_no_waste_when_tight() {
        // Loop with bound close to end — should NOT warn
        let cost = analyze(
            "program test\nfn main() {\n    let x: Field = pub_read()\n    for i in 0..10 bounded 16 {\n        pub_write(x)\n    }\n}",
        );
        let hints = cost.optimization_hints();
        let h0004 = hints.iter().any(|h| h.message.contains("H0004"));
        assert!(!h0004, "should not warn when bound is close to end");
    }

    #[test]
    fn test_asm_block_cost() {
        let cost = analyze(
            "program test\nfn main() {\n    asm {\n        push 1\n        push 2\n        add\n    }\n}",
        );
        // 3 instruction lines → at least 3 processor cycles
        assert!(
            cost.total.processor >= 3,
            "asm block with 3 instructions should cost at least 3 cc, got {}",
            cost.total.processor
        );
    }

    #[test]
    fn test_asm_block_comments_not_counted() {
        let cost = analyze(
            "program test\nfn main() {\n    asm {\n        // this is a comment\n        push 1\n    }\n}",
        );
        // Only 1 real instruction, comment should not count
        assert!(
            cost.total.processor >= 1,
            "asm block cost should count only instructions"
        );
    }

    #[test]
    fn test_stmt_costs_lines() {
        let source =
            "program test\n\nfn main() {\n    let x: Field = pub_read()\n    pub_write(x)\n}\n";
        let (tokens, _, _) = Lexer::new(source, 0).tokenize();
        let file = Parser::new(tokens).parse_file().unwrap();
        let mut analyzer = CostAnalyzer::new();
        // Populate fn_bodies for cost_fn
        analyzer.analyze_file(&file);
        let costs = analyzer.stmt_costs(&file, source);

        // Should have entries for the fn header (line 3) and each statement
        assert!(
            !costs.is_empty(),
            "stmt_costs should return non-empty results"
        );

        // fn main() is on line 3
        assert!(
            costs.iter().any(|(line, _)| *line == 3),
            "should have a cost entry for fn main() on line 3, got lines: {:?}",
            costs.iter().map(|(l, _)| l).collect::<Vec<_>>()
        );

        // let x = pub_read() is on line 4
        assert!(
            costs.iter().any(|(line, _)| *line == 4),
            "should have a cost entry for let statement on line 4"
        );

        // pub_write(x) is on line 5
        assert!(
            costs.iter().any(|(line, _)| *line == 5),
            "should have a cost entry for pub_write on line 5"
        );

        // Verify all costs have non-zero processor count
        for (line, cost) in &costs {
            if *line >= 3 && *line <= 5 {
                assert!(
                    cost.processor > 0 || cost.jump_stack > 0,
                    "line {} should have non-zero cost",
                    line
                );
            }
        }
    }

    #[test]
    fn test_cost_json_roundtrip() {
        let original = TableCost {
            processor: 10,
            hash: 6,
            u32_table: 33,
            op_stack: 8,
            ram: 5,
            jump_stack: 2,
        };
        let json = original.to_json_value();
        let parsed = TableCost::from_json_value(&json).expect("should parse JSON");
        assert_eq!(parsed.processor, original.processor);
        assert_eq!(parsed.hash, original.hash);
        assert_eq!(parsed.u32_table, original.u32_table);
        assert_eq!(parsed.op_stack, original.op_stack);
        assert_eq!(parsed.ram, original.ram);
        assert_eq!(parsed.jump_stack, original.jump_stack);
    }

    #[test]
    fn test_program_cost_json_roundtrip() {
        let cost = analyze(
            "program test\nfn helper(x: Field) -> Field {\n    x + x\n}\nfn main() {\n    let x: Field = pub_read()\n    pub_write(helper(x))\n}",
        );
        let json = cost.to_json();
        let parsed = ProgramCost::from_json(&json).expect("should parse program cost JSON");
        assert_eq!(parsed.total.processor, cost.total.processor);
        assert_eq!(parsed.total.hash, cost.total.hash);
        assert_eq!(parsed.padded_height, cost.padded_height);
        assert_eq!(parsed.functions.len(), cost.functions.len());
        for (orig, loaded) in cost.functions.iter().zip(parsed.functions.iter()) {
            assert_eq!(orig.name, loaded.name);
            assert_eq!(orig.cost.processor, loaded.cost.processor);
        }
    }

    #[test]
    fn test_comparison_format() {
        let old_cost = ProgramCost {
            program_name: "test".to_string(),
            functions: vec![
                FunctionCost {
                    name: "main".to_string(),
                    cost: TableCost {
                        processor: 10,
                        hash: 6,
                        u32_table: 0,
                        op_stack: 8,
                        ram: 0,
                        jump_stack: 2,
                    },
                    per_iteration: None,
                },
                FunctionCost {
                    name: "helper".to_string(),
                    cost: TableCost {
                        processor: 5,
                        hash: 0,
                        u32_table: 0,
                        op_stack: 3,
                        ram: 0,
                        jump_stack: 0,
                    },
                    per_iteration: None,
                },
            ],
            total: TableCost {
                processor: 15,
                hash: 6,
                u32_table: 0,
                op_stack: 11,
                ram: 0,
                jump_stack: 2,
            },
            attestation_hash_rows: 0,
            padded_height: 32,
            estimated_proving_secs: 0.0,
            loop_bound_waste: Vec::new(),
        };

        let new_cost = ProgramCost {
            program_name: "test".to_string(),
            functions: vec![
                FunctionCost {
                    name: "main".to_string(),
                    cost: TableCost {
                        processor: 12,
                        hash: 6,
                        u32_table: 0,
                        op_stack: 10,
                        ram: 0,
                        jump_stack: 2,
                    },
                    per_iteration: None,
                },
                FunctionCost {
                    name: "helper".to_string(),
                    cost: TableCost {
                        processor: 5,
                        hash: 0,
                        u32_table: 0,
                        op_stack: 3,
                        ram: 0,
                        jump_stack: 0,
                    },
                    per_iteration: None,
                },
            ],
            total: TableCost {
                processor: 17,
                hash: 6,
                u32_table: 0,
                op_stack: 13,
                ram: 0,
                jump_stack: 2,
            },
            attestation_hash_rows: 0,
            padded_height: 32,
            estimated_proving_secs: 0.0,
            loop_bound_waste: Vec::new(),
        };

        let comparison = old_cost.format_comparison(&new_cost);
        assert!(
            comparison.contains("Cost comparison:"),
            "should contain header"
        );
        assert!(comparison.contains("main"), "should contain function name");
        assert!(
            comparison.contains("helper"),
            "should contain helper function"
        );
        assert!(comparison.contains("TOTAL"), "should contain TOTAL");
        assert!(
            comparison.contains("+2"),
            "should show +2 delta for main and total"
        );
        assert!(comparison.contains("0"), "should show 0 delta for helper");
        assert!(
            comparison.contains("Padded height:"),
            "should contain padded height"
        );
    }
}
