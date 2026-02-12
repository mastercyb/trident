# Source Architecture

The compiler is organized as a pipeline. Source text enters at the frontend, flows through type checking and optional analysis passes, and exits as target assembly. Each stage lives in its own module with a clear boundary.

```
source.tri
    |
    v
 frontend/     lexer -> parser -> AST
    |
    v
 typecheck/    type checking, borrow checking, generics
    |
    v
 legacy/       old AST-to-assembly emitter (deprecated, comparison tests only)
    |
    v
 output.tasm
```

Parallel to the main pipeline, several modules provide analysis, tooling, and package management:

```
 cost/         static cost analysis (trace height estimation)
 verify/       formal verification (symbolic execution, SMT, equivalence)
 tools/        LSP, scaffolding, module resolution, introspection
 pkgmgmt/     content-addressed package management, registry, on-chain proofs
```

## Module Map

| Module | LOC | What it does |
|--------|----:|--------------|
| [`common/`](common/) | 314 | Shared infrastructure: [source spans](common/span.rs), [diagnostics](common/diagnostic.rs), [type definitions](common/types.rs) |
| [`frontend/`](frontend/) | 4,392 | [Lexer](frontend/lexer.rs), [parser](frontend/parser.rs), [token definitions](frontend/lexeme.rs), [pretty-printer/formatter](frontend/format.rs) |
| [`typecheck/`](typecheck/) | 3,004 | [Type checker](typecheck/mod.rs) with borrow analysis, generics, and [builtin registration](typecheck/builtins.rs) |
| [`legacy/`](legacy/) | 4,189 | Old AST-to-assembly [emitter](legacy/emitter/), [backend trait](legacy/backend/) (deprecated) |
| [`stack.rs`](stack.rs) | — | LRU [stack manager](stack.rs) with automatic RAM spill/reload |
| [`linker.rs`](linker.rs) | — | Multi-module [linker](linker.rs) for cross-module calls |
| [`legacy/backend/`](legacy/backend/) | 802 | [`StackBackend`](legacy/backend/mod.rs) trait + five targets: [Triton](legacy/backend/triton.rs), [Miden](legacy/backend/miden.rs), [OpenVM](legacy/backend/openvm.rs), [SP1](legacy/backend/sp1.rs), [Cairo](legacy/backend/cairo.rs) |
| [`cost/`](cost/) | 2,335 | Static cost [analyzer](cost/analyzer.rs), per-function breakdown, [optimization hints and reports](cost/report.rs) |
| [`cost/model/`](cost/model/) | 771 | [`CostModel`](cost/model/mod.rs) trait + four targets: [Triton](cost/model/triton.rs), [Miden](cost/model/miden.rs), [Cycle](cost/model/cycle.rs), [Cairo](cost/model/cairo.rs) |
| [`verify/`](verify/) | 5,570 | [Symbolic execution](verify/sym.rs), [constraint solving](verify/solve.rs), [SMT encoding](verify/smt.rs), [equivalence checking](verify/equiv.rs), [invariant synthesis](verify/synthesize.rs), [JSON reports](verify/report.rs) |
| [`pkgmgmt/`](pkgmgmt/) | 7,737 | [BLAKE3 hashing](pkgmgmt/hash.rs), [Poseidon2](pkgmgmt/poseidon2.rs), [UCM codebase](pkgmgmt/ucm.rs), [registry server/client](pkgmgmt/registry.rs), [on-chain Merkle registry](pkgmgmt/onchain.rs), [dependency manifests](pkgmgmt/manifest.rs), [compilation cache](pkgmgmt/cache.rs) |
| [`tools/`](tools/) | 3,960 | [Language Server](tools/lsp.rs), [code scaffolding](tools/scaffold.rs), [definition viewer](tools/view.rs), [project config](tools/project.rs), [module resolution](tools/resolve.rs), [target configuration](tools/target.rs) |

## Top-Level Files

