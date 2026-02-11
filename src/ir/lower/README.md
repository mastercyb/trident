# ir/lower — IR to Assembly Backends

Consumes `Vec<IROp>` and produces target-specific assembly text.

## Files

- `mod.rs` — `Lowering` trait, `create_lowering(target)` factory
- `triton.rs` — Triton VM backend (TASM): deferred subroutines, `skiz`+`call` branching, `recurse` loops
- `miden.rs` — Miden VM backend (MASM): inline `if.true/else/end`, `proc/end` functions, `exec.self` loops
- `tests.rs` — per-backend unit tests + comparison tests (IRBuilder+TritonLowering vs old Emitter)

## Adding a backend

Implement the `Lowering` trait (one method: `fn lower(&self, ops: &[IROp]) -> Vec<String>`) and register it in `create_lowering`.
