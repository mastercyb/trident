# ir — Intermediate Representation

Target-independent IR between the AST and backend assembly.

The compiler pipeline is: **parse -> typecheck -> IRBuilder -> Lowering -> assembly text**.

## Structure

- [`mod.rs`](mod.rs) — [`IROp`](mod.rs:18) enum (~50 variants): stack ops, arithmetic, control flow, abstract events/storage. [`Display`](mod.rs:143) impl for debug printing.
- [`builder/`](builder/) — AST-to-IR translation (target-independent). See [builder/README.md](builder/README.md).
- [`lower/`](lower/) — IR-to-assembly backends (target-specific). See [lower/README.md](lower/README.md).

## Key design

IROp uses **structural control flow** — `IfElse`, `IfOnly`, `Loop` carry nested `Vec<IROp>` bodies so each backend can choose its own lowering strategy without a shared CFG.

Abstract ops (`EmitEvent`, `SealEvent`, `StorageRead/Write`, `HashDigest`) keep the IR target-independent while backends map them to native instructions.

## Dependencies

- [`TargetConfig`](../tools/target.rs:20) — VM parameters (stack depth, digest width, hash rate)
- [`MonoInstance`](../typecheck/mod.rs:32) — monomorphized generic function instances from the type checker
- [`StackManager`](../codegen/stack.rs:58) / [`SpillFormatter`](../codegen/stack.rs:16) — stack model with automatic RAM spill/reload

## Entry point

Compilation uses IR via [`src/lib.rs`](../lib.rs) — builds IR with [`IRBuilder`](builder/mod.rs:37) then lowers with [`create_lowering`](lower/mod.rs:23).
