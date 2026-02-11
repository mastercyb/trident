# tir — Trident Intermediate Representation

Target-independent TIR between the AST and backend assembly.

The compiler pipeline is: **parse -> typecheck -> TIRBuilder -> Lowering -> assembly text**.

## Structure

- [`mod.rs`](mod.rs) — [`TIROp`](mod.rs:18) enum (53 variants in 4 tiers): Tier 0 structure, Tier 1 universal (stack, arithmetic, I/O, memory, hash, events, storage), Tier 2 provable (sponge, merkle), Tier 3 recursion (extension field, FRI). [`Display`](mod.rs:186) impl for debug printing.
- [`builder/`](builder/) — AST-to-IR translation (target-independent). See [builder/README.md](builder/README.md).
- [`lower/`](lower/) — IR-to-assembly backends (target-specific). See [lower/README.md](lower/README.md).

## Key design

Higher tier = narrower target set. Tier 0 (structure) runs anywhere. Tier 1 (universal) compiles to every blockchain. Tier 2 (provable) requires proof-capable targets. Tier 3 (recursion) requires recursive verification.

Structural ops (`IfElse`, `IfOnly`, `Loop`) carry nested `Vec<TIROp>` bodies so each backend can choose its own control-flow lowering strategy. Abstract ops (`Open`, `ReadStorage/WriteStorage`, `HashDigest`) keep the TIR target-independent while backends map them to native instructions.

## Dependencies

- [`TargetConfig`](../tools/target.rs:20) — VM parameters (stack depth, digest width, hash rate)
- [`MonoInstance`](../typecheck/mod.rs:32) — monomorphized generic function instances from the type checker
- [`StackManager`](../codegen/stack.rs:58) / [`SpillFormatter`](../codegen/stack.rs:16) — stack model with automatic RAM spill/reload

## Entry point

Compilation uses IR via [`src/lib.rs`](../lib.rs) — builds IR with [`TIRBuilder`](builder/mod.rs:37) then lowers with [`create_lowering`](lower/mod.rs:23).
