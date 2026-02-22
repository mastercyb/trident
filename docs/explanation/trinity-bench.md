# Trinity: Provable Private Neural Inference with Quantum Commitment

## What Trinity Is

A single Trident program that combines three computational domains
in one STARK-verifiable trace:

```
Encrypted Input ──> Private Linear ──> Decrypt ──> Dense Layer ──> Quantum Commitment ──> Bool
                       (FHE)                         (AI)              (Quantum)
```

No other system can do this. TFHE encrypts but can't prove.
Cairo proves but can't encrypt. Qiskit simulates but does neither.
Trinity does all three in one proof.

## The Four Phases

### Phase 1: Privacy (LWE homomorphic encryption)

Real Learning With Errors encryption over the Goldilocks field
(p = 2^64 - 2^32 + 1). Ciphertext modulus q = p — no impedance
mismatch between the FHE ring and the STARK field.

Each input is an LWE ciphertext (a, b) where b = <a, s> + m*delta + e.
The private linear layer computes homomorphic dot products:
for each neuron, multiply-accumulate encrypted inputs by plaintext
weights using `ct_scale` and `ct_add`.

Parameters: LWE dimension 8, delta = p/1024 (10-bit plaintext space).

### Phase 1b: Decrypt (bridge to plaintext)

Each encrypted output is decrypted via `io.divine()` — the prover
supplies the candidate plaintext m, the circuit computes the noise
|b - <a,s> - m*delta| and verifies it falls within the bound delta/2.
The STARK proof covers the noise check.

### Phase 2: Neural (dense layer — matvec + bias + ReLU)

Full dense layer: `out = relu(W * x + b)`. Matrix-vector multiply
(NEURONS x NEURONS), bias addition, ReLU activation. Identical to
any neural network hidden layer, executing inside a STARK trace.

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
every gate operation — init, Hadamard, tensor product, CNOT, CZ,
complex arithmetic, norm squared, measurement comparison. The STARK
proof covers the full 2-qubit circuit.

## Parameters

### LWE_N = 8, INPUT_DIM = 8, NEURONS = 16

```
Phase 1  (Privacy):  private_linear — 16 neurons * 8 inputs * LWE ops
Phase 1b (Decrypt):  16 neurons * lwe.decrypt (inner product + noise check)
Phase 2  (Neural):   matvec(16x16) + bias + relu
Phase 3  (Quantum):  2-qubit Bell circuit
```

### Why these numbers

- **LWE_N = 8**: LWE dimension. Ciphertexts are 9 field elements
  (8-element vector a plus scalar b). Lightweight but structurally
  real — same operations as production TFHE, just smaller dimension.

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
std::trinity::inference      112     61   1.84x
```

Compiler generates 112 static instructions, hand baseline 61.
The 1.84x gap is an optimization target for the compiler.

Breakdown (hand): 24 decrypt_loop + 3 dense_layer + 3 quantum_commit
+ 31 trinity pipeline = 61 total.

## File Structure

```
std/fhe/lwe.tri                              LWE encryption module
std/trinity/inference.tri                    Trident module (parametric)
benches/std/trinity/inference.baseline.tasm  Hand-optimized TASM (61 instructions)
benches/std/trinity/inference.reference.rs   Rust ground truth (LWE_N=8, NEURONS=16)
```

## Why This Matters

Trinity proves the 128K milestone:
"Small model inference compiles to provable Trident."

- Real LWE encryption, not polynomial approximation
- Cross-domain composition (std.fhe, std.nn, std.quantum in one program)
- Privacy primitives integrate with neural ops
- Quantum circuits compose naturally
- Everything verifiable in a single STARK proof
- Each revolution contributes meaningfully to the computation

`trident build std/trinity/inference.tri` -> `trisha prove` -> `trisha verify`.
