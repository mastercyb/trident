//! Integration tests: every .tri program must compile to valid TASM.
//!
//! Programs that require runtime input (divine/pub_read) are expected to
//! compile but won't execute without input — we only check compilation here.

use crate::compile_project;
use std::path::Path;

/// Helper: compile a .tri file and assert it succeeds, returning the TASM.
fn assert_compiles(path: &str) -> String {
    let p = Path::new(path);
    if !p.exists() {
        panic!("{} does not exist", path);
    }
    match compile_project(p) {
        Ok(tasm) => {
            assert!(!tasm.is_empty(), "{} produced empty TASM", path);
            tasm
        }
        Err(diags) => {
            let msgs: Vec<String> = diags.iter().map(|d| d.message.clone()).collect();
            panic!("{} failed to compile: {:?}", path, msgs);
        }
    }
}

// ── VM layer ──

#[test]
fn vm_core_field_compiles() {
    assert_compiles("vm/core/field.tri");
}

#[test]
fn vm_core_convert_compiles() {
    assert_compiles("vm/core/convert.tri");
}

#[test]
fn vm_core_u32_compiles() {
    assert_compiles("vm/core/u32.tri");
}

#[test]
fn vm_core_assert_compiles() {
    assert_compiles("vm/core/assert.tri");
}

#[test]
fn vm_io_io_compiles() {
    assert_compiles("vm/io/io.tri");
}

#[test]
fn vm_io_mem_compiles() {
    assert_compiles("vm/io/mem.tri");
}

#[test]
fn vm_crypto_hash_compiles() {
    assert_compiles("vm/crypto/hash.tri");
}

#[test]
fn vm_crypto_merkle_compiles() {
    assert_compiles("vm/crypto/merkle.tri");
}

// ── std layer ──

#[test]
fn std_crypto_poseidon2_compiles() {
    assert_compiles("std/crypto/poseidon2.tri");
}

#[test]
fn std_crypto_poseidon_compiles() {
    assert_compiles("std/crypto/poseidon.tri");
}

#[test]
fn std_crypto_sha256_compiles() {
    assert_compiles("std/crypto/sha256.tri");
}

#[test]
fn std_crypto_keccak256_compiles() {
    assert_compiles("std/crypto/keccak256.tri");
}

#[test]
fn std_crypto_auth_compiles() {
    assert_compiles("std/crypto/auth.tri");
}

#[test]
fn std_crypto_merkle_compiles() {
    assert_compiles("std/crypto/merkle.tri");
}

#[test]
fn std_crypto_bigint_compiles() {
    assert_compiles("std/crypto/bigint.tri");
}

#[test]
fn std_crypto_ecdsa_compiles() {
    assert_compiles("std/crypto/ecdsa.tri");
}

#[test]
fn std_crypto_ed25519_compiles() {
    assert_compiles("std/crypto/ed25519.tri");
}

#[test]
fn std_crypto_secp256k1_compiles() {
    assert_compiles("std/crypto/secp256k1.tri");
}

#[test]
fn std_io_storage_compiles() {
    assert_compiles("std/io/storage.tri");
}

#[test]
fn std_nn_tensor_compiles() {
    assert_compiles("std/nn/tensor.tri");
}

#[test]
fn std_private_poly_compiles() {
    assert_compiles("std/private/poly.tri");
}

#[test]
fn std_quantum_gates_compiles() {
    assert_compiles("std/quantum/gates.tri");
}

#[test]
fn std_trinity_inference_compiles() {
    assert_compiles("std/trinity/inference.tri");
}

#[test]
fn std_target_compiles() {
    assert_compiles("std/target.tri");
}

// ── OS layer ──

#[test]
fn os_neptune_kernel_compiles() {
    assert_compiles("os/neptune/kernel.tri");
}

#[test]
fn os_neptune_proof_compiles() {
    assert_compiles("os/neptune/proof.tri");
}

#[test]
fn os_neptune_recursive_compiles() {
    assert_compiles("os/neptune/recursive.tri");
}

#[test]
fn os_neptune_xfield_compiles() {
    assert_compiles("os/neptune/xfield.tri");
}

#[test]
fn os_neptune_utxo_compiles() {
    assert_compiles("os/neptune/utxo.tri");
}

#[test]
fn os_neptune_plumb_compiles() {
    assert_compiles("os/neptune/standards/plumb.tri");
}

// Programs requiring runtime input — compilation must succeed

#[test]
fn os_neptune_locks_generation_compiles() {
    assert_compiles("os/neptune/locks/generation.tri");
}

#[test]
fn os_neptune_locks_symmetric_compiles() {
    assert_compiles("os/neptune/locks/symmetric.tri");
}

#[test]
fn os_neptune_locks_multisig_compiles() {
    assert_compiles("os/neptune/locks/multisig.tri");
}

#[test]
fn os_neptune_locks_timelock_compiles() {
    assert_compiles("os/neptune/locks/timelock.tri");
}

#[test]
fn os_neptune_standards_coin_compiles() {
    assert_compiles("os/neptune/standards/coin.tri");
}

#[test]
fn os_neptune_standards_card_compiles() {
    assert_compiles("os/neptune/standards/card.tri");
}

#[test]
fn os_neptune_types_custom_token_compiles() {
    assert_compiles("os/neptune/types/custom_token.tri");
}

#[test]
fn os_neptune_types_native_currency_compiles() {
    assert_compiles("os/neptune/types/native_currency.tri");
}

#[test]
fn os_neptune_programs_proof_relay_compiles() {
    assert_compiles("os/neptune/programs/proof_relay.tri");
}

#[test]
fn os_neptune_programs_proof_aggregator_compiles() {
    assert_compiles("os/neptune/programs/proof_aggregator.tri");
}

#[test]
fn os_neptune_programs_recursive_verifier_compiles() {
    assert_compiles("os/neptune/programs/recursive_verifier.tri");
}

#[test]
fn os_neptune_programs_transaction_validation_compiles() {
    assert_compiles("os/neptune/programs/transaction_validation.tri");
}
