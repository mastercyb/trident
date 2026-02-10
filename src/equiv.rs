//! Semantic equivalence checking for Trident functions.
//!
//! Given two functions f and g with the same signature, checks whether
//! f(x) == g(x) for all inputs x. Uses:
//! 1. Content hash comparison (trivial equivalence)
//! 2. Symbolic execution + algebraic simplification
//! 3. Random testing (Schwartz-Zippel)
//! 4. Bounded model checking
//!
//! The checker builds a synthetic "differential test program" that calls
//! both functions with the same inputs and asserts their outputs are equal,
//! then runs the existing verification pipeline on that program.

use std::fmt;

use crate::ast::{self, File, FnDef, Item, Type};
use crate::hash;
use crate::sym::SymValue;
use crate::view;

// ─── Result Types ──────────────────────────────────────────────────

/// Result of an equivalence check.
#[derive(Clone, Debug)]
pub struct EquivalenceResult {
    /// The two function names being compared.
    pub fn_a: String,
    pub fn_b: String,
    /// Whether they are equivalent.
    pub verdict: EquivalenceVerdict,
    /// Counterexample (if not equivalent).
    pub counterexample: Option<EquivalenceCounterexample>,
    /// Method used to determine equivalence.
    pub method: String,
    /// Number of random tests performed.
    pub tests_passed: usize,
}

impl EquivalenceResult {
    /// Format a human-readable report.
    pub fn format_report(&self) -> String {
        let mut report = String::new();
        report.push_str(&format!(
            "Equivalence check: {} vs {}\n",
            self.fn_a, self.fn_b
        ));
        report.push_str(&format!("  Method: {}\n", self.method));
        report.push_str(&format!("  Verdict: {}\n", self.verdict));
        if self.tests_passed > 0 {
            report.push_str(&format!("  Tests passed: {}\n", self.tests_passed));
        }
        if let Some(ref ce) = self.counterexample {
            report.push_str("  Counterexample:\n");
            for (name, value) in &ce.inputs {
                report.push_str(&format!("    {} = {}\n", name, value));
            }
            report.push_str(&format!("    {}(...) = {}\n", self.fn_a, ce.output_a));
            report.push_str(&format!("    {}(...) = {}\n", self.fn_b, ce.output_b));
        }
        report
    }
}

/// Verdict of an equivalence check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EquivalenceVerdict {
    /// Functions are equivalent (proven or high confidence).
    Equivalent,
    /// Functions are NOT equivalent (counterexample found).
    NotEquivalent,
    /// Could not determine (inconclusive).
    Unknown,
}

impl fmt::Display for EquivalenceVerdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EquivalenceVerdict::Equivalent => write!(f, "EQUIVALENT"),
            EquivalenceVerdict::NotEquivalent => write!(f, "NOT EQUIVALENT"),
            EquivalenceVerdict::Unknown => write!(f, "UNKNOWN"),
        }
    }
}

/// A counterexample showing the two functions produce different outputs.
#[derive(Clone, Debug)]
pub struct EquivalenceCounterexample {
    /// Input values that produce different outputs.
    pub inputs: Vec<(String, u64)>,
    /// Output of function A.
    pub output_a: u64,
    /// Output of function B.
    pub output_b: u64,
}

// ─── Main Entry Point ──────────────────────────────────────────────

