use std::time::Instant;
use trident::field::{Goldilocks, PrimeField};

type F = Goldilocks;

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

fn c(re: u64, im: u64) -> Complex {
    Complex {
        re: F::from_u64(re),
        im: F::from_u64(im),
    }
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

fn complex_mul(a: Complex, b: Complex) -> Complex {
    Complex {
        re: a.re.mul(b.re).sub(a.im.mul(b.im)),
        im: a.re.mul(b.im).add(a.im.mul(b.re)),
    }
}

fn complex_norm_sq(c: Complex) -> F {
    c.re.mul(c.re).add(c.im.mul(c.im))
}

fn paulix(q: Qubit) -> Qubit {
    Qubit {
        zero: q.one,
        one: q.zero,
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

fn hadamard(q: Qubit) -> Qubit {
    Qubit {
        zero: complex_add(q.zero, q.one),
        one: complex_sub(q.zero, q.one),
    }
}

fn main() {
    let q = Qubit {
        zero: c(1, 0),
        one: c(0, 0),
    };
    let a = c(3, 7);
    let b = c(11, 13);

    for _ in 0..100 {
        std::hint::black_box(hadamard(paulix(pauliz(q))));
        std::hint::black_box(complex_mul(a, b));
        std::hint::black_box(complex_norm_sq(a));
    }

    let n = 100000u128;
    let start = Instant::now();
    for _ in 0..n {
        std::hint::black_box(hadamard(paulix(pauliz(std::hint::black_box(q)))));
        std::hint::black_box(complex_mul(
            std::hint::black_box(a),
            std::hint::black_box(b),
        ));
        std::hint::black_box(complex_norm_sq(std::hint::black_box(a)));
    }
    println!("rust_ns: {}", start.elapsed().as_nanos() / n);
}
