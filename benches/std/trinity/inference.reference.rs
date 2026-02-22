use std::time::Instant;
use trident::field::{Goldilocks, PrimeField};

type F = Goldilocks;

const HALF_P: u64 = (0xFFFF_FFFF_0000_0001u64 - 1) / 2;

// Pitch parameters:
//   LWE dimension 8, 8 encrypted inputs, 16 neurons.
//   Delta = p / 1024 (10-bit plaintext space).
//   2-qubit Bell quantum commitment.
const LWE_N: usize = 8;
const INPUT_DIM: usize = 8;
const NEURONS: usize = 16;
const PLAINTEXT_SPACE: u64 = 1024;

fn delta() -> F {
    // Delta = p / t. p = 2^64 - 2^32 + 1, t = 1024.
    // p / 1024 = (2^64 - 2^32 + 1) / 1024
    let p = 0xFFFF_FFFF_0000_0001u64;
    F::from_u64(p / PLAINTEXT_SPACE)
}

// Phase 1: LWE encryption

fn inner_product(a: &[F], s: &[F]) -> F {
    let mut sum = F::ZERO;
    for i in 0..a.len() {
        sum = sum.add(a[i].mul(s[i]));
    }
    sum
}

struct Ciphertext {
    a: Vec<F>,
    b: F,
}

fn encrypt(m: F, s: &[F], a: &[F], e: F, delta: F) -> Ciphertext {
    let dot = inner_product(a, s);
    let b = dot.add(m.mul(delta)).add(e);
    Ciphertext {
        a: a.to_vec(),
        b,
    }
}

fn decrypt(ct: &Ciphertext, s: &[F], delta: F) -> F {
    let dot = inner_product(&ct.a, s);
    let phase = ct.b.sub(dot);
    // Round: m = round(phase / delta)
    // Find m such that |phase - m * delta| is minimized
    let phase_u64 = phase.to_u64();
    let delta_u64 = delta.to_u64();
    // Simple rounding: m = (phase + delta/2) / delta
    let half_delta = delta_u64 / 2;
    let shifted = if phase_u64 <= HALF_P {
        phase_u64.wrapping_add(half_delta)
    } else {
        // Negative phase: p - |phase|, so m = t - round(|phase| / delta)
        let neg_phase = 0xFFFF_FFFF_0000_0001u64 - phase_u64;
        let neg_m = (neg_phase + half_delta) / delta_u64;
        return F::from_u64(PLAINTEXT_SPACE - neg_m);
    };
    F::from_u64(shifted / delta_u64)
}

fn ct_add(ct1: &Ciphertext, ct2: &Ciphertext) -> Ciphertext {
    let a: Vec<F> = ct1
        .a
        .iter()
        .zip(ct2.a.iter())
        .map(|(&a1, &a2)| a1.add(a2))
        .collect();
    Ciphertext {
        a,
        b: ct1.b.add(ct2.b),
    }
}

fn ct_scale(ct: &Ciphertext, k: F) -> Ciphertext {
    let a: Vec<F> = ct.a.iter().map(|&ai| ai.mul(k)).collect();
    Ciphertext {
        a,
        b: ct.b.mul(k),
    }
}

fn private_dot(cts: &[Ciphertext], weights: &[F]) -> Ciphertext {
    let n = cts[0].a.len();
    let mut out = Ciphertext {
        a: vec![F::ZERO; n],
        b: F::ZERO,
    };
    for i in 0..cts.len() {
        let scaled = ct_scale(&cts[i], weights[i]);
        out = ct_add(&out, &scaled);
    }
    out
}

fn private_linear(cts: &[Ciphertext], w: &[Vec<F>]) -> Vec<Ciphertext> {
    w.iter()
        .map(|row| private_dot(cts, row))
        .collect()
}

// Phase 2: Dense neural layer with lookup-table activation
//
// The ReLU activation is a precomputed lookup table over the plaintext
// domain [0, PLAINTEXT_SPACE). This is the Rosetta Stone primitive:
// the same table serves as NN activation AND would serve as the FHE
// programmable bootstrapping test polynomial.

fn build_relu_lut() -> Vec<F> {
    let half = PLAINTEXT_SPACE / 2;
    (0..PLAINTEXT_SPACE)
        .map(|i| {
            if i < half {
                F::from_u64(i)
            } else {
                F::ZERO
            }
        })
        .collect()
}

fn lut_read(table: &[F], index: F) -> F {
    table[index.to_u64() as usize]
}

