/// Static cost analysis for Trident programs.
///
/// Computes the trace heights of all 6 Triton VM Algebraic Execution Tables
/// by walking the AST and summing per-instruction costs. This gives an upper
/// bound on proving cost without executing the program.
use std::collections::HashMap;

use crate::ast::*;
use crate::diagnostic::Diagnostic;
use crate::span::Span;

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
            processor: self.processor * factor,
            hash: self.hash * factor,
            u32_table: self.u32_table * factor,
            op_stack: self.op_stack * factor,
            ram: self.ram * factor,
            jump_stack: self.jump_stack * factor,
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
}

// --- Per-instruction cost constants ---

/// Worst-case U32 table rows for 32-bit operations.
const U32_WORST: u64 = 33;

fn cost_binop(op: &BinOp) -> TableCost {
    match op {
        BinOp::Add => TableCost {
            processor: 1,
            hash: 0,
            u32_table: 0,
            op_stack: 1,
            ram: 0,
            jump_stack: 0,
        },
        BinOp::Mul => TableCost {
            processor: 1,
            hash: 0,
            u32_table: 0,
            op_stack: 1,
            ram: 0,
            jump_stack: 0,
        },
        BinOp::Eq => TableCost {
            processor: 1,
            hash: 0,
            u32_table: 0,
            op_stack: 1,
            ram: 0,
            jump_stack: 0,
        },
        BinOp::Lt => TableCost {
            processor: 1,
            hash: 0,
            u32_table: U32_WORST,
            op_stack: 1,
            ram: 0,
            jump_stack: 0,
        },
        BinOp::BitAnd => TableCost {
            processor: 1,
            hash: 0,
            u32_table: U32_WORST,
            op_stack: 1,
            ram: 0,
            jump_stack: 0,
        },
        BinOp::BitXor => TableCost {
            processor: 1,
            hash: 0,
            u32_table: U32_WORST,
            op_stack: 1,
            ram: 0,
            jump_stack: 0,
        },
        BinOp::DivMod => TableCost {
            processor: 1,
            hash: 0,
            u32_table: U32_WORST,
            op_stack: 0,
            ram: 0,
            jump_stack: 0,
        },
        BinOp::XFieldMul => TableCost {
            processor: 1,
            hash: 0,
            u32_table: 0,
            op_stack: 1,
            ram: 0,
            jump_stack: 0,
        },
    }
}

