use super::*;

// ─── Template Matching ─────────────────────────────────────────────

/// Template matching: identify common patterns in function bodies.
pub(crate) fn match_templates(func: &FnDef) -> Vec<SynthesizedSpec> {
    let mut specs = Vec::new();
    let fn_name = &func.name.node;

    if let Some(ref body) = func.body {
        // Collect mutable variable initializations
        let mut_inits = collect_mut_inits(&body.node);

        // Walk statements looking for for-loops
        for stmt in &body.node.stmts {
            if let Stmt::For {
                var: loop_var,
                end,
                body: loop_body,
                ..
            } = &stmt.node
            {
                let loop_var_name = &loop_var.node;
                let end_str = expr_to_string(&end.node);

                // Check each mutable variable for patterns inside this loop
                for (var_name, init_expr) in &mut_inits {
                    let init_str = expr_to_string(init_expr);

                    // Look for accumulation patterns: acc = acc + expr
                    if let Some(pattern) = find_accumulation_pattern(
                        var_name,
                        loop_var_name,
                        &init_str,
                        &loop_body.node,
                    ) {
                        match pattern.op {
                            AccumulationOp::Additive => {
                                // Invariant: accumulator relates to partial sum
                                specs.push(SynthesizedSpec {
                                    function: fn_name.clone(),
                                    kind: SpecKind::LoopInvariant {
                                        loop_var: loop_var_name.clone(),
                                    },
                                    expression: format!(
                                        "{} >= {}",
                                        pattern.acc_var, pattern.init_value
                                    ),
                                    confidence: 60,
                                    explanation: format!(
                                        "Accumulation pattern: {} is additively updated in loop over {}",
                                        pattern.acc_var, pattern.loop_var
                                    ),
                                });
                                // Postcondition: result after full iteration
                                specs.push(SynthesizedSpec {
                                    function: fn_name.clone(),
                                    kind: SpecKind::Postcondition,
                                    expression: format!(
                                        "{} == sum of additions over 0..{}",
                                        pattern.acc_var, end_str
                                    ),
                                    confidence: 50,
                                    explanation: format!(
                                        "After the loop, {} holds the accumulated sum",
                                        pattern.acc_var
                                    ),
                                });
                            }
                            AccumulationOp::Multiplicative => {
                                specs.push(SynthesizedSpec {
                                    function: fn_name.clone(),
                                    kind: SpecKind::LoopInvariant {
                                        loop_var: loop_var_name.clone(),
                                    },
                                    expression: format!(
                                        "{} >= {}",
                                        pattern.acc_var, pattern.init_value
                                    ),
                                    confidence: 50,
                                    explanation: format!(
                                        "Multiplicative accumulation: {} is scaled each iteration",
                                        pattern.acc_var
                                    ),
                                });
                            }
                            AccumulationOp::Counting => {
                                // Counting pattern: count <= i
                                specs.push(SynthesizedSpec {
                                    function: fn_name.clone(),
                                    kind: SpecKind::LoopInvariant {
                                        loop_var: loop_var_name.clone(),
                                    },
                                    expression: format!(
                                        "{} <= {}",
                                        pattern.acc_var, loop_var_name
                                    ),
                                    confidence: 80,
                                    explanation: format!(
                                        "Counting pattern: {} increments conditionally, bounded by loop variable {}",
                                        pattern.acc_var, loop_var_name
                                    ),
                                });
                                // Postcondition: count <= N
                                specs.push(SynthesizedSpec {
                                    function: fn_name.clone(),
                                    kind: SpecKind::Postcondition,
                                    expression: format!("{} <= {}", pattern.acc_var, end_str),
                                    confidence: 80,
                                    explanation: format!(
                                        "After the loop, {} is at most {}",
                                        pattern.acc_var, end_str
                                    ),
                                });
                            }
                        }
                    }

                    // Look for monotonic update patterns
                    if let Some(pattern) =
                        find_monotonic_pattern(var_name, loop_var_name, &init_str, &loop_body.node)
                    {
                        specs.push(SynthesizedSpec {
                            function: fn_name.clone(),
                            kind: SpecKind::LoopInvariant {
                                loop_var: pattern.loop_var.clone(),
                            },
                            expression: format!("{} >= {}", pattern.var, pattern.init_value),
                            confidence: 70,
                            explanation: format!(
                                "Monotonic update: {} only increases in loop over {}",
                                pattern.var, pattern.loop_var
                            ),
                        });
                    }
                }
            }
        }

        // Identity preservation: fn f(x) -> x
        if let Some(spec) = check_identity_preservation(func) {
            specs.push(spec);
        }

        // Range preservation: U32 input -> U32 output
        if let Some(spec) = check_range_preservation(func) {
            specs.push(spec);
        }

        // Constant result detection
        if let Some(spec) = check_constant_result(func, &body.node) {
            specs.push(spec);
        }
    }

    specs
}

