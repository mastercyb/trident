use std::time::Instant;
use trident::field::{Goldilocks, PrimeField};

type F = Goldilocks;

const HALF_P: u64 = (0xFFFF_FFFF_0000_0001u64 - 1) / 2;

// Pitch parameters:
//   64-dim encrypted input polynomial
//   16-neuron hidden layer (16 x 64 weight matrix)
//   Deutsch oracle quantum commitment
const POLY_N: usize = 64;
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

// Phase 2: Neural activation

fn relu(x: F) -> F {
    if x.to_u64() < HALF_P {
        x
    } else {
        F::ZERO
    }
}

fn activate(result: &[F], bias: &[F], out: &mut [F]) {
    for i in 0..NEURONS {
        out[i] = relu(result[i].add(bias[i]));
    }
}

// Phase 3: Quantum commitment (Deutsch's algorithm — 1-qubit)
//
// Single-qubit oracle commitment matching .tri code:
//   |0> -> H -> conditional Z -> H -> measure
//
// class=0 (constant oracle): H|0>=|+>, skip Z, H|+>=|0> -> measure true
// class>0 (balanced oracle):  H|0>=|+>, Z|+>=|->, H|->=|1> -> measure false
//
// Unnormalized Hadamard (no sqrt(2) in field):
//   H: zero' = zero + one, one' = zero - one

fn quantum_commit(class: usize) -> bool {
    // init |0>: zero=(1,0), one=(0,0)
    let q_zero = F::ONE;
    let q_one = F::ZERO;
    // Hadamard: zero' = 1+0 = 1, one' = 1-0 = 1
    let h_zero = q_zero.add(q_one);
    let h_one = q_zero.sub(q_one);
    // Conditional Z: negate one-amplitude if class != 0
    let z_zero = h_zero;
    let z_one = if class != 0 { h_one.neg() } else { h_one };
    // Second Hadamard
    let f_zero = z_zero.add(z_one);
    let f_one = z_zero.sub(z_one);
    // Deterministic measure: |zero|^2 vs |one|^2
    let prob_zero = f_zero.mul(f_zero);
    let prob_one = f_one.mul(f_one);
    let diff = prob_zero.sub(prob_one);
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

fn trinity(input: &[F], weights: &[F], bias: &[F], x: F) -> bool {
    let mut tmp = vec![F::ZERO; POLY_N];
    let mut result = vec![F::ZERO; NEURONS];
    let mut activated = vec![F::ZERO; NEURONS];
    private_linear(input, weights, &mut tmp, x, &mut result);
    activate(&result, bias, &mut activated);
    let class = argmax(&activated);
    quantum_commit(class)
}

fn main() {
    // 64-dim input polynomial
    let input: Vec<F> = (0..POLY_N)
        .map(|i| F::from_u64(i as u64 + 1))
        .collect();
    // 16 x 64 weight matrix (contiguous)
    let weights: Vec<F> = (0..NEURONS * POLY_N)
        .map(|i| F::from_u64(i as u64 + 1))
        .collect();
    // 16-element bias
    let bias: Vec<F> = (0..NEURONS)
        .map(|i| F::from_u64(i as u64))
        .collect();
    let x = F::from_u64(7);

    // Warmup
    for _ in 0..100 {
        std::hint::black_box(trinity(&input, &weights, &bias, x));
    }

    let iters = 10000u128;
    let start = Instant::now();
    for _ in 0..iters {
        std::hint::black_box(trinity(
            std::hint::black_box(&input),
            std::hint::black_box(&weights),
            std::hint::black_box(&bias),
            std::hint::black_box(x),
        ));
    }
    println!("rust_ns: {}", start.elapsed().as_nanos() / iters);
}
