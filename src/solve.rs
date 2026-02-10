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

// ─── Field Arithmetic ──────────────────────────────────────────────

/// Goldilocks field element: u64 with mod-p arithmetic.
fn field_add(a: u64, b: u64) -> u64 {
    ((a as u128 + b as u128) % GOLDILOCKS_P as u128) as u64
}

fn field_sub(a: u64, b: u64) -> u64 {
    if a >= b {
        a - b
    } else {
        ((a as u128 + GOLDILOCKS_P as u128 - b as u128) % GOLDILOCKS_P as u128) as u64
    }
}

fn field_mul(a: u64, b: u64) -> u64 {
    ((a as u128 * b as u128) % GOLDILOCKS_P as u128) as u64
}

fn field_neg(a: u64) -> u64 {
    if a == 0 {
        0
    } else {
        GOLDILOCKS_P - a
    }
}

/// Modular exponentiation via square-and-multiply.
fn field_pow(base: u64, mut exp: u64) -> u64 {
    let mut result: u128 = 1;
    let mut b: u128 = base as u128;
    let p = GOLDILOCKS_P as u128;
    while exp > 0 {
        if exp & 1 == 1 {
            result = (result * b) % p;
        }
        b = (b * b) % p;
        exp >>= 1;
    }
    result as u64
}

/// Multiplicative inverse: a^(p-2) mod p (Fermat's little theorem).
fn field_inv(a: u64) -> u64 {
    assert!(a != 0, "inverse of zero");
    field_pow(a, GOLDILOCKS_P - 2)
}

// ─── Pseudo-Random Number Generator ────────────────────────────────

/// Simple xorshift64* PRNG for reproducible random field elements.
struct Rng {
    state: u64,
}

impl Rng {
    fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 1 } else { seed },
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }

    /// Random field element in [0, p).
    fn next_field(&mut self) -> u64 {
        loop {
            let v = self.next_u64();
            if v < GOLDILOCKS_P {
                return v;
            }
            // Rejection sampling — probability of rejection is tiny
            // since GOLDILOCKS_P ≈ 2^64
        }
    }
}

// ─── Evaluator ─────────────────────────────────────────────────────

/// Concrete evaluator: substitutes variable assignments into symbolic values.
struct Evaluator<'a> {
    assignments: &'a HashMap<String, u64>,
}

impl<'a> Evaluator<'a> {
    fn new(assignments: &'a HashMap<String, u64>) -> Self {
        Self { assignments }
    }

    /// Evaluate a symbolic value to a concrete field element.
    /// Returns None if evaluation encounters an undefined variable.
    fn eval(&self, val: &SymValue) -> Option<u64> {
        match val {
            SymValue::Const(c) => Some(*c % GOLDILOCKS_P),
            SymValue::Var(var) => {
                let key = var.to_string();
                self.assignments.get(&key).copied().or_else(|| {
                    // Try just the name without version
                    self.assignments.get(&var.name).copied()
                })
            }
            SymValue::Add(a, b) => {
                let a = self.eval(a)?;
                let b = self.eval(b)?;
                Some(field_add(a, b))
            }
            SymValue::Mul(a, b) => {
                let a = self.eval(a)?;
                let b = self.eval(b)?;
                Some(field_mul(a, b))
            }
            SymValue::Sub(a, b) => {
                let a = self.eval(a)?;
                let b = self.eval(b)?;
                Some(field_sub(a, b))
            }
            SymValue::Neg(a) => {
                let a = self.eval(a)?;
                Some(field_neg(a))
            }
            SymValue::Inv(a) => {
                let a = self.eval(a)?;
                if a == 0 {
                    None // Division by zero
                } else {
                    Some(field_inv(a))
                }
            }
            SymValue::Eq(a, b) => {
                let a = self.eval(a)?;
                let b = self.eval(b)?;
                Some(if a == b { 1 } else { 0 })
            }
            SymValue::Lt(a, b) => {
                let a = self.eval(a)?;
                let b = self.eval(b)?;
                Some(if a < b { 1 } else { 0 })
            }
            SymValue::Hash(inputs, index) => {
                // Hash is opaque — use a deterministic pseudo-hash based on inputs
                let mut h: u64 = 0x9E3779B97F4A7C15; // golden ratio constant
                for input in inputs {
                    let v = self.eval(input)?;
                    h = h.wrapping_mul(0x517CC1B727220A95).wrapping_add(v);
                }
                // Mix in the index
                h = h
                    .wrapping_mul(0x6C62272E07BB0142)
                    .wrapping_add(*index as u64);
                Some(h % GOLDILOCKS_P)
            }
            SymValue::Divine(idx) => {
                let key = format!("divine_{}", idx);
                self.assignments.get(&key).copied()
            }
            SymValue::PubInput(idx) => {
                let key = format!("pub_in_{}", idx);
                self.assignments.get(&key).copied()
            }
            SymValue::Ite(cond, then_val, else_val) => {
                let c = self.eval(cond)?;
                if c != 0 {
                    self.eval(then_val)
                } else {
                    self.eval(else_val)
                }
            }
        }
    }

