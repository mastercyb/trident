//! Symbolic execution engine for Trident programs.
//!
//! Transforms the AST into a symbolic constraint system suitable for
//! algebraic verification, bounded model checking, and SMT solving.
//!
//! Since Trident programs have no heap, no recursion, bounded loops,
//! and operate over a finite field (Goldilocks: p = 2^64 - 2^32 + 1),
//! every program produces a finite, decidable constraint system.
//!
//! The symbolic engine:
//! 1. Assigns a symbolic variable to each `let` binding
//! 2. Tracks constraints from `assert`, `assert_eq`, `assert_digest`
//! 3. Encodes `if/else` as path conditions
//! 4. Unrolls bounded `for` loops up to their bound
//! 5. Inlines function calls (no recursion → always terminates)
//! 6. Produces a `ConstraintSystem` that can be checked by:
//!    - The algebraic solver (polynomial identity testing)
//!    - A bounded model checker (enumerate concrete values)
//!    - An SMT solver (Z3/CVC5 via SMT-LIB encoding)

use std::collections::HashMap;

use crate::ast::*;
use crate::span::Spanned;

/// The prime modulus for the Goldilocks field.
pub const GOLDILOCKS_P: u64 = 0xFFFFFFFF00000001; // 2^64 - 2^32 + 1

// ─── Symbolic Values ───────────────────────────────────────────────

/// A symbolic value in the constraint system.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SymValue {
    /// A concrete constant.
    Const(u64),
    /// A named symbolic variable (from `let`, `divine`, `pub_read`, etc.).
    Var(SymVar),
    /// Addition: a + b (mod p).
    Add(Box<SymValue>, Box<SymValue>),
    /// Multiplication: a * b (mod p).
    Mul(Box<SymValue>, Box<SymValue>),
    /// Subtraction: a - b (mod p).
    Sub(Box<SymValue>, Box<SymValue>),
    /// Negation: -a (mod p).
    Neg(Box<SymValue>),
    /// Multiplicative inverse: 1/a (mod p). Undefined for a = 0.
    Inv(Box<SymValue>),
    /// Equality test: 1 if a == b, else 0.
    Eq(Box<SymValue>, Box<SymValue>),
    /// Less-than test: 1 if a < b, else 0 (on canonical representatives).
    Lt(Box<SymValue>, Box<SymValue>),
    /// Hash output: hash(inputs)[index]. Treated as opaque.
    Hash(Vec<SymValue>, usize),
    /// A divine (nondeterministic) input. Each occurrence is unique.
    Divine(u32),
    /// Public input. Sequential read index.
    PubInput(u32),
    /// If-then-else: if cond then a else b.
    Ite(Box<SymValue>, Box<SymValue>, Box<SymValue>),
}

impl SymValue {
    pub fn is_const(&self) -> bool {
        matches!(self, SymValue::Const(_))
    }

    pub fn as_const(&self) -> Option<u64> {
        match self {
            SymValue::Const(v) => Some(*v),
            _ => None,
        }
    }

