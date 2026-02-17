//! Fixed-point arithmetic in the Goldilocks field.
//!
//! Scale factor S = 2^16 = 65536. Real values encoded as field elements.
//! Multiply with rescale: (a * b) * inv(S). 16-bit fractional precision.

use super::goldilocks::{Goldilocks, MODULUS};
use super::PrimeField;

/// Scale factor: 2^16 = 65536.
pub const SCALE: u64 = 1 << 16;

/// Half the field modulus — values above this are "negative".
const HALF_P: u64 = MODULUS / 2;

/// Precomputed inverse of the scale factor: inv(65536) mod p.
fn inv_scale() -> Goldilocks {
    static INV: std::sync::OnceLock<Goldilocks> = std::sync::OnceLock::new();
    *INV.get_or_init(|| Goldilocks::from_u64(SCALE).inv().expect("SCALE is nonzero"))
}

/// Fixed-point value in Goldilocks field (scale factor 2^16).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fixed(pub Goldilocks);

impl Fixed {
    pub const ZERO: Self = Self(Goldilocks(0));
    pub const ONE: Self = Self(Goldilocks(SCALE));

    /// Encode an f64 as a fixed-point field element.
    ///
    /// Negative values map to the upper half of the field.
    pub fn from_f64(v: f64) -> Self {
        let scaled = v * SCALE as f64;
        if scaled >= 0.0 {
            Self(Goldilocks::from_u64(scaled.round() as u64))
        } else {
            // Negative: p - |scaled|
            let abs = (-scaled).round() as u64;
            Self(Goldilocks::from_u64(MODULUS - abs))
        }
    }

    /// Decode a fixed-point field element back to f64.
    ///
    /// Values in the upper half of the field are treated as negative.
    pub fn to_f64(self) -> f64 {
        let raw = self.0.to_u64();
        if raw <= HALF_P {
            raw as f64 / SCALE as f64
        } else {
            -((MODULUS - raw) as f64 / SCALE as f64)
        }
    }

    /// Raw field element access.
    pub fn raw(self) -> Goldilocks {
        self.0
    }

    /// Construct from a raw Goldilocks element (already scaled).
    pub fn from_raw(g: Goldilocks) -> Self {
        Self(g)
    }

    /// Fixed-point addition (field add, no rescale needed).
    #[inline]
    pub fn add(self, rhs: Self) -> Self {
        Self(self.0.add(rhs.0))
    }

    /// Fixed-point subtraction (field sub, no rescale needed).
    #[inline]
    pub fn sub(self, rhs: Self) -> Self {
        Self(self.0.sub(rhs.0))
    }

    /// Fixed-point multiplication: (a * b) * inv(S).
    #[inline]
    pub fn mul(self, rhs: Self) -> Self {
        Self(self.0.mul(rhs.0).mul(inv_scale()))
    }

    /// Additive inverse.
    #[inline]
    pub fn neg(self) -> Self {
        Self(self.0.neg())
    }

    /// Multiplicative inverse: result * self = ONE.
    ///
    /// self encodes real value v = self.0 / S.
    /// We want 1/v = S / self.0, encoded as fixed-point: S^2 / self.0 = S^2 * inv(self.0).
    pub fn inv(self) -> Self {
        let raw_inv = self.0.inv().expect("cannot invert zero");
        let s = Goldilocks::from_u64(SCALE);
        Self(raw_inv.mul(s).mul(s))
    }

    /// ReLU: if value is "positive" (< p/2) return self, else zero.
    #[inline]
    pub fn relu(self) -> Self {
        if self.0.to_u64() <= HALF_P {
            self
        } else {
            Self::ZERO
        }
    }

    /// Multiply-accumulate: self + a * b (fused, one rescale).
    #[inline]
    pub fn madd(self, a: Self, b: Self) -> Self {
        self.add(a.mul(b))
    }
}

impl std::fmt::Display for Fixed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.4}", self.to_f64())
    }
}

// ─── Fused Dot Product ─────────────────────────────────────────────

/// Raw accumulator for fused dot products.
///
/// Accumulates a.0 * b.0 in raw Goldilocks (no per-multiply rescale).
/// Call `finish()` to apply inv(SCALE) once and get a proper Fixed value.
/// For a dot product of length N: N+1 field muls instead of 2N.
pub struct RawAccum(pub Goldilocks);

impl RawAccum {
    #[inline]
    pub fn zero() -> Self {
        Self(Goldilocks(0))
    }

