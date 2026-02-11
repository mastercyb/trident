//! TIR — Trident Intermediate Representation.
//!
//! The TIR is a list of stack operations with structural control flow.
//! Each backend implements a `StackLowering` that consumes `Vec<TIROp>` and
//! produces target assembly text.

pub mod builder;
pub mod lower;

use std::fmt;

// ─── IR Operations ────────────────────────────────────────────────

/// 54 TIR operations across 4 tiers. Higher tier = narrower target set.
///
/// **Tier 0 — Structure** (every program, every target)
///   Control flow (6), Program structure (3), Passthrough (2) = 11
///
/// **Tier 1 — Universal** (compiles to every target including non-provable)
///   Stack (4), Modular arithmetic (5), Comparison (2), Bitwise (5),
///   Unsigned arithmetic (5), I/O (2), Memory (2),
///   Assertions (1), Hash (1), Events (2), Storage (2) = 31
///
/// **Tier 2 — Provable** (requires a proof-capable target)
///   Witness (1), Sponge (4), Merkle (2) = 7
///
/// **Tier 3 — Recursion** (requires recursive verification capability)
///   Extension field (2), Folding (2), Verification (1) = 5
///
/// Total: 11 + 31 + 7 + 5 = 54 variants
#[derive(Debug, Clone)]
pub enum TIROp {
    // ═══════════════════════════════════════════════════════════════
    // Tier 0 — Structure (11)
    // The scaffolding. Present in every program, on every target.
    // Not blockchain-specific — just computation.
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

    // ── Program structure (3) ──
    /// Function start (label name).
    FnStart(String),
    /// Function end.
    FnEnd,
    /// Program entry point (main function label).
    Entry(String),

    // ── Passthrough (2) ──
    /// Comment text (without prefix — lowering adds target-specific prefix).
    Comment(String),
    /// Inline assembly passed through verbatim with declared stack effect.
    Asm {
        lines: Vec<String>,
        effect: i32,
    },

    // ═══════════════════════════════════════════════════════════════
    // Tier 1 — Universal (31)
    // Compiles to every target. Stack primitives, arithmetic,
    // I/O, memory, hashing, events, storage.
    // ═══════════════════════════════════════════════════════════════

    // ── Stack (4) ──
    Push(u64),
    Pop(u32),
    Dup(u32),
    Swap(u32),

    // ── Modular arithmetic (5) ──
    Add,
    Sub,
    Mul,
    Neg,
    Invert,

    // ── Comparison (2) ──
    Eq,
    Lt,

    // ── Bitwise (5) ──
    And,
    Or,
    Xor,
    PopCount,
    Split,

    // ── Unsigned arithmetic (5) ──
    DivMod,
    Shl,
    Shr,
    Log2,
    Pow,

    // ── I/O (2) ──
    ReadIo(u32),
    WriteIo(u32),

    // ── Memory (2) ──
    ReadMem(u32),
    WriteMem(u32),

    // ── Assertions (1) ──
    /// Assert `n` elements. Assert(1) = single, Assert(5) = vector.
    Assert(u32),

    // ── Hash (1) ──
    /// Cryptographic hash. Width is metadata for optimization;
    /// both targets emit the same instruction regardless.
    Hash {
        width: u32,
    },

    // ── Events (2) ──
    /// Reveal an observable event. Fields are on the stack (topmost = first field).
    /// Lowering maps to target-native events (Triton: write_io, EVM: LOG, etc.).
    Reveal {
        name: String,
        tag: u64,
        field_count: u32,
    },
    /// Seal (hash-commit) an event. Fields are on the stack (topmost = first field).
    Seal {
        name: String,
        tag: u64,
        field_count: u32,
    },

    // ── Storage (2) ──
    /// Read from persistent storage. Key is on the stack.
    /// Produces `width` elements. Lowering maps to target-native storage.
    ReadStorage {
        width: u32,
    },
    /// Write to persistent storage. Key and value(s) are on the stack.
    /// Lowering maps to target-native storage.
    WriteStorage {
        width: u32,
    },

    // ═══════════════════════════════════════════════════════════════
    // Tier 2 — Provable (7)
    // Requires a proof-capable target. Witness input, sponge construction,
    // and Merkle authentication have no meaningful equivalent on
    // conventional VMs.
    // ═══════════════════════════════════════════════════════════════

    // ── Witness (1) ──
    /// Non-deterministic hint input. Hints are a proof-system concept,
    /// not general I/O.
    Hint(u32),

    // ── Sponge (4) ──
    SpongeInit,
    SpongeAbsorb,
    SpongeSqueeze,
    SpongeLoad,

    // ── Merkle (2) ──
    MerkleStep,
    MerkleLoad,

    // ═══════════════════════════════════════════════════════════════
    // Tier 3 — Recursion (5)
    // STARK-in-STARK verification primitives. Extension field
    // arithmetic, FRI folding steps, and proof verification blocks.
    // Currently Triton-only; any backend with recursive verification
    // will need equivalents.
    // ═══════════════════════════════════════════════════════════════

    // ── Extension field (2) ──
    ExtMul,
    ExtInvert,

    // ── Folding (2) ──
    FoldExt,
    FoldBase,

    // ── Verification (1) ──
    /// Recursive proof verification block. The body contains the
    /// verification circuit (typically Tier 3 ops). Backends with native
    /// recursion can optimize the entire block; others lower the body
    /// as plain arithmetic.
    ProofBlock {
        program_hash: String,
        body: Vec<TIROp>,
    },
}

