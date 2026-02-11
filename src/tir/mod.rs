//! TIR — Trident Intermediate Representation.
//!
//! The TIR is a list of stack operations with structural control flow.
//! Each backend implements a `Lowering` that consumes `Vec<TIROp>` and
//! produces target assembly text.

pub mod builder;
pub mod lower;

use std::fmt;

// ─── IR Operations ────────────────────────────────────────────────

/// A single IR operation. Flat ops map 1:1 to stack-machine instructions.
/// Structural ops (`IfElse`, `IfOnly`, `Loop`) carry nested bodies so each
/// backend can choose its own control-flow lowering strategy.
#[derive(Debug, Clone)]
/// 53 variants in three tiers:
///
/// **Tier 1 — Core instructions** (1:1 with stack machine ops, universal)
///   Stack (5), Arithmetic (12), I/O (3), Memory (2), Assertions (2) = 24
///
/// **Tier 2 — Abstract operations** (semantic intent; each backend expands)
///   Hash (6), Merkle (2), Events (2), Storage (2) = 12
///
/// **Tier 3 — Structure & control flow**
///   Control flow (6), Program structure (5), Passthrough (2) = 13
///
/// **Recursion extension** (STARK-in-STARK verification; Triton-only for now) = 4
///
/// Total: 24 + 12 + 13 + 4 = 53 variants
pub enum TIROp {
    // ═══════════════════════════════════════════════════════════════
    // Tier 1 — Core instructions
    // 1:1 with stack machine primitives. Every backend maps these
    // directly to native instructions.
    // ═══════════════════════════════════════════════════════════════

    // ── Stack (5) ──
    Push(u64),
    PushNegOne,
    Pop(u32),
    Dup(u32),
    Swap(u32),

    // ── Arithmetic (12) ──
    Add,
    Mul,
    Eq,
    Lt,
    And,
    Xor,
    DivMod,
    Invert,
    Split,
    Log2,
    Pow,
    PopCount,

    // ── I/O (3) ──
    ReadIo(u32),
    WriteIo(u32),
    Divine(u32),

    // ── Memory (2) ──
    ReadMem(u32),
    WriteMem(u32),

    // ── Assertions (2) ──
    Assert,
    AssertVector,

    // ═══════════════════════════════════════════════════════════════
    // Tier 2 — Abstract operations
    // Semantic intent that each backend expands to its own native
    // pattern. The IR says *what*, the lowering decides *how*.
    // ═══════════════════════════════════════════════════════════════

    // ── Hash (6) ──
    Hash,
    SpongeInit,
    SpongeAbsorb,
    SpongeSqueeze,
    SpongeAbsorbMem,
    /// Compute a cryptographic hash digest. Inputs on stack per target config.
    /// Produces `digest_width` elements (from TargetConfig).
    HashDigest,

    // ── Merkle (2) ──
    MerkleStep,
    MerkleStepMem,

    // ── Events (2) ──
    /// Emit an observable event. Fields are on the stack (topmost = first field).
    /// Lowering maps to target-native events (Triton: write_io, EVM: LOG, etc.).
    EmitEvent {
        name: String,
        tag: u64,
        field_count: u32,
    },
    /// Emit a sealed (hashed) event commitment. ZK targets only.
    /// Fields are on the stack (topmost = first field).
    SealEvent {
        name: String,
        tag: u64,
        field_count: u32,
    },

    // ── Storage (2) ──
    /// Read from persistent storage. Key is on the stack.
    /// Produces `width` elements. Lowering maps to target-native storage.
    StorageRead {
        width: u32,
    },
    /// Write to persistent storage. Key and value(s) are on the stack.
    /// Lowering maps to target-native storage.
    StorageWrite {
        width: u32,
    },

    // ═══════════════════════════════════════════════════════════════
    // Tier 3 — Structure & control flow
    // Program organization and control flow. Structural ops carry
    // nested bodies so each backend chooses its own lowering strategy.
    // ═══════════════════════════════════════════════════════════════

    // ── Control flow — flat (3) ──
    Call(String),
    Return,
    Halt,

    // ── Control flow — structural (3) ──
    /// Conditional branch with both then and else bodies.
    /// Condition bool has already been consumed from the stack.
    IfElse {
        then_body: Vec<TIROp>,
        else_body: Vec<TIROp>,
    },
    /// Conditional branch with only a then body (no else).
    IfOnly {
        then_body: Vec<TIROp>,
    },
    /// Counted loop. Counter is on the stack. Body decrements and repeats.
    Loop {
        label: String,
        body: Vec<TIROp>,
    },

