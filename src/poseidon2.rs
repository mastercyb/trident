//! Poseidon2 hash function over the Goldilocks field (p = 2^64 - 2^32 + 1).
//!
//! Implements the Poseidon2 permutation (Grassi et al., 2023) with:
//!   - State width t = 8, rate = 4, capacity = 4
//!   - S-box x^7
//!   - 8 full rounds (4 + 4) and 22 partial rounds
//!   - Round constants derived deterministically from BLAKE3

/// Goldilocks prime: p = 2^64 - 2^32 + 1
const P: u64 = 0xFFFF_FFFF_0000_0001;

/// Poseidon2 state width.
const T: usize = 8;
/// Rate (number of input elements absorbed per permutation call).
const RATE: usize = 4;
/// Number of full rounds.
const R_F: usize = 8;
/// Number of partial rounds.
const R_P: usize = 22;
/// S-box exponent: gcd(7, p-1) = 1 for the Goldilocks prime.
#[cfg(test)]
const ALPHA: u64 = 7;

/// Internal diagonal constants: d_i = 1 + 2^i.
const DIAG: [u64; T] = [2, 3, 5, 9, 17, 33, 65, 129];

// ---------------------------------------------------------------------------
// Goldilocks field element
// ---------------------------------------------------------------------------

/// A field element in the Goldilocks field (u64 modulo `P`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GoldilocksField(pub u64);

impl GoldilocksField {
    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(1);

    /// Canonical constructor -- reduces `v` modulo `P`.
    #[inline]
    pub fn new(v: u64) -> Self {
        Self(v % P)
    }

    /// Reduce a u128 value modulo P using 2^64 = 2^32 - 1 (mod P).
    #[inline]
    fn reduce128(x: u128) -> Self {
        let lo = x as u64;
        let hi = (x >> 64) as u64;
        let hi_shifted = (hi as u128) * ((1u128 << 32) - 1);
        let sum = lo as u128 + hi_shifted;
        let lo2 = sum as u64;
        let hi2 = (sum >> 64) as u64;
        if hi2 == 0 {
            Self(if lo2 >= P { lo2 - P } else { lo2 })
        } else {
            let r = lo2 as u128 + (hi2 as u128) * ((1u128 << 32) - 1);
            let r = r as u64;
            Self(if r >= P { r - P } else { r })
        }
    }

    #[inline]
    pub fn add(self, rhs: Self) -> Self {
        let (sum, carry) = self.0.overflowing_add(rhs.0);
        if carry {
            let r = sum + (u32::MAX as u64);
            Self(if r >= P { r - P } else { r })
        } else {
            Self(if sum >= P { sum - P } else { sum })
        }
    }

    #[inline]
    pub fn sub(self, rhs: Self) -> Self {
        if self.0 >= rhs.0 {
            Self(self.0 - rhs.0)
        } else {
            Self(P - rhs.0 + self.0)
        }
    }

    #[inline]
    pub fn mul(self, rhs: Self) -> Self {
        Self::reduce128((self.0 as u128) * (rhs.0 as u128))
    }

    /// Exponentiation via square-and-multiply.
    pub fn pow(self, mut exp: u64) -> Self {
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

    /// The Poseidon2 S-box: x^7.
    #[inline]
    pub fn sbox(self) -> Self {
        let x2 = self.mul(self);
        let x3 = x2.mul(self);
        let x6 = x3.mul(x3);
        x6.mul(self)
    }
}

// ---------------------------------------------------------------------------
// Round-constant generation
// ---------------------------------------------------------------------------

const TOTAL_ROUNDS: usize = R_F + R_P;

/// Generate the round constant for (`round`, `element`) deterministically.
fn round_constant(round: usize, element: usize) -> GoldilocksField {
    let tag = format!("Poseidon2-Goldilocks-t8-RF8-RP22-{round}-{element}");
    let digest = blake3::hash(tag.as_bytes());
    let bytes: [u8; 8] = digest.as_bytes()[..8].try_into().unwrap();
    GoldilocksField::new(u64::from_le_bytes(bytes))
}

/// Generate all round constants: T per full round, 1 per partial round.
fn generate_all_constants() -> Vec<GoldilocksField> {
    let mut constants = Vec::new();
    for r in 0..TOTAL_ROUNDS {
        let is_full = r < R_F / 2 || r >= R_F / 2 + R_P;
        if is_full {
            for e in 0..T {
                constants.push(round_constant(r, e));
            }
        } else {
            constants.push(round_constant(r, 0));
        }
    }
    constants
}

// ---------------------------------------------------------------------------
// Poseidon2 state & permutation
// ---------------------------------------------------------------------------

/// The Poseidon2 internal state (8 Goldilocks elements).
pub struct Poseidon2State {
    pub state: [GoldilocksField; T],
}

impl Poseidon2State {
    pub fn new() -> Self {
        Self {
            state: [GoldilocksField::ZERO; T],
        }
    }