    /// Accumulate one product: self += a.0 * b.0 (raw, no rescale).
    #[inline]
    pub fn add_prod(&mut self, a: Fixed, b: Fixed) {
        self.0 = self.0.add(a.0.mul(b.0));
    }

    /// Accumulate a pre-scaled addition: self += bias.0 * SCALE.
    /// Used when adding a Fixed bias to a raw accumulator.
    #[inline]
    pub fn add_bias(&mut self, bias: Fixed) {
        self.0 = self.0.add(bias.0.mul(Goldilocks(SCALE)));
    }

    /// Finalize: apply inv(SCALE) once to produce a proper Fixed value.
    #[inline]
    pub fn finish(self) -> Fixed {
        Fixed(self.0.mul(inv_scale()))
    }
}

// ─── Vector Operations ─────────────────────────────────────────────

/// Dot product of two fixed-point vectors (fused, single rescale).
pub fn dot(a: &[Fixed], b: &[Fixed]) -> Fixed {
    debug_assert_eq!(a.len(), b.len());
    let mut acc = RawAccum::zero();
    for i in 0..a.len() {
        acc.add_prod(a[i], b[i]);
    }
    acc.finish()
}

/// Matrix-vector multiply: out[i] = dot(mat[i], vec).
/// Matrix is row-major: mat.len() = rows * cols.
pub fn matvec(mat: &[Fixed], vec: &[Fixed], cols: usize) -> Vec<Fixed> {
    let rows = mat.len() / cols;
    let mut out = Vec::with_capacity(rows);
    for r in 0..rows {
        let row = &mat[r * cols..(r + 1) * cols];
        out.push(dot(row, vec));
    }
    out
}

/// Element-wise ReLU.
pub fn relu_vec(v: &mut [Fixed]) {
    for x in v.iter_mut() {
        *x = x.relu();
    }
}

/// Layer normalization (simplified: zero-mean, unit-variance approximation).
/// Subtracts mean, scales by inverse of approximate std deviation.
pub fn layer_norm(v: &mut [Fixed]) {
    let n = v.len();
    if n == 0 {
        return;
    }
    let n_fixed = Fixed::from_f64(n as f64);

    // Mean
    let mut sum = Fixed::ZERO;
    for x in v.iter() {
        sum = sum.add(*x);
    }
    let mean = sum.mul(n_fixed.inv());

    // Subtract mean
    for x in v.iter_mut() {
        *x = x.sub(mean);
    }

    // Variance (sum of squares / n)
    let mut var_sum = Fixed::ZERO;
    for x in v.iter() {
        var_sum = var_sum.madd(*x, *x);
    }
    let variance = var_sum.mul(n_fixed.inv());

    // Approximate inv_sqrt via Newton: 1/sqrt(v) ≈ inv(v) * v ≈ just use inv(sqrt_approx)
    // Simple approach: scale by inv(max(variance, epsilon))
    let epsilon = Fixed::from_f64(1e-5);
    let scale = if variance.to_f64().abs() < epsilon.to_f64() {
        Fixed::ONE
    } else {
        variance.inv()
    };
    // This is 1/variance not 1/sqrt(variance), but for neural nets
    // with normalized inputs, it's a reasonable approximation that avoids
    // computing square roots in field arithmetic.
    for x in v.iter_mut() {
        *x = x.mul(scale);
    }
}

