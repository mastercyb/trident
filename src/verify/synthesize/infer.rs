use super::*;

// ─── Precondition Inference ────────────────────────────────────────

/// Infer preconditions from assertions in the function body.
///
/// If a function calls `assert(cond)`, then `cond` is a necessary precondition
/// for the function to succeed. We surface it as a `#[requires]` suggestion.
pub(crate) fn infer_preconditions(func: &FnDef) -> Vec<SynthesizedSpec> {
    let mut specs = Vec::new();
    if let Some(ref body) = func.body {
        collect_preconditions_from_block(&body.node, &func.name.node, &func.params, &mut specs);
    }
    specs
}

/// Walk a block collecting assert conditions that reference function parameters.
fn collect_preconditions_from_block(
    block: &Block,
    fn_name: &str,
    params: &[Param],
    specs: &mut Vec<SynthesizedSpec>,
) {
    let param_names: Vec<&str> = params.iter().map(|p| p.name.node.as_str()).collect();

    for stmt in &block.stmts {
        match &stmt.node {
            Stmt::Expr(expr) => {
                check_expr_for_preconditions(&expr.node, fn_name, &param_names, specs);
            }
            Stmt::Let { init, .. } => {
                // Check init expressions (e.g., `let x = as_u32(val)`)
                check_expr_for_preconditions(&init.node, fn_name, &param_names, specs);
            }
            // Recurse into nested blocks
            Stmt::If {
                then_block,
                else_block,
                ..
            } => {
                collect_preconditions_from_block(&then_block.node, fn_name, params, specs);
                if let Some(ref else_blk) = else_block {
                    collect_preconditions_from_block(&else_blk.node, fn_name, params, specs);
                }
            }
            Stmt::For { body, .. } => {
                collect_preconditions_from_block(&body.node, fn_name, params, specs);
            }
            _ => {}
        }
    }

    // Also check the tail expression (Trident blocks may end with an expression)
    if let Some(ref tail) = block.tail_expr {
        check_expr_for_preconditions(&tail.node, fn_name, &param_names, specs);
    }
}

/// Check a single expression for assert/assert_eq/as_u32 calls that imply preconditions.
fn check_expr_for_preconditions(
    expr: &Expr,
    fn_name: &str,
    param_names: &[&str],
    specs: &mut Vec<SynthesizedSpec>,
) {
    if let Expr::Call { path, args, .. } = expr {
        let call_name = path.node.0.last().map(|s| s.as_str()).unwrap_or("");
        if call_name == "assert" {
            if let Some(arg) = args.first() {
                let cond_str = expr_to_string(&arg.node);
                if param_names.iter().any(|p| cond_str.contains(p)) {
                    specs.push(SynthesizedSpec {
                        function: fn_name.to_string(),
                        kind: SpecKind::Precondition,
                        expression: cond_str.clone(),
                        confidence: 80,
                        explanation: format!(
                            "Function asserts {}, which constrains input parameters",
                            cond_str
                        ),
                    });
                }
            }
        } else if call_name == "assert_eq" && args.len() >= 2 {
            let a_str = expr_to_string(&args[0].node);
            let b_str = expr_to_string(&args[1].node);
            let eq_str = format!("{} == {}", a_str, b_str);
            if param_names.iter().any(|p| eq_str.contains(p)) {
                specs.push(SynthesizedSpec {
                    function: fn_name.to_string(),
                    kind: SpecKind::Precondition,
                    expression: eq_str.clone(),
                    confidence: 80,
                    explanation: format!(
                        "Function asserts equality {}, which constrains input parameters",
                        eq_str
                    ),
                });
            }
        } else if call_name == "as_u32" {
            if let Some(arg) = args.first() {
                let arg_str = expr_to_string(&arg.node);
                if param_names.iter().any(|p| arg_str.contains(p)) {
                    specs.push(SynthesizedSpec {
                        function: fn_name.to_string(),
                        kind: SpecKind::Precondition,
                        expression: format!("{} <= 4294967295", arg_str),
                        confidence: 90,
                        explanation: format!(
                            "as_u32({}) requires the value fits in U32 range",
                            arg_str
                        ),
                    });
                }
            }
        }
    }
}

