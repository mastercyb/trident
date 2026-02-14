# tir/builder — AST to TIR Translation

Walks a type-checked AST and produces `Vec<TIROp>`. Target-independent.

## Files

| File | Purpose | Key symbols |
|------|---------|-------------|
| [`mod.rs`](mod.rs) | Core struct and entry points | [`TIRBuilder`](mod.rs:37), [`build_file`](mod.rs:144), [`build_fn`](mod.rs:291) |
| [`stmt.rs`](stmt.rs) | Statement emission | [`build_block`](stmt.rs:15), [`build_stmt`](stmt.rs:24), [`build_match`](stmt.rs:283) |
| [`expr.rs`](expr.rs) | Expression emission | [`build_expr`](expr.rs:11), [`build_var_expr`](expr.rs:116), [`build_field_access`](expr.rs:200), [`build_index`](expr.rs:271) |
| [`call.rs`](call.rs) | Function call dispatch | [`build_call`](call.rs:12) (~40 intrinsics), [`build_user_call`](call.rs:225) |
| [`helpers.rs`](helpers.rs) | Stack and control helpers | [`parse_spill_effect`](helpers.rs:16), [`flush_stack_effects`](helpers.rs:87), [`emit_and_push`](helpers.rs:96), [`fresh_label`](helpers.rs:80) |
| [`layout.rs`](layout.rs) | Type width and struct layout | [`format_type_name`](layout.rs:13), [`resolve_type_width`](layout.rs:29), [`register_struct_layout_from_type`](layout.rs:68) |
| [`tests.rs`](tests.rs) | Unit tests | builder output verification, spill parser tests |

## How it works

[`TIRBuilder`](mod.rs:37) maintains a [`StackManager`](../stack.rs:58) that models the runtime stack with LRU spill/reload to RAM. As it walks the AST, it pushes [`TIROp`](../mod.rs:18) variants and keeps the stack model in sync. Structural control flow (`IfElse`, `Loop`) captures nested bodies as `Vec<TIROp>` rather than emitting flat labels.

## Data flow

1. [`build_file`](mod.rs:144) — pre-scans for functions, structs, events, constants; emits preamble; iterates items
2. [`build_fn`](mod.rs:291) — registers params on stack, walks body via [`build_block`](stmt.rs:15)
3. [`build_stmt`](stmt.rs:24) — dispatches per statement kind (let/assign/if/for/match/emit/seal/asm)
4. [`build_expr`](expr.rs:11) — dispatches per expression kind (literal/var/binop/call/tuple/array/struct)
5. [`build_call`](call.rs:12) — resolves intrinsics or delegates to [`build_user_call`](call.rs:225)

## Dependencies

- [`TIROp`](../mod.rs:18) — the IR operations this builder produces
- [`StackManager`](../stack.rs:58) — stack depth tracking and spill/reload
- [`TargetConfig`](../../tools/target.rs:20) — VM parameters (stack depth, field widths)
- [`MonoInstance`](../../typecheck/mod.rs:32) — resolved generic instantiations
