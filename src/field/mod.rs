//! Prime field arithmetic and universal proving primitives.
//!
//! This module provides field-generic math that every target warrior reuses:
//! - `PrimeField` trait with concrete implementations (Goldilocks, BabyBear, Mersenne31)
//! - `poseidon2` — generic Poseidon2 sponge hash over any PrimeField
//! - `proof` — universal STARK proof estimation (padded height, FRI params, proof size)
//!
//! Three fields cover all 20 supported VMs:
//! - Goldilocks (2^64 - 2^32 + 1): Triton, Miden, OpenVM, Plonky3
//! - BabyBear (2^31 - 2^27 + 1): SP1, RISC Zero, Jolt
//! - Mersenne31 (2^31 - 1): Plonky3, Circle STARKs

pub mod babybear;
pub mod goldilocks;
pub mod mersenne31;
pub mod poseidon2;
pub mod proof;

pub use babybear::BabyBear;
pub use goldilocks::Goldilocks;
pub use mersenne31::Mersenne31;

/// Trait for prime field arithmetic.
///
/// Warriors use this for field-generic hash functions, proof estimation,
/// and verification. Trident provides concrete implementations;
/// warriors call them without reimplementing the math.
pub trait PrimeField: Copy + Clone + Eq + PartialEq + Ord + PartialOrd + std::fmt::Debug {
    /// The field modulus as u128 (fits all supported primes).
    const MODULUS: u128;
    /// Number of bits in the modulus.
    const BITS: u32;
    /// Additive identity.
    const ZERO: Self;
    /// Multiplicative identity.
    const ONE: Self;

    /// Construct from a u64 value (reduced mod p).
    fn from_u64(v: u64) -> Self;
    /// Extract the canonical u64 representative.
    fn to_u64(self) -> u64;

    /// Field addition: (a + b) mod p.
    fn add(self, rhs: Self) -> Self;
    /// Field subtraction: (a - b) mod p.
    fn sub(self, rhs: Self) -> Self;
    /// Field multiplication: (a * b) mod p.
    fn mul(self, rhs: Self) -> Self;
    /// Additive inverse: (-a) mod p.
    fn neg(self) -> Self;

    /// Multiplicative inverse via Fermat: a^(p-2) mod p.
    /// Returns None for zero.
    fn inv(self) -> Option<Self> {
        if self == Self::ZERO {
            return None;
        }
        // Default: Fermat's little theorem. Implementors may override
        // with target-specific optimized inversion.
        let exp = Self::MODULUS - 2;
        Some(self.pow_u128(exp))
    }

    /// Exponentiation: a^exp mod p (square-and-multiply, u64 exponent).
    fn pow(self, mut exp: u64) -> Self {
        let mut base = self;
        let mut acc = Self::ONE;
        while exp > 0 {
            if exp & 1 == 1 {
                acc = acc.mul(base);
            }
            base = base.mul(base);
            exp >>= 1;
        }
        acc
    }

    /// Exponentiation with u128 exponent (for Fermat inversion).
    fn pow_u128(self, mut exp: u128) -> Self {
        let mut base = self;
        let mut acc = Self::ONE;
        while exp > 0 {
            if exp & 1 == 1 {
                acc = acc.mul(base);
            }
            base = base.mul(base);
            exp >>= 1;
        }
        acc
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Generic test suite — run for every PrimeField implementation.
    fn test_field_laws<F: PrimeField>() {
        let zero = F::ZERO;
        let one = F::ONE;
        let a = F::from_u64(42);
        let b = F::from_u64(1337);

        // Additive identity
        assert_eq!(a.add(zero), a);
        assert_eq!(zero.add(a), a);

        // Multiplicative identity
        assert_eq!(a.mul(one), a);
        assert_eq!(one.mul(a), a);

        // Multiplicative zero
        assert_eq!(a.mul(zero), zero);

        // Commutativity
        assert_eq!(a.add(b), b.add(a));
        assert_eq!(a.mul(b), b.mul(a));

        // Negation
        assert_eq!(a.add(a.neg()), zero);
        assert_eq!(zero.neg(), zero);

        // Subtraction is add(neg)
        assert_eq!(a.sub(b), a.add(b.neg()));

        // Inverse
        if let Some(inv_a) = a.inv() {
            assert_eq!(a.mul(inv_a), one);
        }
        assert!(zero.inv().is_none());

        // Power
        assert_eq!(a.pow(0), one);
        assert_eq!(a.pow(1), a);
        assert_eq!(a.pow(3), a.mul(a).mul(a));

        // (-1)^2 = 1
        let neg_one = one.neg();
        assert_eq!(neg_one.mul(neg_one), one);
    }

    #[test]
    fn goldilocks_field_laws() {
        test_field_laws::<Goldilocks>();
    }

    #[test]
    fn babybear_field_laws() {
        test_field_laws::<BabyBear>();
    }

    #[test]
    fn mersenne31_field_laws() {
        test_field_laws::<Mersenne31>();
    }

    /// Edge cases: values at modulus boundary, overflow in add, reduction on input.
    fn test_field_edge_cases<F: PrimeField>() {
        let zero = F::ZERO;
        let one = F::ONE;
        let p_minus_1 = F::from_u64((F::MODULUS - 1) as u64);

        // p-1 + 1 = 0 (mod p)
        assert_eq!(p_minus_1.add(one), zero);

        // p-1 + p-1 = p-2 (mod p)
        let p_minus_2 = F::from_u64((F::MODULUS - 2) as u64);
        assert_eq!(p_minus_1.add(p_minus_1), p_minus_2);

        // 0 - 1 = p-1
        assert_eq!(zero.sub(one), p_minus_1);

        // (p-1) * (p-1) = 1 (since (p-1) = -1 and (-1)^2 = 1)
        assert_eq!(p_minus_1.mul(p_minus_1), one);

        // from_u64 reduces values >= p
        let p_as_u64 = F::MODULUS as u64;
        assert_eq!(F::from_u64(p_as_u64), zero);
        assert_eq!(F::from_u64(p_as_u64 + 1), one);

        // Inverse of p-1 is p-1 (since -1 * -1 = 1)
        assert_eq!(p_minus_1.inv(), Some(p_minus_1));

        // Inverse of 1 is 1
        assert_eq!(one.inv(), Some(one));

        // pow(p-1, 2) = 1
        assert_eq!(p_minus_1.pow(2), one);
    }

    #[test]
    fn goldilocks_edge_cases() {
        test_field_edge_cases::<Goldilocks>();

        // Goldilocks-specific: test reduce128 with large products
        let large = Goldilocks::from_u64(u64::MAX);
        let result = large.mul(large);
        // (u64::MAX mod p)^2 mod p — just verify it doesn't panic
        assert!(result.to_u64() < goldilocks::MODULUS);
    }

    #[test]
    fn babybear_edge_cases() {
        test_field_edge_cases::<BabyBear>();
    }

    #[test]
    fn mersenne31_edge_cases() {
        test_field_edge_cases::<Mersenne31>();
    }
}
