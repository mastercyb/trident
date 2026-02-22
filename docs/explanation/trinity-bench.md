# Trinity: Provable Private Neural Inference with Quantum Commitment

## What Trinity Is

A single Trident program that combines three computational domains
in one STARK-verifiable trace:

```
Encrypted Input --> Private Linear --> Decrypt --> Dense Layer --> argmax --> Quantum Commit --> Bool
                       (FHE)                        (AI)                     (Quantum)
```

To our knowledge, no existing system composes all three domains in a
single proof. TFHE encrypts but can't prove. Cairo proves but can't
encrypt. Qiskit simulates but does neither. Trinity demonstrates that
FHE, neural inference, and quantum circuits can execute inside one
STARK trace with data-dependent coupling between phases.

## The Four Phases

### Phase 1: Privacy (LWE homomorphic encryption)

Real Learning With Errors encryption over the Goldilocks field
(p = 2^64 - 2^32 + 1). Ciphertext modulus q = p -- no impedance
mismatch between the FHE ring and the STARK field.

Each input is an LWE ciphertext (a, b) where b = <a, s> + m*delta + e.
The private linear layer computes homomorphic dot products:
for each neuron, multiply-accumulate encrypted inputs by plaintext
weights using `ct_scale` and `ct_add`.

Parameters: LWE dimension 8, delta = p/1024 (10-bit plaintext space).

The current bench uses LWE-style encryption with a `divine()` bridge
to plaintext (Phase 1b). The full production path would use RLWE
with programmable bootstrapping (PBS), where the ReLU lookup table
from Phase 2 serves as the PBS test polynomial -- eliminating the
decrypt step entirely. The LWE bench demonstrates the correct
algebraic structure; RLWE + PBS adds polynomial multiplication
via NTT (see Roadmap).

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

### Phase 2: Neural (dense layer -- matvec + bias + ReLU)

Full dense layer: `out = relu(W * x + b)`. Matrix-vector multiply
(NEURONS x NEURONS), bias addition, ReLU activation. Identical to
any neural network hidden layer, executing inside a STARK trace.

ReLU activation is implemented via a RAM-based lookup table
(`std.math.lut`). The table maps each input to its ReLU output:
values below p/2 are "positive" (kept), values at or above p/2 are
"negative" (zeroed). This is the Rosetta Stone stepping stone -- the
same table that serves as NN activation here would serve as the FHE
programmable bootstrapping test polynomial (see Rosetta Stone below).

The argmax comparison (for classification) uses `convert.split()` to
decompose field elements into (hi, lo) U32 pairs and compares the
high word against `HALF_P >> 32`.

### Phase 3: Quantum (2-qubit Bell pair commitment)

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
class   --> Phase 3 input            (quantum commit on computed class)
```

The class fed to quantum commitment is computed inside the pipeline
via `tensor.argmax()` on the dense layer output. The prover supplies
an `expected_class` hint, and the circuit asserts it matches the
computed argmax. This prevents shortcutting: you cannot substitute a
class without performing the actual inference, and you cannot remove
the quantum phase without breaking the pipeline's return value.

Every phase consumes the output of the previous phase. The STARK
trace cannot be "cut" into independent sub-traces.

## Parameters

### LWE_N = 8, INPUT_DIM = 8, NEURONS = 16

```
Phase 1  (Privacy):  private_linear -- 16 neurons * 8 inputs * LWE ops
Phase 1b (Decrypt):  16 neurons * lwe.decrypt (inner product + noise check)
Phase 2  (Neural):   matvec(16x16) + bias + lut_relu + argmax
Phase 3  (Quantum):  2-qubit Bell circuit
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

- **2-qubit Bell**: Entanglement + measurement. Architecturally proves
  quantum circuits compose with FHE and neural ops. More substantial
  than 1-qubit Deutsch (which collapses to a single comparison).

## Static Instruction Count

```
Module                       Tri   Hand   Ratio
std::trinity::inference      125     82   1.52x
```

Compiler generates 125 static instructions, hand baseline 82.
The 1.52x gap is an optimization target for the compiler.

Breakdown (hand): 24 decrypt_loop + 17 dense_layer + 3 quantum_commit
+ 38 trinity pipeline = 82 total. The dense_layer grew from 3 to 17
because it now makes three separate external calls (matvec, bias_add,
lut.apply) instead of one monolithic call.

## The Rosetta Stone

Trinity implements the first step of the Rosetta Stone unification
described in `docs/explanation/vision.md`. The key insight: a single
lookup table over F_p simultaneously serves as:

1. **STARK**: lookup argument (LogUp) for proof authentication
2. **Neural network**: activation function (ReLU, GELU, SiLU)
3. **FHE**: test polynomial for programmable bootstrapping (PBS)
4. **Crypto**: S-box for hash round function (Tip5)

