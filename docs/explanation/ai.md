# Trident and Verifiable AI

## Why zkML Should Start from Prime Fields, Not from ONNX

Every major zkML framework — EZKL, DeepProve, JOLT Atlas, zkPyTorch —
follows the same pipeline. Take a neural network trained in PyTorch.
Export to ONNX. Convert floating-point weights into fixed-point integers.
Translate those integers into arithmetic constraints over a finite field.
Generate a zero-knowledge proof.

This pipeline has a fundamental flaw: it starts in the wrong
representation and spends enormous effort converting to the right one.
Quantization — the conversion from float32 to field elements — is where
everything breaks. Accuracy degrades across layers. Operators trivial in
floating-point (softmax, LayerNorm, GELU) become nightmares in field
arithmetic.

And most of these frameworks use SNARKs — proof systems built on
elliptic curve pairings that will be broken by quantum computers.

Trident offers a different path. Not a better converter from floats to
fields. A language where computation is born in the field.

---

## 1. The Quantization Problem — Eliminated

When EZKL processes a neural network, float32 weights are multiplied by
a scaling factor and rounded. Every multiplication produces results
scaled quadratically, requiring rescaling — more field operations, more
constraints, more proof overhead. Nonlinear activations are catastrophic:
ReLU requires comparison (field elements are not ordered), softmax
requires exponentiation and division over all elements. A single softmax
layer over 512 elements generates tens of thousands of constraints.

Trident's approach: there is no quantization because there were never
any floats.

**Weights are field elements from the start.** Field multiplication in
F_p produces a field element. No rescaling. a * b mod p is a single gate
in the arithmetic circuit.

**Activations become lookup tables.** The same mechanism Triton VM uses
for its Tip5 hash function's S-box. The lookup argument authenticates
that the activation was applied correctly — zero additional constraints.
ReLU, GELU, SiLU all become single lookup operations.

**Division is native.** In F_p, every nonzero element has a
multiplicative inverse: a / b = a * b^(p-2) mod p. Softmax becomes:
exponentiate each element (via lookup), sum, divide each by the sum.

**There is no quantization error** because there is no quantization.

The difference is categorical. EZKL adds constraints to handle the gap
between floats and fields. Trident has no gap.

---

## 2. The Lookup Argument: Where Hash Functions Meet Neural Networks

Triton VM's STARK prover uses a cryptographic hash function called Tip5.
Its internal S-box maps field elements to field elements, implemented as
a lookup table authenticated by a lookup argument in the STARK proof.
The proof cost is essentially constant regardless of the function's
complexity.

Neural network activations are also nonlinear maps from field elements
to field elements. ReLU: f(x) = max(0, x). GELU: f(x) = x * Phi(x).
Each can be precomputed as a lookup table over F_p.

The STARK mechanism that authenticates Tip5's S-box is identical to the
mechanism that authenticates a ReLU activation. One lookup. Done.

Cryptographic S-boxes and neural network activations serve the same
mathematical purpose: injecting nonlinearity into an otherwise linear
system. Cryptographers need nonlinearity to resist algebraic attacks.
Neural network designers need it to learn nontrivial functions. The
proof mechanism is the same.

Trident inherits this for free. Any function expressible as a lookup
table over F_p becomes a zero-overhead provable activation. The hash
function's security guarantees (maximal algebraic degree, permutation)
translate to desirable neural network properties (high expressiveness,
no dead zones, information-preserving).

---

## 3. Field-Native Neural Networks

A matrix multiplication — the core operation of every neural network —
is a sequence of multiply-accumulate operations over field elements. In
Trident, this is native:

```
fn matmul(a: &[Field], b: &[Field], rows: u32, inner: u32, cols: u32) -> Vec<Field> {
    let mut result = vec![Field::zero(); rows * cols];
    for i in 0..rows {
        for j in 0..cols {
            for k in 0..inner {
                result[i * cols + j] += a[i * inner + k] * b[k * cols + j];
            }
        }
    }
    result
}
```

This compiles to Triton VM, executes, and produces a STARK proof of
correct execution. The `std.nn` library provides:

- **Linear layers** — matrix multiply-accumulate, native field ops
- **Convolutional layers** — sliding window dot products, bounded loops
- **Attention mechanisms** — dot products, softmax via field inversion,
  value aggregation
- **Lookup-table activations** — ReLU, GELU, SiLU via Triton's lookup
  argument
- **Normalization** — LayerNorm/BatchNorm via field addition, inversion
- **Embedding layers** — token ID to weight vector, reusing the lookup
  mechanism
- **Loss functions** — cross-entropy, MSE, computable in field arithmetic

Each function is pure Trident code. The entire neural network compiles
to a single arithmetic circuit over F_p, proven as a unit.

---

## 4. The `divine()` Primitive for AI

Trident's `divine()` tells the prover: inject a value that satisfies
subsequent constraints. For neural networks, this serves multiple
purposes.

**Private weights.** The model owner provides weights via `divine()`.
The STARK proof verifies inference without revealing the weights.
Privacy-preserving inference as a natural consequence of proof
architecture:

```
fn private_inference(input: [Field; N]) -> [Field; M] {
    let weights = divine()        // private: model owner provides
    let bias = divine()           // private: model owner provides
    let output = linear(input, weights, bias)
    let activated = relu_lookup(output)
    activated                     // public: inference result
}
```

**Optimization search.** For AI agents finding optimal actions in a
large space, `divine()` injects the solution. Constraints verify
optimality. The proof guarantees validity without revealing the search
process:

```
fn optimal_action(state: [Field; S], constraints: &RiskParams) -> Field {
    let action = divine()         // prover searches for optimal action
    assert(satisfies_constraints(action, constraints))
    assert(is_local_optimum(state, action))
    action
}
```

