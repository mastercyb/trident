# Trinity: Rosetta Stone Unification — Provable Private Neural Inference

## What Trinity Is

A single Trident program that demonstrates the Rosetta Stone unification:
**one lookup table, four readers**, across five computational domains
in one STARK-verifiable trace.

```
Encrypted Input --> Private Linear --> Decrypt --> Dense Layer --> argmax
                       (FHE)                     (AI, Reader 1)
  --> LUT Sponge Hash --> Poseidon2 Hash --> PBS Demo --> Quantum Commit --> Bool
     (Crypto, Reader 2)    (Crypto)       (FHE, Reader 3)   (Quantum)
```

The four readers share a single RAM-based ReLU lookup table (`lut_addr`):

| Reader | Phase | Module | Operation | Status |
|--------|-------|--------|-----------|--------|
| 1 | Phase 2 (Neural) | `std.math.lut.apply` | ReLU activation | demonstrated |
| 2 | Phase 3a (LUT Sponge) | `std.math.lut.read` | Crypto S-box | demonstrated |
| 3 | Phase 4 (PBS Demo) | `std.math.lut.read` | FHE test polynomial | demonstrated |
| 4 | STARK trace | Triton VM LogUp | Proof authentication | upstream |

To our knowledge, no existing system composes all five domains in a
single proof. TFHE encrypts but can't prove. Cairo proves but can't
encrypt. Qiskit simulates but does neither. Trinity demonstrates that
FHE, neural inference, LUT-based cryptographic hashing, Poseidon2,
programmable bootstrapping, and quantum circuits can execute inside one
STARK trace with data-dependent coupling between phases.

## The Seven Phases

### Phase 1: Privacy (LWE homomorphic encryption)

Real Learning With Errors encryption over the Goldilocks field
(p = 2^64 - 2^32 + 1). Ciphertext modulus q = p -- no impedance
mismatch between the FHE ring and the STARK field.

Each input is an LWE ciphertext (a, b) where b = <a, s> + m*delta + e.
The private linear layer computes homomorphic dot products:
for each neuron, multiply-accumulate encrypted inputs by plaintext
weights using `ct_scale` and `ct_add`.

Parameters: LWE dimension 8, delta = p/1024 (10-bit plaintext space).

### Phase 1b: Decrypt (bridge to plaintext)

Each encrypted output is decrypted via `io.divine()` -- the prover
supplies the candidate plaintext m, the circuit computes the noise
|b - <a,s> - m*delta| and verifies it falls within the bound delta/2.
The STARK proof covers the noise check.

`divine()` is Trident's primary mechanism for non-deterministic prover
input. The same interface serves FHE decryption, neural weight
injection, and quantum measurement outcomes. The proof constrains the
divined value -- unconstrained divine calls are flagged by
`trident audit`.

### Phase 2: Neural — Reader 1 (dense layer + LUT activation)

Full dense layer: `out = relu(W * x + b)`. Matrix-vector multiply
(NEURONS x NEURONS), bias addition, ReLU activation. Identical to
any neural network hidden layer, executing inside a STARK trace.

**Reader 1**: ReLU activation is implemented via `lut.apply`, which
reads the shared RAM-based lookup table. The table maps each input
to its ReLU output: values below p/2 are "positive" (kept), values
at or above p/2 are "negative" (zeroed).

The argmax comparison (for classification) uses `convert.split()` to
decompose field elements into (hi, lo) U32 pairs and compares the
high word against `HALF_P >> 32`.

### Phase 3a: LUT Sponge Hash — Reader 2 (crypto S-box)

A custom sponge hash where the S-box reads from the **same lookup
table** as the ReLU activation. This is the Rosetta Stone crypto
reader — proving that a single table can serve both neural and
cryptographic roles.

Construction: Rescue-style sponge with bounded S-box.
- State width: 8, Rate: 4, Capacity: 4
- S-box: `lut.read(lut_addr, x mod D)` where D = 1024 (table domain)
- MDS: circulant(2,1,1,...,1) — same structure as Poseidon2 external
- Rounds: 14 (conservative for 10-bit S-box)
- Round constants: 14 * 8 = 112 field elements from RAM

**Reader 2**: Each S-box application calls `lut.read` on the shared
table. After the MDS layer, state elements exceed [0, D), so a
`reduce_mod` step uses `divine()` + constraint to bring them back:
the prover supplies r = x mod D and k = x/D, the circuit verifies
x == k*D + r and r < D via `convert.split()`.

The hash binds (weights_digest, key_digest, output_digest, class)
into a single digest. The computed digest is asserted against the
prover's `expected_lut_digest`.

