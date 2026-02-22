# Rosetta Stone Unification

## Context

Trinity proves 4 domains in one STARK trace (FHE + AI + Hash + Quantum).
The Rosetta Stone promise: ONE lookup table, MULTIPLE readers.
Currently only 1 reader (NN ReLU activation in Phase 2).
This plan adds 2 more readers — crypto S-box and FHE test polynomial —
making the same `lut_addr` serve 3 domains in one program.

LogUp (STARK-native lookup) is Triton VM upstream — out of scope.

## Architecture

```
lut_addr (ReLU table, 1024 entries in RAM)
    |
    +--- Phase 2: lut.apply()         [Reader 1: NN activation]     EXISTS
    |
    +--- Phase 3: lut_sponge S-box    [Reader 2: Crypto S-box]      NEW
    |
    +--- Phase 5: pbs.build_test_poly [Reader 3: FHE bootstrap]     NEW
```

Three independent functions calling `lut.read(lut_addr, x)` on the
same RAM address. STARK proof authenticates all reads via RAM consistency.

## Step 1: LUT-Based Sponge Hash (`std/crypto/lut_sponge.tri`)

A sponge construction where the S-box IS a LUT read — not x^7, not
a full-domain permutation, but a read from the shared Rosetta Stone table.

**Parameters:**
- State width: 8, Rate: 4, Capacity: 4 (matching Poseidon2 shape)
- S-box: `lut.read(lut_addr, x mod D)` where D = 1024
- MDS: circulant(2,1,1,...,1) — same as Poseidon2 external_linear
- Rounds: 14 (conservative for 10-bit S-box)
- Round constants: 14 * 8 = 112 field elements from RAM

**The `reduce_mod` pattern:**
State elements grow past [0, D) after MDS. The prover supplies
`r = x mod D` via `io.divine()`, circuit verifies `x - r = k * D`
(k also divined). For D = 1024 = 2^10, range check on r uses
`convert.split()` to verify r_hi == 0 and r_lo < 1024.

**Functions:**
- `reduce_mod(x, d) -> Field` — divine + verify modular reduction
- `sbox_layer(st, lut_addr, domain) -> State` — 8x reduce + 8x lut.read
- `mds(st) -> State` — circulant multiply (copy from poseidon2.external_linear)
- `add_constants_from_ram(st, rc_addr) -> State` — 8 RAM reads
- `round(st, lut_addr, domain, rc_addr) -> State` — sbox + rc + mds
- `permute(st, lut_addr, domain, rc_addr) -> State` — 14 rounds
- `hash4_to_digest(a, b, c, d, lut_addr, domain, rc_addr) -> Field`

**Files:**
- `std/crypto/lut_sponge.tri` — new module (~200 lines)

## Step 2: RLWE Module (`std/fhe/rlwe.tri`)

Ring-LWE over R_q = F_p[X]/(X^N + 1). Prerequisite for PBS.
Uses `std.private.poly` (NTT/INTT/poly_mul already complete).

**Memory layout:** ct = (a(X), b(X)), each N coefficients. Stride = 2N.

**Pitch parameters:** N = 64 (ring dimension), log_n = 6.
Structurally identical to production TFHE (N = 1024+), just smaller.

**Functions:**
- `encrypt(a_addr, s_addr, m_addr, ct_addr, delta, e_addr, n, omega, omega_inv, n_inv, log_n, tmp1, tmp2)` — b = a*s + m*delta + e via poly_mul
- `decrypt(ct_addr, s_addr, out_addr, delta, n, omega, omega_inv, n_inv, log_n, tmp1, tmp2)` — m = round((b - a*s) / delta) via divine per coefficient
- `add(ct1, ct2, out, n)` — componentwise poly add
- `external_product(ct, poly, out, n, ntt_params..., tmps)` — multiply ct by plaintext polynomial

**Files:**
- `std/fhe/rlwe.tri` — new module (~200 lines)

## Step 3: Programmable Bootstrapping (`std/fhe/pbs.tri`)

The ReLU LUT read as a polynomial, evaluated on encrypted data via blind rotation.
This is the third reader of the Rosetta Stone table.

**Algorithm:**
1. `build_test_poly(lut_addr, poly_addr, n, d)` — reads D entries from lut_addr via `lut.read`, writes as polynomial coefficients. **This IS the third Rosetta Stone reader.**
2. `monomial_mul(poly_addr, k, n, tmp)` — multiply by X^k mod (X^N + 1)
3. `blind_rotate(acc, ct, bsk, n, lwe_n, ntt_params, tmps)` — CMux loop
4. `sample_extract(rlwe, lwe_out, n)` — extract LWE from RLWE coefficient 0
5. `key_switch(in, ksk, out, n, lwe_n, tmp)` — key basis conversion
6. `bootstrap(ct, lut_addr, bsk, ksk, out, lwe_n, n, d, ntt_params, tmps)` — full pipeline

**Pitch parameters:** N = 64, LWE_N = 8, D = 1024.

**Files:**
- `std/fhe/pbs.tri` — new module (~300 lines)

## Step 4: Trinity Integration

Updated pipeline (7 phases):

```
Phase 1:  LWE private linear           [existing]
Phase 1b: Decrypt via divine()          [existing]
Phase 2:  Dense layer + LUT ReLU        [existing, Reader 1]
Phase 3:  LUT sponge hash commitment    [NEW, Reader 2 — replaces Poseidon2]
Phase 4:  PBS demo on one ciphertext    [NEW, Reader 3]
Phase 5:  Quantum Bell commitment       [existing]
```

Data flow:
```
Phase 1 → Phase 1b → Phase 2 (lut.apply) → argmax → class
                                |                      |
                          Phase 3 (lut_sponge)    Phase 5 (quantum)
                                                       |
                          Phase 4 (pbs.bootstrap using lut_addr)
                                |
                          assert bootstrapped_m == original_m
```

Phase 4 bootstraps one sample ciphertext from Phase 1 output through
the same LUT as test polynomial. The circuit asserts the bootstrapped
plaintext matches the divine()-decrypted value — proving PBS produces
the same result as direct decryption.

**Updated trinity() signature:** grows from 19 to ~32 args.

**Files to modify:**
- `std/trinity/inference.tri` — add lut_hash_commit, pbs_demo, update trinity()
- `benches/std/trinity/inference.reference.rs` — Rust ground truth for all new phases
- `benches/std/trinity/inference.baseline.tasm` — hand TASM for new phases
- `docs/explanation/trinity-bench.md` — update to reflect 3 readers

## Step 5: Documentation

- Update `docs/explanation/trinity-bench.md` — "The Six Phases", 3 readers diagram
- Update Rosetta Stone section: "one table, three readers — demonstrated"

## Execution Order

1. `std/crypto/lut_sponge.tri` — standalone, no deps beyond lut.tri
2. `std/fhe/rlwe.tri` — depends on poly.tri (exists)
3. `std/fhe/pbs.tri` — depends on rlwe.tri + lut.tri
4. Trinity integration — wires everything together
5. Benchmarks + docs

Each step compiles independently. Commit after each step.

## Verification

After each step:
- `trident build <module>` — compiles
- `cargo test` — no regressions
- After step 4: `trident bench benches/std/trinity` — full numbers
- reference.rs must produce identical results to .tri code
