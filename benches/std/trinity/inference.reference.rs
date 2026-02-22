use std::time::Instant;
use trident::field::{Goldilocks, PrimeField};

type F = Goldilocks;

const HALF_P: u64 = (0xFFFF_FFFF_0000_0001u64 - 1) / 2;

// Pitch parameters:
//   8-dim encrypted input polynomial (Z_p[x]/(x^8+1))
//   16-neuron hidden layer
//   2-qubit Bell pair quantum commitment
//
// ~11K dynamic ops, ~3s prove (GPU), ~5MB proof.
// See .cortex/plans/trinity-explainer.md for rationale.
const POLY_N: usize = 8;
const NEURONS: usize = 16;

// Phase 1: Private linear layer (FHE-style polynomial arithmetic)

fn pointwise_mul(a: &[F], b: &[F], out: &mut [F]) {
    for i in 0..a.len() {
        out[i] = a[i].mul(b[i]);
    }
}

fn poly_eval(coeffs: &[F], x: F) -> F {
    let mut result = F::ZERO;
    for &c in coeffs.iter().rev() {
        result = result.mul(x).add(c);
    }
    result
}

fn private_neuron(input: &[F], weight: &[F], tmp: &mut [F], x: F) -> F {
    pointwise_mul(input, weight, tmp);
    poly_eval(tmp, x)
}

fn private_linear(input: &[F], weights: &[F], tmp: &mut [F], x: F, result: &mut [F]) {
    for i in 0..NEURONS {
        let w = &weights[i * POLY_N..(i + 1) * POLY_N];
        result[i] = private_neuron(input, w, tmp, x);
    }
}

// Phase 2: Dense neural layer (matvec + bias + ReLU)

fn relu(x: F) -> F {
    if x.to_u64() < HALF_P {
        x
    } else {
        F::ZERO
    }
}

fn matvec(mat: &[F], vec: &[F], out: &mut [F], rows: usize, cols: usize) {
    for i in 0..rows {
        let mut sum = F::ZERO;
        for j in 0..cols {
            sum = sum.add(mat[i * cols + j].mul(vec[j]));
        }
        out[i] = sum;
    }
}

fn dense(w: &[F], x: &[F], b: &[F], out: &mut [F], tmp: &mut [F], rows: usize, cols: usize) {
    matvec(w, x, tmp, rows, cols);
    for i in 0..rows {
        out[i] = relu(tmp[i].add(b[i]));
    }
}

// Phase 3: Quantum commitment (2-qubit Bell pair)
//
// Superdense coding commitment circuit:
//   |00> -> H(q0) -> CNOT -> conditional CZ -> CNOT -> H(q0) -> measure q0
//
// class=0: |00> -> Bell -> skip CZ -> decode -> |00> -> p0=4,p1=0 -> true
// class>0: |00> -> Bell -> CZ -> decode -> |10> -> p0=0,p1=4 -> false

fn quantum_commit(class: usize) -> bool {
    // init |00>, H(q0), tensor product
    // q0: zero=(1,0), one=(0,0) -> H -> zero=(1,0), one=(1,0)
    // q1: zero=(1,0), one=(0,0)
    // product: q00=(1,0), q01=(0,0), q10=(1,0), q11=(0,0)
    // CNOT (swap q10<->q11): q00=(1,0), q01=(0,0), q10=(0,0), q11=(1,0)
    let (q00, q01, mut q10, mut q11) = (F::ONE, F::ZERO, F::ZERO, F::ONE);
    // Conditional CZ: negate q11 if class != 0
    if class != 0 {
        q11 = q11.neg();
    }
    // Decode: CNOT (swap q10<->q11)
    std::mem::swap(&mut q10, &mut q11);
    // H on q0: q00' = q00+q10, q01' = q01+q11, q10' = q00-q10, q11' = q01-q11
    let h00 = q00.add(q10);
    let h01 = q01.add(q11);
    let h10 = q00.sub(q10);
    let h11 = q01.sub(q11);
    // Measure q0: p0 = |q00|^2 + |q01|^2, p1 = |q10|^2 + |q11|^2
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
    input: &[F],
    poly_weights: &[F],
    dense_w: &[F],
    dense_b: &[F],
    x: F,
) -> bool {
    let mut tmp = vec![F::ZERO; POLY_N];
    let mut result = vec![F::ZERO; NEURONS];
    let mut dense_tmp = vec![F::ZERO; NEURONS];
    let mut activated = vec![F::ZERO; NEURONS];
    // Phase 1: Private linear layer
    private_linear(input, poly_weights, &mut tmp, x, &mut result);
    // Phase 2: Dense neural layer
    dense(dense_w, &result, dense_b, &mut activated, &mut dense_tmp, NEURONS, NEURONS);
    // Phase 3: Quantum commitment
    let class = argmax(&activated);
    quantum_commit(class)
}

fn main() {
    // 8-dim input polynomial
    let input: Vec<F> = (0..POLY_N)
        .map(|i| F::from_u64(i as u64 + 1))
        .collect();
    // 16 x 8 polynomial weight matrix (contiguous)
    let poly_weights: Vec<F> = (0..NEURONS * POLY_N)
        .map(|i| F::from_u64(i as u64 + 1))
        .collect();
    // 16 x 16 dense weight matrix
    let dense_w: Vec<F> = (0..NEURONS * NEURONS)
        .map(|i| F::from_u64((i as u64 + 1) % 7))
        .collect();
    // 16-element dense bias
    let dense_b: Vec<F> = (0..NEURONS)
        .map(|i| F::from_u64(i as u64))
        .collect();
    let x = F::from_u64(7);

    // Warmup
    for _ in 0..100 {
        std::hint::black_box(trinity(&input, &poly_weights, &dense_w, &dense_b, x));
    }

    let iters = 10000u128;
    let start = Instant::now();
    for _ in 0..iters {
        std::hint::black_box(trinity(
            std::hint::black_box(&input),
            std::hint::black_box(&poly_weights),
            std::hint::black_box(&dense_w),
            std::hint::black_box(&dense_b),
            std::hint::black_box(x),
        ));
    }
    println!("rust_ns: {}", start.elapsed().as_nanos() / iters);
}