// ─── Accumulation Pattern Detection ────────────────────────────────

/// Look for `acc = acc + expr` or `acc = acc + 1` (inside if) patterns in a loop body.
fn find_accumulation_pattern(
    var_name: &str,
    loop_var: &str,
    init_str: &str,
    body: &Block,
) -> Option<AccumulationPattern> {
    for stmt in &body.stmts {
        match &stmt.node {
            // Direct assignment: acc = acc + expr
            Stmt::Assign { place, value } => {
                if place_is_var(&place.node, var_name) {
                    if let Some(op) = classify_accumulation(&value.node, var_name) {
                        return Some(AccumulationPattern {
                            acc_var: var_name.to_string(),
                            loop_var: loop_var.to_string(),
                            init_value: init_str.to_string(),
                            op,
                        });
                    }
                }
            }
            // Conditional increment: if cond { acc = acc + 1 }
            Stmt::If { then_block, .. } => {
                if let Some(mut pattern) =
                    find_accumulation_pattern(var_name, loop_var, init_str, &then_block.node)
                {
                    // If the accumulation is `acc = acc + 1` inside a conditional,
                    // reclassify as counting.
                    if pattern.op == AccumulationOp::Additive {
                        if is_increment_by_one_in_block(&then_block.node, var_name) {
                            pattern.op = AccumulationOp::Counting;
                        }
                    }
                    return Some(pattern);
                }
            }
            _ => {}
        }
    }
    None
}