    /// Simplify obvious identities: x + 0 = x, x * 1 = x, etc.
    pub fn simplify(&self) -> SymValue {
        match self {
            SymValue::Add(a, b) => {
                let a = a.simplify();
                let b = b.simplify();
                match (&a, &b) {
                    (SymValue::Const(0), _) => b,
                    (_, SymValue::Const(0)) => a,
                    (SymValue::Const(x), SymValue::Const(y)) => {
                        SymValue::Const(x.wrapping_add(*y) % GOLDILOCKS_P)
                    }
                    _ => SymValue::Add(Box::new(a), Box::new(b)),
                }
            }
            SymValue::Mul(a, b) => {
                let a = a.simplify();
                let b = b.simplify();
                match (&a, &b) {
                    (SymValue::Const(0), _) | (_, SymValue::Const(0)) => SymValue::Const(0),
                    (SymValue::Const(1), _) => b,
                    (_, SymValue::Const(1)) => a,
                    (SymValue::Const(x), SymValue::Const(y)) => {
                        SymValue::Const(((*x as u128 * *y as u128) % GOLDILOCKS_P as u128) as u64)
                    }
                    _ => SymValue::Mul(Box::new(a), Box::new(b)),
                }
            }
            SymValue::Sub(a, b) => {
                let a = a.simplify();
                let b = b.simplify();
                match (&a, &b) {
                    (_, SymValue::Const(0)) => a,
                    (SymValue::Const(x), SymValue::Const(y)) => {
                        SymValue::Const(x.wrapping_sub(*y) % GOLDILOCKS_P)
                    }
                    _ if a == b => SymValue::Const(0),
                    _ => SymValue::Sub(Box::new(a), Box::new(b)),
                }
            }
            SymValue::Neg(a) => {
                let a = a.simplify();
                match &a {
                    SymValue::Const(0) => SymValue::Const(0),
                    SymValue::Const(v) => SymValue::Const(GOLDILOCKS_P - v),
                    _ => SymValue::Neg(Box::new(a)),
                }
            }
            SymValue::Eq(a, b) => {
                let a = a.simplify();
                let b = b.simplify();
                if a == b {
                    SymValue::Const(1)
                } else {
                    match (&a, &b) {
                        (SymValue::Const(x), SymValue::Const(y)) => {
                            SymValue::Const(if x == y { 1 } else { 0 })
                        }
                        _ => SymValue::Eq(Box::new(a), Box::new(b)),
                    }
                }
            }
            _ => self.clone(),
        }
    }
}

/// A named symbolic variable.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SymVar {
    pub name: String,
    /// SSA version number (for mutable variables).
    pub version: u32,
}

impl std::fmt::Display for SymVar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.version == 0 {
            write!(f, "{}", self.name)
        } else {
            write!(f, "{}_{}", self.name, self.version)
        }
    }
}

// ─── Constraints ───────────────────────────────────────────────────

/// A constraint in the system.
#[derive(Clone, Debug)]
pub enum Constraint {
    /// a == b (from `assert_eq` or `assert(a == b)`)
    Equal(SymValue, SymValue),
    /// a == 0 (from `assert(cond)` where cond is truthy)
    AssertTrue(SymValue),
    /// Conditional: if path_condition then constraint holds
    Conditional(SymValue, Box<Constraint>),
    /// Range check: value fits in U32 (from `as_u32`)
    RangeU32(SymValue),
    /// Digest equality: 5-element vector comparison
    DigestEqual(Vec<SymValue>, Vec<SymValue>),
}

impl Constraint {
    /// Check if this constraint is trivially satisfied.
    pub fn is_trivial(&self) -> bool {
        match self {
            Constraint::Equal(a, b) => a == b,
            Constraint::AssertTrue(v) => matches!(v, SymValue::Const(1)),
            Constraint::RangeU32(v) => {
                if let SymValue::Const(c) = v {
                    *c <= u32::MAX as u64
                } else {
                    false
                }
            }
            Constraint::DigestEqual(a, b) => a == b,
            Constraint::Conditional(cond, inner) => {
                matches!(cond, SymValue::Const(0)) || inner.is_trivial()
            }
        }
    }

    /// Check if this constraint is trivially violated.
    pub fn is_violated(&self) -> bool {
        match self {
            Constraint::Equal(SymValue::Const(a), SymValue::Const(b)) => a != b,
            Constraint::AssertTrue(SymValue::Const(0)) => true,
            Constraint::RangeU32(SymValue::Const(c)) => *c > u32::MAX as u64,
            _ => false,
        }
    }
}

// ─── Constraint System ─────────────────────────────────────────────

/// The complete constraint system for a program or function.
#[derive(Clone, Debug)]
pub struct ConstraintSystem {
    /// All constraints that must hold.
    pub constraints: Vec<Constraint>,
    /// Symbolic variables introduced (name → latest version).
    pub variables: HashMap<String, u32>,
    /// Public inputs read (in order).
    pub pub_inputs: Vec<SymVar>,
    /// Public outputs written (in order).
    pub pub_outputs: Vec<SymValue>,
    /// Divine inputs consumed (in order).
    pub divine_inputs: Vec<SymVar>,
    /// Number of unique symbolic variables.
    pub num_variables: u32,
}

