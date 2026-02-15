//! Algebraic solver and bounded model checker for Trident constraint systems.
//!
//! Takes the `ConstraintSystem` from `sym.rs` and checks it using:
//!
//! 1. **Schwartz-Zippel testing**: Evaluate polynomial constraints at random
//!    field points. If a polynomial identity holds at k random points over F_p,
//!    the probability it's false is ≤ d/p where d is the degree. For our
//!    degrees (< 2^16) and Goldilocks p (≈ 2^64), this is negligibly small.
//!
//! 2. **Bounded model checking**: Enumerate concrete variable assignments
//!    and check all constraints. For programs with few free variables (< 20),
//!    we can check a large sample. For programs with many variables, we use
//!    random sampling with the Schwartz-Zippel guarantee.
//!
//! 3. **Counterexample generation**: When a constraint fails, report the
//!    concrete variable assignment that violates it.
//!
//! 4. **Redundant assertion detection**: Identify constraints that hold for
//!    all tested inputs (candidate tautologies) — these can be eliminated
//!    to reduce proving cost.

use std::collections::HashMap;

use crate::sym::{Constraint, ConstraintSystem, SymValue, GOLDILOCKS_P};

mod eval;
mod solver;
#[cfg(test)]
mod tests;

pub(crate) use eval::*;
pub use solver::*;

// ─── Solver Results ────────────────────────────────────────────────

/// A concrete counterexample showing a constraint violation.
#[derive(Clone, Debug)]
pub struct Counterexample {
    /// The constraint index that was violated.
    pub constraint_index: usize,
    /// Human-readable description of the constraint.
    pub constraint_desc: String,
    /// The variable assignments that caused the violation.
    pub assignments: HashMap<String, u64>,
}

impl Counterexample {
    pub fn format(&self) -> String {
        let mut s = format!(
            "  Constraint #{}: {}\n",
            self.constraint_index, self.constraint_desc
        );
        s.push_str("  Counterexample:\n");
        let mut vars: Vec<_> = self.assignments.iter().collect();
        vars.sort_by_key(|(k, _)| (*k).clone());
        for (name, value) in &vars {
            // Only show user-visible variables (not internal __*)
            if !name.starts_with("__") {
                s.push_str(&format!("    {} = {}\n", name, value));
            }
        }
        s
    }
}

/// Result of solving/checking a constraint system.
#[derive(Clone, Debug)]
pub struct SolverResult {
    /// Number of constraints checked.
    pub constraints_checked: usize,
    /// Number of test rounds performed.
    pub rounds: usize,
    /// Counterexamples found (violated constraints).
    pub counterexamples: Vec<Counterexample>,
    /// Indices of constraints that were satisfied in all rounds (candidate tautologies).
    pub always_satisfied: Vec<usize>,
    /// Indices of constraints that could not be evaluated (insufficient info).
    pub unevaluable: Vec<usize>,
    /// Whether all evaluable constraints passed in all rounds.
    pub all_passed: bool,
}

impl SolverResult {
    pub fn format_report(&self) -> String {
        let mut report = String::new();
        report.push_str(&format!(
            "Solver: {} constraints, {} rounds\n",
            self.constraints_checked, self.rounds
        ));

        if self.counterexamples.is_empty() {
            report.push_str("  Result: ALL PASSED\n");
        } else {
            report.push_str(&format!(
                "  Result: {} VIOLATION(S) FOUND\n",
                self.counterexamples.len()
            ));
            for ce in &self.counterexamples {
                report.push_str(&ce.format());
            }
        }

        if !self.always_satisfied.is_empty() {
            report.push_str(&format!(
                "  Redundant assertions (always true): {}\n",
                self.always_satisfied.len()
            ));
        }

        if !self.unevaluable.is_empty() {
            report.push_str(&format!(
                "  Unevaluable constraints: {}\n",
                self.unevaluable.len()
            ));
        }

        report
    }
}

// ─── Combined Verification ─────────────────────────────────────────

/// Full verification result combining static analysis, random testing, and BMC.
#[derive(Clone, Debug)]
pub struct VerificationReport {
    /// Static analysis: trivially violated constraints.
    pub static_violations: Vec<String>,
    /// Random testing (Schwartz-Zippel) result.
    pub random_result: SolverResult,
    /// Bounded model checking result.
    pub bmc_result: SolverResult,
    /// Redundant assertions that could be eliminated.
    pub redundant_assertions: Vec<usize>,
    /// Overall verdict.
    pub verdict: Verdict,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// All checks passed — no violations found.
    Safe,
    /// Static analysis found definite violations.
    StaticViolation,
    /// Random testing found violations (high confidence).
    RandomViolation,
    /// BMC found violations (definite for tested values).
    BmcViolation,
}

