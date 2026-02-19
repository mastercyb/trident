use std::time::Instant;
use trident::field::{Goldilocks, PrimeField};

type F = Goldilocks;

const HALF_P: u64 = (0xFFFF_FFFF_0000_0001u64 - 1) / 2;

fn dot(a: &[F], b: &[F]) -> F {
    a.iter()
        .zip(b.iter())
        .fold(F::ZERO, |acc, (&x, &y)| acc.add(x.mul(y)))
}

fn relu(x: F) -> F {
    if x.to_u64() < HALF_P {
        x
    } else {
        F::ZERO
    }
}

fn matvec(mat: &[F], vec: &[F], rows: usize, cols: usize) -> Vec<F> {
    (0..rows)
        .map(|r| dot(&mat[r * cols..(r + 1) * cols], vec))
        .collect()
}

fn relu_layer(v: &[F]) -> Vec<F> {
    v.iter().map(|&x| relu(x)).collect()
}

fn main() {
    let rows = 16;
    let cols = 16;
    let mat: Vec<F> = (0..rows * cols)
        .map(|i| F::from_u64(i as u64 + 1))
        .collect();
    let vec: Vec<F> = (0..cols).map(|i| F::from_u64(i as u64 * 7 + 3)).collect();

    for _ in 0..100 {
        let r = matvec(&mat, &vec, rows, cols);
        std::hint::black_box(relu_layer(&r));
    }

    let n = 10000u128;
    let start = Instant::now();
    for _ in 0..n {
        let r = matvec(&mat, &vec, rows, cols);
        std::hint::black_box(relu_layer(&r));
    }
    println!("rust_ns: {}", start.elapsed().as_nanos() / n);
}
