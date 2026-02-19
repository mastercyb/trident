#![allow(dead_code)]
use std::time::Instant;
use trident::field::{Goldilocks, PrimeField};

type F = Goldilocks;

fn eval(coeffs: &[F], x: F) -> F {
    let mut result = F::ZERO;
    for &c in coeffs.iter().rev() {
        result = result.mul(x).add(c);
    }
    result
}

fn poly_add(a: &[F], b: &[F], out: &mut [F]) {
    for i in 0..a.len() {
        out[i] = a[i].add(b[i]);
    }
}

fn pointwise_mul(a: &[F], b: &[F], out: &mut [F]) {
    for i in 0..a.len() {
        out[i] = a[i].mul(b[i]);
    }
}

fn ntt(a: &mut [F], omega: F) {
    let n = a.len();
    if n <= 1 {
        return;
    }
    // Bit-reversal permutation
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            a.swap(i, j);
        }
    }
    // Cooley-Tukey butterfly
    let mut len = 2;
    while len <= n {
        let wlen = omega.pow((n / len) as u64);
        let half = len / 2;
        for i in (0..n).step_by(len) {
            let mut w = F::ONE;
            for k in 0..half {
                let u = a[i + k];
                let v = a[i + k + half].mul(w);
                a[i + k] = u.add(v);
                a[i + k + half] = u.sub(v);
                w = w.mul(wlen);
            }
        }
        len <<= 1;
    }
}

fn main() {
    let n = 64;
    let coeffs: Vec<F> = (0..n).map(|i| F::from_u64(i as u64 + 1)).collect();
    let x = F::from_u64(7);
    // Primitive root of unity for n=64 in Goldilocks
    let g = F::from_u64(185078157600u64);
    let omega = g.pow((0xFFFF_FFFF_0000_0001u64 - 1) / n as u64);

    let mut buf = coeffs.clone();
    let mut out = vec![F::ZERO; n];

    for _ in 0..100 {
        std::hint::black_box(eval(&coeffs, x));
        buf.copy_from_slice(&coeffs);
        ntt(&mut buf, omega);
        pointwise_mul(&coeffs, &coeffs, &mut out);
    }

    let iters = 10000u128;
    let start = Instant::now();
    for _ in 0..iters {
        std::hint::black_box(eval(std::hint::black_box(&coeffs), std::hint::black_box(x)));
        buf.copy_from_slice(&coeffs);
        ntt(&mut buf, omega);
        std::hint::black_box(&buf);
        pointwise_mul(
            std::hint::black_box(&coeffs),
            std::hint::black_box(&coeffs),
            &mut out,
        );
        std::hint::black_box(&out);
    }
    println!("rust_ns: {}", start.elapsed().as_nanos() / iters);
}