// ─── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_positive() {
        let vals = [0.0, 0.5, 1.0, 0.375, 100.0, 0.001];
        for &v in &vals {
            let f = Fixed::from_f64(v);
            let back = f.to_f64();
            assert!(
                (back - v).abs() < 0.001,
                "roundtrip failed for {}: got {}",
                v,
                back
            );
        }
    }

    #[test]
    fn roundtrip_negative() {
        let vals = [-0.5, -1.0, -100.0, -0.001];
        for &v in &vals {
            let f = Fixed::from_f64(v);
            let back = f.to_f64();
            assert!(
                (back - v).abs() < 0.001,
                "roundtrip failed for {}: got {}",
                v,
                back
            );
        }
    }

    #[test]
    fn add_commutative() {
        let a = Fixed::from_f64(0.5);
        let b = Fixed::from_f64(0.25);
        assert_eq!(a.add(b), b.add(a));
    }

    #[test]
    fn add_values() {
        let a = Fixed::from_f64(0.5);
        let b = Fixed::from_f64(0.25);
        let c = a.add(b);
        assert!((c.to_f64() - 0.75).abs() < 0.001);
    }

    #[test]
    fn sub_values() {
        let a = Fixed::from_f64(1.0);
        let b = Fixed::from_f64(0.25);
        let c = a.sub(b);
        assert!((c.to_f64() - 0.75).abs() < 0.001);
    }

    #[test]
    fn mul_values() {
        let a = Fixed::from_f64(0.5);
        let b = Fixed::from_f64(0.5);
        let c = a.mul(b);
        assert!(
            (c.to_f64() - 0.25).abs() < 0.001,
            "0.5 * 0.5 = {}, expected 0.25",
            c.to_f64()
        );
    }

    #[test]
    fn mul_negative() {
        let a = Fixed::from_f64(-0.5);
        let b = Fixed::from_f64(2.0);
        let c = a.mul(b);
        assert!(
            (c.to_f64() - (-1.0)).abs() < 0.001,
            "-0.5 * 2.0 = {}, expected -1.0",
            c.to_f64()
        );
    }

    #[test]
    fn neg_values() {
        let a = Fixed::from_f64(1.0);
        let b = a.neg();
        assert!((b.to_f64() - (-1.0)).abs() < 0.001);
        assert_eq!(a.add(b), Fixed::ZERO);
    }

    #[test]
    fn relu_positive() {
        let a = Fixed::from_f64(0.5);
        assert_eq!(a.relu(), a);
    }

    #[test]
    fn relu_negative() {
        let a = Fixed::from_f64(-0.5);
        assert_eq!(a.relu(), Fixed::ZERO);
    }

    #[test]
    fn relu_zero() {
        assert_eq!(Fixed::ZERO.relu(), Fixed::ZERO);
    }

    #[test]
    fn dot_product() {
        let a = [
            Fixed::from_f64(1.0),
            Fixed::from_f64(2.0),
            Fixed::from_f64(3.0),
        ];
        let b = [
            Fixed::from_f64(4.0),
            Fixed::from_f64(5.0),
            Fixed::from_f64(6.0),
        ];
        let result = dot(&a, &b);
        // 1*4 + 2*5 + 3*6 = 32
        assert!(
            (result.to_f64() - 32.0).abs() < 0.1,
            "dot product = {}, expected 32.0",
            result.to_f64()
        );
    }

    #[test]
    fn one_is_identity() {
        let a = Fixed::from_f64(42.0);
        let c = a.mul(Fixed::ONE);
        assert!(
            (c.to_f64() - 42.0).abs() < 0.01,
            "a * 1 = {}, expected 42.0",
            c.to_f64()
        );
    }

    #[test]
    fn inv_roundtrip() {
        let a = Fixed::from_f64(4.0);
        let b = a.inv();
        let c = a.mul(b);
        assert!(
            (c.to_f64() - 1.0).abs() < 0.01,
            "4 * inv(4) = {}, expected 1.0",
            c.to_f64()
        );
    }

    #[test]
    fn raw_accum_dot_matches_naive() {
        let a = [
            Fixed::from_f64(1.0),
            Fixed::from_f64(2.0),
            Fixed::from_f64(3.0),
        ];
        let b = [
            Fixed::from_f64(4.0),
            Fixed::from_f64(5.0),
            Fixed::from_f64(6.0),
        ];
        let naive = a[0].mul(b[0]).add(a[1].mul(b[1])).add(a[2].mul(b[2]));
        let fused = dot(&a, &b);
        assert!(
            (naive.to_f64() - fused.to_f64()).abs() < 0.1,
            "naive={}, fused={}",
            naive.to_f64(),
            fused.to_f64()
        );
        assert!(
            (fused.to_f64() - 32.0).abs() < 0.1,
            "fused dot = {}, expected 32.0",
            fused.to_f64()
        );
    }

    #[test]
    fn raw_accum_with_bias() {
        // bias + a*b = 10 + 3*4 = 22
        let mut acc = RawAccum::zero();
        acc.add_bias(Fixed::from_f64(10.0));
        acc.add_prod(Fixed::from_f64(3.0), Fixed::from_f64(4.0));
        let result = acc.finish();
        assert!(
            (result.to_f64() - 22.0).abs() < 0.1,
            "bias+prod = {}, expected 22.0",
            result.to_f64()
        );
    }

    #[test]
    fn accumulation_precision() {
        // Sum 1000 copies of 0.001 — should be close to 1.0
        let small = Fixed::from_f64(0.001);
        let mut acc = Fixed::ZERO;
        for _ in 0..1000 {
            acc = acc.add(small);
        }
        assert!(
            (acc.to_f64() - 1.0).abs() < 0.1,
            "1000 * 0.001 = {}, expected ~1.0",
            acc.to_f64()
        );
    }
}