**Adversarial robustness.** An adversary injects examples via
`divine()`. Constraints check misclassification. The proof either
demonstrates a vulnerability or certifies robustness within the bounded
search space.

In the quantum compilation target, every `divine()` becomes a Grover
oracle query — quadratic speedup on witness search with zero code
changes.

---

## 5. The ONNX Bridge

Trident doesn't need to replace PyTorch. It consumes its output.

**Import:** PyTorch -> ONNX export -> Trident transpiler. Each ONNX
operator maps to a `std.nn` function. Float32 weights are quantized to
F_p once at import time, not at proof time. The critical difference from
EZKL: Trident produces readable `.tri` source code that developers can
inspect, modify, and optimize — not a monolithic circuit.

**Export:** Extract the `std.nn` computational graph, convert F_p weights
back to float32, generate ONNX. Trident-native models can run inference
in PyTorch, TensorFlow, or any ONNX runtime — useful for development
and environments where ZK proof is not needed.

---

## 6. Training in the Field

Current zkML focuses on proving inference. Training remains in float32.
This creates a trust gap: you can prove inference was correct, but not
that training was correct.

Trident enables provable training. The entire loop — forward pass, loss
computation, backpropagation, weight update — is field arithmetic.

**Gradient computation:** Backpropagation is chain-rule multiplication
of Jacobians — matrix operations over the same field. Gradient of a
lookup-table activation is another lookup (the derivative table).

**Optimizer:** SGD is w <- w - eta * g — field subtraction and
multiplication. Adam requires running averages and square root (field
exponentiation). All native.

**Provable claims:**

- "This model was trained on this dataset for N epochs" — proven by STARK
- "This model achieves accuracy above T on test set D" — proven by STARK
- "Weights have not been modified since training" — proven by hash commit

This enables a model marketplace where sellers prove training claims
without revealing weights (zero-knowledge), buyers verify proofs, and
the entire transaction settles on-chain.

---

## 7. Post-Quantum Security

Almost every deployed zkML system uses proof systems that will be broken
by quantum computers. EZKL uses Halo2 (discrete log). Modulus Labs uses
custom SNARKs (elliptic curve). Circom and Noir use Groth16/Plonk over
BN254 (directly broken by Shor's algorithm). Giza, using StarkWare's
STWO prover, is the exception.

Trident is STARK-native. Every proof relies exclusively on hash
functions (Tip5) and polynomial commitments over F_p — no elliptic
curves, no pairings, no discrete log. Security reduces to hash
collision resistance, where quantum computers achieve only Grover's
square-root speedup — manageable by doubling hash output size.

Every Trident neural network proof is automatically post-quantum secure.
An AI agent's decision proof generated today will remain verifiable
after quantum computers have matured. A model marketplace built on
Trident proofs will not need to migrate proof systems.

---

## 8. Competitive Landscape

**vs. EZKL (Halo2, ONNX -> zkSNARK).** The most mature general-purpose
zkML framework. Accepts any ONNX model. Proof sizes are 15x larger than
alternatives. Proving time runs to minutes or hours for medium models.
Not post-quantum. Trident eliminates quantization overhead and provides
STARK proofs natively.

**vs. DeepProve (GKR-based).** The fastest zkML prover: 50-150x speedup
over EZKL through GKR interactive proofs. Still starts from ONNX, still
quantizes. Post-quantum resistance under analysis. DeepProve's GKR
approach could integrate as an alternative backend for Trident's IR.

**vs. Giza / Cairo (STARK-based, Starknet).** Closest to Trident: STARK
proofs, field-native arithmetic. Goldilocks (2^64 - 2^32 + 1) is faster
than Stark252 (2^251 + 17 * 2^192 + 1) on 64-bit hardware. Trident's
`divine()` maps to quantum oracles — a connection Cairo lacks. Trident's
three-level architecture enables cross-chain deployment.

**vs. Ritual (EVM++ AI infrastructure).** An orchestration layer, not a
proof system. Delegates proving to external systems. A Trident neural
network could run as a Ritual sidecar, providing STARK-verified
inference to Ritual's network.

| Framework | Proof System | Post-Quantum | Native Field | Quantization |
|---|---|---|---|---|
| **Trident** | STARK | Yes | F_p (Goldilocks) | None |
| EZKL | Halo2 (SNARK) | No | Via conversion | Required |
| DeepProve | GKR | Under analysis | Via conversion | Required |
| Giza | STARK (STWO) | Yes | F_p (Stark252) | Minimal |
| Ritual | Delegated | Depends | N/A | Depends |

---

## 9. Roadmap

**Immediate:** Implement `std.nn` core — linear layers, lookup-table
activations, basic loss functions. ONNX import for simple architectures.
Demonstrate: import a PyTorch MNIST classifier, prove inference on
Triton VM.

**Near-term:** Extend to attention, embedding, normalization. ONNX
import for transformers. Provable training loop. Benchmarks against
EZKL and DeepProve.

**Medium-term:** AI agent framework with on-chain deployment. Model
marketplace contracts. Integration with Ritual and Inference Labs.
Quantum compilation backend for `std.nn` operations.

---

## See Also

- [Quantum](quantum.md) — Prime fields, quantum advantage, and the
  convergence thesis
- [Privacy](privacy.md) — Zero-knowledge as structural property
- [How STARK Proofs Work](stark-proofs.md) — The proof system behind
  Trident's post-quantum security
- [Vision](vision.md) — Why Trident exists
- [Standard Library Reference](../../reference/stdlib.md) — Full `std.*`
  module inventory
- [Language Reference](../../reference/language.md) — Types, operators,
  builtins, grammar

---

*mastercyb, 2025. Cyber Valley Research.*
