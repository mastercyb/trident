use std::time::Instant;
use trident::field::{poseidon2, Goldilocks, PrimeField};

fn main() {
    let input: Vec<Goldilocks> = (0..4).map(|i| Goldilocks::from_u64(i + 1)).collect();

    // Warmup
    for _ in 0..100 {
        std::hint::black_box(poseidon2::hash_fields_goldilocks(std::hint::black_box(
            &input,
        )));
    }

    let n = 1000u128;
    let start = Instant::now();
    for _ in 0..n {
        std::hint::black_box(poseidon2::hash_fields_goldilocks(std::hint::black_box(
            &input,
        )));
    }
    println!("rust_ns: {}", start.elapsed().as_nanos() / n);
}