Trinity's Phase 2 uses a RAM-based lookup table (`std.math.lut`)
for ReLU activation. The table is precomputed via `lut.build_relu`
and read via `lut.apply` -- O(1) per element. The STARK proof
authenticates all reads through RAM consistency.

**The same table IS the FHE PBS test polynomial.** In programmable
bootstrapping, the test polynomial encodes the target function
(ReLU) and is evaluated on encrypted data via blind rotation. In
Trinity, the table is read on decrypted data, but the mathematical
object is identical -- the RAM-based ReLU table can serve both roles.

This is the RAM-emulated version of the native LogUp lookup. When
Triton VM exposes user-defined lookup arguments, `std.math.lut`
becomes a thin wrapper and the cost drops to zero per read.

Current implementation: `std.math.lut` provides `build_relu`,
`read`, and `apply`. The table is shared across the pipeline via
a single `lut_addr` parameter.

## Roadmap

### Done: Lookup-Table Activation (Rosetta Stone step)

Phase 2 uses `std.math.lut` for ReLU activation via RAM-based lookup
table. Same table can serve as FHE PBS test polynomial.

### Next: Hash Commitment Phase (Poseidon2)

Add a Poseidon2 commitment phase between Neural and Quantum:
hash (weights_commit, key_commit, output) into a digest, feed the
digest into the quantum commitment. This binds the proof to specific
model parameters -- "this result was produced by THIS model with
THIS key", not abstractly "some model".

`std.crypto.poseidon2` already exists (991 lines, production-grade,
t=8 state, RF=8 full rounds, RP=22 partial rounds). Adding it turns
Trinity into a tetralogy: FHE + AI + Hash + Quantum.

### Future: NTT as Shared Workhorse

`std.private.poly` already has NTT/INTT over Goldilocks (exploiting
the field's 2^32 roots of unity). Moving from LWE to RLWE would
make NTT the shared primitive between FHE polynomial multiplication
and STARK proof generation. Relevant at dimension >= 256.

### Future: Benchmark Matrix

| Variant        | Change                                     | Metric                      |
|----------------|--------------------------------------------|-----------------------------|
| base           | LWE_N=8, NEURONS=16, 2-qubit              | control point               |
| +hash          | Poseidon2 commitment of model/key/output   | commitment cost share       |
| +lookup        | ReLU via lookup table                      | Rosetta Stone demo          |
| sweep          | LWE_N in {8,16}, NEURONS in {16,32}       | scaling trends              |
| transparent    | divine() off, all inputs public            | witness cost measurement    |

## File Structure

```
std/fhe/lwe.tri                              LWE encryption module
std/math/lut.tri                             RAM-based lookup table (Rosetta Stone)
std/nn/tensor.tri                            Neural primitives (matvec, argmax)
std/quantum/gates.tri                        Quantum gate library
std/trinity/inference.tri                    Trinity module
benches/std/trinity/inference.baseline.tasm  Hand-optimized TASM (82 instructions)
benches/std/trinity/inference.reference.rs   Rust ground truth (LWE_N=8, NEURONS=16)
```

## What Is Proven

The STARK proof covers every field operation in the trace:

- **LWE encryption**: inner products, ciphertext scaling and addition,
  homomorphic dot products over Goldilocks.
- **Decryption noise check**: |b - <a,s> - m*delta| < delta/2 for each
  divined plaintext. The prover supplies m via `divine()`, the circuit
  verifies the bound.
- **Dense layer**: matrix-vector multiply (16x16), bias addition, ReLU
  lookup table reads. All RAM accesses authenticated by the STARK RAM
  consistency argument.
- **Argmax**: field-native comparison of 16 outputs via `convert.split()`.
  The computed class is asserted equal to the prover's `expected_class`.
- **Quantum circuit**: 2-qubit Bell pair state preparation, conditional CZ,
  inverse Bell decoding, trace-out, probability comparison. Every complex
  arithmetic operation is in the trace.
- **Data flow**: each phase consumes the output of the previous phase.
  The trace cannot be cut into independent sub-traces.

What the proof does NOT cover: the choice of weights, the choice of
secret key, or the semantic meaning of the classification. The proof
says "this computation was performed correctly on these inputs," not
"these inputs are meaningful."

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
RLWE + PBS, add more qubits. The algebraic structure does not change.

## Why This Matters

Trinity proves the 128K milestone:
"Small model inference compiles to provable Trident."

- Real LWE encryption, not polynomial approximation
- Data-dependent phases -- class computed from AI output, not injected
- Cross-domain composition (std.fhe, std.nn, std.quantum in one program)
- Everything verifiable in a single STARK proof
- Each revolution contributes meaningfully to the computation
- Stepping stone toward the Rosetta Stone unification

`trident build std/trinity/inference.tri` -> `trisha prove` -> `trisha verify`.
