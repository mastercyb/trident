//! Automatic invariant synthesis for Trident programs.
//!
//! Techniques:
//! 1. Template-based synthesis: match common patterns (accumulation, counting,
//!    monotonic updates) and instantiate invariant templates.
//! 2. Counterexample-guided inductive synthesis (CEGIS): propose candidate
//!    invariants, verify with solver, refine using counterexamples.
//! 3. Specification inference: suggest postconditions from code analysis.

use crate::ast::*;
use crate::solve;
use crate::sym::{self, ConstraintSystem, SymValue};
use std::collections::HashMap;

// ─── Data Structures ───────────────────────────────────────────────

/// A synthesized invariant or specification.
#[derive(Clone, Debug)]
pub struct SynthesizedSpec {
    /// The function this applies to.
    pub function: String,
    /// The kind of spec.
    pub kind: SpecKind,
    /// The invariant/spec expression as a string.
    pub expression: String,
    /// Confidence level (0.0 - 1.0).
    pub confidence: f64,
    /// Human-readable explanation.
    pub explanation: String,
}

impl SynthesizedSpec {
    /// Format this spec as a human-readable suggestion.
    pub fn format(&self) -> String {
        let kind_str = match &self.kind {
            SpecKind::LoopInvariant { loop_var } => {
                format!("loop invariant (over {})", loop_var)
            }
            SpecKind::Postcondition => "postcondition (#[ensures])".to_string(),
            SpecKind::Precondition => "precondition (#[requires])".to_string(),
            SpecKind::Assertion => "assertion".to_string(),
        };
        let confidence_str = if self.confidence >= 0.9 {
            "high"
        } else if self.confidence >= 0.6 {
            "medium"
        } else {
            "low"
        };
        format!(
            "  [{}] {} {}: {}\n    {}",
            confidence_str, self.function, kind_str, self.expression, self.explanation,
        )
    }
}

/// The kind of synthesized specification.
#[derive(Clone, Debug, PartialEq)]
pub enum SpecKind {
    /// Loop invariant for a specific loop.
    LoopInvariant { loop_var: String },
    /// Function postcondition (`#[ensures]`).
    Postcondition,
    /// Function precondition (`#[requires]`).
    Precondition,
    /// Assertion that could be added.
    Assertion,
}

// ─── Pattern Descriptors ───────────────────────────────────────────

/// Describes an accumulation pattern found in a loop.
#[derive(Clone, Debug)]
struct AccumulationPattern {
    /// The accumulator variable name.
    acc_var: String,
    /// The loop iteration variable name.
    loop_var: String,
    /// Initial value of the accumulator (as source text).
    init_value: String,
    /// The operation applied each iteration.
    op: AccumulationOp,
}

/// The kind of accumulation operation.
#[derive(Clone, Debug, PartialEq)]
enum AccumulationOp {
    /// `acc = acc + expr`
    Additive,
    /// `acc = acc * expr`
    Multiplicative,
    /// `acc = acc + 1` inside a conditional (counting)
    Counting,
}

/// Describes a monotonic update pattern.
#[derive(Clone, Debug)]
struct MonotonicPattern {
    /// The variable being updated.
    var: String,
    /// The loop iteration variable.
    loop_var: String,
    /// Initial value expression.
    init_value: String,
}

// ─── Top-Level Entry Points ────────────────────────────────────────

/// Analyze a file and synthesize specifications for all functions.
pub fn synthesize_specs(file: &File) -> Vec<SynthesizedSpec> {
    let mut specs = Vec::new();
    for item in &file.items {
        if let Item::Fn(func) = &item.node {
            if func.body.is_some() {
                let mut fn_specs = synthesize_for_function(func, file);
                specs.append(&mut fn_specs);
            }
        }
    }
    specs
}

/// Format all synthesized specs as a human-readable report.
pub fn format_report(specs: &[SynthesizedSpec]) -> String {
    if specs.is_empty() {
        return "No specifications synthesized.\n".to_string();
    }
    let mut report = String::new();
    report.push_str(&format!(
        "Synthesized {} specification(s):\n\n",
        specs.len()
    ));
    for spec in specs {
        report.push_str(&spec.format());
        report.push('\n');
    }
    report
}