impl VerificationReport {
    pub fn is_safe(&self) -> bool {
        self.verdict == Verdict::Safe
    }

    pub fn format_report(&self) -> String {
        let mut report = String::new();
        report.push_str("═══ Verification Report ═══\n\n");

        // Static analysis
        if self.static_violations.is_empty() {
            report.push_str("Static analysis: PASS (no trivially violated assertions)\n");
        } else {
            report.push_str(&format!(
                "Static analysis: FAIL ({} trivially violated assertion(s))\n",
                self.static_violations.len()
            ));
            for v in &self.static_violations {
                report.push_str(&format!("  - {}\n", v));
            }
        }
        report.push('\n');

        // Random testing
        report.push_str("Random testing (Schwartz-Zippel):\n");
        report.push_str(&self.random_result.format_report());
        report.push('\n');

        // BMC
        report.push_str("Bounded model checking:\n");
        report.push_str(&self.bmc_result.format_report());
        report.push('\n');

        // Redundant assertions
        if !self.redundant_assertions.is_empty() {
            report.push_str(&format!(
                "Optimization: {} assertion(s) appear redundant (always true)\n",
                self.redundant_assertions.len()
            ));
            report.push_str("  These could be removed to reduce proving cost.\n");
        }
        report.push('\n');

        // Verdict
        let verdict_str = match &self.verdict {
            Verdict::Safe => "SAFE — no violations found",
            Verdict::StaticViolation => "UNSAFE — static analysis found definite violations",
            Verdict::RandomViolation => {
                "UNSAFE — random testing found violations (high confidence)"
            }
            Verdict::BmcViolation => "UNSAFE — bounded model checking found violations",
        };
        report.push_str(&format!("Verdict: {}\n", verdict_str));

        report
    }
}

/// Run full verification: static + random + BMC.
pub fn verify(system: &ConstraintSystem) -> VerificationReport {
    // 1. Static analysis
    let static_violations: Vec<String> = system
        .violated_constraints()
        .iter()
        .map(|c| format_constraint(c))
        .collect();

    // 2. Random testing (Schwartz-Zippel)
    let random_result = solve(system, &SolverConfig::default());

    // 3. Bounded model checking
    let bmc_result = bounded_check(system, &BmcConfig::default());

    // 4. Collect redundant assertions (from both methods)
    let mut redundant: Vec<usize> = random_result.always_satisfied.clone();
    for idx in &bmc_result.always_satisfied {
        if !redundant.contains(idx) {
            redundant.push(*idx);
        }
    }
    // Only keep constraints that are redundant in BOTH methods
    redundant.retain(|idx| {
        random_result.always_satisfied.contains(idx) && bmc_result.always_satisfied.contains(idx)
    });
    redundant.sort();

    // 5. Determine verdict
    let verdict = if !static_violations.is_empty() {
        Verdict::StaticViolation
    } else if !random_result.all_passed {
        Verdict::RandomViolation
    } else if !bmc_result.all_passed {
        Verdict::BmcViolation
    } else {
        Verdict::Safe
    };

    VerificationReport {
        static_violations,
        random_result,
        bmc_result,
        redundant_assertions: redundant,
        verdict,
    }
}

// ─── Helpers ───────────────────────────────────────────────────────

/// Collect all variable names referenced in the constraint system.
fn collect_variables(system: &ConstraintSystem) -> Vec<String> {
    let mut names = Vec::new();
    for (name, max_version) in &system.variables {
        for v in 0..=*max_version {
            let key = if v == 0 {
                name.clone()
            } else {
                format!("{}_{}", name, v)
            };
            if !names.contains(&key) {
                names.push(key);
            }
        }
    }
    // Add pub_input and divine variables
    for pi in &system.pub_inputs {
        let key = pi.to_string();
        if !names.contains(&key) {
            names.push(key);
        }
    }
    for di in &system.divine_inputs {
        let key = di.to_string();
        if !names.contains(&key) {
            names.push(key);
        }
    }
    names
}

/// Generate interesting field values for testing.
fn interesting_field_values(count: usize) -> Vec<u64> {
    let mut values = vec![
        0,                   // zero
        1,                   // one (multiplicative identity)
        GOLDILOCKS_P - 1,    // -1 (additive inverse of 1)
        2,                   // smallest prime
        GOLDILOCKS_P - 2,    // -2
        42,                  // common test value
        u32::MAX as u64,     // boundary of U32 range
        u32::MAX as u64 + 1, // just above U32 range
    ];

    // Add small primes
    let primes = [3, 5, 7, 11, 13, 17, 19, 23, 29, 31];
    for &p in &primes {
        if values.len() < count {
            values.push(p);
        }
    }

    // Add powers of 2
    let mut pow2 = 1u64;
    for _ in 0..63 {
        pow2 = pow2.wrapping_mul(2);
        if pow2 < GOLDILOCKS_P && values.len() < count {
            values.push(pow2);
        }
    }

    values.truncate(count);
    values
}