### Phase 3b: Poseidon2 Hash (production binding)

Binds the proof to specific model parameters by hashing
(weights_digest, key_digest, output_digest, class) into a single
field element via Poseidon2 (t=8 state, 4+22+4 rounds, x^7 S-box).

weights_digest and key_digest are precomputed commitments to the
model weights and encryption key. output_digest is computed inside
the pipeline as the sum of activated outputs. The prover supplies
an `expected_digest` hint; the circuit asserts it matches the
computed hash.

This means the proof says "THIS model with THIS key produced THIS
result and THIS classification," not just "some model produced some
result." Without the hash commitment, a prover could substitute a
different model or key and still produce a valid proof.

Round constants (86 field elements) are stored in RAM and read via
`poseidon2.permute_from_ram` -- the same RAM-based pattern as the
ReLU lookup table. Both are authenticated by the STARK consistency
argument.

### Phase 4: PBS Demo — Reader 3 (FHE test polynomial)

Programmable Bootstrapping evaluates the shared lookup table on
encrypted data. The test polynomial is built by reading from the
**same ReLU table** via `lut.read` — proving the table serves as
both NN activation and FHE functional evaluation.

**Reader 3**: `pbs.build_test_poly` reads N entries from `lut_addr`
to construct the test polynomial for blind rotation. The same table
that activates neurons now drives FHE bootstrapping.

The demo: decrypt a sample ciphertext, apply the lookup table,
verify the result matches the expected plaintext. The full production
path would perform blind rotation on encrypted data without decryption.

Parameters: ring dimension 64, domain 1024.

### Phase 5: Quantum (2-qubit Bell pair commitment)

Superdense coding commitment circuit with entanglement:

```
|00> -> H(q0) -> CNOT -> conditional CZ -> CNOT -> H(q0) -> measure q0
```

Bell pair encodes entanglement. CZ marks the class into the phase.
Decode via inverse Bell circuit (CNOT + H), then measure q0.

class=0: decode recovers |00> -> p0 > p1 -> true.
class>0: CZ shifts phase -> decode gives |10> -> p0 < p1 -> false.

The algebraic reduction is `class == 0`, but the .tri code traces
every gate operation -- init, Hadamard, tensor product, CNOT, CZ,
complex arithmetic, norm squared, measurement comparison. The STARK
proof covers the full 2-qubit circuit.

Measurement model: the prover computes outcome probabilities
(p0 = |q00|^2 + |q01|^2, p1 = |q10|^2 + |q11|^2 after tracing out
q1) and the circuit verifies which outcome has greater probability
via field arithmetic. For states with deterministic outcomes (like
Bell pairs), this is equivalent to a physical measurement -- the
probability is 0 or 1. The comparison uses `convert.split()` over
the Goldilocks field, same as `std.quantum.gates.measure_deterministic`
for single-qubit states.

## Data Dependency: Phases Cannot Be Separated

The phases are bound by data flow, not merely concatenated:

```
Phase 1  output --> Phase 1b input   (encrypted ciphertexts in RAM)
Phase 1b output --> Phase 2  input   (decrypted plaintext in RAM)
Phase 2  output --> argmax --> class  (computed classification)
class   --> assert.eq(expected_class) (prover's claim must match)
Phase 2  output + class --> Phase 3a (LUT sponge hash inputs)
Phase 3a output --> assert.eq(expected_lut_digest)
Phase 2  output + class --> Phase 3b (Poseidon2 hash inputs)
Phase 3b output --> assert.eq(expected_digest)
Phase 1  output + lut --> Phase 4    (PBS on encrypted data + same table)
Phase 4  output --> assert.eq(expected_m)
class   --> Phase 5 input            (quantum commit on computed class)
```

The class fed to quantum commitment is computed inside the pipeline
via `tensor.argmax()` on the dense layer output. The prover supplies
an `expected_class` hint, and the circuit asserts it matches the
computed argmax. This prevents shortcutting: you cannot substitute a
class without performing the actual inference.

Both hash digests (LUT sponge and Poseidon2) bind the proof to
specific model parameters. The PBS demo binds the FHE evaluation to
the same table. All are asserted against prover hints. You cannot
remove any phase without breaking the pipeline's data flow.

Every phase consumes the output of the previous phase. The STARK
trace cannot be "cut" into independent sub-traces.

## Parameters

### LWE_N = 8, INPUT_DIM = 8, NEURONS = 16, ring_n = 64, domain = 1024

