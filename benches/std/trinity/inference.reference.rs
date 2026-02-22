use std::time::Instant;
use trident::field::{Goldilocks, PrimeField};

type F = Goldilocks;

const HALF_P: u64 = (0xFFFF_FFFF_0000_0001u64 - 1) / 2;
const N: usize = 4;

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

fn private_linear(input: &[F], w: &[&[F]], tmp: &mut [F], x: F, result: &mut [F]) {
    for i in 0..N {
        result[i] = private_neuron(input, w[i], tmp, x);
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
    for i in 0..N {
        out[i] = relu(result[i].add(bias[i]));
    }
}

// Phase 3: Quantum commitment

#[derive(Clone, Copy)]
struct Complex {
    re: F,
    im: F,
}

#[derive(Clone, Copy)]
struct Qubit {
    zero: Complex,
    one: Complex,
}

fn complex_add(a: Complex, b: Complex) -> Complex {
    Complex {
        re: a.re.add(b.re),
        im: a.im.add(b.im),
    }
}

fn complex_sub(a: Complex, b: Complex) -> Complex {
    Complex {
        re: a.re.sub(b.re),
        im: a.im.sub(b.im),
    }
}

fn hadamard(q: Qubit) -> Qubit {
    Qubit {
        zero: complex_add(q.zero, q.one),
        one: complex_sub(q.zero, q.one),
    }
}

fn pauliz(q: Qubit) -> Qubit {
    Qubit {
        zero: q.zero,
        one: Complex {
            re: q.one.re.neg(),
            im: q.one.im.neg(),
        },
    }
}

fn measure_deterministic(q: Qubit) -> bool {
    let prob_zero = q.zero.re.mul(q.zero.re).add(q.zero.im.mul(q.zero.im));
    let prob_one = q.one.re.mul(q.one.re).add(q.one.im.mul(q.one.im));
    let diff = prob_zero.sub(prob_one);
    let hi = (diff.to_u64() >> 32) as u32;
    hi < 2147483647u32
}

fn quantum_commit(class: usize) -> bool {
    let q = Qubit {
        zero: Complex {
            re: F::ONE,
            im: F::ZERO,
        },
        one: Complex {
            re: F::ZERO,
            im: F::ZERO,
        },
    };
    let q1 = hadamard(q);
    let q2 = if class != 0 { pauliz(q1) } else { q1 };
    let q3 = hadamard(q2);
    measure_deterministic(q3)
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

fn trinity(input: &[F], w: &[&[F]], bias: &[F], x: F) -> bool {
    let mut tmp = [F::ZERO; N];
    let mut result = [F::ZERO; N];
    let mut activated = [F::ZERO; N];
    private_linear(input, w, &mut tmp, x, &mut result);
    activate(&result, bias, &mut activated);
    let class = argmax(&activated);
    quantum_commit(class)
}

fn main() {
    let input: [F; N] = [
        F::from_u64(1),
        F::from_u64(2),
        F::from_u64(3),
        F::from_u64(4),
    ];
    let w0: [F; N] = [
        F::from_u64(1),
        F::from_u64(2),
        F::from_u64(3),
        F::from_u64(4),
    ];
    let w1: [F; N] = [
        F::from_u64(5),
        F::from_u64(6),
        F::from_u64(7),
        F::from_u64(8),
    ];
    let w2: [F; N] = [
        F::from_u64(9),
        F::from_u64(10),
        F::from_u64(11),
        F::from_u64(12),
    ];
    let w3: [F; N] = [
        F::from_u64(13),
        F::from_u64(14),
        F::from_u64(15),
        F::from_u64(16),
    ];
    let w: [&[F]; N] = [&w0, &w1, &w2, &w3];
    let bias: [F; N] = [F::ZERO, F::ZERO, F::ZERO, F::ZERO];
    let x = F::from_u64(7);

    // Warmup
    for _ in 0..100 {
        std::hint::black_box(trinity(&input, &w, &bias, x));
    }

    let iters = 100000u128;
    let start = Instant::now();
    for _ in 0..iters {
        std::hint::black_box(trinity(
            std::hint::black_box(&input),
            std::hint::black_box(&w),
            std::hint::black_box(&bias),
            std::hint::black_box(x),
        ));
    }
    println!("rust_ns: {}", start.elapsed().as_nanos() / iters);
}
