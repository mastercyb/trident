# Trinity: Provable Private Neural Inference with Quantum Commitment

## What Trinity Is

A single Trident program that combines three computational domains
in one STARK-verifiable trace:

```
Encrypted Input ──> Private Linear Layer ──> Neural Activation ──> Quantum Commitment ──> Bool
                        (FHE)                    (AI)                  (Quantum)
```

No other system can do this. TFHE encrypts but can't prove.
Cairo proves but can't encrypt. Qiskit simulates but does neither.
Trinity does all three in one proof.

## The Three Phases

### Phase 1: Privacy (FHE-style polynomial arithmetic)

Input is an encrypted polynomial of dimension POLY_N. Each neuron
computes a dot product via polynomial pointwise multiplication +
Horner evaluation. This is the same structure as RLWE-based FHE
schemes (TFHE, BFV, CKKS) — the polynomial ring acts as the
ciphertext space.

Cost per neuron: `(23 + 18) * POLY_N = 41 * POLY_N` dynamic ops.
Total: `NEURONS * 41 * POLY_N`.

### Phase 2: Neural (ReLU activation + bias)

Standard dense layer post-processing. Bias addition then ReLU
(max(0, x) via field split). Identical to any neural network
activation layer, but executing inside a STARK trace.

Cost: `NEURONS * 43` dynamic ops.

### Phase 3: Quantum (Deutsch oracle commitment)

Deutsch's algorithm — the first quantum speedup ever proven.
Single-qubit circuit: H → conditional Z → H → measure.

class=0 (constant oracle): returns true.
class>0 (balanced oracle): returns false.

The prover supplies `expected_class` from argmax of the activated
layer. The quantum circuit commits to this classification. If the
prover lies about the class, the proof is invalid.

Cost: 3 dynamic ops (reduces algebraically to `class == 0`).

## Parameter Trade-offs

### Cost Model

Phase 1 dominates. Everything else is noise.

```
Total dynamic ops ≈ NEURONS * 41 * POLY_N
```

### Concrete Numbers

POLY_N  NEURONS  Phase1     Phase2  Total    Trace   Prove   Proof
------  -------  ---------  ------  -------  ------  ------  -----
4       4        656        172     ~830     2^10    <1s     ~1MB
8       8        2,624      344     ~3K      2^12    ~1s     ~2MB
16      16       10,496     688     ~11K     2^14    ~3s     ~5MB
32      16       20,992     688     ~22K     2^15    ~5s     ~10MB
64      16       41,984     688     ~43K     2^16    ~8s     ~15MB
64      64       167,936    2,752   ~171K    2^18    ~30s    ~50MB

Trace = padded height (power of 2, determines proof complexity).
Prove = estimated GPU time via trisha (Metal/wgpu).
Proof = estimated STARK proof size.

## Recommended Parameters

### POLY_N = 16, NEURONS = 16

Rationale:

1. **Credible FHE.** 16-dimensional polynomial ring is the smallest
   size where polynomial arithmetic resembles real RLWE. Degree-4
   is a toy. Degree-64 is realistic but expensive. Degree-16 is
   the sweet spot for a demo that isn't laughable but isn't slow.

2. **Real neural layer.** 16 neurons is a genuine hidden layer.
   Small enough to prove, large enough to learn non-trivial
   functions. Standard in compact on-device models.

3. **Provable in CI.** ~11K dynamic ops, ~3s prove time, ~5MB
   proof. Fits in continuous integration without timeouts.

4. **Phase ratio is honest.** 94% privacy, 6% neural, <1% quantum.
   Privacy dominates, which is correct — FHE is the expensive part.
   The quantum commitment is cheap because Deutsch's algorithm is
   genuinely efficient. Hiding this ratio would be dishonest.

5. **Scales up.** The code is parametric. Change two constants to
   go to 64-dim for a pitch deck or 256-dim for a stress test.
   No recompilation of .tri needed.

### What the parameters mean physically

- **POLY_N = 16**: Ciphertext lives in Z_p[x]/(x^16 + 1).
  Each encrypted value is 16 field elements. Each neuron performs
  16 multiplications + 16-step Horner evaluation = 656 ops.

- **NEURONS = 16**: Hidden layer with 16 units. Weight matrix
  is 16 x 16 = 256 field elements. Total private computation:
  16 neurons * 656 ops = 10,496 ops.

- **Deutsch oracle**: 1 qubit, 3 gates (H, conditional Z, H).
  Proves quantum circuit simulation works inside the STARK.
  The point is not computational power — it's architectural proof
  that quantum circuits compose with FHE and neural ops.

## File Structure

```
std/trinity/inference.tri                    The Trident module (parametric)
benches/std/trinity/inference.baseline.tasm  Hand-optimized TASM (71 static instructions)
benches/std/trinity/inference.reference.rs   Rust ground truth (POLY_N, NEURONS constants)
```

Static instruction count (baseline): 71.
Compiler output: 61 (0.86x — compiler wins).
Neural optimizer: 1067 (garbage, expected at this training stage).

## Why This Matters

Trinity is the proof of concept for the 128K milestone:
"Small model inference compiles to provable Trident."

It demonstrates:
- Cross-domain composition works (3 std.* modules in one program)
- Privacy primitives integrate with neural ops
- Quantum circuits compose naturally
- Everything verifiable in a single STARK proof
- Compiler handles the full pipeline (and beats hand baseline)

When someone asks "can Trident really do FHE + AI + Quantum?",
the answer is `trident build std/trinity/inference.tri` followed
by `trisha prove` and `trisha verify`. End of discussion.
