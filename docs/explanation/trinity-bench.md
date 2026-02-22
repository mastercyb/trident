# Trinity: Provable Private Neural Inference with Quantum Commitment

## What Trinity Is

A single Trident program that combines three computational domains
in one STARK-verifiable trace:

```
Encrypted Input ──> Private Linear ──> Dense Layer ──> Quantum Commitment ──> Bool
                       (FHE)             (AI)              (Quantum)
```

No other system can do this. TFHE encrypts but can't prove.
Cairo proves but can't encrypt. Qiskit simulates but does neither.
Trinity does all three in one proof.

## The Three Phases

### Phase 1: Privacy (FHE-style polynomial arithmetic)

Input is an encrypted polynomial of dimension POLY_N. Each neuron
computes a dot product via polynomial pointwise multiplication +
Horner evaluation. Same structure as RLWE-based FHE schemes
(TFHE, BFV, CKKS) — the polynomial ring is the ciphertext space.

Cost per neuron: `(23 + 18) * POLY_N = 41 * POLY_N` dynamic ops.
Total: `NEURONS * 41 * POLY_N`.

### Phase 2: Neural (dense layer — matvec + bias + ReLU)

Full dense layer: `out = relu(W * x + b)`. Matrix-vector multiply
(NEURONS x NEURONS), bias addition, ReLU activation. Identical to
any neural network hidden layer, executing inside a STARK trace.

Cost: `NEURONS^2 * ~14 + NEURONS * 43` dynamic ops (~4K for 16 neurons).

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

### POLY_N = 8, NEURONS = 16

```
Phase 1 (Privacy):  16 neurons * 41 * 8 = ~5,250 ops    (45%)
Phase 2 (Neural):   matvec(16x16) + bias + relu = ~4,300 ops  (37%)
Phase 3 (Quantum):  2-qubit Bell circuit = ~2,000 ops   (17%)
Total: ~11,500 dynamic ops
```

Balance: **45 / 37 / 17**. Each phase contributes meaningfully.
Privacy dominates (correct — FHE is expensive), but Neural and
Quantum are not decoration.

### Why these numbers

- **POLY_N = 8**: Ciphertext in Z_p[x]/(x^8+1). 8-dim polynomials
  appear in lightweight FHE (e.g. PASTA, Masta). Not a toy, not huge.

- **NEURONS = 16**: Real hidden layer. 16x16 weight matrix =
  256 field elements. Standard in compact on-device models.

- **2-qubit Bell**: Entanglement + measurement. Architecturally proves
  quantum circuits compose with FHE and neural ops. More substantial
  than 1-qubit Deutsch (which collapses to a single comparison).

### Proving cost

```
Dynamic ops: ~11,500
Trace height: ~2^14 (16K, padded)
Proof size:   ~5 MB
Prove time:   ~3 sec (GPU via trisha)
Verify time:  ~0.5 sec
```

Fits CI. Scales up by changing constants.

## Static Instruction Count

```
Module                       Tri   Hand   Ratio
std::trinity::inference      109     67   1.63x
```

Compiler generates 109 static instructions, hand baseline 67.
The 1.63x gap is an optimization target for the compiler.

## File Structure

```
std/trinity/inference.tri                    Trident module (parametric)
benches/std/trinity/inference.baseline.tasm  Hand-optimized TASM (67 instructions)
benches/std/trinity/inference.reference.rs   Rust ground truth (POLY_N=8, NEURONS=16)
```

## Why This Matters

Trinity proves the 128K milestone:
"Small model inference compiles to provable Trident."

- Cross-domain composition (3 std.* modules in one program)
- Privacy primitives integrate with neural ops
- Quantum circuits compose naturally
- Everything verifiable in a single STARK proof
- Each revolution contributes meaningfully to the computation

`trident build std/trinity/inference.tri` → `trisha prove` → `trisha verify`.