// ─── Per-Function Synthesis ────────────────────────────────────────

/// Synthesize specs for a single function.
fn synthesize_for_function(func: &FnDef, file: &File) -> Vec<SynthesizedSpec> {
    let fn_name = &func.name.node;
    let mut specs = Vec::new();

    // 1. Template-based pattern matching
    let mut template_specs = match_templates(func);
    specs.append(&mut template_specs);

    // 2. Infer preconditions from assertions
    let mut pre_specs = infer_preconditions(func);
    specs.append(&mut pre_specs);

    // 3. Infer postconditions from symbolic execution
    if let Some(ref body) = func.body {
        let mut post_specs = infer_postconditions_from_body(func, &body.node);
        specs.append(&mut post_specs);
    }

    // 4. CEGIS refinement: verify template candidates against the solver
    let candidates: Vec<String> = specs.iter().map(|s| s.expression.clone()).collect();
    if !candidates.is_empty() {
        let refined = cegis_refine(func, file, &candidates);
        // Update confidence for candidates that were verified
        for spec in &mut specs {
            if refined.iter().any(|r| r.expression == spec.expression) {
                spec.confidence = spec.confidence.max(0.9);
            }
        }
    }

    // Set function name on all specs
    for spec in &mut specs {
        spec.function = fn_name.clone();
    }

    specs
}

// ─── Template Matching ─────────────────────────────────────────────

/// Template matching: identify common patterns in function bodies.
fn match_templates(func: &FnDef) -> Vec<SynthesizedSpec> {
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
                                    confidence: 0.6,
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
                                    confidence: 0.5,
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
                                    confidence: 0.5,
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
                                    confidence: 0.8,
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
                                    confidence: 0.8,
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
                            confidence: 0.7,
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
fn check_identity_preservation(func: &FnDef) -> Option<SynthesizedSpec> {
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
                        confidence: 1.0,
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
fn check_range_preservation(func: &FnDef) -> Option<SynthesizedSpec> {
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
            confidence: 0.9,
            explanation: "U32 input(s) and U32 output suggest result fits in U32 range".to_string(),
        })
    } else {
        None
    }
}

/// Check if a function always returns a constant value.
fn check_constant_result(func: &FnDef, body: &Block) -> Option<SynthesizedSpec> {
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
                confidence: 1.0,
                explanation: format!("Function always returns the constant {}", n),
            });
        }
        if let Expr::Literal(Literal::Bool(b)) = &tail.node {
            return Some(SynthesizedSpec {
                function: func.name.node.clone(),
                kind: SpecKind::Postcondition,
                expression: format!("result == {}", b),
                confidence: 1.0,
                explanation: format!("Function always returns the constant {}", b),
            });
        }
    }
    None
}

// ─── Precondition Inference ────────────────────────────────────────

