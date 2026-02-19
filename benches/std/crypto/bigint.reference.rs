#![allow(dead_code)]
use std::time::Instant;

type U256 = [u32; 8];

fn zero256() -> U256 {
    [0; 8]
}
fn one256() -> U256 {
    let mut r = [0u32; 8];
    r[0] = 1;
    r
}

fn add256(a: &U256, b: &U256) -> (U256, u32) {
    let mut result = [0u32; 8];
    let mut carry = 0u64;
    for i in 0..8 {
        let sum = a[i] as u64 + b[i] as u64 + carry;
        result[i] = sum as u32;
        carry = sum >> 32;
    }
    (result, carry as u32)
}

fn sub256(a: &U256, b: &U256) -> U256 {
    let mut result = [0u32; 8];
    let mut borrow = 0i64;
    for i in 0..8 {
        let diff = a[i] as i64 - b[i] as i64 - borrow;
        if diff < 0 {
            result[i] = (diff + (1i64 << 32)) as u32;
            borrow = 1;
        } else {
            result[i] = diff as u32;
            borrow = 0;
        }
    }
    result
}

fn lt256(a: &U256, b: &U256) -> bool {
    for i in (0..8).rev() {
        if a[i] < b[i] {
            return true;
        }
        if a[i] > b[i] {
            return false;
        }
    }
    false
}

fn mul256_low(a: &U256, b: &U256) -> U256 {
    let mut result = [0u64; 8];
    for i in 0..8 {
        let mut carry = 0u64;
        for j in 0..(8 - i) {
            let prod = a[i] as u64 * b[j] as u64 + result[i + j] + carry;
            result[i + j] = prod & 0xFFFF_FFFF;
            carry = prod >> 32;
        }
    }
    let mut out = [0u32; 8];
    for i in 0..8 {
        out[i] = result[i] as u32;
    }
    out
}

fn main() {
    let a: U256 = [
        0xDEADBEEF, 0x12345678, 0xABCDEF01, 0x87654321, 0x11111111, 0x22222222, 0x33333333,
        0x44444444,
    ];
    let b: U256 = [
        0xCAFEBABE, 0x98765432, 0xFEDCBA98, 0x13579BDF, 0x55555555, 0x66666666, 0x77777777,
        0x01010101,
    ];

    for _ in 0..100 {
        std::hint::black_box(add256(&a, &b));
        std::hint::black_box(sub256(&a, &b));
        std::hint::black_box(mul256_low(&a, &b));
        std::hint::black_box(lt256(&a, &b));
    }

    let n = 10000u128;
    let start = Instant::now();
    for _ in 0..n {
        std::hint::black_box(add256(&a, &b));
        std::hint::black_box(sub256(&a, &b));
        std::hint::black_box(mul256_low(&a, &b));
        std::hint::black_box(lt256(&a, &b));
    }
    println!("rust_ns: {}", start.elapsed().as_nanos() / n);
}