fn matvec(mat: &[F], vec: &[F], rows: usize, cols: usize) -> Vec<F> {
    (0..rows)
        .map(|i| {
            let mut sum = F::ZERO;
            for j in 0..cols {
                sum = sum.add(mat[i * cols + j].mul(vec[j]));
            }
            sum
        })
        .collect()
}

fn dense(w: &[F], x: &[F], b: &[F], lut: &[F], rows: usize, cols: usize) -> Vec<F> {
    let mv = matvec(w, x, rows, cols);
    mv.iter()
        .zip(b.iter())
        .map(|(&v, &bi)| lut_read(lut, v.add(bi)))
        .collect()
}

// Phase 3: Quantum commitment (2-qubit Bell pair)

fn quantum_commit(class: usize) -> bool {
    let (q00, q01, mut q10, mut q11) = (F::ONE, F::ZERO, F::ZERO, F::ONE);
    if class != 0 {
        q11 = q11.neg();
    }
    std::mem::swap(&mut q10, &mut q11);
    let h00 = q00.add(q10);
    let h01 = q01.add(q11);
    let h10 = q00.sub(q10);
    let h11 = q01.sub(q11);
    let p0 = h00.mul(h00).add(h01.mul(h01));
    let p1 = h10.mul(h10).add(h11.mul(h11));
    let diff = p0.sub(p1);
    let hi = (diff.to_u64() >> 32) as u32;
    hi < 2147483647u32
}

// Full pipeline

fn argmax(v: &[F]) -> usize {
    let mut best = 0;
    let mut best_val = v[0].to_u64();
    for i in 1..v.len() {
        let val = v[i].to_u64();
        if val < HALF_P && (best_val >= HALF_P || val > best_val) {
            best = i;
            best_val = val;
        }
    }
    best
}

fn trinity(
    cts: &[Ciphertext],
    s: &[F],
    priv_w: &[Vec<F>],
    dense_w: &[F],
    dense_b: &[F],
    lut: &[F],
    d: F,
) -> bool {
    // Phase 1: Encrypted linear layer
    let ct_out = private_linear(cts, priv_w);
    // Phase 1b: Decrypt
    let result: Vec<F> = ct_out.iter().map(|ct| decrypt(ct, s, d)).collect();
    // Phase 2: Dense neural layer (activation via shared lookup table)
    let activated = dense(dense_w, &result, dense_b, lut, NEURONS, NEURONS);
    // Phase 3: Quantum commitment
    let class = argmax(&activated);
    quantum_commit(class)
}

fn main() {
    let d = delta();
    // Secret key (small values for demo)
    let s: Vec<F> = (0..LWE_N)
        .map(|i| F::from_u64((i as u64 + 1) % 3))
        .collect();
    // Encrypt INPUT_DIM values
    let cts: Vec<Ciphertext> = (0..INPUT_DIM)
        .map(|i| {
            let m = F::from_u64((i as u64 + 1) % PLAINTEXT_SPACE);
            let a: Vec<F> = (0..LWE_N)
                .map(|j| F::from_u64(((i * LWE_N + j) as u64 + 7) % 97))
                .collect();
            let e = F::from_u64(i as u64 + 1); // small noise
            encrypt(m, &s, &a, e, d)
        })
        .collect();
    // Private layer weights (NEURONS x INPUT_DIM)
    let priv_w: Vec<Vec<F>> = (0..NEURONS)
        .map(|i| {
            (0..INPUT_DIM)
                .map(|j| F::from_u64(((i * INPUT_DIM + j) as u64 + 1) % 5))
                .collect()
        })
        .collect();
    // Dense layer (NEURONS x NEURONS)
    let dense_w: Vec<F> = (0..NEURONS * NEURONS)
        .map(|i| F::from_u64((i as u64 + 1) % 7))
        .collect();
    let dense_b: Vec<F> = (0..NEURONS)
        .map(|i| F::from_u64(i as u64))
        .collect();
    // Shared lookup table: ReLU over plaintext domain [0, 1024)
    // Same table serves as NN activation and FHE PBS test polynomial.
    let lut = build_relu_lut();

    // Warmup
    for _ in 0..100 {
        std::hint::black_box(trinity(&cts, &s, &priv_w, &dense_w, &dense_b, &lut, d));
    }

    let iters = 10000u128;
    let start = Instant::now();
    for _ in 0..iters {
        std::hint::black_box(trinity(
            std::hint::black_box(&cts),
            std::hint::black_box(&s),
            std::hint::black_box(&priv_w),
            std::hint::black_box(&dense_w),
            std::hint::black_box(&dense_b),
            std::hint::black_box(&lut),
            std::hint::black_box(d),
        ));
    }
    println!("rust_ns: {}", start.elapsed().as_nanos() / iters);
}