/// Check if two functions in a file are semantically equivalent.
///
/// Runs a series of checks in order:
/// 1. Signature compatibility
/// 2. Content hash comparison (trivial alpha-equivalence)
/// 3. Polynomial normalization (for pure field arithmetic)
/// 4. Differential testing via the verification pipeline
pub fn check_equivalence(file: &File, fn_a: &str, fn_b: &str) -> EquivalenceResult {
    // Find both functions in the file.
    let func_a = find_fn(file, fn_a);
    let func_b = find_fn(file, fn_b);

    let (func_a, func_b) = match (func_a, func_b) {
        (Some(a), Some(b)) => (a, b),
        (None, _) => {
            return EquivalenceResult {
                fn_a: fn_a.to_string(),
                fn_b: fn_b.to_string(),
                verdict: EquivalenceVerdict::Unknown,
                counterexample: None,
                method: format!("error: function '{}' not found", fn_a),
                tests_passed: 0,
            };
        }
        (_, None) => {
            return EquivalenceResult {
                fn_a: fn_a.to_string(),
                fn_b: fn_b.to_string(),
                verdict: EquivalenceVerdict::Unknown,
                counterexample: None,
                method: format!("error: function '{}' not found", fn_b),
                tests_passed: 0,
            };
        }
    };

    // Check signature compatibility.
    if let Err(msg) = check_signatures(func_a, func_b) {
        return EquivalenceResult {
            fn_a: fn_a.to_string(),
            fn_b: fn_b.to_string(),
            verdict: EquivalenceVerdict::Unknown,
            counterexample: None,
            method: format!("error: {}", msg),
            tests_passed: 0,
        };
    }

    // Step 1: Hash comparison (alpha-equivalence).
    if let Some(result) = check_hash_equivalence(file, fn_a, fn_b) {
        return result;
    }

    // Step 2: Polynomial normalization (for pure field arithmetic).
    if let Some(result) = check_polynomial_equivalence(file, fn_a, fn_b) {
        return result;
    }

    // Step 3: Differential testing via the verification pipeline.
    check_differential(file, fn_a, fn_b)
}

// ─── Signature Checking ────────────────────────────────────────────

/// Verify that two functions have compatible signatures.
fn check_signatures(a: &FnDef, b: &FnDef) -> Result<(), String> {
    if a.params.len() != b.params.len() {
        return Err(format!(
            "parameter count mismatch: {} has {} params, {} has {}",
            a.name.node,
            a.params.len(),
            b.name.node,
            b.params.len()
        ));
    }

    for (i, (pa, pb)) in a.params.iter().zip(b.params.iter()).enumerate() {
        if pa.ty.node != pb.ty.node {
            return Err(format!(
                "parameter {} type mismatch: {} has {}, {} has {}",
                i,
                a.name.node,
                format_type(&pa.ty.node),
                b.name.node,
                format_type(&pb.ty.node),
            ));
        }
    }

    let ret_a = a.return_ty.as_ref().map(|t| &t.node);
    let ret_b = b.return_ty.as_ref().map(|t| &t.node);
    if ret_a != ret_b {
        return Err(format!(
            "return type mismatch: {} returns {}, {} returns {}",
            a.name.node,
            ret_a
                .map(|t| format_type(t))
                .unwrap_or_else(|| "()".to_string()),
            b.name.node,
            ret_b
                .map(|t| format_type(t))
                .unwrap_or_else(|| "()".to_string()),
        ));
    }

    Ok(())
}

// ─── Step 1: Hash Equivalence ──────────────────────────────────────

/// Check equivalence using content hashes (trivial check).
///
/// The hash module normalizes variable names via de Bruijn indices,
/// so functions that differ only in variable naming will hash the same.
fn check_hash_equivalence(file: &File, fn_a: &str, fn_b: &str) -> Option<EquivalenceResult> {
    let fn_hashes = hash::hash_file(file);

    let hash_a = fn_hashes.get(fn_a)?;
    let hash_b = fn_hashes.get(fn_b)?;

    if hash_a == hash_b {
        Some(EquivalenceResult {
            fn_a: fn_a.to_string(),
            fn_b: fn_b.to_string(),
            verdict: EquivalenceVerdict::Equivalent,
            counterexample: None,
            method: "content hash (alpha-equivalence)".to_string(),
            tests_passed: 0,
        })
    } else {
        None // Hashes differ — doesn't mean non-equivalent, just not trivially equal.
    }
}

// ─── Step 2: Polynomial Normalization ──────────────────────────────

