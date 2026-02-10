# Trident Development Plan

## Status: Core Complete

197 tests, 0 clippy warnings. Compiler pipeline (lexer, parser, typeck,
emitter, linker, cost analyzer), 13 stdlib modules, formatter, LSP with
intelligence features, and CLI tooling are all operational.

---

## Phase 1: Developer Experience (next)

### CLI
- [ ] `trident test` — testing framework for .tri programs
      Design: test functions marked with `#[test]`, runner compiles and
      verifies each, reports pass/fail with cost summary
- [ ] `trident build --annotate` — per-line cost annotations in source
- [ ] `trident build --compare` — compare function costs across builds
- [ ] `trident doc` — documentation generation with cost annotations

### LSP
- [ ] Signature help (function parameter hints on `(` trigger)
- [ ] Hover: show cost alongside type signatures

### Tests
- [ ] Error message quality audit (review all diagnostic messages)

---

## Phase 2: Language Features

### Compiler
- [ ] Size-generic functions — parameterize array sizes (not types)
      `fn sum<N>(arr: [Field; N]) -> Field`
- [ ] Inline TASM escape hatch — hand-optimization with explicit opt-in
      `asm { push 1 add }` blocks with type annotations
- [ ] Conditional compilation — debug/release proving targets
- [ ] Pattern matching — syntactic sugar over nested if/else

### Standard Library
- [ ] Gadget library: SHA-256, Keccak, secp256k1, BLS12-381
- [ ] Recursive STARK verifier — xx_dot_step/xb_dot_step integration
- [ ] Bridge validators — Bitcoin and Ethereum light client verification

---

## Phase 3: Documentation & Ecosystem

### Documentation
- [ ] README with quick start
- [ ] Language spec (clean up spec.md for public release)
- [ ] Language tutorial / developer guide
- [ ] Optimization guide — cost pitfalls, boundary management, hash batching
- [ ] Complete error catalog with recovery suggestions

### Website
- [ ] Landing page
- [ ] Web playground (compile .tri to TASM in browser)
- [ ] Extension download links

### Ecosystem
- [ ] Token factory — registry for token deploy
- [ ] Package manager (when ecosystem justifies it)
- [ ] Browser extension integration library
- [ ] Browser extension

---

## Phase 4: Verification & Hardening

- [ ] Rewrite Neptune transaction validation in Trident (trace length < 2x hand-written)
- [ ] Benchmark suite — Trident vs hand-optimized TASM
- [ ] Formal verification of compiler correctness
- [ ] Quantum-safe operation support (lattice-based signatures)

---

## Completed

### Core
- [x] Compiler pipeline: lexer, parser, type checker, emitter, linker
- [x] Cost analysis: all 6 Triton VM tables (Processor, Hash, U32, OpStack, RAM, JumpStack)
- [x] Stack spilling to RAM (LRU-based, automatic)
- [x] Events system: emit (open) + seal (hashed)
- [x] Multi-module compilation with trident.toml
- [x] Module constant resolution (cross-module pub const)
- [x] Recursion detection across all modules
- [x] Dead code detection
- [x] Unused import warnings
- [x] Optimization hints H0001-H0004
- [x] Surface type-checker warnings in build/check output

### Standard Library (13 modules)
- [x] std.io, std.hash, std.field, std.convert, std.u32, std.xfield
- [x] std.mem, std.assert, std.merkle, std.auth
- [x] std.kernel, std.utxo, std.storage
- [x] Digest destructuring: `let (f0, f1, f2, f3, f4) = digest`
- [x] #[intrinsic] restricted to std modules only

### CLI
- [x] `trident build` — compile to TASM with --costs, --hotspots, --hints
- [x] `trident check` — type-check with --costs
- [x] `trident fmt` — format files/directories with --check
- [x] `trident init` — scaffold new project
- [x] `trident lsp` — start LSP server

### LSP (src/lsp.rs)
- [x] Diagnostics + formatting
- [x] Multi-module project-aware type checking
- [x] Document symbols (outline)
- [x] Go-to-definition (project-wide symbol index)
- [x] Hover (type signatures for builtins, functions, structs, constants)
- [x] Completions (keywords, types, builtins, dot-triggered module members)

### Editor Support
- [x] Zed extension
- [x] Tree-sitter grammar with highlights

### Tests (197)
- [x] Formatter (29), Diagnostics (7), LSP (27), Integration (27)
- [x] Emitter (20), TypeChecker (21), Parser (12), Lexer (7)
- [x] Cost (15), Stack (6), Linker (3), Resolve (3), Project (1)
- [x] Round-trip, idempotency, edge cases, spilling, all operators

### Documentation
- [x] Language specification (docs/spec.md)
- [x] Programming model (docs/programming-model.md)
- [x] Cost analysis (docs/analysis.md)
- [x] Fungible token example with spec
