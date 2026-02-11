# ir/lower — IR to Assembly Backends

Consumes `Vec<IROp>` and produces target-specific assembly text.

## Files

| File | Purpose | Key symbols |
|------|---------|-------------|
| [`mod.rs`](mod.rs) | Trait and factory | [`Lowering`](mod.rs:17) trait, [`create_lowering`](mod.rs:23) factory |
| [`triton.rs`](triton.rs) | Triton VM backend (TASM) | [`TritonLowering`](triton.rs:19), [`lower_op`](triton.rs:42), [`flush_deferred`](triton.rs:255) |
| [`miden.rs`](miden.rs) | Miden VM backend (MASM) | [`MidenLowering`](miden.rs:10), [`lower_op`](miden.rs:37) |
| [`tests.rs`](tests.rs) | Unit + comparison tests | per-backend tests, IRBuilder+Lowering vs [`Emitter`](../../codegen/emitter/mod.rs:30) comparison |

## Backend strategies

| Construct | Triton ([`triton.rs`](triton.rs)) | Miden ([`miden.rs`](miden.rs)) |
|-----------|--------|-------|
| `IfElse` | deferred subroutines: `skiz`+`call` ([`flush_deferred`](triton.rs:255)) | inline `if.true/else/end` |
| `IfOnly` | `skiz`+`call` to deferred block | inline `if.true/end` |
| `Loop` | labeled subroutine with `recurse` | inline `if.true/drop/else/exec.self/end` |
| `FnStart` | `__name:` label | `proc.name` |
| `FnEnd` | flushes deferred blocks | `end` |
| `EmitEvent` | `push tag; write_io 1` per field | comment + `drop` |
| `SealEvent` | pad + `hash` + `write_io 5` | pad + `hperm` + `drop` |

## Adding a backend

1. Create `new_target.rs` implementing [`Lowering`](mod.rs:17) (one method: `fn lower(&self, ops: &[IROp]) -> Vec<String>`)
2. Register in [`create_lowering`](mod.rs:23)
3. Add a [`TargetConfig`](../../tools/target.rs:20) variant

## Dependencies

- [`IROp`](../mod.rs:18) — the IR operations consumed by lowering
- [`IRBuilder`](../builder/mod.rs:37) — used in comparison tests
- [`Emitter`](../../codegen/emitter/mod.rs:30) — old backend, used as reference in comparison tests
