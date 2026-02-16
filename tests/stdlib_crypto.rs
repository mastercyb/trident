use trident::compile_project;

/// Helper: write a temp program file in the repo root (so module resolution
/// finds `std/`, `vm/`, `os/`) and compile it.
fn compile_test_program(name: &str, source: &str) -> String {
    let path = std::path::Path::new(name);
    std::fs::write(path, source).expect("write temp program");
    let result = compile_project(path);
    std::fs::remove_file(path).ok();
    result.unwrap_or_else(|errs| {
        panic!(
            "{} should compile, got {} errors: {:?}",
            name,
            errs.len(),
            errs.iter().map(|e| &e.message).collect::<Vec<_>>()
        );
    })
}

// ── std.crypto.poseidon ──

#[test]
fn test_std_crypto_poseidon_compiles() {
    let tasm = compile_test_program(
        "_test_poseidon.tri",
        r#"program test_poseidon
use std.crypto.poseidon

fn main() {
    let a: Field = pub_read()
    let b: Field = pub_read()
    let h1: Field = poseidon.hash1(a)
    let h2: Field = poseidon.hash2(a, b)
    pub_write(h1 + h2)
}
"#,
    );
    assert!(tasm.contains("__hash1:"), "missing hash1 function");
    assert!(tasm.contains("__hash2:"), "missing hash2 function");
}

// ── std.crypto.poseidon2 ──

#[test]
fn test_std_crypto_poseidon2_compiles() {
    let tasm = compile_test_program(
        "_test_poseidon2.tri",
        r#"program test_poseidon2
use std.crypto.poseidon2

fn main() {
    let a: Field = pub_read()
    let st: poseidon2.State = poseidon2.State {
        s0: 0, s1: 0, s2: 0, s3: 0,
        s4: 0, s5: 0, s6: 0, s7: 0,
    }
    let st2: poseidon2.State = poseidon2.absorb1(st, a)
    let out: Field = poseidon2.squeeze1(st2)
    pub_write(out)
}
"#,
    );
    assert!(tasm.contains("__absorb1:"), "missing absorb1 function");
    assert!(tasm.contains("__squeeze1:"), "missing squeeze1 function");
}

// ── std.crypto.auth ──

#[test]
fn test_std_crypto_auth_compiles() {
    let tasm = compile_test_program(
        "_test_auth.tri",
        r#"program test_auth
use std.crypto.auth

fn main() {
    let expected: Digest = divine5()
    auth.verify_preimage(expected)
    pub_write(0)
}
"#,
    );
    assert!(
        tasm.contains("__verify_preimage:"),
        "missing verify_preimage function"
    );
}

// ── std.crypto.merkle ──

#[test]
fn test_std_crypto_merkle_compiles() {
    let tasm = compile_test_program(
        "_test_merkle.tri",
        r#"program test_merkle
use std.crypto.merkle

fn main() {
    let leaf: Digest = divine5()
    let root: Digest = divine5()
    let (idx, _hi) = split(pub_read())
    merkle.verify3(leaf, root, idx)
    pub_write(0)
}
"#,
    );
    assert!(tasm.contains("__verify3:"), "missing verify3 function");
    assert!(
        tasm.contains("merkle_step"),
        "merkle should emit merkle_step"
    );
}

// ── std.crypto.bigint ──

#[test]
fn test_std_crypto_bigint_compiles() {
    let tasm = compile_test_program(
        "_test_bigint.tri",
        r#"program test_bigint
use std.crypto.bigint

fn main() {
    let a: bigint.U256 = bigint.zero256()
    let b: bigint.U256 = bigint.one256()
    let (sum, carry) = bigint.add256(a, b)
    let eq: Bool = bigint.eq256(sum, b)
    pub_write(0)
}
"#,
    );
    assert!(tasm.contains("__zero256:"), "missing zero256 function");
    assert!(tasm.contains("__add256:"), "missing add256 function");
    assert!(tasm.contains("__eq256:"), "missing eq256 function");
}

// ── std.crypto.keccak256 ──

#[test]
fn test_std_crypto_keccak256_compiles() {
    let tasm = compile_test_program(
        "_test_keccak256.tri",
        r#"program test_keccak256
use std.crypto.keccak256

fn main() {
    let st: keccak256.KeccakState = keccak256.zero_state()
    let st2: keccak256.KeccakState = keccak256.theta(st)
    let st3: keccak256.KeccakState = keccak256.chi(st2)
    pub_write(0)
}
"#,
    );
    assert!(
        tasm.contains("__zero_state:"),
        "missing zero_state function"
    );
    assert!(tasm.contains("__theta:"), "missing theta function");
    assert!(tasm.contains("__chi:"), "missing chi function");
}

// ── std.crypto.sha256 ──

#[test]
fn test_std_crypto_sha256_compiles() {
    let tasm = compile_test_program(
        "_test_sha256.tri",
        r#"program test_sha256
use std.crypto.sha256

fn main() {
    let st: sha256.Sha256State = sha256.init()
    pub_write(0)
}
"#,
    );
    assert!(tasm.contains("__init:"), "missing init function");
}

// ── std.crypto.ecdsa ──

#[test]
fn test_std_crypto_ecdsa_compiles() {
    let tasm = compile_test_program(
        "_test_ecdsa.tri",
        r#"program test_ecdsa
use std.crypto.bigint
use std.crypto.ecdsa

fn main() {
    let sig: ecdsa.Signature = ecdsa.divine_signature()
    let order: bigint.U256 = bigint.one256()
    let ok: Bool = ecdsa.valid_range(sig, order)
    pub_write(0)
}
"#,
    );
    assert!(
        tasm.contains("__divine_signature:"),
        "missing divine_signature function"
    );
    assert!(
        tasm.contains("__valid_range:"),
        "missing valid_range function"
    );
}

// ── std.crypto.secp256k1 ──

#[test]
fn test_std_crypto_secp256k1_compiles() {
    let tasm = compile_test_program(
        "_test_secp256k1.tri",
        r#"program test_secp256k1
use std.crypto.secp256k1

fn main() {
    let g: secp256k1.Point = secp256k1.generator()
    let ok: Bool = secp256k1.on_curve(g)
    pub_write(0)
}
"#,
    );
    assert!(tasm.contains("__generator:"), "missing generator function");
    assert!(tasm.contains("__on_curve:"), "missing on_curve function");
}

// ── std.crypto.ed25519 ──

#[test]
fn test_std_crypto_ed25519_compiles() {
    let tasm = compile_test_program(
        "_test_ed25519.tri",
        r#"program test_ed25519
use std.crypto.ed25519

fn main() {
    let bp: ed25519.EdPoint = ed25519.base_point()
    let ok: Bool = ed25519.on_curve(bp)
    pub_write(0)
}
"#,
    );
    assert!(
        tasm.contains("__base_point:"),
        "missing base_point function"
    );
    assert!(tasm.contains("__on_curve:"), "missing on_curve function");
}
