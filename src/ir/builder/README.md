# ir/builder — AST to IR Translation

Walks a type-checked AST and produces `Vec<IROp>`. Target-independent.

## Files

- `mod.rs` — `IRBuilder` struct, configuration, `build_file`/`build_fn`/`build_mono_fn`
- `stmt.rs` — statement emission: let, assign, if/else, for, match, emit, seal, asm
- `expr.rs` — expression emission: literals, variables, binops, field access, indexing, structs
- `call.rs` — function call dispatch: intrinsic resolution (~40 builtins) and user-defined calls
- `helpers.rs` — spill effect parser, cfg flag checks, label generation, stack flush helpers
- `layout.rs` — type width calculation, struct field layout registration and lookup
- `tests.rs` — unit tests

## How it works

IRBuilder maintains a `StackManager` that models the runtime stack with LRU spill/reload to RAM. As it walks the AST, it pushes `IROp` variants and keeps the stack model in sync. Structural control flow (`IfElse`, `Loop`) captures nested bodies as `Vec<IROp>` rather than emitting flat labels.