/// Classify an assignment RHS as an accumulation operation on `var_name`.
fn classify_accumulation(expr: &Expr, var_name: &str) -> Option<AccumulationOp> {
    match expr {
        Expr::BinOp { op, lhs, rhs } => {
            let lhs_is_var = expr_is_var(&lhs.node, var_name);
            let rhs_is_var = expr_is_var(&rhs.node, var_name);
            match op {
                BinOp::Add if lhs_is_var || rhs_is_var => Some(AccumulationOp::Additive),
                BinOp::Mul if lhs_is_var || rhs_is_var => Some(AccumulationOp::Multiplicative),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Check if a block contains `var_name = var_name + 1`.
fn is_increment_by_one_in_block(block: &Block, var_name: &str) -> bool {
    for stmt in &block.stmts {
        if let Stmt::Assign { place, value } = &stmt.node {
            if place_is_var(&place.node, var_name) {
                if let Expr::BinOp {
                    op: BinOp::Add,
                    lhs,
                    rhs,
                } = &value.node
                {
                    let lhs_is_var = expr_is_var(&lhs.node, var_name);
                    let rhs_is_one = matches!(&rhs.node, Expr::Literal(Literal::Integer(1)));
                    let rhs_is_var = expr_is_var(&rhs.node, var_name);
                    let lhs_is_one = matches!(&lhs.node, Expr::Literal(Literal::Integer(1)));
                    if (lhs_is_var && rhs_is_one) || (rhs_is_var && lhs_is_one) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

// ─── Monotonic Pattern Detection ───────────────────────────────────

/// Look for monotonically increasing updates: `var = var + positive_expr`.
fn find_monotonic_pattern(
    var_name: &str,
    loop_var: &str,
    init_str: &str,
    body: &Block,
) -> Option<MonotonicPattern> {
    for stmt in &body.stmts {
        if let Stmt::Assign { place, value } = &stmt.node {
            if place_is_var(&place.node, var_name) {
                if let Expr::BinOp {
                    op: BinOp::Add,
                    lhs,
                    rhs,
                } = &value.node
                {
                    let lhs_is_var = expr_is_var(&lhs.node, var_name);
                    let rhs_is_var = expr_is_var(&rhs.node, var_name);
                    // `var = var + expr` where expr is non-negative (literal)
                    let other_is_nonneg_literal = if lhs_is_var {
                        is_nonneg_literal(&rhs.node)
                    } else if rhs_is_var {
                        is_nonneg_literal(&lhs.node)
                    } else {
                        false
                    };
                    if other_is_nonneg_literal {
                        return Some(MonotonicPattern {
                            var: var_name.to_string(),
                            loop_var: loop_var.to_string(),
                            init_value: init_str.to_string(),
                        });
                    }
                }
            }
        }
    }
    None
}

/// Check if an expression is a non-negative integer literal.
fn is_nonneg_literal(expr: &Expr) -> bool {
    matches!(expr, Expr::Literal(Literal::Integer(_)))
}

// ─── Identity / Range / Constant Detection ─────────────────────────

/// Check if a function just returns one of its parameters (identity preservation).
pub(crate) fn check_identity_preservation(func: &FnDef) -> Option<SynthesizedSpec> {
    // Requires: exactly one parameter, a return type, and a body that is just
    // the parameter name as a tail expression.
    if func.params.len() != 1 {
        return None;
    }
    let param_name = &func.params[0].name.node;
    if let Some(ref body) = func.body {
        if body.node.stmts.is_empty() {
            if let Some(ref tail) = body.node.tail_expr {
                if expr_is_var(&tail.node, param_name) {
                    return Some(SynthesizedSpec {
                        function: func.name.node.clone(),
                        kind: SpecKind::Postcondition,
                        expression: format!("result == {}", param_name),
                        confidence: 100,
                        explanation: format!(
                            "Function returns its parameter {} unchanged (identity)",
                            param_name
                        ),
                    });
                }
            }
        }
    }
    None
}

/// Check if input and output are both U32, suggesting range preservation.
pub(crate) fn check_range_preservation(func: &FnDef) -> Option<SynthesizedSpec> {
    // All params are U32 and return type is U32
    if func.params.is_empty() {
        return None;
    }
    let all_u32_params = func.params.iter().all(|p| p.ty.node == Type::U32);
    let returns_u32 = func
        .return_ty
        .as_ref()
        .map_or(false, |ty| ty.node == Type::U32);
    if all_u32_params && returns_u32 {
        Some(SynthesizedSpec {
            function: func.name.node.clone(),
            kind: SpecKind::Postcondition,
            expression: "result <= 4294967295".to_string(),
            confidence: 90,
            explanation: "U32 input(s) and U32 output suggest result fits in U32 range".to_string(),
        })
    } else {
        None
    }
}

/// Check if a function always returns a constant value.
pub(crate) fn check_constant_result(func: &FnDef, body: &Block) -> Option<SynthesizedSpec> {
    // Simple case: no statements, tail expression is a literal
    if !body.stmts.is_empty() {
        return None;
    }
    if let Some(ref tail) = body.tail_expr {
        if let Expr::Literal(Literal::Integer(n)) = &tail.node {
            return Some(SynthesizedSpec {
                function: func.name.node.clone(),
                kind: SpecKind::Postcondition,
                expression: format!("result == {}", n),
                confidence: 100,
                explanation: format!("Function always returns the constant {}", n),
            });
        }
        if let Expr::Literal(Literal::Bool(b)) = &tail.node {
            return Some(SynthesizedSpec {
                function: func.name.node.clone(),
                kind: SpecKind::Postcondition,
                expression: format!("result == {}", b),
                confidence: 100,
                explanation: format!("Function always returns the constant {}", b),
            });
        }
    }
    None
}
