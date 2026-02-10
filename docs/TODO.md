# Trident Development Plan

Everything below the "Done" section is already shipped. Everything above it
is the roadmap, organized by strategic priority.

---

## Milestone 1: Neptune Production Readiness

The immediate goal: Trident compiles real Neptune transaction validation logic
and produces output within 2x of hand-written TASM.

- [ ] Rewrite Neptune transaction validation in Trident (target: < 2x hand-written TASM)
- [ ] Benchmark suite: Trident vs hand-optimized TASM for standard patterns
- [ ] Gadget library: SHA-256, Keccak (needed for bridge verification)
- [ ] Recursive STARK verifier via `xx_dot_step` / `xb_dot_step` builtins
- [ ] Language spec: clean up spec.md for public release
- [ ] Landing page + web playground (compile .tri to TASM in browser)

## Milestone 2: Formal Verification (Embedded)

Trident's restrictions (no heap, no recursion, bounded loops, first-order) make
formal verification decidable. This is the killer feature.

### Phase 1: Assertion Analysis Engine
- [ ] Symbolic execution engine (`sym.rs`): AST to symbolic constraint system
- [ ] Algebraic solver: polynomial identity testing over F_p (Schwartz-Zippel)
- [ ] Bounded model checker: unroll loops up to 64, check all paths
- [ ] `trident verify` CLI command with verification report
- [ ] Redundant assertion elimination (saves proving cost)
- [ ] Counterexample generation for failing assertions

### Phase 2: Specification Annotations + SMT
- [ ] `#[requires(P)]`, `#[ensures(P)]`, `#[invariant(P)]` annotations
- [ ] Z3/CVC5 backend: encode Trident constraints as SMT-LIB queries
- [ ] Goldilocks field arithmetic theory for SMT (bit-vector encoding)
- [ ] Witness existence checking for `divine()` values
- [ ] Loop invariant verification (Hoare logic)
- [ ] `#[contract_invariant(P)]` for module-level properties
- [ ] Verification certificates (machine-checkable proof artifacts)

### Phase 3: LLM Integration Framework
- [ ] Machine-readable verification output (JSON for LLM consumption)
- [ ] `trident generate` command: natural language → verified contract
- [ ] Structured counterexamples with fix suggestions
- [ ] Prompt templates and Trident language reference in LLM-optimized format
- [ ] Benchmark: 20 contract specs with known-correct implementations

### Phase 4: Automatic Invariant Synthesis (Research)
- [ ] Template-based invariant synthesis (summation, accumulation patterns)
- [ ] Counterexample-guided refinement (CEGIS)
- [ ] Specification inference: suggest postconditions from code analysis

## Milestone 3: Content-Addressed Codebase

Every function identified by its cryptographic hash. Names are metadata.
Compilation and verification cached forever. Inspired by Unison, but for
provable computation where code identity is cryptographically essential.

### Phase 1: Local Content Addressing
- [ ] AST normalization (de Bruijn indices, dependency hash substitution)
- [ ] BLAKE3 hashing of serialized normalized AST
- [ ] Local codebase database (hash-keyed definitions store)
- [ ] Compilation cache by (source hash, target)
- [ ] Verification cache by verification hash
- [ ] `trident hash` command (show hash of any function)
- [ ] Integration with `trident build` (cache lookups, skip unchanged)

### Phase 2: Codebase Manager (UCM)
- [ ] `trident ucm` interactive CLI (REPL-like interface)
- [ ] Edit/update workflow: watch files, parse, hash, store
- [ ] Dependency tracking and automatic propagation
- [ ] Name management (rename, alias, deprecate — instant, non-breaking)
- [ ] History (all versions of a name, diff between hashes)
- [ ] Pretty-printing from stored AST (`trident view`)

### Phase 3: Global Registry
- [ ] Registry server (HTTP API over hash-keyed database)
- [ ] `trident publish` / `trident pull` commands
- [ ] Search by type signature, tags, verification status, cost
- [ ] Verification certificate sharing
- [ ] Cross-target compilation artifact sharing

### Phase 4: Semantic Equivalence
- [ ] Equivalence checking: prove `f(x) == g(x)` for all x
- [ ] Equivalence class management in registry
- [ ] Canonical forms for pure field arithmetic (polynomial normalization)

### Phase 5: On-Chain Registry
- [ ] Merkle tree registry contract (written in Trident)
- [ ] On-chain verification certificate validation
- [ ] Cross-chain equivalence proof generation

## Milestone 4: Multi-Target Backends

Same source → multiple zkVMs. Each backend multiplies the value of every
existing Trident program.

- [ ] OpenVM backend (RISC-V zkVM, EVM proof verification)
- [ ] Miden backend (Polygon Miden, stack-based, Winterfell prover)
- [ ] SP1 backend (Succinct, RISC-V, Plonky3 prover)
- [ ] Cairo backend (StarkNet/StarkWare, Sierra intermediate)
- [ ] Cross-target testing framework
- [ ] Target-specific optimizations
- [ ] Target comparison benchmarks (same program, different backends)
- [ ] Verified compiler: prove backend emission preserves semantics (Coq/Lean)

## Milestone 5: Ecosystem

