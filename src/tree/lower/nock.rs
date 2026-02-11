//! Nock lowering — produces Nock formulas from TIR.
//!
//! Nock is a 13-opcode combinator VM where all data is binary trees (nouns)
//! and all computation is subject-formula evaluation. The Nockchain blockchain
//! runs Nock with jet-accelerated cryptographic primitives.
//!
//! ## Lowering Strategy
//!
//! TIR's stack semantics map to Nock's subject-formula model:
//!
//! 1. **Subject as stack**: the operand stack is encoded as a right-nested
//!    cons list in the subject: `[top [second [third ...]]]`.
//!    Stack depth N → axis `2^N` for the Nth element.
//!
//! 2. **Push → Nock 8**: `[8 [1 value] continuation]` — prepend value to subject.
//!
//! 3. **Dup(n) → Nock 0**: `[0 axis]` — look up the nth stack element by axis.
//!
//! 4. **Arithmetic → Jets**: field arithmetic maps to jet-matched formulas.
//!    The Nock formula must match the jet's registered hash for acceleration.
//!    Without jet matching, arithmetic would be Church-encoded (unusably slow).
//!
//! 5. **IfElse → Nock 6**: `[6 test then else]` — native conditional.
//!
//! 6. **Call → Nock 9**: `[9 axis core]` — pull arm from core and evaluate.
//!
//! 7. **Hash → Tip5 jet**: the `%tip5` jet provides native Tip5 hashing
//!    (DIGEST=5, RATE=10, ROUNDS=7, Goldilocks field).
//!
//! ## Jet Categories (from Nockchain)
//!
//! - `BASE_FIELD_JETS` — Belt (Goldilocks) arithmetic: add, sub, mul, inv, neg
//! - `EXTENSION_FIELD_JETS` — Felt ([Belt; 3]) cubic extension arithmetic
//! - `ZTD_JETS` — Tip5 hash, sponge operations, Merkle authentication
//! - `CURVE_JETS` — Cheetah curve (F6lt) point operations
//! - `ZKVM_TABLE_JETS_V2` — STARK verification table lookups
//! - `XTRA_JETS` — FRI verification, STARK recursion

use super::{Noun, TreeLowering};
use crate::tir::TIROp;

/// Nock lowering backend for the Nockchain VM.
pub struct NockLowering;

impl NockLowering {
    pub fn new() -> Self {
        Self
    }

    /// Compute the axis for stack position `depth` in a right-nested cons list.
    ///
    /// Stack layout: `[top [1 [2 [3 ...]]]]`
    /// - depth 0 → axis 2 (head of subject)
    /// - depth 1 → axis 6 (head of tail)
    /// - depth 2 → axis 14 (head of tail of tail)
    /// - depth n → axis 2 * (2^(n+1) - 1) ... simplified: 2 * 2^n + something
    ///
    /// Actually for right-nested: depth 0 = axis 2, depth n = 2*(2^n) + walk.
    /// Simpler: depth 0 → 2, depth n → (2 << n) | path bits.
    /// Even simpler: for a list `[a [b [c d]]]`:
    ///   a = /2, b = /6, c = /14, d = /15
    /// Pattern: item at depth n has axis = 2^(n+1) for n < last, or 2^n + 1 for last.
    /// For our stack (arbitrarily deep):
    ///   depth 0 → /2
    ///   depth n → /((1 << (n+1)) + (1 << n) - 1)  ... no.
    /// Just: to reach depth n, go right n times then left once.
    ///   axis = 2 (left) after n rights from root.
    ///   right = *3 (multiply axis by 2 and add 1)
    ///   left  = *2 (multiply axis by 2)
    ///   Start at 1 (root). Go right n times: axis = (1 << n) + (1 << n) - 1
    ///   Actually: right once = 3, right twice = 7, right n = 2^(n+1) - 1
    ///   Then left: axis = 2 * (2^(n+1) - 1) = 2^(n+2) - 2
    ///   depth 0: axis = 2*(2^1 - 1) = 2. Correct.
    ///   depth 1: axis = 2*(2^2 - 1) = 6. Correct.
    ///   depth 2: axis = 2*(2^3 - 1) = 14. Correct.
    pub fn stack_axis(depth: u32) -> u64 {
        2 * ((1u64 << (depth + 1)) - 1)
    }