```
Phase 1  (Privacy):     private_linear -- 16 neurons * 8 inputs * LWE ops
Phase 1b (Decrypt):     16 neurons * lwe.decrypt (inner product + noise check)
Phase 2  (Neural):      matvec(16x16) + bias + lut_relu + argmax  [Reader 1]
Phase 3a (LUT Sponge):  sum + 14-round sponge (8 S-box reads/round) [Reader 2]
Phase 3b (Poseidon2):   sum + permute (86 round constants from RAM)
Phase 4  (PBS Demo):    build_test_poly + bootstrap              [Reader 3]
Phase 5  (Quantum):     2-qubit Bell circuit
```

### Why these numbers

- **LWE_N = 8**: LWE dimension. Ciphertexts are 9 field elements
  (8-element vector a plus scalar b). Lightweight but structurally
  real -- same operations as production TFHE, just smaller dimension.

- **INPUT_DIM = 8**: 8 encrypted inputs, each an LWE ciphertext.
  The private linear layer produces 16 encrypted outputs.

- **NEURONS = 16**: Real hidden layer. 16x16 weight matrix =
  256 field elements. Standard in compact on-device models.

- **delta = p/1024**: 10-bit plaintext space. Plaintexts in [0, 1024).
  Noise tolerance delta/2 for correct decryption.

- **ring_n = 64**: Ring dimension for RLWE/PBS operations. Structurally
  identical to production N = 1024+. Goldilocks has 2^32 roots of unity,
  making NTT native.

- **domain = 1024**: Lookup table domain size. Matches the plaintext
  space. The LUT sponge S-box reduces state elements to [0, 1024) via
  constrained modular reduction before table reads.

- **2-qubit Bell**: Entanglement + measurement. Architecturally proves
  quantum circuits compose with FHE and neural ops. More substantial
  than 1-qubit Deutsch (which collapses to a single comparison).

## Static Instruction Count

```
Module                       Tri   Hand   Ratio
std::trinity::inference      211    167   1.26x
```

Per-function breakdown:

```
Function          Tri   Hand   Ratio   Notes
decrypt_loop        -     24       -   hand-only loop (compiler inlines)
dense_layer        19     17   1.12x   matvec + bias + lut.apply
sum_loop            -     13       -   hand-only helper (compiler inlines)
hash_commit        13     15   0.87x   compiler beats hand
lut_hash_commit    15     17   0.88x   compiler beats hand
quantum_commit     53      3  17.67x   hand uses algebraic shortcut (class == 0)
trinity           111     78   1.42x   pipeline orchestration (29 args)
```

The compiler beats hand in `hash_commit` and `lut_hash_commit` (sum + hash
call) because the compiler's sum loop is more compact. The `quantum_commit`
gap (53 vs 3) is structural: the .tri code traces every quantum gate while
hand TASM uses the algebraic reduction `class == 0`. The `trinity` pipeline
at 1.42x is the main optimization target — orchestrating 29 arguments and
6 phase calls.

## End-to-End Example

Running `ref_std_trinity_inference` produces the full data trace:

```
--- Parameters ---
  p = 18446744069414584321, delta = 18014398505287680
  LWE_N = 8, INPUT_DIM = 8, NEURONS = 16, RING_N = 64, domain = 1024

--- Phase 1: LWE Encryption ---
  plaintexts = [1, 2, 3, 4, 5, 6, 7, 8]

--- Phase 1b: Decrypt ---
  decrypted = [74, 62, 90, 63, 71, 74, 62, 90, 63, 71, 74, 62, 90, 63, 71, 74]
  encrypt/decrypt round-trip = PASS

--- Phase 2: Dense Layer + ReLU (Reader 1) ---
  activated = [136, 153, 155, 137, 149, 141, 158, 160, 142, 154, 146, 163, 165, 147, 159, 163]
  class (argmax) = 12

--- Phase 3a: LUT Sponge Hash (Reader 2) ---
  lut_digest = 546  (112 table reads from shared LUT)

--- Phase 3b: Poseidon2 Hash ---
  poseidon_digest = 812426740292758636

--- Phase 4: PBS Demo (Reader 3) ---
  pbs_result = lut[74] = 74, PBS == direct = PASS

--- Phase 5: Quantum Commitment ---
  class = 12, quantum_commit = false  (class > 0)

VERDICT: ALL CHECKS PASS
```

Every value is deterministic. The reference generates prover hints
(expected_class, expected_digest, expected_lut_digest, pbs_expected_m)
that the .tri circuit asserts via `assert.eq`.

## The Rosetta Stone

Trinity implements the Rosetta Stone unification: **one lookup table,
four readers**. A single RAM-based ReLU table (`lut_addr`) is read
by four independent subsystems within the same STARK trace:

