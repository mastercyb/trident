# Roadmap

Trident uses kelvin versioning. Versions count down toward absolute
zero. At 0K a layer is frozen forever — no further changes. Lower
layers freeze before higher layers.

## Now → First Release

Three targets before first release:

1. Self-hosting — compiler compiles itself in Trident
2. Atlas — on-chain package registry live
3. Revolution demos — small proven inference, FHE circuit
   compilation, quantum circuit simulation

First release ships when all three land. The compiler that compiles
itself, the registry that connects developers, and proof that the
three frontiers are reachable.

## The Stack

```
Layer           Current   First Release
───────────────────────────────────────
vm spec          32K         16K
language         64K         32K
TIR              64K         32K
compiler        128K         32K
std.*           128K         64K
os.*            128K         64K
tooling          64K         32K
```

## vm spec — 32K → 0K

```
  32K  Intrinsic set stable (no new vm.* builtins)
  16K  Triton backend emission proven correct
   8K  3+ backends passing conformance suite
   4K  TargetConfig / StackBackend / CostModel traits frozen
   2K  Every intrinsic has a formal cost proof
   0K  vm.* namespace sealed forever
```

## language — 64K → 0K

```
  64K  Indexed assignment (arr[i] = val, s.field = val)
  32K  Trait-like interfaces
  16K  Grammar finalized — no new syntax forms
   8K  Type system finalized — no new type rules
   4K  Every language feature has a formal soundness proof
   2K  Formal semantics published
   0K  Syntax and semantics sealed forever
```

## TIR — 64K → 0K

```
  64K  TIROp set stable (no new variants without language change)
  32K  Per-function benchmarks < 1.2x vs hand-written baselines
  16K  Cost-driven optimization passes land
   8K  Stack effect contracts proven for all ops
   4K  Every lowering path formally verified
   2K  TIR-to-target roundtrip proven equivalent
   0K  IR spec sealed forever
```

## compiler — 128K → 0K

```
 128K  Lexer + parser rewritten in .tri
  64K  Type checker rewritten in .tri
  32K  Pipeline fully in Trident — compiler compiles itself
  16K  Each compilation produces a proof certificate (self-proving)
   8K  Incremental proving (per-module proofs, composed)
   4K  Proof verified on-chain, src/ deleted
   2K  Compiler proves its own correctness
   0K  Compiler sealed forever
```

Only the pipeline (lexer, parser, typecheck, TIR, lowering) needs to
be provable. LSP, CLI, pretty-printing run outside the proof.

## std.* — 128K → 0K

```
 128K  std.token, std.coin, std.card shipped
  64K  std.skill.* (23 skills) shipped
  32K  #[requires]/#[ensures] contracts on all public functions
  16K  std.crypto.* formally verified (poseidon, merkle, ecdsa)
   8K  All modules verified — every public function proven
   4K  Public APIs frozen, no new exports
   2K  Cross-module composition proofs complete
   0K  Standard library sealed forever
```

## os.* — 128K → 0K

```
 128K  os.neptune.* complete, Atlas on-chain registry live
  64K  3+ OS namespaces operational
  32K  Per-OS namespace governance established
  16K  os.neptune.* frozen
   8K  All active OS namespaces frozen
   4K  Cross-OS portability proven (same .tri runs on any OS)
   2K  Every OS binding formally verified
   0K  OS layer sealed forever
```

## tooling — 64K → 0K

```
  64K  Web playground (compile .tri in browser)
  32K  ZK coprocessor integrations (Axiom, Brevis, Herodotus)
  16K  GPU-accelerated proving
   8K  FPGA proving backend
   4K  Tool chain self-hosts (trident builds trident tooling)
   2K  ASIC proving backend
   0K  Tooling sealed forever
```

---

# The Three Revolutions

The foundation exists to enable these. No other language can prove
computation across all three.

## AI — verifiable intelligence

Disrupts: cloud AI, model marketplaces, MLOps, alignment industry.
Every inference provable. Every model weight auditable.