// ─── Display ──────────────────────────────────────────────────────

impl fmt::Display for TIROp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TIROp::Push(v) => write!(f, "push {}", v),
            TIROp::Pop(n) => write!(f, "pop {}", n),
            TIROp::Dup(d) => write!(f, "dup {}", d),
            TIROp::Swap(d) => write!(f, "swap {}", d),
            TIROp::Add => write!(f, "add"),
            TIROp::Sub => write!(f, "sub"),
            TIROp::Mul => write!(f, "mul"),
            TIROp::Neg => write!(f, "neg"),
            TIROp::Eq => write!(f, "eq"),
            TIROp::Lt => write!(f, "lt"),
            TIROp::And => write!(f, "and"),
            TIROp::Or => write!(f, "or"),
            TIROp::Xor => write!(f, "xor"),
            TIROp::DivMod => write!(f, "div_mod"),
            TIROp::Shl => write!(f, "shl"),
            TIROp::Shr => write!(f, "shr"),
            TIROp::Invert => write!(f, "invert"),
            TIROp::Split => write!(f, "split"),
            TIROp::Log2 => write!(f, "log2"),
            TIROp::Pow => write!(f, "pow"),
            TIROp::PopCount => write!(f, "pop_count"),
            TIROp::ExtMul => write!(f, "ext_mul"),
            TIROp::ExtInvert => write!(f, "ext_invert"),
            TIROp::FoldExt => write!(f, "fold_ext"),
            TIROp::FoldBase => write!(f, "fold_base"),
            TIROp::ProofBlock { program_hash, body } => {
                write!(f, "proof_block {}(body={})", program_hash, body.len())
            }
            TIROp::ReadIo(n) => write!(f, "read_io {}", n),
            TIROp::WriteIo(n) => write!(f, "write_io {}", n),
            TIROp::Hint(n) => write!(f, "hint {}", n),
            TIROp::ReadMem(n) => write!(f, "read_mem {}", n),
            TIROp::WriteMem(n) => write!(f, "write_mem {}", n),
            TIROp::Hash { width } => write!(f, "hash {}", width),
            TIROp::SpongeInit => write!(f, "sponge_init"),
            TIROp::SpongeAbsorb => write!(f, "sponge_absorb"),
            TIROp::SpongeSqueeze => write!(f, "sponge_squeeze"),
            TIROp::SpongeLoad => write!(f, "sponge_load"),
            TIROp::MerkleStep => write!(f, "merkle_step"),
            TIROp::MerkleLoad => write!(f, "merkle_load"),
            TIROp::Assert(n) => write!(f, "assert {}", n),
            TIROp::Reveal {
                name, field_count, ..
            } => write!(f, "reveal {}({})", name, field_count),
            TIROp::Seal {
                name, field_count, ..
            } => write!(f, "seal {}({})", name, field_count),
            TIROp::ReadStorage { width } => write!(f, "read_storage {}", width),
            TIROp::WriteStorage { width } => write!(f, "write_storage {}", width),
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
            TIROp::FnStart(name) => write!(f, "fn_start {}", name),
            TIROp::FnEnd => write!(f, "fn_end"),
            TIROp::Entry(main) => write!(f, "entry {}", main),
            TIROp::Comment(text) => write!(f, "// {}", text),
            TIROp::Asm { lines, effect } => {
                write!(f, "asm({} lines, effect={})", lines.len(), effect)
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
            TIROp::Pop(1),
            TIROp::Dup(0),
            TIROp::Swap(1),
            TIROp::Add,
            TIROp::Sub,
            TIROp::Mul,
            TIROp::Neg,
            TIROp::Eq,
            TIROp::Lt,
            TIROp::And,
            TIROp::Or,
            TIROp::Xor,
            TIROp::DivMod,
            TIROp::Shl,
            TIROp::Shr,
            TIROp::Invert,
            TIROp::Split,
            TIROp::Log2,
            TIROp::Pow,
            TIROp::PopCount,
            TIROp::ExtMul,
            TIROp::ExtInvert,
            TIROp::FoldExt,
            TIROp::FoldBase,
            TIROp::ReadIo(1),
            TIROp::WriteIo(1),
            TIROp::Hint(1),
            TIROp::ReadMem(1),
            TIROp::WriteMem(1),
            TIROp::Hash { width: 0 },
            TIROp::SpongeInit,
            TIROp::SpongeAbsorb,
            TIROp::SpongeSqueeze,
            TIROp::SpongeLoad,
            TIROp::MerkleStep,
            TIROp::MerkleLoad,
            TIROp::Assert(1),
            TIROp::Assert(5),
            TIROp::Reveal {
                name: "Transfer".into(),
                tag: 0,
                field_count: 2,
            },
            TIROp::Seal {
                name: "Nullifier".into(),
                tag: 1,
                field_count: 1,
            },
            TIROp::ReadStorage { width: 1 },
            TIROp::WriteStorage { width: 1 },
            TIROp::ProofBlock {
                program_hash: "abc123".into(),
                body: vec![TIROp::ExtMul],
            },
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
            TIROp::FnStart("main".into()),
            TIROp::FnEnd,
            TIROp::Entry("main".into()),
            TIROp::Comment("test".into()),
            TIROp::Asm {
                lines: vec!["nop".into()],
                effect: 0,
            },
        ];
    }
}