    /// Lower a single TIR operation to a Nock formula fragment.
    ///
    /// This is the core translation. Each TIROp becomes a Nock formula
    /// that transforms the subject (stack state).
    fn lower_op(&self, op: &TIROp) -> Noun {
        match op {
            // ── Stack operations ──
            TIROp::Push(value) => {
                // [8 [1 value] [0 1]] — push value onto subject, continue
                Noun::push(Noun::constant(Noun::atom(*value)), Noun::slot(1))
            }

            TIROp::Dup(depth) => {
                // [0 axis] — look up stack element
                Noun::slot(Self::stack_axis(*depth))
            }

            TIROp::Pop(n) => {
                // Remove top n elements: take the (n+1)th tail
                // For pop 1: [0 3] (tail of subject = rest of stack)
                let mut axis = 1u64;
                for _ in 0..*n {
                    axis = axis * 2 + 1; // go right (tail)
                }
                Noun::slot(axis)
            }

            TIROp::Swap(_depth) => {
                // Swap top with element at depth — complex tree edit
                // [10 [axis [0 2]] [10 [2 [0 old_axis]] [0 1]]]
                // Stub: identity for now
                Noun::slot(1)
            }

            // ── Arithmetic (jet-matched) ──
            TIROp::Add => {
                // Jet: %add in BASE_FIELD_JETS
                // Formula must hash-match the jet registration.
                // Stub: hint-wrapped identity
                Noun::hint(Noun::atom(0x616464), Noun::slot(1)) // hint %add
            }
            TIROp::Sub => {
                Noun::hint(Noun::atom(0x737562), Noun::slot(1)) // hint %sub
            }
            TIROp::Mul => {
                Noun::hint(Noun::atom(0x6d756c), Noun::slot(1)) // hint %mul
            }
            TIROp::Neg => {
                Noun::hint(Noun::atom(0x6e6567), Noun::slot(1)) // hint %neg
            }
            TIROp::Invert => {
                Noun::hint(Noun::atom(0x696e76), Noun::slot(1)) // hint %inv
            }

            // ── Comparison ──
            TIROp::Eq => {
                // [5 [0 2] [0 6]] — Nock 5 = equality test on top two
                Noun::equals(Noun::slot(2), Noun::slot(6))
            }
            TIROp::Lt => {
                Noun::hint(Noun::atom(0x6c74), Noun::slot(1)) // hint %lt — needs jet
            }

            // ── Control flow ──
            TIROp::IfElse {
                then_body,
                else_body,
            } => {
                // [6 [0 2] then else] — branch on top of stack
                let then_noun = self.lower_sequence(then_body);
                let else_noun = self.lower_sequence(else_body);
                Noun::branch(Noun::slot(2), then_noun, else_noun)
            }
            TIROp::IfOnly { then_body } => {
                let then_noun = self.lower_sequence(then_body);
                Noun::branch(Noun::slot(2), then_noun, Noun::slot(1))
            }
            TIROp::Loop { label: _, body } => {
                // Nock loop: [8 [1 0] [6 test [9 2 [0 1]] [0 1]]]
                // Stub: lower body once (proper recursion needs core pattern)
                self.lower_sequence(body)
            }

            // ── Functions ──
            TIROp::Call(_name) => {
                // [9 axis core] — invoke arm. Axis determined by function table.
                // Stub: use hint with function name
                Noun::hint(
                    Noun::cell(Noun::atom(0x63616c6c), Noun::atom(0)), // %call
                    Noun::slot(1),
                )
            }
            TIROp::Return => Noun::slot(1),
            TIROp::Halt => Noun::slot(1),

            // ── Hash (Tip5 jet) ──
            TIROp::Hash { width: _ } => {
                // Tip5 hash — jet-matched via ZTD_JETS
                // DIGEST_LENGTH=5, STATE_SIZE=16, RATE=10, NUM_ROUNDS=7
                Noun::hint(Noun::atom(0x74697035), Noun::slot(1)) // hint %tip5
            }

            // ── I/O ──
            TIROp::ReadIo(n) => {
                // Nock scry: [12 ref path] — opcode 12
                Noun::cell(
                    Noun::atom(12),
                    Noun::cell(Noun::atom(0), Noun::atom(*n as u64)),
                )
            }
            TIROp::WriteIo(n) => {
                // Output via hint effect
                Noun::hint(
                    Noun::cell(Noun::atom(0x696f), Noun::atom(*n as u64)), // %io
                    Noun::slot(1),
                )
            }

            // ── Witness ──
            TIROp::Hint(n) => Noun::hint(
                Noun::cell(Noun::atom(0x68696e74), Noun::atom(*n as u64)),
                Noun::slot(1),
            ),

            // ── Memory ──
            TIROp::ReadMem(n) => {
                // Tree addressing: memory is part of the subject tree
                Noun::hint(
                    Noun::cell(Noun::atom(0x726d), Noun::atom(*n as u64)), // %rm
                    Noun::slot(1),
                )
            }
            TIROp::WriteMem(n) => {
                // [10 [axis value] subject] — edit subject tree
                Noun::hint(
                    Noun::cell(Noun::atom(0x776d), Noun::atom(*n as u64)), // %wm
                    Noun::slot(1),
                )
            }

            // ── Assertions ──
            TIROp::Assert(_n) => {
                // Assert via crash: [6 test [0 1] [0 0]] — crash on false
                Noun::branch(Noun::slot(2), Noun::slot(1), Noun::slot(0))
            }

            // ── Sponge (Tier 2 — Tip5 sponge jets) ──
            TIROp::SpongeInit => {
                Noun::hint(Noun::atom(0x73696e6974), Noun::slot(1)) // %sinit
            }
            TIROp::SpongeAbsorb => {
                Noun::hint(Noun::atom(0x736162), Noun::slot(1)) // %sab
            }
            TIROp::SpongeSqueeze => {
                Noun::hint(Noun::atom(0x73717a), Noun::slot(1)) // %sqz
            }
            TIROp::SpongeLoad => {
                Noun::hint(Noun::atom(0x736c64), Noun::slot(1)) // %sld
            }

            // ── Merkle (Tier 2) ──
            TIROp::MerkleStep => {
                Noun::hint(Noun::atom(0x6d73), Noun::slot(1)) // %ms
            }
            TIROp::MerkleLoad => {
                Noun::hint(Noun::atom(0x6d6c), Noun::slot(1)) // %ml
            }

            // ── Extension field (Tier 3 — cubic extension jets) ──
            TIROp::ExtMul => {
                // EXTENSION_FIELD_JETS — Felt multiplication
                Noun::hint(Noun::atom(0x786d756c), Noun::slot(1)) // %xmul
            }
            TIROp::ExtInvert => {
                Noun::hint(Noun::atom(0x78696e76), Noun::slot(1)) // %xinv
            }

            // ── Folding (Tier 3 — FRI jets via XTRA_JETS) ──
            TIROp::FoldExt => {
                Noun::hint(Noun::atom(0x66657874), Noun::slot(1)) // %fext
            }
            TIROp::FoldBase => {
                Noun::hint(Noun::atom(0x66626173), Noun::slot(1)) // %fbas
            }

            // ── Verification (Tier 3) ──
            TIROp::ProofBlock {
                program_hash: _,
                body,
            } => {
                // Recursive verification via ZKVM_TABLE_JETS_V2
                self.lower_sequence(body)
            }

            // ── Events ──
            TIROp::Reveal {
                name: _,
                tag,
                field_count: _,
            } => {
                Noun::hint(
                    Noun::cell(Noun::atom(0x726576), Noun::atom(*tag)), // %rev
                    Noun::slot(1),
                )
            }
            TIROp::Seal {
                name: _,
                tag,
                field_count: _,
            } => {
                Noun::hint(
                    Noun::cell(Noun::atom(0x7365616c), Noun::atom(*tag)), // %seal
                    Noun::slot(1),
                )
            }

            // ── Storage ──
            TIROp::ReadStorage { width } => Noun::hint(
                Noun::cell(Noun::atom(0x7273), Noun::atom(*width as u64)),
                Noun::slot(1),
            ),
            TIROp::WriteStorage { width } => Noun::hint(
                Noun::cell(Noun::atom(0x7773), Noun::atom(*width as u64)),
                Noun::slot(1),
            ),

            // ── Bitwise ──
            TIROp::And => Noun::hint(Noun::atom(0x616e64), Noun::slot(1)),
            TIROp::Or => Noun::hint(Noun::atom(0x6f72), Noun::slot(1)),
            TIROp::Xor => Noun::hint(Noun::atom(0x786f72), Noun::slot(1)),
            TIROp::PopCount => Noun::hint(Noun::atom(0x706f70), Noun::slot(1)),
            TIROp::Split => Noun::hint(Noun::atom(0x73706c), Noun::slot(1)),

            // ── Unsigned arithmetic ──
            TIROp::DivMod => Noun::hint(Noun::atom(0x64766d), Noun::slot(1)),
            TIROp::Shl => Noun::hint(Noun::atom(0x73686c), Noun::slot(1)),
            TIROp::Shr => Noun::hint(Noun::atom(0x736872), Noun::slot(1)),
            TIROp::Log2 => Noun::hint(Noun::atom(0x6c6732), Noun::slot(1)),
            TIROp::Pow => Noun::hint(Noun::atom(0x706f77), Noun::slot(1)),

            // ── Structure (passthrough) ──
            TIROp::FnStart(_) | TIROp::FnEnd | TIROp::Entry(_) => Noun::slot(1),
            TIROp::Comment(_) => Noun::slot(1),
            TIROp::Asm { .. } => Noun::slot(1), // inline asm not supported for tree targets
        }
    }