impl ConstraintSystem {
    pub fn new() -> Self {
        Self {
            constraints: Vec::new(),
            variables: HashMap::new(),
            pub_inputs: Vec::new(),
            pub_outputs: Vec::new(),
            divine_inputs: Vec::new(),
            num_variables: 0,
        }
    }

    /// Count of non-trivial constraints.
    pub fn active_constraints(&self) -> usize {
        self.constraints.iter().filter(|c| !c.is_trivial()).count()
    }

    /// Check for trivially violated constraints (static analysis).
    pub fn violated_constraints(&self) -> Vec<&Constraint> {
        self.constraints
            .iter()
            .filter(|c| c.is_violated())
            .collect()
    }

    /// Summary for display.
    pub fn summary(&self) -> String {
        format!(
            "Variables: {}, Constraints: {} ({} active), Inputs: {} pub + {} divine, Outputs: {}",
            self.num_variables,
            self.constraints.len(),
            self.active_constraints(),
            self.pub_inputs.len(),
            self.divine_inputs.len(),
            self.pub_outputs.len(),
        )
    }
}

// ─── Symbolic Executor ─────────────────────────────────────────────

/// Symbolic executor that walks the AST and builds a constraint system.
pub struct SymExecutor {
    /// The constraint system being built.
    system: ConstraintSystem,
    /// Variable bindings: name → symbolic value.
    env: HashMap<String, SymValue>,
    /// SSA version counter per variable name.
    versions: HashMap<String, u32>,
    /// Counter for divine inputs.
    divine_counter: u32,
    /// Counter for public inputs.
    pub_input_counter: u32,
    /// Current path condition (conjunction of conditions leading here).
    path_condition: Vec<SymValue>,
    /// Function definitions for inlining.
    functions: HashMap<String, FnDef>,
    /// Recursion guard (prevent infinite inlining — shouldn't happen in Trident).
    call_depth: u32,
    /// Maximum call depth before giving up.
    max_call_depth: u32,
}

impl SymExecutor {
    pub fn new() -> Self {
        Self {
            system: ConstraintSystem::new(),
            env: HashMap::new(),
            versions: HashMap::new(),
            divine_counter: 0,
            pub_input_counter: 0,
            path_condition: Vec::new(),
            functions: HashMap::new(),
            call_depth: 0,
            max_call_depth: 64,
        }
    }

    /// Execute a file and produce its constraint system.
    pub fn execute_file(mut self, file: &File) -> ConstraintSystem {
        // Register all functions
        for item in &file.items {
            if let Item::Fn(func) = &item.node {
                if func.body.is_some() && !func.is_test {
                    self.functions.insert(func.name.node.clone(), func.clone());
                }
            }
        }

        // Execute the main function
        if let Some(main_fn) = self.functions.get("main").cloned() {
            if let Some(ref body) = main_fn.body {
                self.execute_block(&body.node);
            }
        }

        self.system
    }

    /// Create a fresh symbolic variable.
    fn fresh_var(&mut self, name: &str) -> SymVar {
        let version = self.versions.entry(name.to_string()).or_insert(0);
        let var = SymVar {
            name: name.to_string(),
            version: *version,
        };
        *version += 1;
        self.system.num_variables += 1;
        self.system.variables.insert(name.to_string(), var.version);
        var
    }

    /// Create a divine input variable.
    fn fresh_divine(&mut self) -> SymValue {
        let idx = self.divine_counter;
        self.divine_counter += 1;
        let var = self.fresh_var(&format!("divine_{}", idx));
        self.system.divine_inputs.push(var.clone());
        SymValue::Var(var)
    }

    /// Create a public input variable.
    fn fresh_pub_input(&mut self) -> SymValue {
        let idx = self.pub_input_counter;
        self.pub_input_counter += 1;
        let var = self.fresh_var(&format!("pub_in_{}", idx));
        self.system.pub_inputs.push(var.clone());
        SymValue::Var(var)
    }

