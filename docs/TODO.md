# Trident Development Plan

## Compiler

- [x] Size-generic functions: `fn sum<N>(arr: [Field; N]) -> Field`
- [x] Inline TASM: `asm { push 1 add }` with type annotations
- [x] Conditional compilation: `#[cfg(debug)]` / `#[cfg(release)]` with `--target` flag
- [x] Pattern matching: syntactic sugar over nested if/else

## CLI

- [x] `trident test` — `#[test]` functions, compile + verify, cost summary
- [x] `trident doc` — documentation generation with cost annotations
- [x] `trident build --annotate` — per-line cost annotations in source
- [x] `trident build --compare` — diff function costs across builds
- [x] `trident build --save-costs` — JSON cost serialization

## LSP

- [x] Signature help (parameter hints on `(` trigger)
- [x] Hover: show cost alongside type

## Tests

- [x] Error message quality audit (30+ error path tests added)

## Security

- [x] Security audit (path traversal, stack overflow, parser depth, emit safety)

## Documentation

- [x] README with quick start
- [x] Language tutorial / developer guide
- [x] Optimization guide: cost pitfalls, boundary management, hash batching
- [x] Error catalog with recovery suggestions
- [ ] Language spec: clean up spec.md for public release

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
type-checker warnings surfaced in build/check output, pattern matching
(match/wildcard desugared to if/else), inline TASM blocks with stack
effect annotations.

**Standard Library** — 13 modules: std.io, std.hash, std.field,
std.convert, std.u32, std.xfield, std.mem, std.assert, std.merkle,
std.auth, std.kernel, std.utxo, std.storage. Digest destructuring.
`#[intrinsic]` restricted to std modules.

**CLI** — `trident build` (--costs, --hotspots, --hints, --annotate,
--save-costs, --compare), `trident check` (--costs), `trident fmt`
(directories, --check), `trident init`, `trident test`, `trident doc`,
`trident lsp`.

**LSP** — diagnostics, formatting, project-aware type checking, document
symbols, go-to-definition, hover (with cost), completions, signature help.

**Editor** — Zed extension, tree-sitter grammar with highlights.

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
tutorial, optimization guide, error catalog, fungible token example.
