use super::*;
use crate::field::{Goldilocks, PrimeField};

// ─── Step 2: Polynomial Normalization ──────────────────────────────

/// A polynomial term: (coefficient, sorted list of variable names).
///
/// For example, `3 * x * y` is represented as `(3, ["x", "y"])`.
/// The constant term `5` is `(5, [])`.
type PolyTerm = (u64, Vec<String>);

/// Normalize a symbolic value into a multivariate polynomial over the
/// Goldilocks field. Returns `None` if the expression contains
/// non-polynomial operations (hash, divine, ITE, etc.).
pub(crate) fn normalize_polynomial(val: &SymValue) -> Option<Vec<PolyTerm>> {
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
                .map(|(c, vars)| (Goldilocks::from_u64(c).neg().to_u64(), vars))
                .collect();
            let mut combined = pa;
            combined.extend(neg_pb);
            Some(canonicalize_poly(combined))
        }
        SymValue::Neg(a) => {
            let pa = normalize_polynomial(a)?;
            let negated: Vec<PolyTerm> = pa
                .into_iter()
                .map(|(c, vars)| (Goldilocks::from_u64(c).neg().to_u64(), vars))
                .collect();
            Some(canonicalize_poly(negated))
        }
        SymValue::Mul(a, b) => {
            let pa = normalize_polynomial(a)?;
            let pb = normalize_polynomial(b)?;
            let mut product = Vec::new();
            for (ca, va) in &pa {
                for (cb, vb) in &pb {
                    let coeff = Goldilocks::from_u64(*ca)
                        .mul(Goldilocks::from_u64(*cb))
                        .to_u64();
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
        | SymValue::Ite(_, _, _)
        | SymValue::FieldAccess(_, _) => None,
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
                last.0 = Goldilocks::from_u64(last.0)
                    .add(Goldilocks::from_u64(coeff))
                    .to_u64();
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
pub(super) fn check_polynomial_equivalence(
    file: &File,
    fn_a: &str,
    fn_b: &str,
) -> Option<EquivalenceResult> {
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
    let mut env = std::collections::BTreeMap::new();
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
    env: &std::collections::BTreeMap<String, SymValue>,
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
