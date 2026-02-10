# Trident Development Plan

## Features


## Compiler

- [x] Size-generic functions: `fn sum<N>(arr: [Field; N]) -> Field`
- [x] Inline TASM: `asm { push 1 add }` with type annotations
- [x] Conditional compilation: `#[cfg(debug)]` / `#[cfg(release)]` with `--target` flag
- [x] Pattern matching: syntactic sugar over nested if/else
- [x] TargetConfig and `targets/triton.toml`
- [x] TypeChecker parameterized with TargetConfig
- [x] StackBackend trait + TritonBackend
- [x] CostModel trait + TritonCostModel
- [x] Target-tagged asm blocks: `asm(triton) { ... }`

## CLI

- [x] `trident test` — `#[test]` functions, compile + verify, cost summary
- [x] `trident doc` — documentation generation with cost annotations
- [x] `trident build --annotate` — per-line cost annotations in source
- [x] `trident build --compare` — diff function costs across builds
- [x] `trident build --save-costs` — JSON cost serialization
- [x] `--target` CLI flag for VM targets

## LSP

- [x] Signature help (parameter hints on `(` trigger)
- [x] Hover: show cost alongside type

## Tests

- [x] Error message quality audit (30+ error path tests added)

## Security

- [x] Security audit (path traversal, stack overflow, parser depth, emit safety)

## Tooling

- [x] Tree-sitter grammar updated (match, asm target tags)
- [x] Editor extensions updated

## Documentation

- [x] README with quick start
- [x] Language tutorial / developer guide
- [x] Optimization guide: cost pitfalls, boundary management, hash batching
- [x] Error catalog with recovery suggestions
- [x] Vision document: manifesto, showcase, comparative analysis
- [x] For Developers guide: zero-to-ZK bridge for conventional programmers
- [x] For Blockchain Devs guide: mental model migration from EVM/SVM/CosmWasm
- [x] Quick Reference card: types, operators, builtins, grammar, CLI
- [x] STARK education article: arithmetization, FRI, Triton VM tables, recursive verification
- [x] Documentation updated for universality
- [x] Lifecycle docs: Writing, Compiling, Running, Deploying, Generating Proofs, Verifying Proofs
- [ ] Language spec: clean up spec.md for public release

## Website

- [ ] Landing page
- [ ] Web playground: compile .tri to TASM in browser
- [ ] Editor extension download links

## Standard Library

- [x] Restructured: `std/core/`, `std/io/`, `std/crypto/` + `ext/triton/`
- [ ] Gadget library: SHA-256, Keccak, secp256k1, BLS12-381
- [ ] Recursive STARK verifier via xx_dot_step/xb_dot_step

## Verification

- [ ] Rewrite Neptune transaction validation in Trident (< 2x hand-written)
- [ ] Benchmark suite: Trident vs hand-optimized TASM
- [ ] Formal verification of compiler correctness

## Ecosystem

- [ ] Token factory: registry for token deploy
- [ ] Package manager
- [ ] Browser extension integration library
- [ ] Browser extension
- [ ] Bridge validators: Bitcoin and Ethereum light clients

## Multi-Target

- [ ] OpenVM backend (RISC-V zkVM, Rust guest programs, EVM proof verification)
- [ ] Miden backend (Polygon Miden, stack-based, Winterfell prover)
- [ ] SP1 backend (Succinct, RISC-V, Plonky3 prover)
- [ ] Cairo backend (StarkNet/StarkWare, Sierra intermediate)
- [ ] Cross-target testing framework
- [ ] Target-specific optimizations
- [ ] Target comparison benchmarks (same program, different backends)
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

**Tests (352)** — formatter (29), diagnostics (7), LSP (27),
integration (27), emitter (20), type checker (21), parser (13), lexer (7),
cost (15), stack (6), linker (3), resolve (4), project (1), error paths
(30+), security (nesting depth, path traversal). Round-trip, idempotency,
edge cases, spilling, all operators.

**Security** — audit completed: path traversal fix in module resolution,
lexer iterative error recovery (no stack overflow), parser nesting depth
limit (256), emitter unreachable variable halts VM, seal padding
defense-in-depth.

**Docs** — README, language spec, programming model, cost analysis,
tutorial, optimization guide, error catalog, fungible token example,
vision manifesto, developer guide (zero-to-ZK), blockchain developer
guide (EVM/SVM migration), quick reference card, comparative analysis.
Updated for universality (multi-target architecture, TargetConfig, backend
traits). Lifecycle documentation: Writing a Program, Compiling a Program,
Running a Program, Deploying a Program, Generating Proofs, Verifying
Proofs.