    /// Lower a sequence of TIR ops by composing them with Nock 7 (compose).
    fn lower_sequence(&self, ops: &[TIROp]) -> Noun {
        if ops.is_empty() {
            return Noun::slot(1); // identity
        }
        if ops.len() == 1 {
            return self.lower_op(&ops[0]);
        }

        // Chain: [7 op1 [7 op2 [7 op3 ...]]]
        let mut result = self.lower_op(&ops[ops.len() - 1]);
        for op in ops[..ops.len() - 1].iter().rev() {
            result = Noun::compose(self.lower_op(op), result);
        }
        result
    }
}

impl TreeLowering for NockLowering {
    fn target_name(&self) -> &str {
        "nock"
    }

    fn lower(&self, ops: &[TIROp]) -> Noun {
        self.lower_sequence(ops)
    }

    fn serialize(&self, noun: &Noun) -> Vec<u8> {
        // Jam serialization stub.
        // Full implementation: bit-level encoding per Nock jam spec.
        // For now, produce a human-readable representation.
        let text = format!("{}", noun);
        text.into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stack_axis() {
        assert_eq!(NockLowering::stack_axis(0), 2); // top of stack
        assert_eq!(NockLowering::stack_axis(1), 6); // second element
        assert_eq!(NockLowering::stack_axis(2), 14); // third element
        assert_eq!(NockLowering::stack_axis(3), 30); // fourth element
    }

    #[test]
    fn test_lower_push() {
        let lowering = NockLowering::new();
        let ops = vec![TIROp::Push(42)];
        let noun = lowering.lower(&ops);
        // [8 [1 42] [0 1]]
        assert_eq!(format!("{}", noun), "[8 [[1 42] [0 1]]]");
    }

    #[test]
    fn test_lower_push_add() {
        let lowering = NockLowering::new();
        let ops = vec![TIROp::Push(1), TIROp::Push(2), TIROp::Add];
        let noun = lowering.lower(&ops);
        // Should compose: push 1, then push 2, then add (jet hint)
        let text = format!("{}", noun);
        assert!(text.contains("[8 [[1 1]")); // push 1
        assert!(text.contains("[8 [[1 2]")); // push 2
    }

    #[test]
    fn test_lower_if_else() {
        let lowering = NockLowering::new();
        let ops = vec![TIROp::IfElse {
            then_body: vec![TIROp::Push(1)],
            else_body: vec![TIROp::Push(0)],
        }];
        let noun = lowering.lower(&ops);
        let text = format!("{}", noun);
        // Should contain Nock 6 (branch)
        assert!(text.starts_with("[6 "));
    }

    #[test]
    fn test_lower_hash() {
        let lowering = NockLowering::new();
        let ops = vec![TIROp::Hash { width: 10 }];
        let noun = lowering.lower(&ops);
        let text = format!("{}", noun);
        // Should contain Tip5 hint
        assert!(text.contains("[11 "));
    }

    #[test]
    fn test_lower_empty() {
        let lowering = NockLowering::new();
        let noun = lowering.lower(&[]);
        assert_eq!(format!("{}", noun), "[0 1]"); // identity
    }

    #[test]
    fn test_serialize() {
        let lowering = NockLowering::new();
        let noun = Noun::atom(42);
        let bytes = lowering.serialize(&noun);
        assert_eq!(bytes, b"42");
    }
}
