# Trident Development Plan

Status as of February 2026. Checked items are shipped. Unchecked items
are the remaining roadmap.

---

## Milestone 1: Neptune Production Readiness

- [ ] Rewrite Neptune transaction validation in Trident (target: < 2x hand-written TASM)
- [x] Benchmark suite: Trident vs hand-optimized TASM for standard patterns
- [x] Gadget library: SHA-256, Keccak (needed for bridge verification)
- [ ] Recursive STARK verifier via `xx_dot_step` / `xb_dot_step` builtins
- [x] Language spec: clean up spec.md for public release (v0.5)
- [ ] Landing page + web playground (compile .tri to TASM in browser)

## Milestone 2: Formal Verification (Embedded)

### Phase 1: Assertion Analysis Engine — COMPLETE
- [x] Symbolic execution engine (`sym.rs`)
- [x] Algebraic solver: polynomial identity testing over F_p (Schwartz-Zippel)
- [x] Bounded model checker: unroll loops, check all paths
- [x] `trident verify` CLI command with verification report
- [x] Redundant assertion elimination
- [x] Counterexample generation for failing assertions

### Phase 2: Specification Annotations + SMT — COMPLETE
- [x] `#[requires(P)]`, `#[ensures(P)]`, `#[invariant(P)]` annotations
- [x] Z3/CVC5 backend (`smt.rs`): SMT-LIB2 encoder
- [x] Goldilocks field arithmetic theory for SMT (bit-vector encoding)
- [x] Witness existence checking for `divine()` values
- [x] Loop invariant verification (Hoare logic)
- [x] `#[contract_invariant(P)]` for module-level properties
- [x] Verification certificates (machine-checkable proof artifacts)

### Phase 3: LLM Integration Framework — COMPLETE
- [x] Machine-readable verification output (JSON via `report.rs`)
- [x] `trident generate` command: spec-driven code scaffolding
- [x] Structured counterexamples with fix suggestions
- [x] LLM-optimized reference (`docs/llm-reference.md`)
- [ ] Benchmark: 20 contract specs with known-correct implementations

### Phase 4: Automatic Invariant Synthesis — COMPLETE
- [x] Template-based invariant synthesis (`synthesize.rs`)
- [x] Counterexample-guided refinement (CEGIS)
- [x] Specification inference: suggest postconditions from code analysis

## Milestone 3: Content-Addressed Codebase — COMPLETE

### Phase 1: Local Content Addressing
- [x] AST normalization (de Bruijn indices, Poseidon2 hashing)
- [x] Content hashing (`hash.rs`)
- [x] Local codebase database (`ucm.rs`)
- [x] Compilation and verification caching (`cache.rs`)
- [x] `trident hash` command
- [x] Integration with `trident build` (cache lookups)

### Phase 2: Codebase Manager (UCM)
- [x] `trident ucm` CLI (add/list/view/rename/stats/history/deps)
- [x] Edit/update workflow
- [x] Dependency tracking and automatic propagation
- [x] Name management (rename, alias, deprecate)
- [x] History tracking (all versions of a name)
- [x] Pretty-printing from stored AST (`trident view`)

### Phase 3: Global Registry
- [x] Registry server (`registry.rs`): HTTP API, 10 endpoints
- [x] `trident publish` / `trident pull` commands
- [x] Search by type signature, tags, verification status
- [x] Verification certificate sharing

### Phase 4: Semantic Equivalence
- [x] Equivalence checking (`equiv.rs`): prove f(x) == g(x)
- [x] Canonical forms for pure field arithmetic (polynomial normalization)

### Phase 5: On-Chain Registry
- [x] Merkle tree registry contract (`ext/triton/registry.tri`)
- [x] On-chain verification certificate validation (`onchain.rs`)
- [x] Proof generation (register/verify/update/equivalence)

## Milestone 4: Multi-Target Backends

Architecture is in place (TargetConfig, StackBackend trait, CostModel
trait, target-tagged asm blocks, 5 target TOML configs). Only Triton VM
backend is fully implemented; others are stubs.

- [x] Target abstraction: `TargetConfig`, `targets/*.toml` (triton, miden, openvm, sp1, cairo)
- [x] `StackBackend` trait + `TritonBackend`
- [x] `CostModel` trait + `TritonCostModel`
- [x] Target-tagged asm blocks: `asm(triton) { ... }`
- [x] Cross-target testing framework
- [ ] Miden backend (full implementation, not stub)
- [ ] OpenVM backend (full implementation)
- [ ] SP1 backend (full implementation)
- [ ] Cairo backend (full implementation)
- [ ] Verified compiler: prove backend emission preserves semantics (Coq/Lean)