    /// Apply the S-box to every element (full round).
    #[inline]
    fn full_sbox(&mut self) {
        for s in self.state.iter_mut() {
            *s = s.sbox();
        }
    }

    /// Apply the S-box to element 0 only (partial round).
    #[inline]
    fn partial_sbox(&mut self) {
        self.state[0] = self.state[0].sbox();
    }

    /// External linear layer: circ(2,1,1,...,1).
    /// new[i] = 2*state[i] + sum(state).
    fn external_linear(&mut self) {
        let sum = self
            .state
            .iter()
            .fold(GoldilocksField::ZERO, |a, &b| a.add(b));
        for s in self.state.iter_mut() {
            *s = s.add(sum); // state[i] + sum(all) = 2*state[i] + sum(others)
        }
    }

    /// Internal linear layer: diag(d_0,...,d_7) + ones_matrix.
    /// new[i] = d_i * state[i] + sum(state).
    fn internal_linear(&mut self) {
        let sum = self
            .state
            .iter()
            .fold(GoldilocksField::ZERO, |a, &b| a.add(b));
        for (i, s) in self.state.iter_mut().enumerate() {
            *s = GoldilocksField(DIAG[i]).mul(*s).add(sum);
        }
    }

    /// Full Poseidon2 permutation (in-place).
    pub fn permutation(&mut self) {
        let constants = generate_all_constants();
        let mut ci = 0;

        // First R_F/2 full rounds
        for _ in 0..R_F / 2 {
            for s in self.state.iter_mut() {
                *s = s.add(constants[ci]);
                ci += 1;
            }
            self.full_sbox();
            self.external_linear();
        }

        // R_P partial rounds
        for _ in 0..R_P {
            self.state[0] = self.state[0].add(constants[ci]);
            ci += 1;
            self.partial_sbox();
            self.internal_linear();
        }

        // Last R_F/2 full rounds
        for _ in 0..R_F / 2 {
            for s in self.state.iter_mut() {
                *s = s.add(constants[ci]);
                ci += 1;
            }
            self.full_sbox();
            self.external_linear();
        }

        debug_assert_eq!(ci, constants.len());
    }
}

// ---------------------------------------------------------------------------
// Sponge-based hasher
// ---------------------------------------------------------------------------

/// Poseidon2 sponge hasher (absorb / squeeze interface).
pub struct Poseidon2Hasher {
    state: Poseidon2State,
    absorbed: usize,
}

impl Poseidon2Hasher {
    pub fn new() -> Self {
        Self {
            state: Poseidon2State::new(),
            absorbed: 0,
        }
    }

    /// Absorb field elements into the sponge (rate portion of the state).
    pub fn absorb(&mut self, elements: &[GoldilocksField]) {
        for &elem in elements {
            if self.absorbed == RATE {
                self.state.permutation();
                self.absorbed = 0;
            }
            self.state.state[self.absorbed] = self.state.state[self.absorbed].add(elem);
            self.absorbed += 1;
        }
    }

    /// Absorb raw bytes (7 bytes per element to stay below P).
    pub fn absorb_bytes(&mut self, data: &[u8]) {
        const BYTES_PER_ELEM: usize = 7;
        let mut elements = Vec::with_capacity(data.len() / BYTES_PER_ELEM + 2);
        for chunk in data.chunks(BYTES_PER_ELEM) {
            let mut buf = [0u8; 8];
            buf[..chunk.len()].copy_from_slice(chunk);
            elements.push(GoldilocksField::new(u64::from_le_bytes(buf)));
        }
        // Length separator so [] and [0x00] hash differently.
        elements.push(GoldilocksField::new(data.len() as u64));
        self.absorb(&elements);
    }

    /// Squeeze `count` field elements out of the sponge.
    pub fn squeeze(&mut self, count: usize) -> Vec<GoldilocksField> {
        let mut out = Vec::with_capacity(count);
        self.state.permutation();
        self.absorbed = 0;
        let mut squeezed = 0;
        loop {
            for &elem in self.state.state[..RATE].iter() {
                out.push(elem);
                squeezed += 1;
                if squeezed == count {
                    return out;
                }
            }
            self.state.permutation();
        }
    }

    /// Finalize and return a single field-element hash.
    pub fn finalize(mut self) -> GoldilocksField {
        self.squeeze(1)[0]
    }

