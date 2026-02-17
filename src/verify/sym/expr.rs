use super::*;

impl SymExecutor {
    pub(crate) fn eval_expr(&mut self, expr: &Expr) -> SymValue {
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
    pub(crate) fn eval_call(&mut self, path: &ModulePath, args: &[Spanned<Expr>]) -> SymValue {
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
                return SymValue::Hash(inputs, 0);
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
            "as_field" => {
                // Type conversion: U32 → Field (identity in the field)
                if let Some(arg) = args.first() {
                    return self.eval_expr(&arg.node);
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
    pub(crate) fn project_tuple(&mut self, val: &SymValue, i: usize) -> SymValue {
        // If projecting from a hash, preserve the Hash origin with the index
        if let SymValue::Hash(inputs, _) = val {
            return SymValue::Hash(inputs.clone(), i);
        }
        let var = self.fresh_var(&format!("__proj_{}", i));
        SymValue::Var(var)
    }
}