    /// Add a constraint, wrapping with current path condition.
    fn add_constraint(&mut self, c: Constraint) {
        if self.path_condition.is_empty() {
            self.system.constraints.push(c);
        } else {
            // Combine path conditions: cond1 AND cond2 AND ... => constraint
            let mut combined = self.path_condition[0].clone();
            for pc in &self.path_condition[1..] {
                combined = SymValue::Mul(Box::new(combined), Box::new(pc.clone()));
            }
            self.system
                .constraints
                .push(Constraint::Conditional(combined, Box::new(c)));
        }
    }

    /// Execute a block of statements.
    fn execute_block(&mut self, block: &Block) {
        for stmt in &block.stmts {
            self.execute_stmt(&stmt.node);
        }
        // Evaluate tail expression for side effects (e.g., assert calls)
        if let Some(ref tail) = block.tail_expr {
            let _ = self.eval_expr(&tail.node);
        }
    }

    /// Execute a single statement.
    fn execute_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let {
                pattern,
                init,
                mutable,
                ..
            } => {
                let value = self.eval_expr(&init.node);
                match pattern {
                    Pattern::Name(name) => {
                        let var = self.fresh_var(&name.node);
                        self.env.insert(name.node.clone(), value.clone());
                        // If mutable, we track the name for SSA versioning
                        if *mutable {
                            self.env.insert(name.node.clone(), value);
                        }
                    }
                    Pattern::Tuple(names) => {
                        // Tuple destructuring: each element gets a variable
                        for (i, name) in names.iter().enumerate() {
                            let elem = self.project_tuple(&value, i);
                            let _var = self.fresh_var(&name.node);
                            self.env.insert(name.node.clone(), elem);
                        }
                    }
                }
            }
            Stmt::Assign { place, value } => {
                let val = self.eval_expr(&value.node);
                if let Place::Var(name) = &place.node {
                    let _var = self.fresh_var(name);
                    self.env.insert(name.clone(), val);
                }
            }
            Stmt::If {
                cond,
                then_block,
                else_block,
            } => {
                let cond_val = self.eval_expr(&cond.node);

                // Save environment
                let saved_env = self.env.clone();

                // Execute then branch
                self.path_condition.push(cond_val.clone());
                self.execute_block(&then_block.node);
                let then_env = self.env.clone();
                self.path_condition.pop();

                // Execute else branch
                self.env = saved_env.clone();
                if let Some(else_blk) = else_block {
                    let neg_cond =
                        SymValue::Sub(Box::new(SymValue::Const(1)), Box::new(cond_val.clone()));
                    self.path_condition.push(neg_cond);
                    self.execute_block(&else_blk.node);
                    self.path_condition.pop();
                }
                let else_env = self.env.clone();

                // Merge environments: for each variable modified in either branch,
                // create an ITE symbolic value
                let mut merged = saved_env;
                for (name, then_val) in &then_env {
                    let else_val = else_env.get(name).unwrap_or(then_val);
                    if then_val != else_val {
                        let ite = SymValue::Ite(
                            Box::new(cond_val.clone()),
                            Box::new(then_val.clone()),
                            Box::new(else_val.clone()),
                        );
                        merged.insert(name.clone(), ite);
                    } else {
                        merged.insert(name.clone(), then_val.clone());
                    }
                }
                // Also merge vars that only exist in else_env
                for (name, else_val) in &else_env {
                    if !then_env.contains_key(name) {
                        merged.insert(name.clone(), else_val.clone());
                    }
                }
                self.env = merged;
            }
            Stmt::For {
                var,
                start,
                end,
                body,
                ..
            } => {
                let start_val = self.eval_expr(&start.node);
                let end_val = self.eval_expr(&end.node);

                // If both are constants, unroll exactly
                if let (Some(s), Some(e)) = (start_val.as_const(), end_val.as_const()) {
                    for i in s..e {
                        self.env.insert(var.node.clone(), SymValue::Const(i));
                        self.execute_block(&body.node);
                    }
                } else {
                    // Dynamic bound: unroll up to the declared bound
                    // Each iteration gets a path condition: i < end
                    let bound = 64u64; // default max unroll
                    if let Some(s) = start_val.as_const() {
                        for i in s..(s + bound) {
                            let iter_val = SymValue::Const(i);
                            let in_range =
                                SymValue::Lt(Box::new(iter_val.clone()), Box::new(end_val.clone()));
                            self.env.insert(var.node.clone(), iter_val);
                            self.path_condition.push(in_range);
                            self.execute_block(&body.node);
                            self.path_condition.pop();
                        }
                    }
                }
            }
            Stmt::Expr(expr) => {
                // Expression statement: evaluate for side effects (e.g., assert)
                let _ = self.eval_expr(&expr.node);
            }
            Stmt::Return(_) => {
                // Return from function — handled by caller
            }
            Stmt::Emit { .. } | Stmt::Seal { .. } => {
                // Events don't produce constraints (they're output-only)
            }
            Stmt::Asm { .. } => {
                // Inline assembly is opaque to symbolic execution
            }
            Stmt::Match { expr, arms } => {
                let match_val = self.eval_expr(&expr.node);
                let saved_env = self.env.clone();
                let mut merged_envs: Vec<(SymValue, HashMap<String, SymValue>)> = Vec::new();

                for arm in arms {
                    self.env = saved_env.clone();
                    let cond = match &arm.pattern.node {
                        MatchPattern::Literal(Literal::Integer(n)) => {
                            SymValue::Eq(Box::new(match_val.clone()), Box::new(SymValue::Const(*n)))
                        }
                        MatchPattern::Literal(Literal::Bool(b)) => SymValue::Eq(
                            Box::new(match_val.clone()),
                            Box::new(SymValue::Const(if *b { 1 } else { 0 })),
                        ),
                        MatchPattern::Wildcard => SymValue::Const(1),
                    };
                    self.path_condition.push(cond.clone());
                    self.execute_block(&arm.body.node);
                    self.path_condition.pop();
                    merged_envs.push((cond, self.env.clone()));
                }

                // Merge all arm environments
                self.env = saved_env;
                for (cond, arm_env) in merged_envs {
                    for (name, val) in arm_env {
                        if let Some(current) = self.env.get(&name) {
                            if *current != val {
                                let ite = SymValue::Ite(
                                    Box::new(cond.clone()),
                                    Box::new(val),
                                    Box::new(current.clone()),
                                );
                                self.env.insert(name, ite);
                            }
                        } else {
                            self.env.insert(name, val);
                        }
                    }
                }
            }
            Stmt::TupleAssign { names, value } => {
                let val = self.eval_expr(&value.node);
                for (i, name) in names.iter().enumerate() {
                    let elem = self.project_tuple(&val, i);
                    let _var = self.fresh_var(&name.node);
                    self.env.insert(name.node.clone(), elem);
                }
            }
        }
    }

    /// Evaluate an expression to a symbolic value.
    fn eval_expr(&mut self, expr: &Expr) -> SymValue {
        match expr {
            Expr::Literal(Literal::Integer(n)) => SymValue::Const(*n),
            Expr::Literal(Literal::Bool(b)) => SymValue::Const(if *b { 1 } else { 0 }),
            Expr::Var(name) => {
                self.env.get(name).cloned().unwrap_or_else(|| {
                    // Unknown variable — treat as fresh symbolic
                    let var = self.fresh_var(name);
                    SymValue::Var(var)
                })
            }
            Expr::BinOp { op, lhs, rhs } => {
                let l = self.eval_expr(&lhs.node);
                let r = self.eval_expr(&rhs.node);
                match op {
                    BinOp::Add => SymValue::Add(Box::new(l), Box::new(r)).simplify(),
                    BinOp::Mul => SymValue::Mul(Box::new(l), Box::new(r)).simplify(),
                    BinOp::Eq => SymValue::Eq(Box::new(l), Box::new(r)).simplify(),
                    BinOp::Lt => SymValue::Lt(Box::new(l), Box::new(r)),
                    _ => {
                        // BitAnd, BitXor, DivMod, XFieldMul — leave as opaque
                        SymValue::Var(self.fresh_var("__binop"))
                    }
                }
            }
            Expr::Call { path, args, .. } => self.eval_call(&path.node, args),
            Expr::Tuple(elems) => {
                // Tuples are represented as the first element for simplicity.
                // Full tuple tracking would require a SymValue::Tuple variant.
                if elems.len() == 1 {
                    self.eval_expr(&elems[0].node)
                } else {
                    // Create fresh variables for each tuple element
                    let var = self.fresh_var("__tuple");
                    SymValue::Var(var)
                }
            }
            Expr::FieldAccess { expr, .. } => {
                let _ = self.eval_expr(&expr.node);
                let var = self.fresh_var("__field");
                SymValue::Var(var)
            }
            Expr::Index { expr, .. } => {
                let _ = self.eval_expr(&expr.node);
                let var = self.fresh_var("__index");
                SymValue::Var(var)
            }
            Expr::StructInit { fields, .. } => {
                for (_, val) in fields {
                    let _ = self.eval_expr(&val.node);
                }
                let var = self.fresh_var("__struct");
                SymValue::Var(var)
            }
            Expr::ArrayInit(elems) => {
                for e in elems {
                    let _ = self.eval_expr(&e.node);
                }
                let var = self.fresh_var("__array");
                SymValue::Var(var)
            }
        }
    }

    /// Evaluate a function call (builtin or user-defined).
    fn eval_call(&mut self, path: &ModulePath, args: &[Spanned<Expr>]) -> SymValue {
        let name = path.as_dotted();
        let func_name = path.0.last().map(|s| s.as_str()).unwrap_or("");

        // Handle builtins
        match func_name {
            "pub_read" | "read" => return self.fresh_pub_input(),
            "pub_read2" | "read2" => {
                self.fresh_pub_input();
                return self.fresh_pub_input();
            }
            "pub_read5" | "read5" => {
                for _ in 0..5 {
                    self.fresh_pub_input();
                }
                let var = self.fresh_var("__digest");
                return SymValue::Var(var);
            }
            "pub_write" | "write" => {
                if let Some(arg) = args.first() {
                    let val = self.eval_expr(&arg.node);
                    self.system.pub_outputs.push(val);
                }
                return SymValue::Const(0);
            }
            "divine" => return self.fresh_divine(),
            "divine3" => {
                for _ in 0..3 {
                    self.fresh_divine();
                }
                let var = self.fresh_var("__divine3");
                return SymValue::Var(var);
            }
            "divine5" => {
                for _ in 0..5 {
                    self.fresh_divine();
                }
                let var = self.fresh_var("__divine5");
                return SymValue::Var(var);
            }
            "hash" | "tip5" => {
                let inputs: Vec<SymValue> = args.iter().map(|a| self.eval_expr(&a.node)).collect();
                let var = self.fresh_var("__hash");
                return SymValue::Var(var);
            }
            "assert" => {
                if let Some(arg) = args.first() {
                    let val = self.eval_expr(&arg.node);
                    self.add_constraint(Constraint::AssertTrue(val));
                }
                return SymValue::Const(0);
            }
            "assert_eq" | "eq" => {
                if args.len() >= 2 {
                    let a = self.eval_expr(&args[0].node);
                    let b = self.eval_expr(&args[1].node);
                    self.add_constraint(Constraint::Equal(a, b));
                }
                return SymValue::Const(0);
            }
            "assert_digest" | "digest" => {
                // Digest equality: 5-element vector comparison
                if args.len() >= 2 {
                    let a = self.eval_expr(&args[0].node);
                    let b = self.eval_expr(&args[1].node);
                    self.add_constraint(Constraint::Equal(a, b));
                }
                return SymValue::Const(0);
            }
            "as_u32" => {
                if let Some(arg) = args.first() {
                    let val = self.eval_expr(&arg.node);
                    self.add_constraint(Constraint::RangeU32(val.clone()));
                    return val;
                }
                return SymValue::Const(0);
            }
            "sub" => {
                if args.len() >= 2 {
                    let a = self.eval_expr(&args[0].node);
                    let b = self.eval_expr(&args[1].node);
                    return SymValue::Sub(Box::new(a), Box::new(b)).simplify();
                }
                return SymValue::Const(0);
            }
            "neg" => {
                if let Some(arg) = args.first() {
                    let val = self.eval_expr(&arg.node);
                    return SymValue::Neg(Box::new(val)).simplify();
                }
                return SymValue::Const(0);
            }
            "inv" => {
                if let Some(arg) = args.first() {
                    let val = self.eval_expr(&arg.node);
                    return SymValue::Inv(Box::new(val));
                }
                return SymValue::Const(0);
            }
            _ => {}
        }

        // Try user-defined function inlining
        if self.call_depth < self.max_call_depth {
            // Look up the function: try full path first, then last component
            let func = self
                .functions
                .get(&name)
                .cloned()
                .or_else(|| self.functions.get(func_name).cloned());

            if let Some(func) = func {
                if let Some(ref body) = func.body {
                    self.call_depth += 1;
                    let saved_env = self.env.clone();

                    // Bind parameters
                    for (param, arg) in func.params.iter().zip(args.iter()) {
                        let val = self.eval_expr(&arg.node);
                        self.env.insert(param.name.node.clone(), val);
                    }

                    // Execute function body
                    self.execute_block(&body.node);

                    // Restore environment (except new constraints are kept)
                    self.env = saved_env;
                    self.call_depth -= 1;
                }
            }
        }

        // Default: return a fresh symbolic variable
        let var = self.fresh_var(&format!("__call_{}", func_name));
        SymValue::Var(var)
    }

    /// Project element `i` from a tuple-like symbolic value.
    fn project_tuple(&mut self, val: &SymValue, i: usize) -> SymValue {
        // For now, each projection creates a fresh variable linked to the source
        let var = self.fresh_var(&format!("__proj_{}", i));
        SymValue::Var(var)
    }
}