    /// Finalize and return 4 field elements (256-bit equivalent).
    pub fn finalize_4(mut self) -> [GoldilocksField; 4] {
        let v = self.squeeze(4);
        [v[0], v[1], v[2], v[3]]
    }
}

// ---------------------------------------------------------------------------
// Convenience helpers
// ---------------------------------------------------------------------------

/// Hash arbitrary bytes to a 256-bit content hash (32 bytes).
pub fn hash_bytes(data: &[u8]) -> [u8; 32] {
    let mut hasher = Poseidon2Hasher::new();
    hasher.absorb_bytes(data);
    let result = hasher.finalize_4();
    let mut out = [0u8; 32];
    for (i, elem) in result.iter().enumerate() {
        out[i * 8..i * 8 + 8].copy_from_slice(&elem.0.to_le_bytes());
    }
    out
}

/// Hash a slice of field elements directly, returning 4 field elements.
pub fn hash_fields(elements: &[GoldilocksField]) -> [GoldilocksField; 4] {
    let mut hasher = Poseidon2Hasher::new();
    hasher.absorb(elements);
    hasher.finalize_4()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_goldilocks_arithmetic() {
        let a = GoldilocksField::new(P - 1);
        let b = GoldilocksField::ONE;
        // (p-1) + 1 = 0
        assert_eq!(a.add(b), GoldilocksField::ZERO);
        // 0 - 1 = p-1
        assert_eq!(GoldilocksField::ZERO.sub(b), a);
        // Multiplication identity and zero
        let x = GoldilocksField::new(123456789);
        assert_eq!(x.mul(GoldilocksField::ONE), x);
        assert_eq!(x.mul(GoldilocksField::ZERO), GoldilocksField::ZERO);
        // Commutativity
        let y = GoldilocksField::new(987654321);
        assert_eq!(x.mul(y), y.mul(x));
        // Pow: x^0 = 1, x^1 = x, x^3 = x*x*x
        assert_eq!(x.pow(0), GoldilocksField::ONE);
        assert_eq!(x.pow(1), x);
        assert_eq!(x.pow(3), x.mul(x).mul(x));
        // (-1)^2 = 1
        assert_eq!(a.mul(a), GoldilocksField::ONE);
    }

    #[test]
    fn test_sbox() {
        let x = GoldilocksField::new(42);
        assert_eq!(x.sbox(), x.pow(ALPHA));
        assert_eq!(GoldilocksField::ZERO.sbox(), GoldilocksField::ZERO);
        assert_eq!(GoldilocksField::ONE.sbox(), GoldilocksField::ONE);
        let z = GoldilocksField::new(1000);
        assert_ne!(z.sbox(), z);
        assert_eq!(z.sbox(), z.pow(7));
    }

    #[test]
    fn test_permutation_deterministic() {
        let input: [GoldilocksField; T] =
            core::array::from_fn(|i| GoldilocksField::new(i as u64 + 1));
        let mut s1 = Poseidon2State { state: input };
        let mut s2 = Poseidon2State { state: input };
        s1.permutation();
        s2.permutation();
        assert_eq!(s1.state, s2.state);
    }

    #[test]
    fn test_permutation_diffusion() {
        let base: [GoldilocksField; T] =
            core::array::from_fn(|i| GoldilocksField::new(i as u64 + 100));
        let mut s_base = Poseidon2State { state: base };
        s_base.permutation();

        let mut tweaked = base;
        tweaked[0] = tweaked[0].add(GoldilocksField::ONE);
        let mut s_tweak = Poseidon2State { state: tweaked };
        s_tweak.permutation();

        for i in 0..T {
            assert_ne!(
                s_base.state[i], s_tweak.state[i],
                "Element {i} unchanged after input tweak"
            );
        }
    }

    #[test]
    fn test_hash_bytes_deterministic() {
        assert_eq!(hash_bytes(b"hello world"), hash_bytes(b"hello world"));
    }

    #[test]
    fn test_hash_bytes_different_inputs() {
        assert_ne!(hash_bytes(b"hello"), hash_bytes(b"world"));
    }

    #[test]
    fn test_absorb_squeeze() {
        let elems: Vec<GoldilocksField> =
            (0..10).map(|i| GoldilocksField::new(i * 7 + 3)).collect();

        let mut h1 = Poseidon2Hasher::new();
        h1.absorb(&elems);
        let out1 = h1.squeeze(4);

        let mut h2 = Poseidon2Hasher::new();
        h2.absorb(&elems);
        let out2 = h2.squeeze(4);

        assert_eq!(out1, out2);
        assert!(out1.iter().any(|e| *e != GoldilocksField::ZERO));
    }

    #[test]
    fn test_hash_fields() {
        let elems: Vec<GoldilocksField> = (1..=5).map(GoldilocksField::new).collect();
        assert_eq!(hash_fields(&elems), hash_fields(&elems));
    }

    #[test]
    fn test_empty_hash() {
        let h = hash_bytes(b"");
        assert_eq!(h, hash_bytes(b""));
        assert_ne!(h, [0u8; 32]);
    }

    #[test]
    fn test_collision_resistance() {
        let hashes: Vec<[u8; 32]> = (0u64..20).map(|i| hash_bytes(&i.to_le_bytes())).collect();
        for i in 0..hashes.len() {
            for j in i + 1..hashes.len() {
                assert_ne!(hashes[i], hashes[j], "Collision between inputs {i} and {j}");
            }
        }
    }
}