/// A polynomial term: (coefficient, sorted list of variable names).
///
/// For example, `3 * x * y` is represented as `(3, ["x", "y"])`.
/// The constant term `5` is `(5, [])`.
type PolyTerm = (u64, Vec<String>);

/// Normalize a symbolic value into a multivariate polynomial over the
/// Goldilocks field. Returns `None` if the expression contains
/// non-polynomial operations (hash, divine, ITE, etc.).
fn normalize_polynomial(val: &SymValue) -> Option<Vec<PolyTerm>> {
    match val {
        SymValue::Const(c) => {
            if *c == 0 {
                Some(Vec::new())
            } else {
                Some(vec![(*c, Vec::new())])
            }
        }
        SymValue::Var(var) => Some(vec![(1, vec![var.to_string()])]),
        SymValue::Add(a, b) => {
            let mut pa = normalize_polynomial(a)?;
            let pb = normalize_polynomial(b)?;
            pa.extend(pb);
            Some(canonicalize_poly(pa))
        }
        SymValue::Sub(a, b) => {
            let pa = normalize_polynomial(a)?;
            let pb = normalize_polynomial(b)?;
            let neg_pb: Vec<PolyTerm> = pb
                .into_iter()
                .map(|(c, vars)| (field_neg(c), vars))
                .collect();
            let mut combined = pa;
            combined.extend(neg_pb);
            Some(canonicalize_poly(combined))
        }
        SymValue::Neg(a) => {
            let pa = normalize_polynomial(a)?;
            let negated: Vec<PolyTerm> = pa
                .into_iter()
                .map(|(c, vars)| (field_neg(c), vars))
                .collect();
            Some(canonicalize_poly(negated))
        }
        SymValue::Mul(a, b) => {
            let pa = normalize_polynomial(a)?;
            let pb = normalize_polynomial(b)?;
            let mut product = Vec::new();
            for (ca, va) in &pa {
                for (cb, vb) in &pb {
                    let coeff = field_mul(*ca, *cb);
                    if coeff != 0 {
                        let mut vars = va.clone();
                        vars.extend(vb.iter().cloned());
                        vars.sort();
                        product.push((coeff, vars));
                    }
                }
            }
            Some(canonicalize_poly(product))
        }
        // Non-polynomial operations: give up.
        SymValue::Inv(_)
        | SymValue::Eq(_, _)
        | SymValue::Lt(_, _)
        | SymValue::Hash(_, _)
        | SymValue::Divine(_)
        | SymValue::PubInput(_)
        | SymValue::Ite(_, _, _) => None,
    }
}

/// Canonicalize a polynomial: sort terms, combine like terms, remove zeros.
fn canonicalize_poly(mut terms: Vec<PolyTerm>) -> Vec<PolyTerm> {
    // Sort each term's variables, then sort terms lexicographically.
    for (_, vars) in &mut terms {
        vars.sort();
    }
    terms.sort_by(|a, b| a.1.cmp(&b.1));

    // Combine like terms.
    let mut canonical: Vec<PolyTerm> = Vec::new();
    for (coeff, vars) in terms {
        if let Some(last) = canonical.last_mut() {
            if last.1 == vars {
                last.0 = field_add(last.0, coeff);
                continue;
            }
        }
        canonical.push((coeff, vars));
    }

    // Remove zero-coefficient terms.
    canonical.retain(|(c, _)| *c != 0);

    canonical
}