/// Add special values for early rounds to improve coverage.
fn add_special_values(assignments: &mut HashMap<String, u64>, var_names: &[String], round: usize) {
    let special = [0, 1, GOLDILOCKS_P - 1, 2, u32::MAX as u64];
    if round < special.len() {
        // In early rounds, set all variables to the same special value
        let val = special[round];
        for name in var_names {
            assignments.insert(name.clone(), val);
        }
    }
}

/// Generate combinations of variable assignments for exhaustive testing.
/// Caps at `max_combos` to prevent combinatorial explosion.
fn generate_combinations(
    var_names: &[String],
    values: &[u64],
    max_combos: usize,
) -> Vec<HashMap<String, u64>> {
    let num_vars = var_names.len();
    let num_values = values.len();

    // Total combinations = num_values ^ num_vars
    // If too large, sample instead
    let total: u128 = (num_values as u128)
        .checked_pow(num_vars as u32)
        .unwrap_or(u128::MAX);

    if total <= max_combos as u128 {
        // Exhaustive enumeration
        let mut combos = Vec::new();
        let mut indices = vec![0usize; num_vars];
        loop {
            let mut assignment = HashMap::new();
            for (i, name) in var_names.iter().enumerate() {
                assignment.insert(name.clone(), values[indices[i]]);
            }
            combos.push(assignment);

            // Increment indices (odometer-style)
            let mut carry = true;
            for i in (0..num_vars).rev() {
                if carry {
                    indices[i] += 1;
                    if indices[i] >= num_values {
                        indices[i] = 0;
                    } else {
                        carry = false;
                    }
                }
            }
            if carry {
                break; // All combinations exhausted
            }
        }
        combos
    } else {
        // Sample: pick max_combos random combinations
        let mut rng = Rng::new(0xBEEF_CAFE);
        let mut combos = Vec::with_capacity(max_combos);
        for _ in 0..max_combos {
            let mut assignment = HashMap::new();
            for name in var_names {
                let idx = (rng.next_u64() as usize) % num_values;
                assignment.insert(name.clone(), values[idx]);
            }
            combos.push(assignment);
        }
        combos
    }
}

/// Format a constraint for human-readable display.
pub fn format_constraint(c: &Constraint) -> String {
    match c {
        Constraint::Equal(a, b) => {
            format!("{} == {}", format_sym_value(a), format_sym_value(b))
        }
        Constraint::AssertTrue(v) => {
            format!("assert({})", format_sym_value(v))
        }
        Constraint::Conditional(cond, inner) => {
            format!(
                "if {} then {}",
                format_sym_value(cond),
                format_constraint(inner)
            )
        }
        Constraint::RangeU32(v) => {
            format!("{} ∈ U32", format_sym_value(v))
        }
        Constraint::DigestEqual(_, _) => {
            format!("digest_eq([..], [..])")
        }
    }
}

/// Format a symbolic value for display (abbreviated).
pub fn format_sym_value(v: &SymValue) -> String {
    match v {
        SymValue::Const(c) => format!("{}", c),
        SymValue::Var(var) => var.to_string(),
        SymValue::Add(a, b) => format!("({} + {})", format_sym_value(a), format_sym_value(b)),
        SymValue::Mul(a, b) => format!("({} * {})", format_sym_value(a), format_sym_value(b)),
        SymValue::Sub(a, b) => format!("({} - {})", format_sym_value(a), format_sym_value(b)),
        SymValue::Neg(a) => format!("(-{})", format_sym_value(a)),
        SymValue::Inv(a) => format!("(1/{})", format_sym_value(a)),
        SymValue::Eq(a, b) => format!("({} == {})", format_sym_value(a), format_sym_value(b)),
        SymValue::Lt(a, b) => format!("({} < {})", format_sym_value(a), format_sym_value(b)),
        SymValue::Hash(_, idx) => format!("hash[{}]", idx),
        SymValue::Divine(idx) => format!("divine_{}", idx),
        SymValue::PubInput(idx) => format!("pub_in_{}", idx),
        SymValue::Ite(c, t, e) => format!(
            "(if {} then {} else {})",
            format_sym_value(c),
            format_sym_value(t),
            format_sym_value(e)
        ),
        SymValue::FieldAccess(inner, field) => {
            format!("{}.{}", format_sym_value(inner), field)
        }
    }
}
