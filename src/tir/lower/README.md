# tir/lower — TIR to Assembly Backend

Consumes `Vec<TIROp>` and produces Triton VM assembly (TASM).

## Files

| File | Purpose | Key symbols |
|------|---------|-------------|
| [`mod.rs`](mod.rs) | Trait and factory | [`StackLowering`](mod.rs:14) trait, [`create_stack_lowering`](mod.rs:20) factory |
| [`triton.rs`](triton.rs) | Triton VM backend (TASM) | [`TritonLowering`](triton.rs:19), [`lower_op`](triton.rs:42), [`flush_deferred`](triton.rs:255) |
| [`tests.rs`](tests.rs) | Unit + regression tests | per-op tests, end-to-end compilation tests |

## Triton lowering strategies

| Construct | Strategy |
|-----------|---------|
| `IfElse` | deferred subroutines: `skiz`+`call` ([`flush_deferred`](triton.rs:255)) |
| `IfOnly` | `skiz`+`call` to deferred block |
| `Loop` | labeled subroutine with `recurse` |
| `FnStart` | `__name:` label |
| `FnEnd` | flushes deferred blocks |
| `Open` | `push tag; write_io 1` per field |
| `Seal` | pad + `hash` + `write_io 5` |

## Adding a backend

1. Create `new_target.rs` implementing [`StackLowering`](mod.rs:14) (one method: `fn lower(&self, ops: &[TIROp]) -> Vec<String>`)
2. Register in [`create_stack_lowering`](mod.rs:20)
3. Add a [`TargetConfig`](../../tools/target.rs:20) variant

## Dependencies

- [`TIROp`](../mod.rs:18) — the IR operations consumed by lowering
- [`TIRBuilder`](../builder/mod.rs:37) — used in regression tests