/// Check equivalence via polynomial normalization.
///
/// This works for pure field-arithmetic functions (using only +, *, -,
/// constants, and variables). Builds symbolic values for both functions
/// and checks if their polynomial normal forms match.
fn check_polynomial_equivalence(file: &File, fn_a: &str, fn_b: &str) -> Option<EquivalenceResult> {
    let func_a = find_fn(file, fn_a)?;
    let func_b = find_fn(file, fn_b)?;

    // Both must have a return type (pure functions producing a value).
    if func_a.return_ty.is_none() || func_b.return_ty.is_none() {
        return None;
    }

    // Build symbolic values for the return expression of each function
    // by symbolically executing them with shared parameter names.
    let sym_a = symbolic_eval_fn(func_a)?;
    let sym_b = symbolic_eval_fn_with_params(func_b, &param_names(func_a))?;

    let poly_a = normalize_polynomial(&sym_a)?;
    let poly_b = normalize_polynomial(&sym_b)?;

    if poly_a == poly_b {
        Some(EquivalenceResult {
            fn_a: fn_a.to_string(),
            fn_b: fn_b.to_string(),
            verdict: EquivalenceVerdict::Equivalent,
            counterexample: None,
            method: "polynomial normalization".to_string(),
            tests_passed: 0,
        })
    } else {
        // Polynomials differ — but this is conclusive for polynomial functions.
        None // Fall through to differential testing for certainty.
    }
}

/// Extract parameter names from a function definition.
fn param_names(func: &FnDef) -> Vec<String> {
    func.params.iter().map(|p| p.name.node.clone()).collect()
}

/// Symbolically evaluate a function's body to get its return value.
///
/// Only works for simple "expression-body" functions (single tail expression
/// or a body that reduces to a single expression).
fn symbolic_eval_fn(func: &FnDef) -> Option<SymValue> {
    let params = param_names(func);
    symbolic_eval_fn_with_params(func, &params)
}

/// Symbolically evaluate a function's body using specific parameter names.
fn symbolic_eval_fn_with_params(func: &FnDef, params: &[String]) -> Option<SymValue> {
    let body = func.body.as_ref()?;

    // Build an environment mapping parameter names to symbolic variables.
    let mut env = std::collections::HashMap::new();
    for (i, param) in func.params.iter().enumerate() {
        let sym_name = if i < params.len() {
            params[i].clone()
        } else {
            param.name.node.clone()
        };
        env.insert(
            param.name.node.clone(),
            SymValue::Var(crate::sym::SymVar {
                name: sym_name,
                version: 0,
            }),
        );
    }

    // Evaluate let bindings in the body, then the tail expression.
    for stmt in &body.node.stmts {
        match &stmt.node {
            ast::Stmt::Let { pattern, init, .. } => {
                let val = eval_expr_simple(&init.node, &env)?;
                match pattern {
                    ast::Pattern::Name(name) => {
                        env.insert(name.node.clone(), val);
                    }
                    ast::Pattern::Tuple(_) => return None, // Too complex.
                }
            }
            ast::Stmt::Return(Some(expr)) => {
                return eval_expr_simple(&expr.node, &env);
            }
            _ => return None, // Statements other than let/return are too complex.
        }
    }

    // Evaluate the tail expression.
    let tail = body.node.tail_expr.as_ref()?;
    eval_expr_simple(&tail.node, &env)
}

/// Simple expression evaluator that produces SymValue.
///
/// Only handles arithmetic expressions (+, *, literals, variables).
/// Returns None for anything more complex.
fn eval_expr_simple(
    expr: &ast::Expr,
    env: &std::collections::HashMap<String, SymValue>,
) -> Option<SymValue> {
    match expr {
        ast::Expr::Literal(ast::Literal::Integer(n)) => Some(SymValue::Const(*n)),
        ast::Expr::Literal(ast::Literal::Bool(b)) => Some(SymValue::Const(if *b { 1 } else { 0 })),
        ast::Expr::Var(name) => env.get(name).cloned(),
        ast::Expr::BinOp { op, lhs, rhs } => {
            let l = eval_expr_simple(&lhs.node, env)?;
            let r = eval_expr_simple(&rhs.node, env)?;
            match op {
                ast::BinOp::Add => Some(SymValue::Add(Box::new(l), Box::new(r)).simplify()),
                ast::BinOp::Mul => Some(SymValue::Mul(Box::new(l), Box::new(r)).simplify()),
                _ => None, // Only + and * are polynomial.
            }
        }
        ast::Expr::Call { path, args, .. } => {
            let func_name = path.node.0.last().map(|s| s.as_str()).unwrap_or("");
            match func_name {
                "sub" if args.len() == 2 => {
                    let a = eval_expr_simple(&args[0].node, env)?;
                    let b = eval_expr_simple(&args[1].node, env)?;
                    Some(SymValue::Sub(Box::new(a), Box::new(b)).simplify())
                }
                "neg" if args.len() == 1 => {
                    let a = eval_expr_simple(&args[0].node, env)?;
                    Some(SymValue::Neg(Box::new(a)).simplify())
                }
                _ => None,
            }
        }
        _ => None,
    }
}

