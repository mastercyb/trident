use super::*;
use crate::field::{Goldilocks, PrimeField};

// ─── Field Arithmetic (thin wrappers over Goldilocks) ──────────────

pub(crate) fn field_add(a: u64, b: u64) -> u64 {
    Goldilocks::from_u64(a)
        .add(Goldilocks::from_u64(b))
        .to_u64()
}

pub(crate) fn field_sub(a: u64, b: u64) -> u64 {
    Goldilocks::from_u64(a)
        .sub(Goldilocks::from_u64(b))
        .to_u64()
}

pub(crate) fn field_mul(a: u64, b: u64) -> u64 {
    Goldilocks::from_u64(a)
        .mul(Goldilocks::from_u64(b))
        .to_u64()
}

pub(crate) fn field_neg(a: u64) -> u64 {
    Goldilocks::from_u64(a).neg().to_u64()
}

/// Multiplicative inverse: a^(p-2) mod p (Fermat's little theorem).
/// Returns `None` for zero (which has no inverse).
pub(crate) fn field_inv(a: u64) -> Option<u64> {
    Goldilocks::from_u64(a).inv().map(|v| v.to_u64())
}

// ─── Pseudo-Random Number Generator ────────────────────────────────

/// Simple xorshift64* PRNG for reproducible random field elements.
pub(crate) struct Rng {
    state: u64,
}

impl Rng {
    pub(crate) fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 1 } else { seed },
        }
    }

    pub(crate) fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }

    /// Random field element in [0, p).
    pub(crate) fn next_field(&mut self) -> u64 {
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
pub(crate) struct Evaluator<'a> {
    assignments: &'a BTreeMap<String, u64>,
}

impl<'a> Evaluator<'a> {
    pub(crate) fn new(assignments: &'a BTreeMap<String, u64>) -> Self {
        Self { assignments }
    }

    /// Evaluate a symbolic value to a concrete field element.
    /// Returns None if evaluation encounters an undefined variable.
    pub(crate) fn eval(&self, val: &SymValue) -> Option<u64> {
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
                field_inv(a)
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
            SymValue::FieldAccess(_, _) => {
                // Field access on a symbolic value — cannot evaluate concretely
                None
            }
        }
    }

    /// Check if a constraint is satisfied under current assignments.
    /// Returns: Some(true) if satisfied, Some(false) if violated, None if unevaluable.
    pub(crate) fn check_constraint(&self, c: &Constraint) -> Option<bool> {
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