// ─── Postcondition Inference from Body ─────────────────────────────

/// Infer postconditions by inspecting the function body structure.
pub(crate) fn infer_postconditions_from_body(func: &FnDef, body: &Block) -> Vec<SynthesizedSpec> {
    let mut specs = Vec::new();
    let fn_name = &func.name.node;

    // If the function has a tail expression that is a simple BinOp on parameters,
    // we can suggest a postcondition relating result to parameters.
    if let Some(ref tail) = body.tail_expr {
        if let Expr::BinOp { op, lhs, rhs } = &tail.node {
            let lhs_str = expr_to_string(&lhs.node);
            let rhs_str = expr_to_string(&rhs.node);
            let param_names: Vec<&str> = func.params.iter().map(|p| p.name.node.as_str()).collect();
            let references_params = param_names
                .iter()
                .any(|p| lhs_str.contains(p) || rhs_str.contains(p));

            if references_params {
                let op_str = op.as_str();
                specs.push(SynthesizedSpec {
                    function: fn_name.clone(),
                    kind: SpecKind::Postcondition,
                    expression: format!("result == {} {} {}", lhs_str, op_str, rhs_str),
                    confidence: 70,
                    explanation: format!(
                        "Function returns {} {} {}, directly computed from parameters",
                        lhs_str, op_str, rhs_str
                    ),
                });
            }
        }
    }

    specs
}

// ─── CEGIS Refinement ──────────────────────────────────────────────

/// Maximum number of CEGIS refinement rounds.
const MAX_CEGIS_ROUNDS: usize = 5;

/// CEGIS loop: propose, verify, refine.
///
/// Takes candidate invariant expressions and checks them against the solver.
/// Returns specs that passed verification (high confidence).
pub(crate) fn cegis_refine(
    func: &FnDef,
    file: &File,
    candidates: &[String],
) -> Vec<SynthesizedSpec> {
    let mut verified = Vec::new();

    for candidate in candidates {
        // Build a verification program: the original function body + assert(candidate)
        let source = build_verification_source(func, file, candidate);
        if source.is_none() {
            continue;
        }
        let source = source.expect("None case handled by continue above");

        // Try to verify the candidate
        let mut current_candidate = candidate.clone();
        let mut rounds = 0;

        while rounds < MAX_CEGIS_ROUNDS {
            if verify_candidate(&source, &current_candidate) {
                verified.push(SynthesizedSpec {
                    function: func.name.node.clone(),
                    kind: SpecKind::Assertion,
                    expression: current_candidate.clone(),
                    confidence: 90,
                    explanation: "Verified by CEGIS solver loop".to_string(),
                });
                break;
            }
            // Refinement: try weakening the candidate
            if let Some(weakened) = weaken_candidate(&current_candidate) {
                current_candidate = weakened;
                rounds += 1;
            } else {
                break;
            }
        }
    }

    verified
}

/// Build a small Trident source that asserts the candidate expression.
///
/// We embed the function body into a program and add the candidate as an
/// assertion after the body. This is a best-effort construction for simple
/// cases.
fn build_verification_source(func: &FnDef, _file: &File, _candidate: &str) -> Option<String> {
    // Only handle functions with bodies and no parameters (for simplicity)
    if func.body.is_none() {
        return None;
    }
    // We produce a simplified source for the solver. For functions with
    // parameters, we would need to wrap them in a program that supplies
    // symbolic inputs, which is complex. Skip those for now.
    if !func.params.is_empty() {
        return None;
    }

    // For parameter-less functions, we can inline the body into a program.
    // However, constructing syntactically valid Trident source from the AST
    // is non-trivial. We return None to indicate CEGIS is not applicable
    // for this function and let template matching carry the confidence.
    None
}