| File | LOC | Role |
|------|----:|------|
| [`ast.rs`](ast.rs) | 371 | AST node definitions shared by every stage |
| [`lib.rs`](lib.rs) | 2,698 | Public API, re-exports, and orchestration functions (`compile`, `analyze_costs`, `check_file`) |
| [`main.rs`](main.rs) | 2,374 | CLI entry point: argument parsing and command dispatch |
| [`trident_lsp.rs`](trident_lsp.rs) | 4 | LSP binary entry point |

## Compilation Pipeline in Detail

**Frontend** ([`frontend/`](frontend/)). The [lexer](frontend/lexer.rs) tokenizes source into the token types defined in [`lexeme.rs`](frontend/lexeme.rs). The [parser](frontend/parser.rs) produces a typed AST ([`ast.rs`](ast.rs)). The [formatter](frontend/format.rs) can pretty-print any AST back to canonical source.

**Type Checking** ([`typecheck/`](typecheck/)). The [type checker](typecheck/mod.rs) validates types, resolves generics via monomorphization, performs borrow/move analysis, and registers builtin function signatures ([`builtins.rs`](typecheck/builtins.rs)). Diagnostics are emitted for type mismatches, undefined variables, unused bindings, and borrow violations.

**Legacy Code Generation** ([`legacy/`](legacy/)). The old [emitter](legacy/emitter/) walks the typed AST and produces target assembly by calling methods on a [`StackBackend`](legacy/backend/mod.rs) trait. Each target ([Triton](legacy/backend/triton.rs), [Miden](legacy/backend/miden.rs), [OpenVM](legacy/backend/openvm.rs), [SP1](legacy/backend/sp1.rs), [Cairo](legacy/backend/cairo.rs)) implements this trait in its own file. Deprecated — kept only for comparison tests against the new TIR pipeline. The [stack manager](stack.rs) tracks operand positions with automatic RAM spill/reload. The [linker](linker.rs) resolves cross-module calls.

**Cost Analysis** ([`cost/`](cost/)). The [analyzer](cost/analyzer.rs) walks the AST and sums per-instruction costs using a target-specific [`CostModel`](cost/model/mod.rs). The [report module](cost/report.rs) formats results, generates optimization hints, and provides JSON serialization for `--compare` workflows.

**Formal Verification** ([`verify/`](verify/)). The [symbolic executor](verify/sym.rs) builds path constraints over the AST. The [solver](verify/solve.rs) uses Schwartz-Zippel randomized testing and bounded model checking. The [SMT module](verify/smt.rs) encodes constraints in SMT-LIB2 for external solvers. The [equivalence checker](verify/equiv.rs) proves two functions compute the same result. The [synthesizer](verify/synthesize.rs) infers loop invariants automatically.

**Package Management** ([`pkgmgmt/`](pkgmgmt/)). Content-addressed storage using BLAKE3 [hashing](pkgmgmt/hash.rs) with [Poseidon2](pkgmgmt/poseidon2.rs) for in-proof verification. The [UCM](pkgmgmt/ucm.rs) manages a local codebase of named, versioned definitions. The [registry](pkgmgmt/registry.rs) provides an HTTP server and client for publishing and pulling definitions. The [on-chain module](pkgmgmt/onchain.rs) implements a Merkle-tree registry for blockchain-anchored code.

## Design Principles

**Direct mapping**. Every language construct maps to a known instruction pattern. The compiler is a thin translation layer, not an optimization engine. This makes proving costs predictable and the compiler auditable.

**Target abstraction**. The [`StackLowering`](tir/lower/mod.rs) trait and [`CostModel`](cost/model/mod.rs) trait isolate all target-specific knowledge. Adding a new backend means implementing these two traits -- the rest of the compiler is shared.

**Re-exports for stability**. [`lib.rs`](lib.rs) re-exports every module at the crate root (`crate::parser`, `crate::emit`, etc.) so that internal reorganization does not break downstream code or the binary crate.

## Test Coverage

670 tests across all modules, including:
- Parser round-trip tests (parse -> format -> re-parse)
- Type checker positive and negative cases
- Code generation output validation per backend
- Cost model accuracy checks
- LSP protocol compliance
- CLI integration tests (20 subprocess tests, formerly in `tests/cli_tests.rs` -- now removed)
