# Roadmap

Trident uses kelvin versioning. Versions count down toward absolute
zero. At 0K the layer is frozen forever — no further changes. Lower
layers freeze before higher layers. The endgame is a frozen foundation
for provable computation.

## The Stack

```
Layer           Current     Target
─────────────────────────────────────
vm spec         50K         0K
language        100K        0K
TIR             120K        0K
compiler        200K        0K
std.*           250K        0K
os.*            300K        0K
tooling         300K        0K
```

## vm spec — 50K

The VM abstraction layer. 20 target TOMLs exist. Triton VM is the
reference backend. Freezing means the TargetConfig/StackBackend/CostModel
traits and the vm.* intrinsic set are final.

- Stabilize intrinsic set across all targets
- Implement Miden, Cairo, SP1, OpenVM backends
- Prove backend emission preserves semantics
- Freeze vm.* namespace

## language — 100K

Syntax, types, operators, builtins, memory model. Freezing means the
grammar and type system accept no further changes.

- Indexed assignment (`arr[i] = val`, `s.field = val`)
- Trait-like interfaces
- Finalize grammar (reference/grammar.md = 0K)
- Finalize type system (reference/language.md = 0K)

## TIR — 120K

The intermediate representation. Freezing means TIROp variants, stack
effects, and lowering contracts are final.

- Stabilize TIROp set
- Per-function benchmarking against baselines (target: < 1.2x overhead)
- Cost-driven optimization passes
- Freeze IR spec (reference/ir.md = 0K)

## compiler — 200K

The Rust implementation. Self-hosting replaces it with a provable
Trident implementation. Freezing means the compiler proves its own
correctness and the Rust code can be deleted.

- Self-hosting: rewrite compiler in Trident
- Self-proving: each compilation produces a proof certificate
- Incremental proving (per-module proofs, composed)
- Delete src/ — frozen compiler lives in the provable stack

Self-hosting only needs the compiler pipeline (lexer, parser, typecheck,
TIR, target lowering) to be provable. LSP, CLI, pretty-printing run
outside the proof. The real gap is rewriting src/ in .tri, not the
dependencies.

## std.* — 250K

Standard library. Freezing means the module APIs are final and every
function has a correctness proof.

- Ship std.token, std.coin, std.card, std.skill.* (23 skills)
- Add #[requires]/#[ensures] contracts to all public functions
- Verification benchmark: 20 specs with proven implementations
- Freeze public APIs

## os.* — 300K

OS bindings. Each OS namespace freezes independently.

- Atlas: on-chain package registry (TSP-2 Cards)
- Per-OS namespace governance
- Freeze os.neptune.* first (reference implementation)

## tooling — 300K

LSP, formatter, verifier, editor extensions, playground.

- Web playground (compile .tri in browser)
- Editor marketplace listings
- ZK coprocessor integrations (Axiom, Brevis, Herodotus)
- Hardware acceleration (FPGA, ASIC, GPU proving)

## 0K

Frozen foundation. Every layer proven correct. No further changes
needed or possible. Trident becomes permanent infrastructure —
a fixed point in the space of provable computation.
