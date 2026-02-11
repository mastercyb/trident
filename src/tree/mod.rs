//! Tree IR — lowering for combinator/tree-rewriting VMs.
//!
//! Tree machines are neither stack nor register. Data is binary trees
//! (nouns) addressed by axes (2=left, 3=right). Computation is
//! subject-formula evaluation: every expression is `[subject formula] → result`.
//!
//! Tree lowering takes TIR directly (like KernelLowering) and produces
//! target-specific tree expressions — Nock formulas, or similar combinator
//! representations for future tree VMs.
//!
//! Pipeline:
//! ```text
//! AST → TIR ─→ StackLowering     → Vec<String>  (stack targets)
//!           ├→ LIR → RegisterLow  → Vec<u8>      (register targets)
//!           ├→ KIR → KernelLow    → String        (GPU kernel source)
//!           └→ TreeLowering       → Vec<u8>       (tree targets: Nock)
//! ```
//!
//! The key insight: TIR's structural control flow (nested IfElse/Loop bodies)
//! maps naturally to tree structure. Stack operations become tree construction
//! and axis addressing. The translation is:
//!
//! - **Stack → Subject**: the operand stack becomes a nested cons-tree (the subject)
//! - **Push → Literal**: `[1 value]` (Nock opcode 1 = constant)
//! - **Dup → Slot**: `[0 axis]` (Nock opcode 0 = tree lookup)
//! - **IfElse → Branch**: `[6 test yes no]` (Nock opcode 6 = branch)
//! - **Call → Evaluate**: `[2 subject formula]` (Nock opcode 2 = eval)
//! - **Hash → Jet**: jet-matched formula for Tip5 hash
//!
//! Supported targets:
//! - Nock (Nockchain) — 13-opcode tree-rewriting machine

pub mod lower;
