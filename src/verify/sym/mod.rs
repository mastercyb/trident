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

use std::collections::BTreeMap;

use crate::ast::*;
use crate::span::Spanned;

/// The prime modulus for the Goldilocks field.
pub const GOLDILOCKS_P: u64 = 0xFFFFFFFF00000001; // 2^64 - 2^32 + 1

mod executor;
mod expr;
#[cfg(test)]
mod tests;

pub use executor::*;

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
    /// Struct field access: value.field_name.
    FieldAccess(Box<SymValue>, String),
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
                        SymValue::Const(((*x as u128 + *y as u128) % GOLDILOCKS_P as u128) as u64)
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
                    (SymValue::Const(x), SymValue::Const(y)) => SymValue::Const(
                        (((*x as u128 + GOLDILOCKS_P as u128) - *y as u128) % GOLDILOCKS_P as u128)
                            as u64,
                    ),
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
    pub variables: BTreeMap<String, u32>,
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
            variables: BTreeMap::new(),
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
