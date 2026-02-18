//! Evolutionary training for the neural optimizer.
//!
//! Population of 16 weight vectors. Tournament selection, uniform
//! crossover, per-weight mutation. No gradients required.

use super::model::NeuralModel;
use crate::field::fixed::Fixed;
use crate::ir::tir::encode::TIRBlock;

/// Population size.
pub const POP_SIZE: usize = 16;
/// Survivors per generation (top 25%).
pub const SURVIVORS: usize = 4;
/// Per-weight mutation rate.
pub const MUTATION_RATE: f64 = 0.01;

/// An individual in the population: weights + fitness.
pub struct Individual {
    pub weights: Vec<Fixed>,
    pub fitness: i64,
}

/// Population of weight vectors for evolutionary optimization.
pub struct Population {
    pub individuals: Vec<Individual>,
    pub generation: u64,
    pub best_fitness: i64,
    /// Adaptive mutation rate: increases on plateau, decreases on improvement.
    pub mutation_rate: f64,
    /// Generations since last improvement (for adaptive rate).
    stale_count: u64,
}

impl Population {
    /// Create a new random population with explicit weight count.
    pub fn new_random_with_size(weight_count: usize, seed: u64) -> Self {
        let mut individuals = Vec::with_capacity(POP_SIZE);

        for i in 0..POP_SIZE {
            let weights = random_weights(weight_count, seed.wrapping_add(i as u64));
            individuals.push(Individual {
                weights,
                fitness: i64::MIN,
            });
        }

        Self {
            individuals,
            generation: 0,
            best_fitness: i64::MIN,
            mutation_rate: MUTATION_RATE,
            stale_count: 0,
        }
    }

    /// Create a new random population.
    pub fn new_random(seed: u64) -> Self {
        let mut individuals = Vec::with_capacity(POP_SIZE);
        let model = NeuralModel::zeros();
        let weight_count = model.weight_count();

        for i in 0..POP_SIZE {
            let weights = random_weights(weight_count, seed.wrapping_add(i as u64));
            individuals.push(Individual {
                weights,
                fitness: i64::MIN,
            });
        }

        Self {
            individuals,
            generation: 0,
            best_fitness: i64::MIN,
            mutation_rate: MUTATION_RATE,
            stale_count: 0,
        }
    }

    /// Create a population seeded from existing weights + perturbations.
    pub fn from_weights(base: &[Fixed], seed: u64) -> Self {
        let mut individuals = Vec::with_capacity(POP_SIZE);

        // First individual: exact copy
        individuals.push(Individual {
            weights: base.to_vec(),
            fitness: i64::MIN,
        });

        // Rest: perturbed copies
        for i in 1..POP_SIZE {
            let mut w = base.to_vec();
            mutate_weights(&mut w, 0.05, seed.wrapping_add(i as u64));
            individuals.push(Individual {
                weights: w,
                fitness: i64::MIN,
            });
        }

        Self {
            individuals,
            generation: 0,
            best_fitness: i64::MIN,
            mutation_rate: MUTATION_RATE,
            stale_count: 0,
        }
    }