| Reader | Phase | Call site | Purpose |
|--------|-------|-----------|---------|
| 1 | Phase 2 | `lut.apply` in `dense_layer` | Neural activation (ReLU) |
| 2 | Phase 3a | `lut.read` in `lut_sponge.sbox_layer` | Crypto S-box for hash |
| 3 | Phase 4 | `lut.read` in `pbs.build_test_poly` | FHE test polynomial |
| 4 | STARK trace | Triton VM LogUp | Proof authentication |

Readers 1-3 are demonstrated in Trinity. Reader 4 is the STARK itself —
when Triton VM exposes user-defined lookup arguments, all RAM reads
become native LogUp lookups.

The table is built once via `lut.build_relu` and threaded through the
entire pipeline as `lut_addr`. All readers access the same RAM region.
The STARK proof authenticates every read through RAM consistency
— it is provably the same table in all four contexts.

### Why not Tip5 or Poseidon2 as Reader 2?

Tip5 and Poseidon2 S-boxes operate on the full Goldilocks field
(~2^64 possible inputs). A RAM-based lookup table cannot store 2^64
entries. The LUT sponge hash was designed specifically to work with
bounded-domain tables: it reduces state elements to [0, D) via
constrained modular reduction before each S-box lookup. This makes it
compatible with the same 1024-entry ReLU table used by the other readers.

### Reader 4: STARK LogUp

Reader 4 is the STARK itself. Triton VM's LogUp argument performs
lookups against predefined tables. When Triton VM exposes user-defined
lookup arguments, `std.math.lut` becomes a thin wrapper and the cost
drops to zero per read. All four readers share a single table — three
demonstrated, one awaiting upstream support.

## Roadmap

### Done: Lookup-Table Activation (Reader 1)

Phase 2 uses `std.math.lut.apply` for ReLU activation via RAM-based
lookup table. The table serves as the foundation for all four readers.

### Done: LUT Sponge Hash (Reader 2)

Phase 3a hashes (weights_digest, key_digest, output_digest, class)
via a custom sponge where every S-box is a read from the shared ReLU
table. 14 rounds, 8 S-box reads per round = 112 table reads per hash.
Module: `std/crypto/lut_sponge.tri`.

### Done: Poseidon2 Hash Commitment

Phase 3b hashes the same inputs via Poseidon2, providing production-grade
binding. Round constants stored in RAM. Trinity computes both hashes
and asserts both digests.

### Done: PBS Demo (Reader 3)

Phase 4 builds the test polynomial from the shared ReLU table and
evaluates it on a sample ciphertext. Modules: `std/fhe/rlwe.tri`
(Ring-LWE), `std/fhe/pbs.tri` (Programmable Bootstrapping).

### Future: Full Blind Rotation

The current PBS demo decrypts before table evaluation. Full blind
rotation would operate entirely on encrypted data, eliminating the
decrypt step. The algebraic structure (RLWE external product, monomial
multiplication, sample extraction) is already implemented in
`std/fhe/pbs.tri` and `std/fhe/rlwe.tri`.

### Future: Native LogUp (Reader 4)

When Triton VM exposes user-defined lookup arguments, all RAM-based
table reads become native LogUp lookups. The cost per read drops to
zero, and the STARK itself becomes the fourth reader.

### Future: Benchmark Matrix

| Variant        | Change                                     | Metric                      |
|----------------|--------------------------------------------|-----------------------------|
| base           | LWE_N=8, NEURONS=16, 2-qubit              | control point               |
| +rosetta       | 4 readers of shared LUT                    | Rosetta Stone demo          |
| sweep          | LWE_N in {8,16}, NEURONS in {16,32}       | scaling trends              |
| transparent    | divine() off, all inputs public            | witness cost measurement    |

## File Structure

```
std/fhe/lwe.tri                              LWE encryption module
std/fhe/rlwe.tri                             Ring-LWE encryption (NTT-based)
std/fhe/pbs.tri                              Programmable Bootstrapping (Reader 3)
std/math/lut.tri                             RAM-based lookup table (Rosetta Stone)
std/nn/tensor.tri                            Neural primitives (matvec, argmax)
std/crypto/poseidon2.tri                     Poseidon2 hash (+ RAM-based variants)
std/crypto/lut_sponge.tri                    LUT sponge hash (Reader 2)
std/quantum/gates.tri                        Quantum gate library
std/trinity/inference.tri                    Trinity module (4 readers, 29 args)
benches/std/trinity/inference.baseline.tasm  Hand-optimized TASM (167 instructions)
benches/std/trinity/inference.reference.rs   Rust ground truth
```

## What Is Proven

The STARK proof covers every field operation in the trace:

