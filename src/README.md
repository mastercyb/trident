# Source Architecture

The compiler is organized as a pipeline. Source text enters at the frontend, flows through type checking and optional analysis passes, and exits as target assembly. Each stage lives in its own module with a clear boundary.

```
source.tri
    |
    v
 frontend/     lexer -> parser -> AST
    |
    v
 typeck/       type checking, borrow checking, generics
    |
    v
 codegen/      AST -> target assembly (via StackBackend trait)
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
| `common/` | 314 | Shared infrastructure: source spans, diagnostics, type definitions |
| `frontend/` | 4,392 | Lexer, parser, token definitions, pretty-printer/formatter |
| `typeck/` | 3,004 | Type checker with borrow analysis, generics, and builtin registration |
| `codegen/` | 4,189 | AST-to-assembly emitter, stack manager, linker, backend trait |
| `codegen/backend/` | 802 | `StackBackend` trait + five target implementations (Triton, Miden, OpenVM, SP1, Cairo) |
| `cost/` | 2,335 | Static cost analyzer, per-function cost breakdown, optimization hints |
| `cost/model/` | 771 | `CostModel` trait + four target cost models |
| `verify/` | 5,570 | Symbolic execution, constraint solving, SMT encoding, equivalence checking, invariant synthesis |
| `pkgmgmt/` | 7,737 | Content-addressed hashing (BLAKE3), Poseidon2, UCM codebase manager, registry server/client, on-chain Merkle registry, dependency manifests, compilation cache |
| `tools/` | 3,960 | Language Server Protocol, code scaffolding, definition viewer, project config, module resolution, target configuration |

## Top-Level Files

| File | LOC | Role |
|------|----:|------|
| `ast.rs` | 371 | AST node definitions shared by every stage |
| `lib.rs` | 2,698 | Public API, re-exports, and orchestration functions (`compile`, `analyze_costs`, `check_file`) |
| `main.rs` | 2,374 | CLI entry point: argument parsing and command dispatch |
| `bin/trident-lsp.rs` | 4 | LSP binary wrapper |

## Compilation Pipeline in Detail

**Frontend** (`frontend/`). The lexer (`lexer.rs`) tokenizes source into the token types defined in `lexeme.rs`. The parser (`parser.rs`) produces a typed AST (`ast.rs`). The formatter (`format.rs`) can pretty-print any AST back to canonical source.

**Type Checking** (`typeck/`). The type checker validates types, resolves generics via monomorphization, performs borrow/move analysis, and registers builtin function signatures (`builtins.rs`). Diagnostics are emitted for type mismatches, undefined variables, unused bindings, and borrow violations.

**Code Generation** (`codegen/`). The emitter (`emitter.rs`) walks the typed AST and produces target assembly by calling methods on a `StackBackend` trait (`backend/mod.rs`). Each target (Triton, Miden, OpenVM, SP1, Cairo) implements this trait in its own file. The stack manager (`stack.rs`) tracks operand positions with automatic RAM spill/reload. The linker (`linker.rs`) resolves cross-module calls.

**Cost Analysis** (`cost/`). The analyzer (`analyzer.rs`) walks the AST and sums per-instruction costs using a target-specific `CostModel` (`model/`). The report module (`report.rs`) formats results, generates optimization hints, and provides JSON serialization for `--compare` workflows.

**Formal Verification** (`verify/`). The symbolic executor (`sym.rs`) builds path constraints over the AST. The solver (`solve.rs`) uses Schwartz-Zippel randomized testing and bounded model checking. The SMT module (`smt.rs`) encodes constraints in SMT-LIB2 for external solvers. The equivalence checker (`equiv.rs`) proves two functions compute the same result. The synthesizer (`synthesize.rs`) infers loop invariants automatically.

**Package Management** (`pkgmgmt/`). Content-addressed storage using BLAKE3 hashing (`hash.rs`) with Poseidon2 (`poseidon2.rs`) for in-proof verification. The UCM (`ucm.rs`) manages a local codebase of named, versioned definitions. The registry (`registry.rs`) provides an HTTP server and client for publishing and pulling definitions. The on-chain module (`onchain.rs`) implements a Merkle-tree registry for blockchain-anchored code.

## Design Principles

**Direct mapping**. Every language construct maps to a known instruction pattern. The compiler is a thin translation layer, not an optimization engine. This makes proving costs predictable and the compiler auditable.

**Target abstraction**. The `StackBackend` trait and `CostModel` trait isolate all target-specific knowledge. Adding a new backend means implementing these two traits -- the rest of the compiler is shared.

**Re-exports for stability**. `lib.rs` re-exports every module at the crate root (`crate::parser`, `crate::emit`, etc.) so that internal reorganization does not break downstream code or the binary crate.

## Test Coverage

670 tests across all modules, including:
- Parser round-trip tests (parse -> format -> re-parse)
- Type checker positive and negative cases
- Code generation output validation per backend
- Cost model accuracy checks
- LSP protocol compliance
- CLI integration tests (20 subprocess tests in `tests/cli_tests.rs`)