    // ── Program structure (5) ──
    /// Label definition.
    Label(String),
    /// Function start (label name).
    FnStart(String),
    /// Function end.
    FnEnd,
    /// Program entry preamble (main function label).
    Preamble(String),
    /// Blank line in output.
    BlankLine,

    // ── Passthrough (2) ──
    /// Comment text (without prefix — lowering adds target-specific prefix).
    Comment(String),
    /// Inline assembly passed through verbatim with declared stack effect.
    RawAsm {
        lines: Vec<String>,
        effect: i32,
    },

    // ═══════════════════════════════════════════════════════════════
    // Recursion extension — STARK-in-STARK verification primitives
    // Extension field arithmetic and FRI folding steps required for
    // recursive proof verification. Currently Triton-only; any backend
    // that supports recursive verification will need equivalents.
    // ═══════════════════════════════════════════════════════════════

    // ── Recursion — extension field & FRI (4) ──
    ExtMul,
    ExtInvert,
    FriFold,
    FriBaseFold,
}

// ─── Display ──────────────────────────────────────────────────────

impl fmt::Display for TIROp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TIROp::Push(v) => write!(f, "push {}", v),
            TIROp::PushNegOne => write!(f, "push -1"),
            TIROp::Pop(n) => write!(f, "pop {}", n),
            TIROp::Dup(d) => write!(f, "dup {}", d),
            TIROp::Swap(d) => write!(f, "swap {}", d),
            TIROp::Add => write!(f, "add"),
            TIROp::Mul => write!(f, "mul"),
            TIROp::Eq => write!(f, "eq"),
            TIROp::Lt => write!(f, "lt"),
            TIROp::And => write!(f, "and"),
            TIROp::Xor => write!(f, "xor"),
            TIROp::DivMod => write!(f, "div_mod"),
            TIROp::Invert => write!(f, "invert"),
            TIROp::Split => write!(f, "split"),
            TIROp::Log2 => write!(f, "log2"),
            TIROp::Pow => write!(f, "pow"),
            TIROp::PopCount => write!(f, "pop_count"),
            TIROp::ExtMul => write!(f, "ext_mul"),
            TIROp::ExtInvert => write!(f, "ext_invert"),
            TIROp::FriFold => write!(f, "fri_fold"),
            TIROp::FriBaseFold => write!(f, "fri_base_fold"),
            TIROp::ReadIo(n) => write!(f, "read_io {}", n),
            TIROp::WriteIo(n) => write!(f, "write_io {}", n),
            TIROp::Divine(n) => write!(f, "divine {}", n),
            TIROp::ReadMem(n) => write!(f, "read_mem {}", n),
            TIROp::WriteMem(n) => write!(f, "write_mem {}", n),
            TIROp::Hash => write!(f, "hash"),
            TIROp::SpongeInit => write!(f, "sponge_init"),
            TIROp::SpongeAbsorb => write!(f, "sponge_absorb"),
            TIROp::SpongeSqueeze => write!(f, "sponge_squeeze"),
            TIROp::SpongeAbsorbMem => write!(f, "sponge_absorb_mem"),
            TIROp::MerkleStep => write!(f, "merkle_step"),
            TIROp::MerkleStepMem => write!(f, "merkle_step_mem"),
            TIROp::Assert => write!(f, "assert"),
            TIROp::AssertVector => write!(f, "assert_vector"),
            TIROp::EmitEvent {
                name, field_count, ..
            } => write!(f, "emit_event {}({})", name, field_count),
            TIROp::SealEvent {
                name, field_count, ..
            } => write!(f, "seal_event {}({})", name, field_count),
            TIROp::StorageRead { width } => write!(f, "storage_read {}", width),
            TIROp::StorageWrite { width } => write!(f, "storage_write {}", width),
            TIROp::HashDigest => write!(f, "hash_digest"),
            TIROp::Call(label) => write!(f, "call {}", label),
            TIROp::Return => write!(f, "return"),
            TIROp::Halt => write!(f, "halt"),
            TIROp::IfElse {
                then_body,
                else_body,
            } => {
                write!(
                    f,
                    "if_else(then={}, else={})",
                    then_body.len(),
                    else_body.len()
                )
            }
            TIROp::IfOnly { then_body } => {
                write!(f, "if_only(then={})", then_body.len())
            }
            TIROp::Loop { label, body } => {
                write!(f, "loop {}(body={})", label, body.len())
            }
            TIROp::Label(name) => write!(f, "label {}", name),
            TIROp::FnStart(name) => write!(f, "fn_start {}", name),
            TIROp::FnEnd => write!(f, "fn_end"),
            TIROp::Preamble(main) => write!(f, "preamble {}", main),
            TIROp::BlankLine => write!(f, ""),
            TIROp::Comment(text) => write!(f, "// {}", text),
            TIROp::RawAsm { lines, effect } => {
                write!(f, "raw_asm({} lines, effect={})", lines.len(), effect)
            }
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_irop_display() {
        assert_eq!(format!("{}", TIROp::Push(42)), "push 42");
        assert_eq!(format!("{}", TIROp::Add), "add");
        assert_eq!(format!("{}", TIROp::Call("main".into())), "call main");
        assert_eq!(format!("{}", TIROp::Pop(3)), "pop 3");
        assert_eq!(format!("{}", TIROp::Dup(0)), "dup 0");
        assert_eq!(format!("{}", TIROp::Swap(5)), "swap 5");
    }

    #[test]
    fn test_irop_structural_display() {
        let op = TIROp::IfElse {
            then_body: vec![TIROp::Push(1), TIROp::Add],
            else_body: vec![TIROp::Push(0)],
        };
        assert_eq!(format!("{}", op), "if_else(then=2, else=1)");

        let op = TIROp::Loop {
            label: "loop_1".into(),
            body: vec![TIROp::Pop(1)],
        };
        assert_eq!(format!("{}", op), "loop loop_1(body=1)");
    }

    #[test]
    fn test_irop_clone() {
        let ops = vec![
            TIROp::Push(10),
            TIROp::Push(20),
            TIROp::Add,
            TIROp::IfElse {
                then_body: vec![TIROp::WriteIo(1)],
                else_body: vec![TIROp::Pop(1)],
            },
        ];
        let cloned = ops.clone();
        assert_eq!(ops.len(), cloned.len());
    }

    #[test]
    fn test_irop_all_variants_construct() {
        // Verify every variant can be constructed without panic
        let _ops: Vec<TIROp> = vec![
            TIROp::Push(0),
            TIROp::PushNegOne,
            TIROp::Pop(1),
            TIROp::Dup(0),
            TIROp::Swap(1),
            TIROp::Add,
            TIROp::Mul,
            TIROp::Eq,
            TIROp::Lt,
            TIROp::And,
            TIROp::Xor,
            TIROp::DivMod,
            TIROp::Invert,
            TIROp::Split,
            TIROp::Log2,
            TIROp::Pow,
            TIROp::PopCount,
            TIROp::ExtMul,
            TIROp::ExtInvert,
            TIROp::FriFold,
            TIROp::FriBaseFold,
            TIROp::ReadIo(1),
            TIROp::WriteIo(1),
            TIROp::Divine(1),
            TIROp::ReadMem(1),
            TIROp::WriteMem(1),
            TIROp::Hash,
            TIROp::SpongeInit,
            TIROp::SpongeAbsorb,
            TIROp::SpongeSqueeze,
            TIROp::SpongeAbsorbMem,
            TIROp::MerkleStep,
            TIROp::MerkleStepMem,
            TIROp::Assert,
            TIROp::AssertVector,
            TIROp::EmitEvent {
                name: "Transfer".into(),
                tag: 0,
                field_count: 2,
            },
            TIROp::SealEvent {
                name: "Nullifier".into(),
                tag: 1,
                field_count: 1,
            },
            TIROp::StorageRead { width: 1 },
            TIROp::StorageWrite { width: 1 },
            TIROp::HashDigest,
            TIROp::Call("f".into()),
            TIROp::Return,
            TIROp::Halt,
            TIROp::IfElse {
                then_body: vec![],
                else_body: vec![],
            },
            TIROp::IfOnly { then_body: vec![] },
            TIROp::Loop {
                label: "l".into(),
                body: vec![],
            },
            TIROp::Label("x".into()),
            TIROp::FnStart("main".into()),
            TIROp::FnEnd,
            TIROp::Preamble("main".into()),
            TIROp::BlankLine,
            TIROp::Comment("test".into()),
            TIROp::RawAsm {
                lines: vec!["nop".into()],
                effect: 0,
            },
        ];
    }
}
