# ir — Intermediate Representation

Target-independent IR between the AST and backend assembly.

The compiler pipeline is: **parse -> typecheck -> IRBuilder -> Lowering -> assembly text**.

## Structure

- `mod.rs` — `IROp` enum (~50 variants): stack ops, arithmetic, control flow, abstract events/storage
- `builder/` — AST-to-IR translation (target-independent)
- `lower/` — IR-to-assembly backends (target-specific)

## Key design

IROp uses **structural control flow** (`IfElse`, `IfOnly`, `Loop` carry nested `Vec<IROp>` bodies) so each backend can choose its own lowering strategy without a shared CFG.

Abstract ops (`EmitEvent`, `SealEvent`, `StorageRead/Write`, `HashDigest`) let the IR stay target-independent while backends map them to native instructions.
