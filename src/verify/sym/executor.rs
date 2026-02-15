use super::*;

// ─── Symbolic Executor ─────────────────────────────────────────────

/// Symbolic executor that walks the AST and builds a constraint system.
pub struct SymExecutor {
    /// The constraint system being built.
    pub(crate) system: ConstraintSystem,
    /// Variable bindings: name → symbolic value.
    pub(crate) env: HashMap<String, SymValue>,
    /// SSA version counter per variable name.
    pub(crate) versions: HashMap<String, u32>,
    /// Counter for divine inputs.
    pub(crate) divine_counter: u32,
    /// Counter for public inputs.
    pub(crate) pub_input_counter: u32,
    /// Current path condition (conjunction of conditions leading here).
    pub(crate) path_condition: Vec<SymValue>,
    /// Function definitions for inlining.
    pub(crate) functions: HashMap<String, FnDef>,
    /// Recursion guard (prevent infinite inlining — shouldn't happen in Trident).
    pub(crate) call_depth: u32,
    /// Maximum call depth before giving up.
    pub(crate) max_call_depth: u32,
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
    pub(crate) fn fresh_var(&mut self, name: &str) -> SymVar {
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
    pub(crate) fn fresh_divine(&mut self) -> SymValue {
        let idx = self.divine_counter;
        self.divine_counter += 1;
        let var = self.fresh_var(&format!("divine_{}", idx));
        self.system.divine_inputs.push(var.clone());
        SymValue::Var(var)
    }

    /// Create a public input variable.
    pub(crate) fn fresh_pub_input(&mut self) -> SymValue {
        let idx = self.pub_input_counter;
        self.pub_input_counter += 1;
        let var = self.fresh_var(&format!("pub_in_{}", idx));
        self.system.pub_inputs.push(var.clone());
        SymValue::Var(var)
    }

    /// Add a constraint, wrapping with current path condition.
    pub(crate) fn add_constraint(&mut self, c: Constraint) {
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
    pub(crate) fn execute_block(&mut self, block: &Block) {
        for stmt in &block.stmts {
            self.execute_stmt(&stmt.node);
        }
        // Evaluate tail expression for side effects (e.g., assert calls)
        if let Some(ref tail) = block.tail_expr {
            let _ = self.eval_expr(&tail.node);
        }
    }

    /// Execute a single statement.
    pub(crate) fn execute_stmt(&mut self, stmt: &Stmt) {
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
                        let _var = self.fresh_var(&name.node);
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
            Stmt::Reveal { .. } | Stmt::Seal { .. } => {
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
                        MatchPattern::Struct { .. } => {
                            // Struct patterns are unconditional (type-checked)
                            SymValue::Const(1)
                        }
                    };
                    self.path_condition.push(cond.clone());
                    // For struct patterns, bind fields before executing body
                    if let MatchPattern::Struct { fields, .. } = &arm.pattern.node {
                        for spf in fields {
                            if let FieldPattern::Binding(var_name) = &spf.pattern.node {
                                // Bind field to a symbolic field access
                                let field_val = SymValue::FieldAccess(
                                    Box::new(match_val.clone()),
                                    spf.field_name.node.clone(),
                                );
                                self.env.insert(var_name.clone(), field_val);
                            }
                        }
                    }
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

}