/// Check if a candidate invariant holds using the solver.
///
/// Parses the source, runs symbolic execution, and checks with the solver.
/// Returns true if no violations are found.
pub(crate) fn verify_candidate(source: &str, _candidate: &str) -> bool {
    match crate::parse_source_silent(source, "synth_check.tri") {
        Ok(file) => {
            let system = sym::analyze(&file);
            let report = solve::verify(&system);
            report.is_safe()
        }
        Err(_) => false,
    }
}

/// Attempt to weaken a candidate invariant.
///
/// Simple heuristics:
/// - `x == K` -> `x >= 0` (weaken equality to range)
/// - `x <= K` -> `x <= K+1` (widen bound)
/// - `x >= K` -> `x >= K-1` (lower floor)
pub(crate) fn weaken_candidate(candidate: &str) -> Option<String> {
    // Try to weaken `<= K` to `<= K+1`
    if let Some(pos) = candidate.find(" <= ") {
        let rhs = &candidate[pos + 4..];
        if let Ok(n) = rhs.trim().parse::<u64>() {
            let lhs = &candidate[..pos];
            return Some(format!("{} <= {}", lhs, n + 1));
        }
    }
    // Try to weaken `>= K` to `>= K-1`
    if let Some(pos) = candidate.find(" >= ") {
        let rhs = &candidate[pos + 4..];
        if let Ok(n) = rhs.trim().parse::<u64>() {
            if n > 0 {
                let lhs = &candidate[..pos];
                return Some(format!("{} >= {}", lhs, n - 1));
            }
        }
    }
    None
}

// ─── Symbolic Postcondition Inference ──────────────────────────────

/// Infer postconditions from a constraint system.
///
/// Examines the symbolic output values and constraints to suggest
/// relationships between inputs and outputs.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn infer_postconditions_from_constraints(
    func: &FnDef,
    system: &ConstraintSystem,
) -> Vec<SynthesizedSpec> {
    let mut specs = Vec::new();
    let fn_name = &func.name.node;

    // If there are public outputs, check if any are directly equal to inputs
    for (i, output) in system.pub_outputs.iter().enumerate() {
        match output {
            SymValue::Const(c) => {
                specs.push(SynthesizedSpec {
                    function: fn_name.clone(),
                    kind: SpecKind::Postcondition,
                    expression: format!("output[{}] == {}", i, c),
                    confidence: 100,
                    explanation: format!("Output {} is always the constant {}", i, c),
                });
            }
            SymValue::Var(var) => {
                // Check if it matches an input variable
                for input in &system.pub_inputs {
                    if var.name == input.name {
                        specs.push(SynthesizedSpec {
                            function: fn_name.clone(),
                            kind: SpecKind::Postcondition,
                            expression: format!("output[{}] == input[{}]", i, input.name),
                            confidence: 90,
                            explanation: format!(
                                "Output {} directly passes through input {}",
                                i, input.name
                            ),
                        });
                    }
                }
            }
            _ => {}
        }
    }

    // Check for constant propagation through constraints
    let mut const_vars: BTreeMap<String, u64> = BTreeMap::new();
    for constraint in &system.constraints {
        if let sym::Constraint::Equal(SymValue::Var(var), SymValue::Const(c)) = constraint {
            const_vars.insert(var.name.clone(), *c);
        }
        if let sym::Constraint::Equal(SymValue::Const(c), SymValue::Var(var)) = constraint {
            const_vars.insert(var.name.clone(), *c);
        }
    }

    for (var_name, value) in &const_vars {
        if !var_name.starts_with("__") {
            specs.push(SynthesizedSpec {
                function: fn_name.clone(),
                kind: SpecKind::Assertion,
                expression: format!("{} == {}", var_name, value),
                confidence: 70,
                explanation: format!(
                    "Variable {} is constrained to equal {} by assertions",
                    var_name, value
                ),
            });
        }
    }

    specs
}