    /// Check if a constraint is satisfied under current assignments.
    /// Returns: Some(true) if satisfied, Some(false) if violated, None if unevaluable.
    fn check_constraint(&self, c: &Constraint) -> Option<bool> {
        match c {
            Constraint::Equal(a, b) => {
                let va = self.eval(a)?;
                let vb = self.eval(b)?;
                Some(va == vb)
            }
            Constraint::AssertTrue(v) => {
                let val = self.eval(v)?;
                Some(val != 0)
            }
            Constraint::Conditional(cond, inner) => {
                let cv = self.eval(cond)?;
                if cv == 0 {
                    Some(true) // Condition is false → constraint vacuously true
                } else {
                    self.check_constraint(inner)
                }
            }
            Constraint::RangeU32(v) => {
                let val = self.eval(v)?;
                Some(val <= u32::MAX as u64)
            }
            Constraint::DigestEqual(a, b) => {
                for (x, y) in a.iter().zip(b.iter()) {
                    let vx = self.eval(x)?;
                    let vy = self.eval(y)?;
                    if vx != vy {
                        return Some(false);
                    }
                }
                Some(true)
            }
        }
    }
}

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

// ─── Solver ────────────────────────────────────────────────────────

/// Configuration for the solver.
#[derive(Clone, Debug)]
pub struct SolverConfig {
    /// Number of random evaluation rounds (Schwartz-Zippel trials).
    pub rounds: usize,
    /// Seed for the PRNG (0 = use default seed).
    pub seed: u64,
    /// Whether to collect counterexamples.
    pub collect_counterexamples: bool,
    /// Whether to detect redundant (always-true) constraints.
    pub detect_redundant: bool,
}

impl Default for SolverConfig {
    fn default() -> Self {
        Self {
            rounds: 100,
            seed: 0xDEAD_BEEF_CAFE_BABE,
            collect_counterexamples: true,
            detect_redundant: true,
        }
    }
}

/// Solve a constraint system using random evaluation (Schwartz-Zippel) and
/// bounded model checking.
pub fn solve(system: &ConstraintSystem, config: &SolverConfig) -> SolverResult {
    let mut rng = Rng::new(config.seed);
    let num_constraints = system.constraints.len();

    // Track which constraints have ever failed or been unevaluable
    let mut ever_failed = vec![false; num_constraints];
    let mut ever_unevaluable = vec![false; num_constraints];
    let mut counterexamples: Vec<Counterexample> = Vec::new();

    // Collect all variable names we need to assign
    let var_names = collect_variables(system);

    for _round in 0..config.rounds {
        // Generate random assignments for all variables
        let mut assignments = HashMap::new();
        for name in &var_names {
            assignments.insert(name.clone(), rng.next_field());
        }

        // Also add special values in early rounds for better coverage
        if _round < 10 {
            add_special_values(&mut assignments, &var_names, _round);
        }

        let evaluator = Evaluator::new(&assignments);

        for (i, constraint) in system.constraints.iter().enumerate() {
            match evaluator.check_constraint(constraint) {
                Some(true) => {} // Satisfied
                Some(false) => {
                    if !ever_failed[i] {
                        ever_failed[i] = true;
                        if config.collect_counterexamples {
                            counterexamples.push(Counterexample {
                                constraint_index: i,
                                constraint_desc: format_constraint(constraint),
                                assignments: assignments.clone(),
                            });
                        }
                    }
                }
                None => {
                    ever_unevaluable[i] = true;
                }
            }
        }
    }

    // Determine always-satisfied constraints
    let always_satisfied = if config.detect_redundant {
        (0..num_constraints)
            .filter(|&i| {
                !ever_failed[i] && !ever_unevaluable[i] && !system.constraints[i].is_trivial()
            })
            .collect()
    } else {
        Vec::new()
    };

    let unevaluable: Vec<usize> = (0..num_constraints)
        .filter(|&i| ever_unevaluable[i] && !ever_failed[i])
        .collect();

    let all_passed = counterexamples.is_empty();

    SolverResult {
        constraints_checked: num_constraints,
        rounds: config.rounds,
        counterexamples,
        always_satisfied,
        unevaluable,
        all_passed,
    }
}

