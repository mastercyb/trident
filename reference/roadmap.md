# Roadmap

Trident uses kelvin versioning. Versions count down toward absolute
zero. At 0K the layer is frozen forever — no further changes. Lower
layers freeze before higher layers. The endgame is a frozen foundation
for provable computation.

## The Stack

```
Layer           Current
───────────────────────
vm spec          32K
language         64K
TIR              64K
compiler        128K
std.*           128K
os.*            128K
tooling          64K
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
  32K  TIR builder rewritten in .tri (pipeline fully in Trident)
  16K  Compiler compiles itself (self-hosting)
   8K  Each compilation produces a proof certificate (self-proving)
   4K  Incremental proving (per-module proofs, composed)
   2K  Proof verified on-chain, src/ deleted
   0K  Compiler proven correct forever
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
 128K  os.neptune.* complete (reference OS implementation)
  64K  Atlas on-chain registry live (TSP-2 Cards)
  32K  3+ OS namespaces operational
  16K  Per-OS namespace governance established
   8K  os.neptune.* frozen
   4K  All active OS namespaces frozen
   2K  Cross-OS portability proven (same .tri runs on any OS)
   0K  OS layer sealed forever
```

## tooling — 64K → 0K

```
  64K  Web playground (compile .tri in browser)
  32K  GPU-accelerated proving
  16K  ZK coprocessor integrations (Axiom, Brevis, Herodotus)
   8K  FPGA proving backend
   4K  Tool chain self-hosts (trident builds trident tooling)
   2K  ASIC proving backend
   0K  Tooling sealed forever
```

---

# The Three Revolutions

The foundation exists to enable these. Each revolution is a frontier
that Trident is uniquely positioned to own — because no other language
can prove computation across all three.

## zkML — verifiable intelligence

Every AI inference provable. Every model weight auditable. Every
training run reproducible. The entire ML pipeline — from gradient
descent to production inference — inside a STARK proof.

```
 256K  Tensor operations in TIR (matmul, conv, attention)
 128K  Model inference compiles to provable Trident
  64K  Proven inference: run GPT-class model, get proof of output
  32K  Proven training: gradient computation inside proof
  16K  On-chain model registry with verified accuracy claims
   8K  Federated learning with proven aggregation
   4K  Any model, any size — proving scales linearly
   2K  AI agents that prove every decision they make
   0K  Intelligence without trust
```

## FHE — sovereign computation

Compute on encrypted data. No one sees inputs, no one sees
intermediate state, everyone can verify the result. Privacy
as a mathematical guarantee, not a policy promise.

```
 256K  FHE primitives in std.crypto (TFHE, BGV, CKKS)
 128K  Trident programs compile to FHE circuits
  64K  Encrypted smart contracts — execute without revealing state
  32K  FHE + ZK composition: prove correctness of encrypted computation
  16K  Multi-party FHE: N parties compute jointly, none sees others' data
   8K  Practical performance: <10x overhead vs plaintext
   4K  Hardware-accelerated FHE (FPGA/ASIC backends)
   2K  Any Trident program runs encrypted by default
   0K  Privacy without permission
```

## Quantum — post-classical computation

Quantum circuits as Trident programs. Classical-quantum interop
with proven correctness on both sides. When quantum hardware
arrives, Trident is ready — the programs are already written.

```
 256K  Quantum gate set in TIR (Hadamard, CNOT, Toffoli, measure)
 128K  Quantum circuit simulation backend
  64K  Hybrid programs: classical control flow + quantum subroutines
  32K  Quantum error correction circuits in std.quantum
  16K  Real hardware backend (IBM, Google, IonQ)
   8K  Quantum advantage: problems solved faster than classical
   4K  Post-quantum crypto native (lattice-based in std.crypto)
   2K  Quantum-classical proofs: STARK verifies quantum computation
   0K  Computation without limits
```

## 0K

Frozen foundation. Proven compiler. Verified intelligence.
Sovereign computation. Post-classical capability. Trident becomes
permanent infrastructure — a fixed point in the space of provable
computation. Write once, prove anywhere.