    /// Evaluate all individuals on a batch of TIR blocks.
    ///
    /// The scorer function takes (model, block) and returns a score
    /// (negative padded height — higher is better). Only verified
    /// outputs count.
    pub fn evaluate<F>(&mut self, blocks: &[TIRBlock], scorer: F)
    where
        F: Fn(&mut NeuralModel, &TIRBlock) -> i64 + Sync,
    {
        let fitnesses: Vec<i64> = std::thread::scope(|s| {
            let handles: Vec<_> = self
                .individuals
                .iter()
                .map(|individual| {
                    let scorer = &scorer;
                    s.spawn(move || {
                        let mut model = NeuralModel::from_weight_vec(&individual.weights);
                        let mut total_fitness = 0i64;
                        for block in blocks {
                            total_fitness = total_fitness.saturating_add(scorer(&mut model, block));
                        }
                        total_fitness
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().expect("evaluate thread panicked"))
                .collect()
        });
        for (individual, fitness) in self.individuals.iter_mut().zip(fitnesses) {
            individual.fitness = fitness;
        }

        self.update_best();
    }

    /// Evaluate all individuals using per-block baselines.
    ///
    /// The scorer function takes (model, block, block_baseline) and returns a score.
    pub fn evaluate_with_baselines<F>(&mut self, blocks: &[TIRBlock], baselines: &[u64], scorer: F)
    where
        F: Fn(&mut NeuralModel, &TIRBlock, u64) -> i64 + Sync,
    {
        let fitnesses: Vec<i64> = std::thread::scope(|s| {
            let handles: Vec<_> = self
                .individuals
                .iter()
                .map(|individual| {
                    let scorer = &scorer;
                    s.spawn(move || {
                        let mut model = NeuralModel::from_weight_vec(&individual.weights);
                        let mut total_fitness = 0i64;
                        for (i, block) in blocks.iter().enumerate() {
                            let baseline = baselines.get(i).copied().unwrap_or(1);
                            total_fitness =
                                total_fitness.saturating_add(scorer(&mut model, block, baseline));
                        }
                        total_fitness
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().expect("evaluate thread panicked"))
                .collect()
        });
        for (individual, fitness) in self.individuals.iter_mut().zip(fitnesses) {
            individual.fitness = fitness;
        }

        self.update_best();
    }

    /// Run one generation: selection + crossover + mutation with adaptive rate.
    pub fn evolve(&mut self, seed: u64) {
        // Sort by fitness (descending)
        self.individuals.sort_by(|a, b| b.fitness.cmp(&a.fitness));

        // Adaptive mutation rate: increase on plateau, decrease on improvement
        let current_best = self.individuals[0].fitness;
        if current_best > self.best_fitness {
            self.stale_count = 0;
            // Decrease rate toward baseline on improvement
            self.mutation_rate = (self.mutation_rate * 0.9).max(MUTATION_RATE * 0.5);
        } else {
            self.stale_count += 1;
            if self.stale_count >= 5 {
                // Increase rate on plateau (cap at 10x baseline)
                self.mutation_rate = (self.mutation_rate * 1.3).min(MUTATION_RATE * 10.0);
            }
        }

        // Keep top SURVIVORS
        let survivors: Vec<Vec<Fixed>> = self.individuals[..SURVIVORS]
            .iter()
            .map(|i| i.weights.clone())
            .collect();

        // Generate new population via crossover + mutation
        let mut new_individuals = Vec::with_capacity(POP_SIZE);

        // Elitism: keep the best unchanged
        new_individuals.push(Individual {
            weights: survivors[0].clone(),
            fitness: i64::MIN,
        });

        for i in 1..POP_SIZE {
            let gen_seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(i as u64);

            let parent_a = &survivors[simple_hash(gen_seed) as usize % SURVIVORS];
            let parent_b = &survivors[simple_hash(gen_seed.wrapping_add(1)) as usize % SURVIVORS];

            let child = crossover(parent_a, parent_b, gen_seed);
            let mut weights = child;
            mutate_weights(&mut weights, self.mutation_rate, gen_seed.wrapping_add(2));

            new_individuals.push(Individual {
                weights,
                fitness: i64::MIN,
            });
        }

        self.individuals = new_individuals;
        self.generation += 1;
    }

    /// Update best_fitness from current individual fitness values.
    /// Used by GPU path where fitness is set externally.
    pub fn update_best(&mut self) {
        if let Some(best) = self.individuals.iter().max_by_key(|i| i.fitness) {
            if best.fitness > self.best_fitness {
                self.best_fitness = best.fitness;
            }
        }
    }

    /// Get the best individual's weights.
    pub fn best_weights(&self) -> &[Fixed] {
        self.individuals
            .iter()
            .max_by_key(|i| i.fitness)
            .map(|i| i.weights.as_slice())
            .unwrap_or(&self.individuals[0].weights)
    }
}

/// Uniform crossover: for each weight, pick from parent A or B.
fn crossover(a: &[Fixed], b: &[Fixed], seed: u64) -> Vec<Fixed> {
    let mut child = Vec::with_capacity(a.len());
    for i in 0..a.len() {
        let hash = simple_hash(seed.wrapping_add(i as u64));
        if hash % 2 == 0 {
            child.push(a[i]);
        } else {
            child.push(b[i]);
        }
    }
    child
}

/// Mutate weights in-place via perturbation (add small delta to existing weight).
fn mutate_weights(weights: &mut [Fixed], rate: f64, seed: u64) {
    let threshold = (rate * u64::MAX as f64) as u64;
    for i in 0..weights.len() {
        let hash = simple_hash(seed.wrapping_add(i as u64));
        if hash < threshold {
            // Perturb existing weight (not replace)
            let delta_hash = simple_hash(hash.wrapping_add(42));
            let delta = (delta_hash % 131072) as f64 / 65536.0 - 1.0; // range [-1, 1]
            let current = weights[i].to_f64();
            weights[i] = Fixed::from_f64(current + delta * 0.05);
        }
    }
}

/// Generate random weights (small values centered around zero).
fn random_weights(count: usize, seed: u64) -> Vec<Fixed> {
    let mut weights = Vec::with_capacity(count);
    for i in 0..count {
        let hash = simple_hash(seed.wrapping_add(i as u64));
        // Xavier-like init: uniform in [-scale, scale] where scale = sqrt(6 / (fan_in + fan_out))
        // For dim=64: scale ≈ 0.17. We use [-0.1, 0.1] as a simpler approximation.
        let val = (hash % 131072) as f64 / 65536.0 - 1.0; // range [-1, 1]
        weights.push(Fixed::from_f64(val * 0.1));
    }
    weights
}

/// Fast deterministic hash for pseudo-random selection.
fn simple_hash(mut x: u64) -> u64 {
    x = x
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51afd7ed558ccd);
    x ^= x >> 33;
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn population_creation() {
        let pop = Population::new_random(42);
        assert_eq!(pop.individuals.len(), POP_SIZE);
        assert_eq!(pop.generation, 0);
    }

    #[test]
    fn evolution_advances_generation() {
        let mut pop = Population::new_random(42);
        // Set some dummy fitness values
        for (i, ind) in pop.individuals.iter_mut().enumerate() {
            ind.fitness = -(i as i64);
        }
        pop.evolve(100);
        assert_eq!(pop.generation, 1);
        assert_eq!(pop.individuals.len(), POP_SIZE);
    }

    #[test]
    fn elitism_preserves_best() {
        let mut pop = Population::new_random(42);
        for (i, ind) in pop.individuals.iter_mut().enumerate() {
            ind.fitness = -(i as i64); // individual 0 has fitness 0 (best)
        }
        let best_before = pop.individuals[0].weights.clone();
        pop.evolve(100);
        // After evolution, the best individual (elitism) should be at index 0
        assert_eq!(pop.individuals[0].weights, best_before);
    }

    #[test]
    fn crossover_produces_valid_output() {
        let a = vec![Fixed::from_f64(1.0); 100];
        let b = vec![Fixed::from_f64(-1.0); 100];
        let child = crossover(&a, &b, 42);
        assert_eq!(child.len(), 100);
        // Should have a mix of 1.0 and -1.0 values
        let pos_count = child.iter().filter(|x| x.to_f64() > 0.0).count();
        assert!(
            pos_count > 0 && pos_count < 100,
            "crossover should mix parents"
        );
    }

    #[test]
    fn mutation_modifies_some_weights() {
        let mut weights = vec![Fixed::from_f64(1.0); 1000];
        mutate_weights(&mut weights, 0.5, 42); // 50% rate
        let modified = weights
            .iter()
            .filter(|x| (x.to_f64() - 1.0).abs() > 0.001)
            .count();
        assert!(modified > 0, "mutation should modify some weights");
        assert!(modified < 1000, "mutation shouldn't modify all weights");
    }

    #[test]
    fn from_weights_preserves_base() {
        let base = vec![Fixed::from_f64(0.5); 100];
        let pop = Population::from_weights(&base, 42);
        assert_eq!(pop.individuals[0].weights, base);
    }
}