// ─── Analysis Functions ────────────────────────────────────────────

/// Analyze a file and return its constraint system.
pub fn analyze(file: &File) -> ConstraintSystem {
    SymExecutor::new().execute_file(file)
}

/// Verification result for a function or program.
#[derive(Clone, Debug)]
pub struct VerificationResult {
    /// The function or program name.
    pub name: String,
    /// Total constraints.
    pub total_constraints: usize,
    /// Active (non-trivial) constraints.
    pub active_constraints: usize,
    /// Trivially violated constraints (definite bugs).
    pub violated: Vec<String>,
    /// Redundant (trivially satisfied) constraints.
    pub redundant_count: usize,
    /// Summary of the constraint system.
    pub system_summary: String,
}

impl VerificationResult {
    pub fn is_safe(&self) -> bool {
        self.violated.is_empty()
    }

    pub fn format_report(&self) -> String {
        let mut report = String::new();
        report.push_str(&format!("Verification: {}\n", self.name));
        report.push_str(&format!("  {}\n", self.system_summary));
        report.push_str(&format!(
            "  Constraints: {} total, {} active, {} redundant\n",
            self.total_constraints, self.active_constraints, self.redundant_count,
        ));
        if self.violated.is_empty() {
            report.push_str("  Status: SAFE (no trivially violated assertions)\n");
        } else {
            report.push_str(&format!(
                "  Status: VIOLATED ({} assertion(s) always fail)\n",
                self.violated.len()
            ));
            for v in &self.violated {
                report.push_str(&format!("    - {}\n", v));
            }
        }
        report
    }
}