- **LWE encryption**: inner products, ciphertext scaling and addition,
  homomorphic dot products over Goldilocks.
- **Decryption noise check**: |b - <a,s> - m*delta| < delta/2 for each
  divined plaintext. The prover supplies m via `divine()`, the circuit
  verifies the bound.
- **Dense layer + Reader 1**: matrix-vector multiply (16x16), bias
  addition, ReLU lookup table reads via `lut.apply`. All RAM accesses
  authenticated by the STARK RAM consistency argument.
- **Argmax**: field-native comparison of 16 outputs via `convert.split()`.
  The computed class is asserted equal to the prover's `expected_class`.
- **LUT sponge hash + Reader 2**: 14-round sponge permutation where
  each S-box is a `lut.read` from the shared ReLU table. Modular
  reduction to [0, 1024) is constrained via `divine()` + `assert.eq`.
  The computed LUT digest is asserted against `expected_lut_digest`.
- **Poseidon2 hash commitment**: permutation (86 round constants from
  RAM, x^7 S-box, 4+22+4 rounds) over (weights_digest, key_digest,
  output_digest, class). The computed digest is asserted against
  `expected_digest`.
- **PBS demo + Reader 3**: test polynomial built from the shared ReLU
  table via `lut.read`. Bootstrap result asserted against `expected_m`.
- **Quantum circuit**: 2-qubit Bell pair state preparation, conditional
  CZ, inverse Bell decoding, trace-out, probability comparison.
- **Data flow**: each phase consumes the output of the previous phase.
  The trace cannot be cut into independent sub-traces.
- **Rosetta Stone binding**: all four readers access the same table.
  The STARK RAM consistency argument proves it is the same table.
  Readers 1-3 via `lut_addr`, Reader 4 via native LogUp (upstream).

The hash commitments (both LUT sponge and Poseidon2) bind the proof to
specific model weights and encryption key via their digests. The proof
says "this computation was performed correctly by THIS model with THIS
key on these inputs." It does not cover the semantic meaning of the
classification or the quality of the model.

## What Is Intentionally Toy

Trinity is a structural demonstration, not a production deployment.
The parameters are chosen to exercise the correct algebraic operations
at minimal scale:

- **LWE_N = 8**: Real LWE operations but not cryptographically secure
  (production TFHE uses N >= 630). The bench proves the homomorphic
  structure compiles and verifies, not that it resists lattice attacks.
- **NEURONS = 16**: Real dense layer but not a useful classifier.
  256 weights is standard for compact on-device models but too small
  for meaningful accuracy on real tasks.
- **ring_n = 64**: Structurally identical to production N = 1024+.
  The NTT, polynomial multiplication, and blind rotation are real
  operations at reduced scale.
- **LUT sponge security**: 14 rounds with a 10-bit S-box is conservative
  but not formally analyzed. The purpose is to demonstrate the Rosetta
  Stone unification (same table, four readers), not to propose a new
  hash standard.
- **PBS demo simplification**: The demo decrypts before table evaluation.
  Full PBS would perform blind rotation on encrypted data. The table
  read path (Reader 3) is demonstrated correctly.
- **2-qubit Bell**: Demonstrates entanglement and conditional phase
  gates. Quantum advantage requires O(100+) qubits; the bench proves
  quantum circuits compose with FHE and neural ops inside a STARK.
- **divine() bridge**: The LWE-to-plaintext decryption via `divine()`
  is a sound proof technique (the noise check constrains the witness)
  but is not how production FHE works. The full path uses RLWE + PBS
  where the ReLU table drives blind rotation directly on ciphertexts.
- **Deterministic measurement**: The quantum measurement selects the
  higher-probability outcome. For Bell states this is exact (probability
  is 0 or 1). For general states with non-trivial probability distributions,
  a sampling-based model would be needed.

The scaling path is clear: increase LWE_N, increase NEURONS, add
full blind rotation, add more qubits. The algebraic structure does
not change.

## Why This Matters

Trinity proves the Rosetta Stone unification:
**one lookup table, four readers, five domains, one proof.**

- Real LWE encryption, not polynomial approximation
- Data-dependent phases -- class computed from AI output, not injected
- Dual hash commitment (LUT sponge + Poseidon2) binds model parameters
- Three independent subsystems read the same table, proven by STARK RAM
- Programmable bootstrapping reads the same table as neural activation
- Cross-domain composition (std.fhe, std.nn, std.crypto, std.quantum)
- Everything verifiable in a single STARK proof
- Each domain contributes meaningfully to the computation

`trident build std/trinity/inference.tri` -> `trisha prove` -> `trisha verify`.
