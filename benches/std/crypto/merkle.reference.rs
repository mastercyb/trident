use std::time::Instant;
use trident::field::{poseidon2, Goldilocks, PrimeField};

type F = Goldilocks;

fn hash_pair(left: [F; 4], right: [F; 4]) -> [F; 4] {
    let mut input = Vec::with_capacity(8);
    input.extend_from_slice(&left);
    input.extend_from_slice(&right);
    poseidon2::hash_fields_goldilocks(&input)
}

fn merkle_verify(leaf: [F; 4], siblings: &[[F; 4]], mut idx: u32) -> [F; 4] {
    let mut current = leaf;
    for sib in siblings {
        current = if idx & 1 == 0 {
            hash_pair(current, *sib)
        } else {
            hash_pair(*sib, current)
        };
        idx >>= 1;
    }
    current
}

fn main() {
    let leaf: [F; 4] = [
        F::from_u64(1),
        F::from_u64(2),
        F::from_u64(3),
        F::from_u64(4),
    ];
    let siblings: Vec<[F; 4]> = (0..3)
        .map(|i| {
            let base = (i + 1) * 10;
            [
                F::from_u64(base),
                F::from_u64(base + 1),
                F::from_u64(base + 2),
                F::from_u64(base + 3),
            ]
        })
        .collect();

    for _ in 0..100 {
        std::hint::black_box(merkle_verify(leaf, &siblings, 5));
    }

    let n = 10000u128;
    let start = Instant::now();
    for _ in 0..n {
        std::hint::black_box(merkle_verify(leaf, &siblings, 5));
    }
    println!("rust_ns: {}", start.elapsed().as_nanos() / n);
}
