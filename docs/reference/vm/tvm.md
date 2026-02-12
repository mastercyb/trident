# TVM (Ton Virtual Machine)

[← Target Reference](../targets.md)

---

## VM Parameters

| Parameter | Value |
|---|---|
| Architecture | Stack (~700 opcodes) |
| Word size | 257-bit signed integers |
| Hash function | SHA-256 |
| Digest width | 32 bytes |
| Stack depth | 256 (operand) + 256 (control) |
| Output format | `.boc` (Bag of Cells) |
| Cost model | Gas (per-opcode + cell creation/storage) |

Stack-based VM with ~700 opcodes operating on 257-bit signed integers and
cells (tree-structured data). TVM operates on a cell-based data model: all
data is trees of cells (up to 1023 bits + 4 references each). This is
distinct from both byte-addressed memory and field-element stacks.

Stack-based architecture shares the `StackLowering` path with TRITON
and MIDEN, adapted for TVM's wider word size and cell model. The compiler
manages cell serialization/deserialization automatically.

Gas costs are per-opcode with additional charges for cell creation, storage,
and message sending.

See [os/ton.md](../os/ton.md) for the Ton OS runtime.

---

## Cost Model (Gas)

Per-opcode gas plus cell creation and storage charges.

| Operation class | Gas | Notes |
|---|---:|---|
| Arithmetic / logic | 18-26 | Stack operations on 257-bit ints |
| Comparison | 18 | Integer comparisons |
| Cell load | 100 | CTOS (cell to slice) |
| Cell store | 500 | STREF, ENDC |
| Dict ops | 50-100 | Tree lookup/insert |
| Hash | 26 | HASHCU, HASHSU |
| Send message | 1,000+ | SENDRAWMSG (plus forwarding fees) |

Cell creation dominates in data-heavy programs. Detailed cost model planned.
