# Roadmap

Trident exists to write [CORE](https://cyber.page/core-spec/) — Conserved Observable Reduction
Equilibrium, a self-verifying substrate for planetary collective
intelligence. 16 reduction patterns, field-first arithmetic, BBG
state, focus dynamics — all written in Trident, all provable.

Kelvin versioning: versions count down toward 0K (frozen forever).
Lower layers freeze first.

Three targets before first release:

1. Self-hosting — compiler compiles itself in Trident
2. Atlas — on-chain package registry live
3. Revolution demos — small proven inference, FHE circuit
   compilation, quantum circuit simulation

```
Layer           Current   First Release
───────────────────────────────────────
CORE            256K         64K
vm spec          32K         16K
language         64K         32K
TIR             128K         64K
compiler        128K         32K
std.*           128K         64K
os.*            128K         64K
tooling          64K         32K
AI              256K        128K
Privacy         256K        128K
Quantum         256K        128K
```

---

## 256K — primitives land

```
- [x] CORE      16 patterns implemented in Trident (reference evaluator)
- [x] AI        Tensor operations (dot, matmul, relu, dense) — DONE: std.nn.tensor
- [x] Privacy   Polynomial ops and NTT for FHE — DONE: std.private.poly
- [x] Quantum   Quantum gate set (H, X, Y, Z, S, T, CNOT, CZ, SWAP) — DONE: std.quantum.gates
- [ ] tooling   Amazing cli
- [ ] tooling   Integration tests and formal verification
- [ ] tooling   Beautiful website
- [ ] tooling   Complete benchmark coverage
```

## 128K — the machine assembles

```
CORE      Poseidon + Merkle as CORE programs, BBG prototype
TIR       Lowering works for stack, register, and tree targets
compiler  Lexer + parser rewritten in .tri
std.*     std.token, std.coin, std.card shipped
os.*      os.neptune.* complete, Atlas on-chain registry live
AI        Small model inference compiles to provable Trident
Privacy   Trident programs compile to FHE circuits
Quantum   Quantum circuit simulation backend
```

## 64K — proof of concept

```
CORE      Transaction circuit, STARK verifier as CORE program
language  Indexed assignment (arr[i] = val, s.field = val)
TIR       5+ OS targets lowering, all three VM types passing tests
compiler  Type checker rewritten in .tri
std.*     23 std.skill.* shipped
os.*      3+ OS namespaces operational
tooling   Web playground: compile .tri in browser
AI        On-chain model registry — verified accuracy, no trust
Privacy   Encrypted smart contracts — execute without revealing state
Quantum   Hybrid programs: classical control + quantum subroutines
```

## 32K — first release

Compiler compiles itself. Atlas live. Revolution demos ship.

```
CORE      Self-verifying: CORE proves its own execution
vm spec   Intrinsic set stable: no new vm.* builtins
language  Protocols: compile-time structural typing, grammar frozen
TIR       TIROp set stable (5+ OS, 1 VM per type prove op set complete)
compiler  Pipeline fully in Trident — compiler compiles itself
std.*     #[requires]/#[ensures] contracts on all public functions
os.*      Per-OS namespace governance established
AI        Proven training: gradient computation inside proof
Privacy   FHE + ZK: prove correctness of encrypted computation
Quantum   Quantum error correction in std.quantum
```

## 16K — the industries fall

```
CORE      Recursive composition — proofs verify proofs
vm spec   Triton backend emission proven correct
language  Type system finalized — no new type rules
TIR       Per-function benchmarks < 1.2x, optimization passes land
compiler  Each compilation produces a proof certificate (self-proving)
std.*     std.crypto.* formally verified (poseidon, merkle, ecdsa)
os.*      os.neptune.* frozen
tooling   GPU proving, ZK coprocessor integrations
AI        GPT-class proven inference (billion+ parameters)
Privacy   Multi-party FHE: N parties compute, none sees others' data
Quantum   Real hardware backends (IBM, Google, IonQ)
```

## 8K — proven everything

```
CORE      Focus dynamics live — collective intelligence emerges
vm spec   3+ backends passing conformance suite
language  Every language feature has a formal soundness proof
TIR       Stack effect contracts proven for all ops
compiler  Incremental proving (per-module proofs, composed)
std.*     All modules verified — every public function proven
os.*      All active OS namespaces frozen
tooling   FPGA proving backend
AI        Federated learning with proven aggregation
Privacy   Practical performance: <10x overhead vs plaintext
Quantum   Quantum advantage: problems classical can't touch
```

## 4K — hardware era

```
CORE      BBG formally verified, all state transitions proven
vm spec   TargetConfig / StackBackend / CostModel traits frozen
language  Protocol system proven sound (composability without dispatch)
TIR       Every lowering path formally verified
compiler  Proof verified on-chain, src/ deleted
std.*     Public APIs frozen, no new exports
os.*      Cross-OS portability proven (same .tri runs on any OS)
tooling   Tool chain self-hosts (trident builds trident tooling)
AI        Autonomous agents that prove every decision they make
Privacy   Hardware-accelerated FHE (FPGA/ASIC)
Quantum   Post-quantum crypto native (lattice-based std.crypto)
```

## 2K — last mile

```
CORE      16 patterns proven correct, conservation laws verified
vm spec   Every intrinsic has a formal cost proof
language  Formal semantics published
TIR       TIR-to-target roundtrip proven equivalent
compiler  Compiler proves its own correctness
std.*     Cross-module composition proofs complete
os.*      Every OS binding formally verified
tooling   ASIC proving backend
AI        Any model, any size — proving scales linearly
Privacy   Any Trident program runs encrypted by default
Quantum   Quantum-classical proofs: STARK verifies quantum computation
```

## 0K

```
CORE      sealed          The substrate verifies itself.
vm spec   sealed          Intelligence without trust.
language  sealed          Privacy without permission.
TIR       sealed          Computation without limits.
compiler  sealed
std.*     sealed          Write once, prove anywhere.
os.*      sealed
tooling   sealed
```
