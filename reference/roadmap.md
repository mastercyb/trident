# Roadmap

Trident uses kelvin versioning. Versions count down toward absolute
zero. At 0K the layer is frozen forever — no further changes. Lower
layers freeze before higher layers. The endgame is a frozen foundation
for provable computation.

## The Stack

```
Layer           Current
───────────────────────
vm spec          64K
language        128K
TIR             128K
compiler        256K
std.*           256K
os.*            256K
tooling         256K
```

## vm spec — 64K → 0K

```
  32K  Intrinsic set stable (no new vm.* builtins)
  16K  Triton backend emission proven correct
   8K  3+ backends passing conformance suite
   4K  TargetConfig / StackBackend / CostModel traits frozen
   2K  Every intrinsic has a formal cost proof
   0K  vm.* namespace sealed forever
```

## language — 128K → 0K

```
  64K  Indexed assignment (arr[i] = val, s.field = val)
  32K  Trait-like interfaces
  16K  Grammar finalized — no new syntax forms
   8K  Type system finalized — no new type rules
   4K  Every language feature has a formal soundness proof
   2K  Formal semantics published
   0K  Syntax and semantics sealed forever
```

## TIR — 128K → 0K

```
  64K  TIROp set stable (no new variants without language change)
  32K  Per-function benchmarks < 1.2x vs hand-written baselines
  16K  Cost-driven optimization passes land
   8K  Stack effect contracts proven for all ops
   4K  Every lowering path formally verified
   2K  No unproven TIR transformation remains
   0K  IR spec sealed forever
```

## compiler — 256K → 0K

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

## std.* — 256K → 0K

```
 128K  std.token, std.coin, std.card shipped
  64K  std.skill.* (23 skills) shipped
  32K  #[requires]/#[ensures] contracts on all public functions
  16K  std.crypto.* formally verified (poseidon, merkle, ecdsa)
   8K  All modules verified, no unproven public function remains
   4K  Public APIs frozen
   2K  Every public function has a proven contract
   0K  Standard library sealed forever
```

## os.* — 256K → 0K

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

## tooling — 256K → 0K

```
 128K  LSP feature-complete, editor extensions published
  64K  Web playground (compile .tri in browser)
  32K  ZK coprocessor integrations (Axiom, Brevis, Herodotus)
  16K  Hardware acceleration (FPGA, ASIC, GPU proving)
   8K  All tools stable, no breaking changes
   4K  Tool chain self-hosts (trident builds trident tooling)
   2K  Every tool output reproducible and verifiable
   0K  Tooling sealed forever
```

## 0K

Frozen foundation. Every layer proven correct. No further changes
needed or possible. Trident becomes permanent infrastructure —
a fixed point in the space of provable computation.
