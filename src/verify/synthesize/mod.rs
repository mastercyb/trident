//! Automatic invariant synthesis for Trident programs.
//!
//! Techniques:
//! 1. Template-based synthesis: match common patterns (accumulation, counting,
//!    monotonic updates) and instantiate invariant templates.
//! 2. Counterexample-guided inductive synthesis (CEGIS): propose candidate
//!    invariants, verify with solver, refine using counterexamples.
//! 3. Specification inference: suggest postconditions from code analysis.

mod templates;
mod infer;
#[cfg(test)]
mod tests;

pub(crate) use templates::{match_templates, check_identity_preservation, check_range_preservation, check_constant_result};
pub(crate) use infer::{infer_preconditions, infer_postconditions_from_body, cegis_refine, infer_postconditions_from_constraints, verify_candidate, weaken_candidate};


pub(crate) use crate::ast::*;
pub(crate) use crate::solve;
pub(crate) use crate::sym::{self, ConstraintSystem, SymValue};
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

// ─── AST Utility Helpers ───────────────────────────────────────────

/// Collect mutable variable initializations from a block.
/// Returns `(variable_name, init_expression)` pairs.
pub(crate) fn collect_mut_inits(block: &Block) -> Vec<(String, Expr)> {
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
pub(crate) fn place_is_var(place: &Place, name: &str) -> bool {
    matches!(place, Place::Var(n) if n == name)
}

/// Check if an expression is a simple variable reference to the given name.
pub(crate) fn expr_is_var(expr: &Expr, name: &str) -> bool {
    matches!(expr, Expr::Var(n) if n == name)
}

/// Convert an expression to a human-readable string (best effort).
pub(crate) fn expr_to_string(expr: &Expr) -> String {
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

