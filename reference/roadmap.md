# Roadmap

Trident uses kelvin versioning. Versions count down toward absolute
zero. At 0K the layer is frozen forever — no further changes. Lower
layers freeze before higher layers. The endgame is a frozen foundation
for provable computation.

## The Stack

```
Layer           Current
───────────────────────
vm spec         50K
language        100K
TIR             120K
compiler        200K
std.*           250K
os.*            300K
tooling         300K
```

## vm spec — 50K → 0K

```
  40K  Intrinsic set stable (no new vm.* builtins)
  30K  Triton backend emission proven correct
  20K  3+ backends passing conformance suite (Miden, Cairo, SP1)
  10K  TargetConfig / StackBackend / CostModel traits frozen
   0K  vm.* namespace sealed forever
```

## language — 100K → 0K

```
  80K  Indexed assignment (arr[i] = val, s.field = val)
  60K  Trait-like interfaces
  40K  Grammar finalized (reference/grammar.md = 0K)
  20K  Type system finalized (reference/language.md = 0K)
  10K  No open language RFCs remain
   0K  Syntax and semantics sealed forever
```

## TIR — 120K → 0K

```
 100K  TIROp set stable (no new variants without language change)
  80K  Per-function benchmarks < 1.2x vs hand-written baselines
  60K  Cost-driven optimization passes land
  40K  Stack effect contracts proven for all ops
  20K  reference/ir.md = 0K
   0K  IR spec sealed forever
```

## compiler — 200K → 0K

```
 150K  Lexer + parser rewritten in .tri
 120K  Type checker rewritten in .tri
 100K  TIR builder rewritten in .tri (pipeline fully in Trident)
  80K  Compiler compiles itself (self-hosting)
  60K  Each compilation produces a proof certificate (self-proving)
  40K  Incremental proving (per-module proofs, composed)
  20K  Proof verified on-chain
  10K  src/ deleted — compiler lives in provable stack
   0K  Compiler proven correct forever
```

Only the pipeline (lexer, parser, typecheck, TIR, lowering) needs to
be provable. LSP, CLI, pretty-printing run outside the proof.

## std.* — 250K → 0K

```
 200K  std.token, std.coin, std.card shipped
 180K  std.skill.* (23 skills) shipped
 150K  #[requires]/#[ensures] contracts on all public functions
 120K  20 specs with proven implementations
 100K  std.crypto.* formally verified (poseidon, merkle, ecdsa)
  50K  All modules verified, no unproven public function remains
  20K  Public APIs frozen
   0K  Standard library sealed forever
```

## os.* — 300K → 0K

```
 250K  os.neptune.* complete (reference OS implementation)
 200K  Atlas on-chain registry live (TSP-2 Cards)
 150K  3+ OS namespaces operational
 100K  Per-OS namespace governance established
  50K  os.neptune.* frozen
  20K  All OS namespaces frozen
   0K  OS layer sealed forever
```

## tooling — 300K → 0K

```
 250K  LSP feature-complete, editor extensions published
 200K  Web playground (compile .tri in browser)
 150K  ZK coprocessor integrations (Axiom, Brevis, Herodotus)
 100K  Hardware acceleration (FPGA, ASIC, GPU proving)
  50K  All tools stable, no breaking changes
   0K  Tooling sealed forever
```

## 0K

Frozen foundation. Every layer proven correct. No further changes
needed or possible. Trident becomes permanent infrastructure —
a fixed point in the space of provable computation.