- [ ] Package manager with content-addressed dependencies
- [ ] Standard cryptographic library (secp256k1, BLS12-381, ed25519)
- [ ] Token factory: templates for Neptune TSP-1/TSP-2 tokens
- [ ] Bridge validators: Bitcoin and Ethereum light clients in Trident
- [ ] Browser extension integration library
- [ ] ZK coprocessor programs (Axiom, Brevis, Herodotus integration)
- [ ] Editor extension download page + marketplace listings

## Language Evolution (Careful Extensions)

These extend the language without violating the minimal design philosophy.

- [ ] Pattern matching on structs: `match p { Point { x: 0, y } => ... }`
- [ ] Const generics in expressions: `fn foo<M, N>() -> [Field; M + N]`
- [ ] Trait-like interfaces for backend extensions (generic over hash function)
- [ ] Proof composition primitives (recursive verification as first-class)
- [ ] `#[pure]` annotation (no I/O — enables aggressive verification)

## Research Directions

Long-term, exploratory, not committed.

- [ ] Optimal backend selection (given cost/memory constraints, pick best target)
- [ ] Cost-driven compilation (transform to minimize proving cost per target)
- [ ] Incremental proving (prove modules independently, compose proofs)
- [ ] Verifiable AI/ML inference (fixed-architecture neural networks in Trident)
- [ ] Provable data pipelines (ETL, aggregation, supply chain verification)
- [ ] Hardware acceleration backends (FPGA, ASIC, GPU proving)
- [ ] Differential privacy in ZK (combine divine/seal with DP mechanisms)

---

## Done

**Compiler** — lexer, parser, type checker, emitter, linker, cost analyzer
(all 6 Triton VM tables), stack spilling to RAM, events (emit + seal),
multi-module compilation, module constants, recursion detection, dead code
detection, unused import warnings, optimization hints H0001-H0004,
type-checker warnings surfaced in build/check output, pattern matching
(match/wildcard desugared to if/else), inline TASM blocks with stack
effect annotations. Universal refactoring: TargetConfig with
`targets/triton.toml`, TypeChecker parameterized by target, StackBackend
trait + TritonBackend, CostModel trait + TritonCostModel, target-tagged
asm blocks (`asm(triton) { ... }`).

**Standard Library** — 13 modules: std.io, std.hash, std.field,
std.convert, std.u32, std.xfield, std.mem, std.assert, std.merkle,
std.auth, std.kernel, std.utxo, std.storage. Digest destructuring.
`#[intrinsic]` restricted to std modules. Restructured for universality:
`std/core/`, `std/io/`, `std/crypto/` + `ext/triton/`.

**CLI** — `trident build` (--costs, --hotspots, --hints, --annotate,
--save-costs, --compare, --target), `trident check` (--costs),
`trident fmt` (directories, --check), `trident init`, `trident test`,
`trident doc`, `trident lsp`.

**LSP** — diagnostics, formatting, project-aware type checking, document
symbols, go-to-definition, hover (with cost), completions, signature help.

**Editor** — Zed extension, tree-sitter grammar with highlights. Updated
for universality: match support, asm target tags in grammar and extensions.

**Tests (359)** — formatter (29), diagnostics (7), LSP (27),
integration (27), emitter (20), type checker (21), parser (13), lexer (7),
cost (15), stack (6), linker (3), resolve (4), project (1), error paths
(30+), security (nesting depth, path traversal). Round-trip, idempotency,
edge cases, spilling, all operators.

**Security** — audit completed: path traversal fix in module resolution,
lexer iterative error recovery (no stack overflow), parser nesting depth
limit (256), emitter unreachable variable halts VM, seal padding
defense-in-depth. TargetConfig field validation, saturating cost arithmetic.

**Docs** — README, language spec, programming model, cost analysis,
tutorial, optimization guide, error catalog, fungible token example,
vision manifesto, developer guide (zero-to-ZK), blockchain developer
guide (EVM/SVM migration), quick reference card, comparative analysis.
Updated for universality. Lifecycle docs (Write, Compile, Run, Deploy,
Prove, Verify). Design docs: universal-design, content-addressed,
formal-verification, opportunities, proving-roadmap, gold-standard.

**Formal Verification** — symbolic execution engine (sym.rs), algebraic
solver (Schwartz-Zippel over Goldilocks F_p), bounded model checker,
`trident verify` CLI with verification reports, redundant assertion
elimination, counterexample generation. SMT-LIB2 backend (smt.rs) with
Z3/CVC5 integration. `#[requires]`, `#[ensures]`, `#[invariant]`
annotations. Machine-readable JSON output. `trident generate` command
for LLM-assisted verified code generation. Automatic invariant synthesis
(template-based, CEGIS refinement, postcondition inference). Semantic
equivalence checking (hash, polynomial normalization, differential
testing). Verification certificates.

**Content-Addressed Codebase** — AST normalization with de Bruijn
indices, BLAKE3 content hashing (hash.rs), local codebase database
(ucm.rs), compilation and verification caching, `trident ucm` CLI
(add/list/view/rename/stats/history/deps), `trident hash` command.
Global registry (registry.rs): HTTP server + client, publish/pull/search
by name/type/tag/verification status, 31 tests. On-chain registry
(onchain.rs): depth-4 Merkle tree (16 entries), FieldElement/Digest
types with simulated Tip5, proof generation (register/verify/update/
equivalence), certificate serialization, 28 tests. On-chain registry
contract (ext/triton/registry.tri): register, verify_membership,
update_certificate, lookup, register_equivalence operations with
Merkle proof verification, authorization, and events.