// ─── Bounded Model Checker ─────────────────────────────────────────

/// Configuration for bounded model checking.
#[derive(Clone, Debug)]
pub struct BmcConfig {
    /// Maximum number of free variables to exhaustively enumerate.
    /// Beyond this, fall back to random sampling.
    pub max_exhaustive_vars: usize,
    /// Number of values to test per variable in exhaustive mode.
    pub values_per_var: usize,
    /// Seed for random sampling.
    pub seed: u64,
}

impl Default for BmcConfig {
    fn default() -> Self {
        Self {
            max_exhaustive_vars: 8,
            values_per_var: 16,
            seed: 0xCAFE_BABE_DEAD_BEEF,
        }
    }
}

/// Run bounded model checking: test constraints against systematic value choices.
///
/// For few variables, tests a grid of interesting values (0, 1, p-1, small primes, etc.).
/// For many variables, uses stratified random sampling.
pub fn bounded_check(system: &ConstraintSystem, config: &BmcConfig) -> SolverResult {
    let var_names = collect_variables(system);
    let num_vars = var_names.len();
    let num_constraints = system.constraints.len();

    let mut ever_failed = vec![false; num_constraints];
    let mut ever_unevaluable = vec![false; num_constraints];
    let mut counterexamples: Vec<Counterexample> = Vec::new();
    let mut total_rounds = 0;

    if num_vars == 0 {
        // No variables: just evaluate once with empty assignment
        let assignments = HashMap::new();
        let evaluator = Evaluator::new(&assignments);
        total_rounds = 1;
        for (i, constraint) in system.constraints.iter().enumerate() {
            match evaluator.check_constraint(constraint) {
                Some(true) => {}
                Some(false) => {
                    ever_failed[i] = true;
                    counterexamples.push(Counterexample {
                        constraint_index: i,
                        constraint_desc: format_constraint(constraint),
                        assignments: HashMap::new(),
                    });
                }
                None => {
                    ever_unevaluable[i] = true;
                }
            }
        }
    } else if num_vars <= config.max_exhaustive_vars {
        // Exhaustive grid: test interesting values for each variable
        let interesting_values = interesting_field_values(config.values_per_var);
        let combos = generate_combinations(&var_names, &interesting_values, 10_000);

        for assignments in &combos {
            total_rounds += 1;
            let evaluator = Evaluator::new(assignments);
            for (i, constraint) in system.constraints.iter().enumerate() {
                match evaluator.check_constraint(constraint) {
                    Some(true) => {}
                    Some(false) => {
                        if !ever_failed[i] {
                            ever_failed[i] = true;
                            counterexamples.push(Counterexample {
                                constraint_index: i,
                                constraint_desc: format_constraint(constraint),
                                assignments: assignments.clone(),
                            });
                        }
                    }
                    None => {
                        ever_unevaluable[i] = true;
                    }
                }
            }
        }
    } else {
        // Too many variables: random sampling
        let mut rng = Rng::new(config.seed);
        let sample_count = config.values_per_var * 100;

        for _ in 0..sample_count {
            total_rounds += 1;
            let mut assignments = HashMap::new();
            for name in &var_names {
                assignments.insert(name.clone(), rng.next_field());
            }

            let evaluator = Evaluator::new(&assignments);
            for (i, constraint) in system.constraints.iter().enumerate() {
                match evaluator.check_constraint(constraint) {
                    Some(true) => {}
                    Some(false) => {
                        if !ever_failed[i] {
                            ever_failed[i] = true;
                            counterexamples.push(Counterexample {
                                constraint_index: i,
                                constraint_desc: format_constraint(constraint),
                                assignments: assignments.clone(),
                            });
                        }
                    }
                    None => {
                        ever_unevaluable[i] = true;
                    }
                }
            }
        }
    }

    let always_satisfied: Vec<usize> = (0..num_constraints)
        .filter(|&i| !ever_failed[i] && !ever_unevaluable[i] && !system.constraints[i].is_trivial())
        .collect();

    let unevaluable: Vec<usize> = (0..num_constraints)
        .filter(|&i| ever_unevaluable[i] && !ever_failed[i])
        .collect();

    let all_passed = counterexamples.is_empty();

    SolverResult {
        constraints_checked: num_constraints,
        rounds: total_rounds,
        counterexamples,
        always_satisfied,
        unevaluable,
        all_passed,
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
fn format_constraint(c: &Constraint) -> String {
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
fn format_sym_value(v: &SymValue) -> String {
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sym;

    fn parse_and_verify(source: &str) -> VerificationReport {
        let file = crate::parse_source(source, "test.tri").unwrap();
        let system = sym::analyze(&file);
        verify(&system)
    }

    #[test]
    fn test_trivial_safe_program() {
        let report = parse_and_verify("program test\nfn main() {\n    assert(true)\n}\n");
        assert!(report.is_safe());
        assert_eq!(report.verdict, Verdict::Safe);
    }

    #[test]
    fn test_trivial_violated_program() {
        let report = parse_and_verify("program test\nfn main() {\n    assert(false)\n}\n");
        assert!(!report.is_safe());
        assert_eq!(report.verdict, Verdict::StaticViolation);
    }

    #[test]
    fn test_constant_equality_safe() {
        let report = parse_and_verify("program test\nfn main() {\n    assert_eq(42, 42)\n}\n");
        assert!(report.is_safe());
    }

    #[test]
    fn test_constant_equality_violated() {
        let report = parse_and_verify("program test\nfn main() {\n    assert_eq(1, 2)\n}\n");
        assert!(!report.is_safe());
    }

    #[test]
    fn test_arithmetic_identity() {
        // x + 0 == x should always hold
        let report = parse_and_verify(
            "program test\nfn main() {\n    let x: Field = pub_read()\n    assert_eq(x + 0, x)\n}\n",
        );
        assert!(report.is_safe());
    }

    #[test]
    fn test_field_arithmetic_safe() {
        // (x + y) * 1 == x + y
        let report = parse_and_verify(
            "program test\nfn main() {\n    let x: Field = pub_read()\n    let y: Field = pub_read()\n    let z: Field = x + y\n    assert_eq(z * 1, z)\n}\n",
        );
        assert!(report.is_safe());
    }

    #[test]
    fn test_counterexample_for_false_assert() {
        let report = parse_and_verify("program test\nfn main() {\n    assert(false)\n}\n");
        assert!(!report.static_violations.is_empty());
    }

    #[test]
    fn test_random_solver_catches_violation() {
        // assert_eq(x, 0) is not always true — random testing should find a counterexample
        let report = parse_and_verify(
            "program test\nfn main() {\n    let x: Field = pub_read()\n    assert_eq(x, 0)\n}\n",
        );
        // Random testing should catch this since most random x != 0
        assert!(!report.random_result.all_passed || !report.bmc_result.all_passed);
    }

    #[test]
    fn test_divine_and_assert() {
        // divine() value with no constraint is unchecked
        let report = parse_and_verify(
            "program test\nfn main() {\n    let x: Field = divine()\n    assert(true)\n}\n",
        );
        assert!(report.is_safe());
    }

    #[test]
    fn test_field_operations() {
        // Test field arithmetic helpers
        assert_eq!(field_add(1, 2), 3);
        assert_eq!(field_mul(3, 4), 12);
        assert_eq!(field_sub(5, 3), 2);
        assert_eq!(field_sub(0, 1), GOLDILOCKS_P - 1);
        assert_eq!(field_neg(0), 0);
        assert_eq!(field_neg(1), GOLDILOCKS_P - 1);
        assert_eq!(field_mul(field_inv(7), 7), 1);
    }

    #[test]
    fn test_interesting_values_coverage() {
        let values = interesting_field_values(8);
        assert!(values.contains(&0));
        assert!(values.contains(&1));
        assert!(values.contains(&(GOLDILOCKS_P - 1)));
    }

    #[test]
    fn test_bmc_empty_system() {
        let system = ConstraintSystem::new();
        let result = bounded_check(&system, &BmcConfig::default());
        assert!(result.all_passed);
    }

    #[test]
    fn test_format_constraint_display() {
        let c = Constraint::Equal(SymValue::Const(1), SymValue::Const(2));
        let s = format_constraint(&c);
        assert!(s.contains("1"));
        assert!(s.contains("2"));
    }

    #[test]
    fn test_solver_with_if_else() {
        let report = parse_and_verify(
            "program test\nfn main() {\n    let x: Field = pub_read()\n    if x == 0 {\n        assert(true)\n    } else {\n        assert(true)\n    }\n}\n",
        );
        assert!(report.is_safe());
    }

    #[test]
    fn test_inlined_function_verification() {
        let report = parse_and_verify(
            "program test\nfn check(x: Field) {\n    assert_eq(x + 0, x)\n}\nfn main() {\n    let a: Field = pub_read()\n    check(a)\n}\n",
        );
        assert!(report.is_safe());
    }
}
