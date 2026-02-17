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

mod differential;
mod polynomial;
#[cfg(test)]
mod tests;

use differential::*;
use polynomial::*;

use std::fmt;

use crate::ast::display;
pub(crate) use crate::ast::display::format_ast_type as format_type;
use crate::ast::{self, File, FnDef, Item, Type};

use crate::hash;
use crate::sym::SymValue;

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

// ─── Helpers ───────────────────────────────────────────────────────

/// Find a function by name in a file.
pub(crate) fn find_fn<'a>(file: &'a File, name: &str) -> Option<&'a FnDef> {
    for item in &file.items {
        if let Item::Fn(func) = &item.node {
            if func.name.node == name {
                return Some(func);
            }
        }
    }
    None
}