// ─── Step 3: Differential Testing ──────────────────────────────────

/// Check equivalence by building a differential test program and running
/// the verification pipeline on it.
fn check_differential(file: &File, fn_a: &str, fn_b: &str) -> EquivalenceResult {
    let program_source = match build_differential_program(file, fn_a, fn_b) {
        Some(src) => src,
        None => {
            return EquivalenceResult {
                fn_a: fn_a.to_string(),
                fn_b: fn_b.to_string(),
                verdict: EquivalenceVerdict::Unknown,
                counterexample: None,
                method: "error: could not build differential program".to_string(),
                tests_passed: 0,
            };
        }
    };

    // Parse the synthetic program.
    let parsed = match crate::parse_source_silent(&program_source, "<equiv>") {
        Ok(f) => f,
        Err(_) => {
            return EquivalenceResult {
                fn_a: fn_a.to_string(),
                fn_b: fn_b.to_string(),
                verdict: EquivalenceVerdict::Unknown,
                counterexample: None,
                method: "error: differential program failed to parse".to_string(),
                tests_passed: 0,
            };
        }
    };

    // Type-check the synthetic program.
    if let Err(_) = crate::typeck::TypeChecker::new().check_file(&parsed) {
        return EquivalenceResult {
            fn_a: fn_a.to_string(),
            fn_b: fn_b.to_string(),
            verdict: EquivalenceVerdict::Unknown,
            counterexample: None,
            method: "error: differential program failed type-check".to_string(),
            tests_passed: 0,
        };
    }

    // Symbolically analyze and verify.
    let system = crate::sym::analyze(&parsed);
    let report = crate::solve::verify(&system);

    let total_tests = report.random_result.rounds + report.bmc_result.rounds;

    if report.is_safe() {
        EquivalenceResult {
            fn_a: fn_a.to_string(),
            fn_b: fn_b.to_string(),
            verdict: EquivalenceVerdict::Equivalent,
            counterexample: None,
            method: "differential testing (random + BMC)".to_string(),
            tests_passed: total_tests,
        }
    } else {
        // Extract counterexample from the verification report.
        let counterexample = extract_counterexample(&report, fn_a, fn_b);

        EquivalenceResult {
            fn_a: fn_a.to_string(),
            fn_b: fn_b.to_string(),
            verdict: EquivalenceVerdict::NotEquivalent,
            counterexample,
            method: "differential testing (counterexample found)".to_string(),
            tests_passed: total_tests,
        }
    }
}