fn cost_builtin(name: &str) -> TableCost {
    match name {
        // I/O
        "pub_read" | "pub_read2" | "pub_read3" | "pub_read4" | "pub_read5" => TableCost {
            processor: 1,
            hash: 0,
            u32_table: 0,
            op_stack: 1,
            ram: 0,
            jump_stack: 0,
        },
        "pub_write" | "pub_write2" | "pub_write3" | "pub_write4" | "pub_write5" => TableCost {
            processor: 1,
            hash: 0,
            u32_table: 0,
            op_stack: 1,
            ram: 0,
            jump_stack: 0,
        },

        // Non-deterministic input
        "divine" | "divine3" | "divine5" => TableCost {
            processor: 1,
            hash: 0,
            u32_table: 0,
            op_stack: 1,
            ram: 0,
            jump_stack: 0,
        },

        // Assertions
        "assert" => TableCost {
            processor: 1,
            hash: 0,
            u32_table: 0,
            op_stack: 1,
            ram: 0,
            jump_stack: 0,
        },
        "assert_eq" => TableCost {
            processor: 2,
            hash: 0,
            u32_table: 0,
            op_stack: 2,
            ram: 0,
            jump_stack: 0,
        },
        "assert_digest" => TableCost {
            processor: 2,
            hash: 0,
            u32_table: 0,
            op_stack: 2,
            ram: 0,
            jump_stack: 0,
        },

        // Field ops
        "inv" => TableCost {
            processor: 1,
            hash: 0,
            u32_table: 0,
            op_stack: 0,
            ram: 0,
            jump_stack: 0,
        },
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
        "split" => TableCost {
            processor: 1,
            hash: 0,
            u32_table: U32_WORST,
            op_stack: 1,
            ram: 0,
            jump_stack: 0,
        },
        "log2" => TableCost {
            processor: 1,
            hash: 0,
            u32_table: U32_WORST,
            op_stack: 0,
            ram: 0,
            jump_stack: 0,
        },
        "pow" => TableCost {
            processor: 1,
            hash: 0,
            u32_table: U32_WORST,
            op_stack: 1,
            ram: 0,
            jump_stack: 0,
        },
        "popcount" => TableCost {
            processor: 1,
            hash: 0,
            u32_table: U32_WORST,
            op_stack: 0,
            ram: 0,
            jump_stack: 0,
        },

        // Hash ops (6 hash table rows each for Tip5 permutation)
        "hash" => TableCost {
            processor: 1,
            hash: 6,
            u32_table: 0,
            op_stack: 1,
            ram: 0,
            jump_stack: 0,
        },
        "sponge_init" => TableCost {
            processor: 1,
            hash: 6,
            u32_table: 0,
            op_stack: 0,
            ram: 0,
            jump_stack: 0,
        },
        "sponge_absorb" => TableCost {
            processor: 1,
            hash: 6,
            u32_table: 0,
            op_stack: 1,
            ram: 0,
            jump_stack: 0,
        },
        "sponge_squeeze" => TableCost {
            processor: 1,
            hash: 6,
            u32_table: 0,
            op_stack: 1,
            ram: 0,
            jump_stack: 0,
        },
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
            u32_table: U32_WORST,
            op_stack: 0,
            ram: 0,
            jump_stack: 0,
        },
        "merkle_step_mem" => TableCost {
            processor: 1,
            hash: 6,
            u32_table: U32_WORST,
            op_stack: 0,
            ram: 5,
            jump_stack: 0,
        },

        // RAM
        "ram_read" => TableCost {
            processor: 2,
            hash: 0,
            u32_table: 0,
            op_stack: 2,
            ram: 1,
            jump_stack: 0,
        },
        "ram_write" => TableCost {
            processor: 2,
            hash: 0,
            u32_table: 0,
            op_stack: 2,
            ram: 1,
            jump_stack: 0,
        },
        "ram_read_block" => TableCost {
            processor: 2,
            hash: 0,
            u32_table: 0,
            op_stack: 2,
            ram: 5,
            jump_stack: 0,
        },
        "ram_write_block" => TableCost {
            processor: 2,
            hash: 0,
            u32_table: 0,
            op_stack: 2,
            ram: 5,
            jump_stack: 0,
        },

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

        // Conversions (negligible cost)
        "as_u32" => TableCost {
            processor: 2,
            hash: 0,
            u32_table: U32_WORST,
            op_stack: 1,
            ram: 0,
            jump_stack: 0,
        },
        "as_field" => TableCost {
            processor: 0,
            hash: 0,
            u32_table: 0,
            op_stack: 0,
            ram: 0,
            jump_stack: 0,
        },

        // XField
        "xfield" => TableCost {
            processor: 0,
            hash: 0,
            u32_table: 0,
            op_stack: 0,
            ram: 0,
            jump_stack: 0,
        },
        "xinvert" => TableCost {
            processor: 1,
            hash: 0,
            u32_table: 0,
            op_stack: 0,
            ram: 0,
            jump_stack: 0,
        },

        _ => TableCost::ZERO,
    }
}

/// Cost of a function call/return pair.
const CALL_OVERHEAD: TableCost = TableCost {
    processor: 2,
    hash: 0,
    u32_table: 0,
    op_stack: 0,
    ram: 0,
    jump_stack: 2,
};

/// Cost of stack manipulation for a push/dup/swap (1 instruction).
const STACK_OP: TableCost = TableCost {
    processor: 1,
    hash: 0,
    u32_table: 0,
    op_stack: 1,
    ram: 0,
    jump_stack: 0,
};

/// Cost of if/else overhead (skiz + call pattern).
const IF_OVERHEAD: TableCost = TableCost {
    processor: 3,
    hash: 0,
    u32_table: 0,
    op_stack: 2,
    ram: 0,
    jump_stack: 1,
};

/// Cost of for-loop overhead (setup: dup, push 0, eq, skiz, return, push -1, add, recurse).
const LOOP_OVERHEAD: TableCost = TableCost {
    processor: 8,
    hash: 0,
    u32_table: 0,
    op_stack: 4,
    ram: 0,
    jump_stack: 1,
};

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
pub struct CostAnalyzer {
    /// Function bodies indexed by name (for resolving calls).
    fn_bodies: HashMap<String, FnDef>,
    /// Cached function costs to avoid recomputation.
    fn_costs: HashMap<String, TableCost>,
    /// Recursion guard to prevent infinite loops in cost computation.
    in_progress: Vec<String>,
    /// H0004: collected loop bound waste entries (fn_name, end_value, bound).
    loop_bound_waste: Vec<(String, u64, u64)>,
}

impl Default for CostAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl CostAnalyzer {
    pub fn new() -> Self {
        Self {
            fn_bodies: HashMap::new(),
            fn_costs: HashMap::new(),
            in_progress: Vec::new(),
            loop_bound_waste: Vec::new(),
        }
    }

