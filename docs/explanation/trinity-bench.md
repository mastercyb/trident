# Trinity: Provable Private Neural Inference with Quantum Commitment

## What Trinity Is

A single Trident program that combines three computational domains
in one STARK-verifiable trace:

```
Encrypted Input --> Private Linear --> Decrypt --> Dense Layer --> argmax --> Quantum Commit --> Bool
                       (FHE)                        (AI)                     (Quantum)
```

No other system can do this. TFHE encrypts but can't prove.
Cairo proves but can't encrypt. Qiskit simulates but does neither.
Trinity does all three in one proof.

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

### Phase 1b: Decrypt (bridge to plaintext)

Each encrypted output is decrypted via `io.divine()` -- the prover
supplies the candidate plaintext m, the circuit computes the noise
|b - <a,s> - m*delta| and verifies it falls within the bound delta/2.
The STARK proof covers the noise check.

This is the witness injection mechanism: `divine()` is Trident's
single interface for non-deterministic prover input. The same mechanism
serves FHE decryption, neural weight injection, and quantum measurement
outcomes. The proof constrains the divined value -- unconstrained
divine calls are flagged by `trident audit`.

### Phase 2: Neural (dense layer -- matvec + bias + ReLU)

Full dense layer: `out = relu(W * x + b)`. Matrix-vector multiply
(NEURONS x NEURONS), bias addition, ReLU activation. Identical to
any neural network hidden layer, executing inside a STARK trace.

ReLU in the Goldilocks field: values below p/2 are "positive" (kept),
values at or above p/2 are "negative" (zeroed). This is the canonical
field-native activation -- no quantization, no approximation. The
comparison uses `convert.split()` to decompose into (hi, lo) U32 pairs
and compares the high word against `HALF_P >> 32`.

Future direction: ReLU via lookup table (see Rosetta Stone below).

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

Measurement is deterministic (provable comparison of outcome
probabilities), not probabilistic. This is explicitly documented in
`std.quantum.gates.measure_deterministic`.

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
Phase 2  (Neural):   matvec(16x16) + bias + relu + argmax
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
std::trinity::inference      120     67   1.79x
```

Compiler generates 120 static instructions, hand baseline 67.
The 1.79x gap is an optimization target for the compiler.

Breakdown (hand): 24 decrypt_loop + 3 dense_layer + 3 quantum_commit
+ 37 trinity pipeline = 67 total.

## The Rosetta Stone

Trinity is a stepping stone toward the Rosetta Stone unification
described in `docs/explanation/vision.md`. The key insight: a single
lookup table over F_p simultaneously serves as:

1. **STARK**: lookup argument (LogUp) for proof authentication
2. **Neural network**: activation function (ReLU, GELU, SiLU)
3. **FHE**: test polynomial for programmable bootstrapping (PBS)
4. **Crypto**: S-box for hash round function (Tip5)

Trinity currently uses field-comparison ReLU (Phase 2) and explicit
LWE arithmetic (Phase 1). The planned evolution:

- **ReLU via lookup table** (`std.nn.activation.relu`): same table
  entry serves as NN activation and STARK lookup authentication.
- **PBS via lookup table**: blind rotation evaluates the same table
  on encrypted data. The FHE test polynomial IS the activation table.
- **Tip5 S-box**: the hash function's nonlinearity uses the same
  table mechanism. Adding a Poseidon2/Tip5 commitment phase (see
  Roadmap) would make all four roles visible in one program.

When lookup-table activation lands, Trinity demonstrates the Rosetta
Stone identity directly: one table, three readers, one proof.

## Roadmap

### Next: Hash Commitment Phase (Poseidon2)

Add a Poseidon2 commitment phase between Neural and Quantum:
hash (weights_commit, key_commit, output) into a digest, feed the
digest into the quantum commitment. This binds the proof to specific
model parameters -- "this result was produced by THIS model with
THIS key", not abstractly "some model".

`std.crypto.poseidon2` already exists (991 lines, production-grade,
t=8 state, RF=8 full rounds, RP=22 partial rounds). Adding it turns
Trinity into a tetralogy: FHE + AI + Hash + Quantum.

### Future: Lookup Table Unification

Replace field-comparison ReLU with lookup-table ReLU. Use the same
table for FHE PBS. Demonstrate the Rosetta Stone identity in code.

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
std/nn/tensor.tri                            Neural primitives (dense, argmax)
std/quantum/gates.tri                        Quantum gate library
std/trinity/inference.tri                    Trinity module
benches/std/trinity/inference.baseline.tasm  Hand-optimized TASM (67 instructions)
benches/std/trinity/inference.reference.rs   Rust ground truth (LWE_N=8, NEURONS=16)
```

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