/// Build a synthetic differential test program.
///
/// The program:
/// 1. Includes both function definitions
/// 2. Generates a main() that reads shared inputs, calls both, asserts equality
fn build_differential_program(file: &File, fn_a: &str, fn_b: &str) -> Option<String> {
    let func_a = find_fn(file, fn_a)?;
    let func_b = find_fn(file, fn_b)?;

    // Get formatted source for each function.
    let src_a = view::format_function(func_a);
    let src_b = view::format_function(func_b);

    // Build input reads and argument lists based on func_a's parameters.
    let mut reads = String::new();
    let mut args = Vec::new();
    for (i, param) in func_a.params.iter().enumerate() {
        let var_name = format!("__input_{}", i);
        let ty_str = format_type(&param.ty.node);
        // For most types, use pub_read().
        // For Digest, use pub_read5(). For XField, three reads.
        let read_call = match &param.ty.node {
            Type::Digest => "pub_read5()",
            _ => "pub_read()",
        };
        reads.push_str(&format!(
            "    let {}: {} = {}\n",
            var_name, ty_str, read_call
        ));
        args.push(var_name);
    }

    let args_str = args.join(", ");

    // Build the main function.
    let has_return = func_a.return_ty.is_some();
    let main_body = if has_return {
        format!(
            "{}\
    let __result_a: Field = {}({})\n\
    let __result_b: Field = {}({})\n\
    assert_eq(__result_a, __result_b)\n",
            reads, fn_a, args_str, fn_b, args_str
        )
    } else {
        // Void functions: just call both (checks only side effects/assertions).
        format!(
            "{}\
    {}({})\n\
    {}({})\n",
            reads, fn_a, args_str, fn_b, args_str
        )
    };

    // Assemble the program.
    let mut program = String::new();
    program.push_str("program __equiv_test\n\n");
    program.push_str(&src_a);
    program.push('\n');
    program.push_str(&src_b);
    program.push('\n');
    program.push_str("fn main() {\n");
    program.push_str(&main_body);
    program.push_str("}\n");

    Some(program)
}

/// Extract a counterexample from the verification report.
fn extract_counterexample(
    report: &crate::solve::VerificationReport,
    _fn_a: &str,
    _fn_b: &str,
) -> Option<EquivalenceCounterexample> {
    // Look for a counterexample in random results first, then BMC.
    let ce = report
        .random_result
        .counterexamples
        .first()
        .or_else(|| report.bmc_result.counterexamples.first())?;

    let mut inputs = Vec::new();
    let mut sorted_assignments: Vec<_> = ce.assignments.iter().collect();
    sorted_assignments.sort_by_key(|(k, _)| (*k).clone());

    for (name, value) in &sorted_assignments {
        if name.starts_with("pub_in_") || name.starts_with("__input_") {
            inputs.push((name.to_string(), **value));
        }
    }

    // Try to extract the output values for each function.
    let output_a = sorted_assignments
        .iter()
        .find(|(k, _)| k.contains("result_a") || k.contains("__call_"))
        .map(|(_, v)| **v)
        .unwrap_or(0);
    let output_b = sorted_assignments
        .iter()
        .find(|(k, _)| k.contains("result_b"))
        .map(|(_, v)| **v)
        .unwrap_or(0);

    Some(EquivalenceCounterexample {
        inputs,
        output_a,
        output_b,
    })
}

// ─── Helpers ───────────────────────────────────────────────────────

/// Find a function by name in a file.
fn find_fn<'a>(file: &'a File, name: &str) -> Option<&'a FnDef> {
    for item in &file.items {
        if let Item::Fn(func) = &item.node {
            if func.name.node == name {
                return Some(func);
            }
        }
    }
    None
}

/// Format a Type for source-code output.
fn format_type(ty: &Type) -> String {
    match ty {
        Type::Field => "Field".to_string(),
        Type::XField => "XField".to_string(),
        Type::Bool => "Bool".to_string(),
        Type::U32 => "U32".to_string(),
        Type::Digest => "Digest".to_string(),
        Type::Array(inner, size) => format!("[{}; {}]", format_type(inner), size),
        Type::Tuple(elems) => {
            let parts: Vec<_> = elems.iter().map(|t| format_type(t)).collect();
            format!("({})", parts.join(", "))
        }
        Type::Named(path) => path.as_dotted(),
    }
}

/// Goldilocks field addition (mod p).
fn field_add(a: u64, b: u64) -> u64 {
    const P: u128 = crate::sym::GOLDILOCKS_P as u128;
    ((a as u128 + b as u128) % P) as u64
}

/// Goldilocks field multiplication (mod p).
fn field_mul(a: u64, b: u64) -> u64 {
    const P: u128 = crate::sym::GOLDILOCKS_P as u128;
    ((a as u128 * b as u128) % P) as u64
}

/// Goldilocks field negation (mod p).
fn field_neg(a: u64) -> u64 {
    if a == 0 {
        0
    } else {
        crate::sym::GOLDILOCKS_P - a
    }
}