```
 256K  Tensor operations in TIR (matmul, conv, attention)
 128K  Small model inference compiles to provable Trident
  64K  On-chain model registry — verified accuracy, no trust
  32K  Proven training: gradient computation inside proof
  16K  GPT-class proven inference (billion+ parameters)
   8K  Federated learning with proven aggregation
   4K  Autonomous agents that prove every decision they make
   2K  Any model, any size — proving scales linearly
   0K  Intelligence without trust
```

## Privacy — sovereign computation

Disrupts: cloud computing, banking, healthcare, surveillance, AdTech.
Compute on encrypted data. No one sees inputs.

```
 256K  FHE primitives in std.crypto (TFHE, BGV, CKKS)
 128K  Trident programs compile to FHE circuits
  64K  Encrypted smart contracts — execute without revealing state
  32K  FHE + ZK: prove correctness of encrypted computation
  16K  Multi-party FHE: N parties compute, none sees others' data
   8K  Practical performance: <10x overhead vs plaintext
   4K  Hardware-accelerated FHE (FPGA/ASIC)
   2K  Any Trident program runs encrypted by default
   0K  Privacy without permission
```

## Quantum — post-classical computation

Disrupts: classical computing, cryptography, drug discovery,
materials science, optimization, finance.

```
 256K  Quantum gate set in TIR (Hadamard, CNOT, Toffoli, measure)
 128K  Quantum circuit simulation backend
  64K  Hybrid programs: classical control + quantum subroutines
  32K  Quantum error correction in std.quantum
  16K  Real hardware backends (IBM, Google, IonQ)
   8K  Quantum advantage: problems classical can't touch
   4K  Post-quantum crypto native (lattice-based std.crypto)
   2K  Quantum-classical proofs: STARK verifies quantum computation
   0K  Computation without limits
```

---

# The Convergence

Read vertically by component for engineering. Read horizontally
by temperature to see how the pieces fuse.

## 256K — primitives land

Tensor ops, FHE schemes, and quantum gates enter the IR. The three
revolutions have raw building blocks but no integration.

## 128K — the machine assembles

Compiler pipeline rewritten in .tri. std.* ships core modules.
os.neptune.* complete. Atlas live on-chain. Small model inference
compiles. FHE circuits compile. Quantum simulation works. Each
revolution can demo but not ship.

## 64K — proof of concept

Language complete. TIR stable. Skills shipped. 3+ OS namespaces.
On-chain model registry. Encrypted smart contracts. Hybrid
classical-quantum programs. Web playground. Each revolution has
a working product.

## 32K — first release

Compiler compiles itself. Contracts on all public functions.
Proven training. FHE + ZK composed. Quantum error correction.
ZK coprocessor integrations. The self-hosting compiler ships
with Atlas and three revolution demos.

## 16K — the industries fall

Self-proving compiler. Verified crypto. GPU-accelerated proving.
GPT-class proven inference kills cloud AI. Multi-party FHE kills
cloud computing. Real quantum hardware backends. No incumbent
is safe.

## 8K — proven everything

Incremental proving. Every module verified. FPGA proving.
Federated learning with proofs. FHE at <10x overhead. Quantum
advantage on real problems. The gap between Trident and
everything else is unbridgeable.

## 4K — hardware era

Proof on-chain, src/ deleted. Frozen APIs. Tool chain self-hosts.
Autonomous agents prove every decision. FHE on FPGA/ASIC.
Post-quantum crypto native. Software is done — hardware takes over.

## 2K — last mile

Compiler proves its own correctness. Composition proofs. Cross-OS
portability. ASIC proving. Any model at any scale. Any program
encrypted by default. STARK verifies quantum computation.

## 0K

Frozen foundation. Proven compiler. Verified intelligence.
Sovereign computation. Post-classical capability.

Intelligence without trust. Privacy without permission.
Computation without limits.

Write once, prove anywhere.