/// Verify a file: analyze constraints and check for violations.
pub fn verify_file(file: &File) -> VerificationResult {
    let system = analyze(file);
    let violated: Vec<String> = system
        .violated_constraints()
        .iter()
        .map(|c| format!("{:?}", c))
        .collect();
    let redundant_count = system.constraints.iter().filter(|c| c.is_trivial()).count();

    VerificationResult {
        name: file.name.node.clone(),
        total_constraints: system.constraints.len(),
        active_constraints: system.active_constraints(),
        violated,
        redundant_count,
        system_summary: system.summary(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_program(source: &str) -> File {
        crate::parse_source(source, "test.tri").unwrap()
    }

    #[test]
    fn test_simple_assert() {
        let file = parse_program("program test\nfn main() {\n    assert(true)\n}\n");
        let system = analyze(&file);
        assert!(!system.constraints.is_empty(), "should have constraints");
        assert!(system.violated_constraints().is_empty());
    }

    #[test]
    fn test_assert_false_violated() {
        let file = parse_program("program test\nfn main() {\n    assert(false)\n}\n");
        let system = analyze(&file);
        assert!(!system.violated_constraints().is_empty());
    }

    #[test]
    fn test_pub_read_symbolic() {
        let file = parse_program(
            "program test\nfn main() {\n    let x: Field = pub_read()\n    pub_write(x)\n}\n",
        );
        let system = analyze(&file);
        assert_eq!(system.pub_inputs.len(), 1);
        assert_eq!(system.pub_outputs.len(), 1);
    }

    #[test]
    fn test_assert_eq_constants() {
        let file = parse_program("program test\nfn main() {\n    assert_eq(42, 42)\n}\n");
        let system = analyze(&file);
        // Should have a constraint that is trivially true
        assert!(system.violated_constraints().is_empty());
    }

    #[test]
    fn test_assert_eq_constants_violated() {
        let file = parse_program("program test\nfn main() {\n    assert_eq(1, 2)\n}\n");
        let system = analyze(&file);
        assert!(!system.violated_constraints().is_empty());
    }

    #[test]
    fn test_divine_input_tracking() {
        let file = parse_program(
            "program test\nfn main() {\n    let x: Field = divine()\n    let y: Field = divine()\n}\n",
        );
        let system = analyze(&file);
        assert_eq!(system.divine_inputs.len(), 2);
    }

    #[test]
    fn test_arithmetic_simplification() {
        let v =
            SymValue::Add(Box::new(SymValue::Const(3)), Box::new(SymValue::Const(4))).simplify();
        assert_eq!(v, SymValue::Const(7));
    }

    #[test]
    fn test_mul_by_zero() {
        let v = SymValue::Mul(
            Box::new(SymValue::Const(0)),
            Box::new(SymValue::Var(SymVar {
                name: "x".to_string(),
                version: 0,
            })),
        )
        .simplify();
        assert_eq!(v, SymValue::Const(0));
    }

    #[test]
    fn test_add_zero_identity() {
        let x = SymValue::Var(SymVar {
            name: "x".to_string(),
            version: 0,
        });
        let v = SymValue::Add(Box::new(SymValue::Const(0)), Box::new(x.clone())).simplify();
        assert_eq!(v, x);
    }

    #[test]
    fn test_range_u32_constraint() {
        let file = parse_program(
            "program test\nfn main() {\n    let x: Field = pub_read()\n    let y: U32 = as_u32(x)\n}\n",
        );
        let system = analyze(&file);
        let has_range = system
            .constraints
            .iter()
            .any(|c| matches!(c, Constraint::RangeU32(_)));
        assert!(has_range);
    }

    #[test]
    fn test_verify_file_safe() {
        let file = parse_program(
            "program test\nfn main() {\n    let x: Field = pub_read()\n    pub_write(x)\n}\n",
        );
        let result = verify_file(&file);
        assert!(result.is_safe());
    }

    #[test]
    fn test_verify_file_violated() {
        let file = parse_program("program test\nfn main() {\n    assert(false)\n}\n");
        let result = verify_file(&file);
        assert!(!result.is_safe());
    }

    #[test]
    fn test_function_inlining() {
        let file = parse_program(
            "program test\nfn helper() {\n    assert(true)\n}\nfn main() {\n    helper()\n}\n",
        );
        let system = analyze(&file);
        // The inlined assert(true) should produce a constraint
        assert!(!system.constraints.is_empty());
    }

    #[test]
    fn test_if_else_symbolic() {
        let file = parse_program(
            "program test\nfn main() {\n    let x: Field = pub_read()\n    if x == 0 {\n        assert(true)\n    } else {\n        assert(true)\n    }\n}\n",
        );
        let system = analyze(&file);
        assert!(system.violated_constraints().is_empty());
    }
}