// ─── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str) -> File {
        crate::parse_source_silent(source, "test.tri").unwrap()
    }

    // --- Trivially equivalent (same body, different names) ---

    #[test]
    fn test_trivially_equivalent() {
        let source = r#"program test

fn add_v1(a: Field, b: Field) -> Field {
    a + b
}

fn add_v2(a: Field, b: Field) -> Field {
    a + b
}

fn main() { }
"#;
        let file = parse(source);
        let result = check_equivalence(&file, "add_v1", "add_v2");
        assert_eq!(result.verdict, EquivalenceVerdict::Equivalent);
        assert!(
            result.method.contains("hash") || result.method.contains("polynomial"),
            "should use fast path; got: {}",
            result.method
        );
    }

    // --- Alpha-equivalent (different variable names) ---

    #[test]
    fn test_alpha_equivalent() {
        let source = r#"program test

fn add_xy(x: Field, y: Field) -> Field {
    x + y
}

fn add_ab(a: Field, b: Field) -> Field {
    a + b
}

fn main() { }
"#;
        let file = parse(source);
        let result = check_equivalence(&file, "add_xy", "add_ab");
        assert_eq!(result.verdict, EquivalenceVerdict::Equivalent);
        assert!(
            result.method.contains("hash"),
            "alpha-equivalent functions should match by hash; got: {}",
            result.method
        );
    }

    // --- Equivalent with different computation paths (commutativity) ---

    #[test]
    fn test_commutative_equivalent() {
        let source = r#"program test

fn f(x: Field, y: Field) -> Field {
    x + y
}

fn g(x: Field, y: Field) -> Field {
    y + x
}

fn main() { }
"#;
        let file = parse(source);
        let result = check_equivalence(&file, "f", "g");
        assert_eq!(result.verdict, EquivalenceVerdict::Equivalent);
    }

    // --- Non-equivalent functions (counterexample expected) ---

    #[test]
    fn test_not_equivalent() {
        let source = r#"program test

fn f(x: Field, y: Field) -> Field {
    x + y
}

fn g(x: Field, y: Field) -> Field {
    x * y
}

fn main() { }
"#;
        let file = parse(source);
        let result = check_equivalence(&file, "f", "g");
        assert_eq!(result.verdict, EquivalenceVerdict::NotEquivalent);
    }

    // --- Signature mismatch ---

    #[test]
    fn test_signature_mismatch() {
        let source = r#"program test

fn f(x: Field) -> Field {
    x
}

fn g(x: Field, y: Field) -> Field {
    x + y
}

fn main() { }
"#;
        let file = parse(source);
        let result = check_equivalence(&file, "f", "g");
        assert_eq!(result.verdict, EquivalenceVerdict::Unknown);
        assert!(result.method.contains("mismatch"));
    }

    // --- Function not found ---

    #[test]
    fn test_function_not_found() {
        let source = r#"program test

fn f(x: Field) -> Field {
    x
}

fn main() { }
"#;
        let file = parse(source);
        let result = check_equivalence(&file, "f", "nonexistent");
        assert_eq!(result.verdict, EquivalenceVerdict::Unknown);
        assert!(result.method.contains("not found"));
    }

    // --- Polynomial normalization ---

    #[test]
    fn test_polynomial_normalization_const() {
        let val = SymValue::Const(42);
        let poly = normalize_polynomial(&val).unwrap();
        assert_eq!(poly, vec![(42, vec![])]);
    }

    #[test]
    fn test_polynomial_normalization_var() {
        let val = SymValue::Var(crate::sym::SymVar {
            name: "x".to_string(),
            version: 0,
        });
        let poly = normalize_polynomial(&val).unwrap();
        assert_eq!(poly, vec![(1, vec!["x".to_string()])]);
    }

    #[test]
    fn test_polynomial_normalization_add() {
        // x + y
        let x = SymValue::Var(crate::sym::SymVar {
            name: "x".to_string(),
            version: 0,
        });
        let y = SymValue::Var(crate::sym::SymVar {
            name: "y".to_string(),
            version: 0,
        });
        let val = SymValue::Add(Box::new(x), Box::new(y));
        let poly = normalize_polynomial(&val).unwrap();
        assert_eq!(
            poly,
            vec![(1, vec!["x".to_string()]), (1, vec!["y".to_string()]),]
        );
    }

    #[test]
    fn test_polynomial_normalization_mul() {
        // x * y
        let x = SymValue::Var(crate::sym::SymVar {
            name: "x".to_string(),
            version: 0,
        });
        let y = SymValue::Var(crate::sym::SymVar {
            name: "y".to_string(),
            version: 0,
        });
        let val = SymValue::Mul(Box::new(x), Box::new(y));
        let poly = normalize_polynomial(&val).unwrap();
        assert_eq!(poly, vec![(1, vec!["x".to_string(), "y".to_string()])]);
    }

    #[test]
    fn test_polynomial_commutativity() {
        // x + y vs y + x should produce the same polynomial
        let x = SymValue::Var(crate::sym::SymVar {
            name: "x".to_string(),
            version: 0,
        });
        let y = SymValue::Var(crate::sym::SymVar {
            name: "y".to_string(),
            version: 0,
        });
        let sum1 = SymValue::Add(Box::new(x.clone()), Box::new(y.clone()));
        let sum2 = SymValue::Add(Box::new(y), Box::new(x));
        let poly1 = normalize_polynomial(&sum1).unwrap();
        let poly2 = normalize_polynomial(&sum2).unwrap();
        assert_eq!(poly1, poly2);
    }

    #[test]
    fn test_polynomial_normalization_complex() {
        // (x + y) * x  =  x*x + x*y
        let x = SymValue::Var(crate::sym::SymVar {
            name: "x".to_string(),
            version: 0,
        });
        let y = SymValue::Var(crate::sym::SymVar {
            name: "y".to_string(),
            version: 0,
        });
        let sum = SymValue::Add(Box::new(x.clone()), Box::new(y.clone()));
        let prod = SymValue::Mul(Box::new(sum), Box::new(x));
        let poly = normalize_polynomial(&prod).unwrap();
        assert_eq!(
            poly,
            vec![
                (1, vec!["x".to_string(), "x".to_string()]),
                (1, vec!["x".to_string(), "y".to_string()]),
            ]
        );
    }

    // --- Equivalent with rearranged arithmetic ---

    #[test]
    fn test_equivalent_rearranged() {
        let source = r#"program test

fn f(x: Field, y: Field) -> Field {
    x + y + x
}

fn g(x: Field, y: Field) -> Field {
    x * 2 + y
}

fn main() { }
"#;
        let file = parse(source);
        let result = check_equivalence(&file, "f", "g");
        // Both reduce to 2*x + y, so polynomial normalization catches it.
        assert_eq!(
            result.verdict,
            EquivalenceVerdict::Equivalent,
            "report: {}",
            result.format_report()
        );
    }

    // --- Differential testing catches non-equivalence ---

    #[test]
    fn test_differential_not_equivalent() {
        let source = r#"program test

fn f(x: Field) -> Field {
    x + 1
}

fn g(x: Field) -> Field {
    x + 2
}

fn main() { }
"#;
        let file = parse(source);
        let result = check_equivalence(&file, "f", "g");
        assert_eq!(result.verdict, EquivalenceVerdict::NotEquivalent);
    }

    // --- Void functions ---

    #[test]
    fn test_void_functions_equivalent() {
        let source = r#"program test

fn f(x: Field) {
    assert(true)
}

fn g(x: Field) {
    assert(true)
}

fn main() { }
"#;
        let file = parse(source);
        let result = check_equivalence(&file, "f", "g");
        // Void functions with same assertions should be equivalent.
        assert_eq!(result.verdict, EquivalenceVerdict::Equivalent);
    }
}