## Milestone 5: Ecosystem

- [x] Package manager with content-addressed dependencies (`package.rs`)
- [x] Standard cryptographic library: secp256k1, ed25519, ECDSA, Poseidon, SHA-256, Keccak-256, bigint
- [x] Token factory: TSP-1 fungible token, TSP-2 NFT standard
- [x] Bridge validators: Bitcoin and Ethereum light clients
- [ ] Browser extension integration library
- [ ] ZK coprocessor programs (Axiom, Brevis, Herodotus integration)
- [ ] Editor extension download page + marketplace listings

## Language Evolution

- [x] Pattern matching on structs: `match p { Point { x: 0, y } => ... }`
- [x] Const generics in expressions: `[Field; M + N]`, `[Field; N * 2]`
- [x] `#[pure]` annotation (no I/O — enables aggressive verification)
- [ ] Trait-like interfaces for backend extensions (generic over hash function)
- [ ] Proof composition primitives (recursive verification as first-class)

## Research Directions

Long-term, exploratory, not committed.

- [ ] Optimal backend selection (given cost/memory constraints, pick best target)
- [ ] Cost-driven compilation (transform to minimize proving cost per target)
- [ ] Incremental proving (prove modules independently, compose proofs)
- [ ] Verifiable AI/ML inference (fixed-architecture neural networks in Trident)
- [ ] Provable data pipelines (ETL, aggregation, supply chain verification)
- [ ] Hardware acceleration backends (FPGA, ASIC, GPU proving)

---

## Done (Detailed)

**Compiler** — lexer, parser, type checker, emitter, linker, cost analyzer
(all 6 Triton VM tables), stack spilling to RAM, events (emit + seal),
multi-module compilation, module constants, recursion detection, dead code
detection, unused import warnings, optimization hints H0001-H0004,
type-checker warnings, pattern matching (match/wildcard/struct patterns),
inline TASM blocks with stack effect annotations, const generic
expressions (`M + N`, `N * 2`), `#[pure]` I/O enforcement.

**Standard Library** — `std/core/` (assert, convert, field, u32),
`std/io/` (io, mem, storage), `std/crypto/` (hash, merkle, auth,
sha256, keccak256, secp256k1, ed25519, ecdsa, poseidon, bigint),
`ext/triton/` (xfield, kernel, utxo, registry). Digest destructuring.
`#[intrinsic]` restricted to std modules.

**CLI** — `trident build` (--costs, --hotspots, --hints, --annotate,
--save-costs, --compare, --target), `trident check`, `trident fmt`,
`trident init`, `trident test`, `trident doc`, `trident lsp`,
`trident verify` (--smt, --z3, --json, --synthesize),
`trident hash`, `trident view`, `trident ucm`, `trident generate`,
`trident bench`.

**LSP** — diagnostics, formatting, project-aware type checking, document
symbols, go-to-definition, hover (with cost), completions, signature help.

**Editor** — Zed extension, tree-sitter grammar, VSCode extension,
Helix support.

**Formal Verification** — symbolic execution (sym.rs), algebraic solver
(solve.rs), SMT-LIB2 backend (smt.rs), `#[requires]`/`#[ensures]`/
`#[invariant]` annotations, JSON reports (report.rs), counterexample
generation, CEGIS invariant synthesis (synthesize.rs), semantic
equivalence checking (equiv.rs), verification certificates.

**Content-Addressed Codebase** — AST normalization + Poseidon2 hashing
(hash.rs), codebase database (ucm.rs), compilation caching (cache.rs),
global registry (registry.rs) with HTTP server, on-chain registry
(onchain.rs) with Merkle proofs, package manager (package.rs).

**Multi-Target Architecture** — TargetConfig (target.rs), 5 target
configs (triton/miden/openvm/sp1/cairo.toml), StackBackend trait,
CostModel trait, target-tagged asm blocks. Triton VM fully implemented.

**Ecosystem** — Crypto library (11 modules), token standards (TSP-1/
TSP-2), bridge validators (Bitcoin/Ethereum light clients), Neptune
examples (lock contracts, token types).

**Tests** — 658+ tests across all modules.

**Docs** — README, spec (v0.5), tutorial, reference, programming model,
optimization guide, error catalog, vision, developer guides, lifecycle
docs, design docs (universal-design, content-addressed, formal-verification).

**Security** — Path traversal fix, iterative lexer recovery, parser
depth limit (256), unreachable variable VM halt, seal padding, target
config validation, saturating cost arithmetic.