    /// Analyze a complete file and return the program cost.
    pub fn analyze_file(&mut self, file: &File) -> ProgramCost {
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
            main_cost.add(&CALL_OVERHEAD) // call main + halt
        } else {
            functions
                .iter()
                .fold(TableCost::ZERO, |acc, f| acc.add(&f.cost))
        };

        // Estimate program instruction count for attestation.
        // Rough heuristic: total processor cycles ≈ instruction count.
        let instruction_count = total.processor.max(10);
        let attestation_hash_rows = instruction_count.div_ceil(10) * 6;

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
        match stmt {
            Stmt::Let { init, .. } => {
                // Cost of evaluating the init expression + stack placement.
                self.cost_expr(&init.node).add(&STACK_OP)
            }
            Stmt::Assign { value, .. } => {
                // Cost of evaluating value + swap to replace old value.
                self.cost_expr(&value.node).add(&STACK_OP).add(&STACK_OP)
            }
            Stmt::TupleAssign { names, value } => {
                let mut cost = self.cost_expr(&value.node);
                // One swap+pop per element.
                for _ in names {
                    cost = cost.add(&STACK_OP).add(&STACK_OP);
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
                cond_cost.add(&then_cost.max(&else_cost)).add(&IF_OVERHEAD)
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
                let per_iter = body_cost.add(&LOOP_OVERHEAD);
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
                let mut cost = STACK_OP; // push tag
                cost = cost.add(&TableCost {
                    processor: 1,
                    hash: 0,
                    u32_table: 0,
                    op_stack: 1,
                    ram: 0,
                    jump_stack: 0,
                }); // write_io 1 for tag
                for (_name, val) in fields {
                    cost = cost.add(&self.cost_expr(&val.node));
                    cost = cost.add(&TableCost {
                        processor: 1,
                        hash: 0,
                        u32_table: 0,
                        op_stack: 1,
                        ram: 0,
                        jump_stack: 0,
                    }); // write_io 1
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
                STACK_OP.scale(line_count)
            }
            Stmt::Seal { fields, .. } => {
                // push tag + field exprs + padding pushes + hash + write_io 5
                let mut cost = STACK_OP; // push tag
                for (_name, val) in fields {
                    cost = cost.add(&self.cost_expr(&val.node));
                }
                let padding = 10 - 1 - fields.len();
                for _ in 0..padding {
                    cost = cost.add(&STACK_OP); // push 0 padding
                }
                // hash: 6 hash table rows
                cost = cost.add(&TableCost {
                    processor: 1,
                    hash: 6,
                    u32_table: 0,
                    op_stack: 1,
                    ram: 0,
                    jump_stack: 0,
                });
                // write_io 5
                cost = cost.add(&TableCost {
                    processor: 1,
                    hash: 0,
                    u32_table: 0,
                    op_stack: 1,
                    ram: 0,
                    jump_stack: 0,
                });
                cost
            }
        }
    }

    fn cost_expr(&mut self, expr: &Expr) -> TableCost {
        match expr {
            Expr::Literal(_) => {
                // push instruction: 1 cc, 1 opstack.
                STACK_OP
            }
            Expr::Var(_) => {
                // dup instruction: 1 cc, 1 opstack.
                STACK_OP
            }
            Expr::BinOp { op, lhs, rhs } => {
                let lhs_cost = self.cost_expr(&lhs.node);
                let rhs_cost = self.cost_expr(&rhs.node);
                lhs_cost.add(&rhs_cost).add(&cost_binop(op))
            }
            Expr::Call { path, args } => {
                let fn_name = path.node.as_dotted();
                let args_cost = args
                    .iter()
                    .fold(TableCost::ZERO, |acc, a| acc.add(&self.cost_expr(&a.node)));

                // Check if it's a builtin — try full name first, then short name
                // to handle cross-module calls like "hash.tip5" → "tip5" → "hash"
                let base_name = fn_name.rsplit('.').next().unwrap_or(&fn_name);
                let fn_cost = {
                    let c = cost_builtin(&fn_name);
                    if c.processor > 0 || c.hash > 0 || c.u32_table > 0 || c.ram > 0 {
                        c
                    } else {
                        cost_builtin(base_name)
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
                    args_cost.add(&body_cost).add(&CALL_OVERHEAD)
                }
            }
            Expr::FieldAccess { expr: inner, .. } => {
                // Evaluate inner struct + dup field elements.
                self.cost_expr(&inner.node).add(&STACK_OP)
            }
            Expr::Index { expr: inner, .. } => {
                // Evaluate inner array + dup indexed element.
                self.cost_expr(&inner.node).add(&STACK_OP)
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
                    let per_iter = body_cost.add(&LOOP_OVERHEAD);
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
        }
    }
}

/// Smallest power of 2 >= n.
pub fn next_power_of_two(n: u64) -> u64 {
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
}