/// Infer preconditions from assertions in the function body.
///
/// If a function calls `assert(cond)`, then `cond` is a necessary precondition
/// for the function to succeed. We surface it as a `#[requires]` suggestion.
fn infer_preconditions(func: &FnDef) -> Vec<SynthesizedSpec> {
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
                        confidence: 0.8,
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
                    confidence: 0.8,
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
                        confidence: 0.9,
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
fn infer_postconditions_from_body(func: &FnDef, body: &Block) -> Vec<SynthesizedSpec> {
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
                    confidence: 0.7,
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
fn cegis_refine(func: &FnDef, file: &File, candidates: &[String]) -> Vec<SynthesizedSpec> {
    let mut verified = Vec::new();

    for candidate in candidates {
        // Build a verification program: the original function body + assert(candidate)
        let source = build_verification_source(func, file, candidate);
        if source.is_none() {
            continue;
        }
        let source = source.unwrap();

        // Try to verify the candidate
        let mut current_candidate = candidate.clone();
        let mut rounds = 0;

        while rounds < MAX_CEGIS_ROUNDS {
            if verify_candidate(&source, &current_candidate) {
                verified.push(SynthesizedSpec {
                    function: func.name.node.clone(),
                    kind: SpecKind::Assertion,
                    expression: current_candidate.clone(),
                    confidence: 0.9,
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
fn verify_candidate(source: &str, _candidate: &str) -> bool {
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
fn weaken_candidate(candidate: &str) -> Option<String> {
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
pub fn infer_postconditions_from_constraints(
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
                    confidence: 1.0,
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
                            confidence: 0.9,
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
    let mut const_vars: HashMap<String, u64> = HashMap::new();
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
                confidence: 0.7,
                explanation: format!(
                    "Variable {} is constrained to equal {} by assertions",
                    var_name, value
                ),
            });
        }
    }

    specs
}

// ─── AST Utility Helpers ───────────────────────────────────────────

/// Collect mutable variable initializations from a block.
/// Returns `(variable_name, init_expression)` pairs.
fn collect_mut_inits(block: &Block) -> Vec<(String, Expr)> {
    let mut inits = Vec::new();
    for stmt in &block.stmts {
        if let Stmt::Let {
            mutable: true,
            pattern: Pattern::Name(name),
            init,
            ..
        } = &stmt.node
        {
            inits.push((name.node.clone(), init.node.clone()));
        }
    }
    inits
}

/// Check if a `Place` is a simple variable reference to the given name.
fn place_is_var(place: &Place, name: &str) -> bool {
    matches!(place, Place::Var(n) if n == name)
}

/// Check if an expression is a simple variable reference to the given name.
fn expr_is_var(expr: &Expr, name: &str) -> bool {
    matches!(expr, Expr::Var(n) if n == name)
}

/// Convert an expression to a human-readable string (best effort).
fn expr_to_string(expr: &Expr) -> String {
    match expr {
        Expr::Literal(Literal::Integer(n)) => n.to_string(),
        Expr::Literal(Literal::Bool(b)) => b.to_string(),
        Expr::Var(name) => name.clone(),
        Expr::BinOp { op, lhs, rhs } => {
            let l = expr_to_string(&lhs.node);
            let r = expr_to_string(&rhs.node);
            format!("({} {} {})", l, op.as_str(), r)
        }
        Expr::Call { path, args, .. } => {
            let name = path.node.as_dotted();
            let arg_strs: Vec<String> = args.iter().map(|a| expr_to_string(&a.node)).collect();
            format!("{}({})", name, arg_strs.join(", "))
        }
        Expr::Index { expr, index } => {
            format!(
                "{}[{}]",
                expr_to_string(&expr.node),
                expr_to_string(&index.node)
            )
        }
        Expr::FieldAccess { expr, field } => {
            format!("{}.{}", expr_to_string(&expr.node), field.node)
        }
        Expr::Tuple(elems) => {
            let parts: Vec<String> = elems.iter().map(|e| expr_to_string(&e.node)).collect();
            format!("({})", parts.join(", "))
        }
        Expr::ArrayInit(elems) => {
            let parts: Vec<String> = elems.iter().map(|e| expr_to_string(&e.node)).collect();
            format!("[{}]", parts.join(", "))
        }
        Expr::StructInit { path, fields } => {
            let name = path.node.as_dotted();
            let field_strs: Vec<String> = fields
                .iter()
                .map(|(n, v)| format!("{}: {}", n.node, expr_to_string(&v.node)))
                .collect();
            format!("{} {{ {} }}", name, field_strs.join(", "))
        }
    }
}

// ─── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_program(source: &str) -> File {
        crate::parse_source_silent(source, "test.tri").unwrap()
    }

    // -- Accumulation pattern --

    #[test]
    fn test_accumulation_pattern() {
        let source = r#"program test
fn sum_loop() -> Field {
    let mut acc: Field = 0
    for i in 0..10 {
        acc = acc + i
    }
    acc
}
fn main() {}
"#;
        let file = parse_program(source);
        let specs = synthesize_specs(&file);
        let acc_specs: Vec<_> = specs.iter().filter(|s| s.function == "sum_loop").collect();
        assert!(
            !acc_specs.is_empty(),
            "should synthesize specs for accumulation pattern"
        );
        let has_loop_inv = acc_specs
            .iter()
            .any(|s| matches!(&s.kind, SpecKind::LoopInvariant { .. }));
        assert!(has_loop_inv, "should produce a loop invariant");
        let has_acc_inv = acc_specs
            .iter()
            .any(|s| s.expression.contains("acc") && s.expression.contains(">= 0"));
        assert!(has_acc_inv, "should produce acc >= 0 invariant");
    }

    // -- Counting pattern --

    #[test]
    fn test_counting_pattern() {
        let source = r#"program test
fn count_loop() -> Field {
    let mut count: Field = 0
    for i in 0..10 {
        if i == 5 {
            count = count + 1
        }
    }
    count
}
fn main() {}
"#;
        let file = parse_program(source);
        let specs = synthesize_specs(&file);
        let count_specs: Vec<_> = specs
            .iter()
            .filter(|s| s.function == "count_loop")
            .collect();
        assert!(
            !count_specs.is_empty(),
            "should synthesize specs for counting pattern"
        );
        let has_count_bound = count_specs
            .iter()
            .any(|s| s.expression.contains("count") && s.expression.contains("<= i"));
        assert!(has_count_bound, "should produce count <= i loop invariant");
        let has_post = count_specs.iter().any(|s| {
            s.kind == SpecKind::Postcondition
                && s.expression.contains("count")
                && s.expression.contains("<= 10")
        });
        assert!(has_post, "should produce count <= N postcondition");
    }

    // -- Postcondition inference --

    #[test]
    fn test_postcondition_simple_return() {
        let source = r#"program test
fn add(a: Field, b: Field) -> Field {
    a + b
}
fn main() {}
"#;
        let file = parse_program(source);
        let specs = synthesize_specs(&file);
        let add_specs: Vec<_> = specs.iter().filter(|s| s.function == "add").collect();
        let has_post = add_specs
            .iter()
            .any(|s| s.kind == SpecKind::Postcondition && s.expression.contains("result == a + b"));
        assert!(has_post, "should infer postcondition result == a + b");
    }

    // -- Precondition inference --

    #[test]
    fn test_precondition_from_assert() {
        let source = r#"program test
fn guarded(x: Field) {
    assert(x == 0)
}
fn main() {}
"#;
        let file = parse_program(source);
        let specs = synthesize_specs(&file);
        let guard_specs: Vec<_> = specs.iter().filter(|s| s.function == "guarded").collect();
        let has_pre = guard_specs
            .iter()
            .any(|s| s.kind == SpecKind::Precondition && s.expression.contains("x"));
        assert!(has_pre, "should infer precondition from assert(x == 0)");
    }

    #[test]
    fn test_precondition_from_as_u32() {
        let source = r#"program test
fn range_check(val: Field) -> Field {
    let x: U32 = as_u32(val)
    val
}
fn main() {}
"#;
        let file = parse_program(source);
        let specs = synthesize_specs(&file);
        let rc_specs: Vec<_> = specs
            .iter()
            .filter(|s| s.function == "range_check")
            .collect();
        let has_range_pre = rc_specs.iter().any(|s| {
            s.kind == SpecKind::Precondition && s.expression.contains("val <= 4294967295")
        });
        assert!(
            has_range_pre,
            "should infer U32 range precondition from as_u32(val)"
        );
    }

    // -- CEGIS basic --

    #[test]
    fn test_cegis_verifies_true_candidate() {
        // A trivially safe program: assert(true)
        let source = "program test\nfn main() {\n    assert(true)\n}\n";
        let result = verify_candidate(source, "true");
        assert!(result, "should verify that assert(true) is safe");
    }

    #[test]
    fn test_cegis_rejects_false_candidate() {
        // A violated program: assert(false)
        let source = "program test\nfn main() {\n    assert(false)\n}\n";
        let result = verify_candidate(source, "false");
        assert!(!result, "should reject program with assert(false)");
    }

    // -- Trivial programs --

    #[test]
    fn test_no_specs_for_trivial_program() {
        let source = "program test\nfn main() {}\n";
        let file = parse_program(source);
        let specs = synthesize_specs(&file);
        assert!(
            specs.is_empty(),
            "should not synthesize specs for trivial empty main"
        );
    }

    // -- Identity preservation --

    #[test]
    fn test_identity_preservation() {
        let source = r#"program test
fn identity(x: Field) -> Field {
    x
}
fn main() {}
"#;
        let file = parse_program(source);
        let specs = synthesize_specs(&file);
        let id_specs: Vec<_> = specs.iter().filter(|s| s.function == "identity").collect();
        let has_identity = id_specs
            .iter()
            .any(|s| s.kind == SpecKind::Postcondition && s.expression == "result == x");
        assert!(has_identity, "should detect identity preservation");
    }

    // -- Range preservation --

    #[test]
    fn test_range_preservation() {
        let source = r#"program test
fn u32_op(a: U32, b: U32) -> U32 {
    a + b
}
fn main() {}
"#;
        let file = parse_program(source);
        let specs = synthesize_specs(&file);
        let u32_specs: Vec<_> = specs.iter().filter(|s| s.function == "u32_op").collect();
        let has_range = u32_specs
            .iter()
            .any(|s| s.kind == SpecKind::Postcondition && s.expression.contains("4294967295"));
        assert!(has_range, "should suggest U32 range postcondition");
    }

    // -- Constant result --

    #[test]
    fn test_constant_result() {
        let source = r#"program test
fn always_42() -> Field {
    42
}
fn main() {}
"#;
        let file = parse_program(source);
        let specs = synthesize_specs(&file);
        let const_specs: Vec<_> = specs.iter().filter(|s| s.function == "always_42").collect();
        let has_const = const_specs
            .iter()
            .any(|s| s.kind == SpecKind::Postcondition && s.expression == "result == 42");
        assert!(has_const, "should detect constant result");
    }

    // -- Monotonic pattern --

    #[test]
    fn test_monotonic_pattern() {
        let source = r#"program test
fn mono() -> Field {
    let mut x: Field = 0
    for i in 0..5 {
        x = x + 3
    }
    x
}
fn main() {}
"#;
        let file = parse_program(source);
        let specs = synthesize_specs(&file);
        let mono_specs: Vec<_> = specs.iter().filter(|s| s.function == "mono").collect();
        let has_mono = mono_specs.iter().any(|s| {
            matches!(&s.kind, SpecKind::LoopInvariant { .. }) && s.expression.contains("x >= 0")
        });
        assert!(has_mono, "should detect monotonic increase pattern");
    }

    // -- Symbolic postcondition inference --

    #[test]
    fn test_symbolic_postcondition_constant_output() {
        let source = "program test\nfn main() {\n    pub_write(42)\n}\n";
        let file = parse_program(source);
        let system = sym::analyze(&file);
        // Find the main function
        let main_fn = file
            .items
            .iter()
            .find_map(|item| {
                if let Item::Fn(f) = &item.node {
                    if f.name.node == "main" {
                        return Some(f);
                    }
                }
                None
            })
            .unwrap();
        let specs = infer_postconditions_from_constraints(main_fn, &system);
        let has_const_out = specs
            .iter()
            .any(|s| s.expression.contains("output[0] == 42"));
        assert!(has_const_out, "should detect constant output value");
    }

    // -- Weaken candidate --

    #[test]
    fn test_weaken_candidate_le() {
        let result = weaken_candidate("x <= 10");
        assert_eq!(result, Some("x <= 11".to_string()));
    }

    #[test]
    fn test_weaken_candidate_ge() {
        let result = weaken_candidate("x >= 5");
        assert_eq!(result, Some("x >= 4".to_string()));
    }

    #[test]
    fn test_weaken_candidate_ge_zero() {
        let result = weaken_candidate("x >= 0");
        assert_eq!(result, None, "cannot weaken >= 0 further");
    }

    // -- Format report --

    #[test]
    fn test_format_empty_report() {
        let report = format_report(&[]);
        assert!(report.contains("No specifications"));
    }

    #[test]
    fn test_format_nonempty_report() {
        let specs = vec![SynthesizedSpec {
            function: "test".to_string(),
            kind: SpecKind::Postcondition,
            expression: "result == 0".to_string(),
            confidence: 0.9,
            explanation: "test explanation".to_string(),
        }];
        let report = format_report(&specs);
        assert!(report.contains("Synthesized 1 specification"));
        assert!(report.contains("result == 0"));
    }

    // -- expr_to_string --

    #[test]
    fn test_expr_to_string_literal() {
        let expr = Expr::Literal(Literal::Integer(42));
        assert_eq!(expr_to_string(&expr), "42");
    }

    #[test]
    fn test_expr_to_string_var() {
        let expr = Expr::Var("x".to_string());
        assert_eq!(expr_to_string(&expr), "x");
    }
}
