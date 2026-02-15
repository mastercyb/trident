use super::*;

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

