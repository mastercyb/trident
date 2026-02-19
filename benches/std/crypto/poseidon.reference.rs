use std::time::Instant;
use trident::field::{Goldilocks, PrimeField};

type F = Goldilocks;

fn sbox(x: F) -> F {
    let x2 = x.mul(x);
    let x4 = x2.mul(x2);
    x4.mul(x) // x^5
}

fn mix2(a: F, b: F) -> (F, F) {
    let new_a = a.add(a).add(b);
    let new_b = a.add(b).add(b).add(b);
    (new_a, new_b)
}

fn round2(a: F, b: F, rc0: F, rc1: F) -> (F, F) {
    let a1 = sbox(a.add(rc0));
    let b1 = sbox(b.add(rc1));
    mix2(a1, b1)
}

fn hash2(a: F, b: F) -> F {
    let rcs: [(u64, u64); 4] = [(3, 7), (11, 13), (17, 19), (23, 29)];
    let mut s = (a, b);
    for &(r0, r1) in &rcs {
        s = round2(s.0, s.1, F::from_u64(r0), F::from_u64(r1));
    }
    s.0
}

fn main() {
    let a = F::from_u64(42);
    let b = F::from_u64(1337);

    for _ in 0..100 {
        std::hint::black_box(hash2(std::hint::black_box(a), std::hint::black_box(b)));
    }

    let n = 10000u128;
    let start = Instant::now();
    for _ in 0..n {
        std::hint::black_box(hash2(std::hint::black_box(a), std::hint::black_box(b)));
    }
    println!("rust_ns: {}", start.elapsed().as_nanos() / n);
}
