# Plan: Add Poseidon2 Commitment Phase to Trinity

## Context

Trinity currently has 3 phases: FHE (LWE encryption) + AI (dense layer with LUT ReLU) + Quantum (Bell pair commitment). The Rosetta Stone lookup table is used by one reader (NN activation). The roadmap says "Next: Hash Commitment Phase (Poseidon2)".

Adding Poseidon2 hashes the neural output + model parameters into a digest, binding the proof to specific weights and key. This turns Trinity into a tetralogy: FHE + AI + Hash + Quantum.

Note: Poseidon2's S-box is x^7 over the full Goldilocks field — cannot be a LUT reader (2^64 entries). The crypto S-box as Rosetta Stone reader requires a different hash (Tip5 with bounded S-box), which is a separate effort. This step adds Poseidon2 as a commitment mechanism, not as a second LUT consumer.

## Design

### What to commit

Hash 4 field elements: `(weights_digest, key_digest, output_digest, class)` → 1 field element digest. This digest is what binds the proof to specific model parameters.

weights_digest and key_digest are precomputed outside the pipeline and passed in. output_digest is computed inside from the activated array (e.g. sum or hash of outputs).

### Problem: 86 round constants

`poseidon2.hash4()` takes 90 parameters (4 inputs + 86 round constants). Impractical as function arguments in Trinity.

**Solution: RAM-based round constants.** Add `permute_from_ram` and `hash4_from_ram` wrappers that read 86 constants from a RAM address. Parallels the LUT pattern — data lives in RAM, authenticated by STARK consistency.

### Wiring

```
argmax → assert.eq(class, expected_class)
hash_commit(activated, weights_digest, key_digest, class, rc_addr) → digest
assert.eq(digest, expected_digest)
quantum_commit(class) → bool
```

Hash commitment and quantum commitment are parallel assertions on the same class. Hash binds model parameters, quantum commits the classification.

### Files to modify

1. **`std/crypto/poseidon2.tri`** — Add `permute_from_ram(st, rc_addr)` and `hash4_from_ram(a, b, c, d, rc_addr) -> Field`
2. **`std/trinity/inference.tri`** — Add `hash_commit()` phase. Update `trinity()`: new params `rc_addr`, `weights_digest`, `key_digest`, `expected_digest`
3. **`benches/std/trinity/inference.reference.rs`** — Poseidon2 hash phase using `trident::field::poseidon2`
4. **`benches/std/trinity/inference.baseline.tasm`** — `__hash_commit` + updated `__trinity`
5. **`docs/explanation/trinity-bench.md`** — Update phases, instruction counts, roadmap

### Steps

1. Add RAM-based Poseidon2 wrappers to `std/crypto/poseidon2.tri`
2. Add `hash_commit` to `std/trinity/inference.tri`
3. Update `trinity()` signature and wiring
4. Update reference.rs
5. Update baseline.tasm
6. Update explainer
7. Build, test, bench, commit

## Verification

- `cargo build` — zero warnings
- `cargo test` — all pass
- `trident bench benches/std/trinity` — verify counts
