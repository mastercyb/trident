# Trident Development Plan

## Compiler

- [x] Size-generic functions: `fn sum<N>(arr: [Field; N]) -> Field`
- [x] Inline TASM: `asm { push 1 add }` with type annotations
- [x] Conditional compilation: `#[cfg(debug)]` / `#[cfg(release)]` with `--target` flag
- [ ] Pattern matching: syntactic sugar over nested if/else



## CLI

- [ ] `trident test` — `#[test]` functions, compile + verify, cost summary
- [ ] `trident doc` — documentation generation with cost annotations
- [ ] `trident build --annotate` — per-line cost annotations in source
- [ ] `trident build --compare` — diff function costs across builds

## LSP

- [ ] Signature help (parameter hints on `(` trigger)
- [ ] Hover: show cost alongside type

## Tests

- [ ] Error message quality audit

## Documentation

- [ ] README with quick start
- [ ] Language spec: clean up spec.md for public release
- [ ] Language tutorial / developer guide
- [ ] Optimization guide: cost pitfalls, boundary management, hash batching
- [ ] Error catalog with recovery suggestions

## Website

- [ ] Landing page
- [ ] Web playground: compile .tri to TASM in browser
- [ ] Editor extension download links

## Ecosystem

- [ ] Token factory: registry for token deploy
- [ ] Package manager
- [ ] Browser extension integration library
- [ ] Browser extension

## Standard Library

- [ ] Gadget library: SHA-256, Keccak, secp256k1, BLS12-381
- [ ] Recursive STARK verifier via xx_dot_step/xb_dot_step
- [ ] Bridge validators: Bitcoin and Ethereum light clients

## Verification

- [ ] Rewrite Neptune transaction validation in Trident (< 2x hand-written)
- [ ] Benchmark suite: Trident vs hand-optimized TASM
- [ ] Formal verification of compiler correctness

---

## Done

**Compiler** — lexer, parser, type checker, emitter, linker, cost analyzer
(all 6 Triton VM tables), stack spilling to RAM, events (emit + seal),
multi-module compilation, module constants, recursion detection, dead code
detection, unused import warnings, optimization hints H0001-H0004,
type-checker warnings surfaced in build/check output.

**Standard Library** — 13 modules: std.io, std.hash, std.field,
std.convert, std.u32, std.xfield, std.mem, std.assert, std.merkle,
std.auth, std.kernel, std.utxo, std.storage. Digest destructuring.
`#[intrinsic]` restricted to std modules.

**CLI** — `trident build` (--costs, --hotspots, --hints), `trident check`
(--costs), `trident fmt` (directories, --check), `trident init`,
`trident lsp`.

**LSP** — diagnostics, formatting, project-aware type checking, document
symbols, go-to-definition, hover, completions.

**Editor** — Zed extension, tree-sitter grammar with highlights.

**Tests (258)** — formatter (29), diagnostics (7), LSP (27),
integration (27), emitter (20), type checker (21), parser (12), lexer (7),
cost (15), stack (6), linker (3), resolve (3), project (1). Round-trip,
idempotency, edge cases, spilling, all operators.

**Docs** — language spec, programming model, cost analysis, fungible token
example with spec.
