//! Intermediate representation between AST and target assembly.
//!
//! The IR is a list of stack operations with structural control flow.
//! Each backend implements a `Lowering` that consumes `Vec<IROp>` and
//! produces target assembly text.

pub mod builder;
pub mod lower;

use std::fmt;

// ─── IR Operations ────────────────────────────────────────────────

/// A single IR operation. Flat ops map 1:1 to stack-machine instructions.
/// Structural ops (`IfElse`, `IfOnly`, `Loop`) carry nested bodies so each
/// backend can choose its own control-flow lowering strategy.
#[derive(Debug, Clone)]
pub enum IROp {
    // ── Stack ──
    Push(u64),
    PushNegOne,
    Pop(u32),
    Dup(u32),
    Swap(u32),

    // ── Arithmetic ──
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

    // ── Extension field ──
    XbMul,
    XInvert,
    XxDotStep,
    XbDotStep,

    // ── I/O ──
    ReadIo(u32),
    WriteIo(u32),
    Divine(u32),

    // ── Memory ──
    ReadMem(u32),
    WriteMem(u32),

    // ── Crypto ──
    Hash,
    SpongeInit,
    SpongeAbsorb,
    SpongeSqueeze,
    SpongeAbsorbMem,
    MerkleStep,
    MerkleStepMem,

    // ── Assertions ──
    Assert,
    AssertVector,

    // ── Abstract operations (target-independent) ──
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
    /// Compute a cryptographic hash digest. Inputs on stack per target config.
    /// Produces `digest_width` elements (from TargetConfig).
    HashDigest,

    // ── Control flow (flat) ──
    Call(String),
    Return,
    Halt,

    // ── Control flow (structural) ──
    /// Conditional branch with both then and else bodies.
    /// Condition bool has already been consumed from the stack.
    IfElse {
        then_body: Vec<IROp>,
        else_body: Vec<IROp>,
    },
    /// Conditional branch with only a then body (no else).
    IfOnly {
        then_body: Vec<IROp>,
    },
    /// Counted loop. Counter is on the stack. Body decrements and repeats.
    Loop {
        label: String,
        body: Vec<IROp>,
    },

    // ── Program structure ──
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

    // ── Passthrough ──
    /// Comment text (without prefix — lowering adds target-specific prefix).
    Comment(String),
    /// Inline assembly passed through verbatim with declared stack effect.
    RawAsm {
        lines: Vec<String>,
        effect: i32,
    },
}

// ─── Display ──────────────────────────────────────────────────────

impl fmt::Display for IROp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IROp::Push(v) => write!(f, "push {}", v),
            IROp::PushNegOne => write!(f, "push -1"),
            IROp::Pop(n) => write!(f, "pop {}", n),
            IROp::Dup(d) => write!(f, "dup {}", d),
            IROp::Swap(d) => write!(f, "swap {}", d),
            IROp::Add => write!(f, "add"),
            IROp::Mul => write!(f, "mul"),
            IROp::Eq => write!(f, "eq"),
            IROp::Lt => write!(f, "lt"),
            IROp::And => write!(f, "and"),
            IROp::Xor => write!(f, "xor"),
            IROp::DivMod => write!(f, "div_mod"),
            IROp::Invert => write!(f, "invert"),
            IROp::Split => write!(f, "split"),
            IROp::Log2 => write!(f, "log2"),
            IROp::Pow => write!(f, "pow"),
            IROp::PopCount => write!(f, "pop_count"),
            IROp::XbMul => write!(f, "xb_mul"),
            IROp::XInvert => write!(f, "x_invert"),
            IROp::XxDotStep => write!(f, "xx_dot_step"),
            IROp::XbDotStep => write!(f, "xb_dot_step"),
            IROp::ReadIo(n) => write!(f, "read_io {}", n),
            IROp::WriteIo(n) => write!(f, "write_io {}", n),
            IROp::Divine(n) => write!(f, "divine {}", n),
            IROp::ReadMem(n) => write!(f, "read_mem {}", n),
            IROp::WriteMem(n) => write!(f, "write_mem {}", n),
            IROp::Hash => write!(f, "hash"),
            IROp::SpongeInit => write!(f, "sponge_init"),
            IROp::SpongeAbsorb => write!(f, "sponge_absorb"),
            IROp::SpongeSqueeze => write!(f, "sponge_squeeze"),
            IROp::SpongeAbsorbMem => write!(f, "sponge_absorb_mem"),
            IROp::MerkleStep => write!(f, "merkle_step"),
            IROp::MerkleStepMem => write!(f, "merkle_step_mem"),
            IROp::Assert => write!(f, "assert"),
            IROp::AssertVector => write!(f, "assert_vector"),
            IROp::EmitEvent {
                name, field_count, ..
            } => write!(f, "emit_event {}({})", name, field_count),
            IROp::SealEvent {
                name, field_count, ..
            } => write!(f, "seal_event {}({})", name, field_count),
            IROp::StorageRead { width } => write!(f, "storage_read {}", width),
            IROp::StorageWrite { width } => write!(f, "storage_write {}", width),
            IROp::HashDigest => write!(f, "hash_digest"),
            IROp::Call(label) => write!(f, "call {}", label),
            IROp::Return => write!(f, "return"),
            IROp::Halt => write!(f, "halt"),
            IROp::IfElse {
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
            IROp::IfOnly { then_body } => {
                write!(f, "if_only(then={})", then_body.len())
            }
            IROp::Loop { label, body } => {
                write!(f, "loop {}(body={})", label, body.len())
            }
            IROp::Label(name) => write!(f, "label {}", name),
            IROp::FnStart(name) => write!(f, "fn_start {}", name),
            IROp::FnEnd => write!(f, "fn_end"),
            IROp::Preamble(main) => write!(f, "preamble {}", main),
            IROp::BlankLine => write!(f, ""),
            IROp::Comment(text) => write!(f, "// {}", text),
            IROp::RawAsm { lines, effect } => {
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
        assert_eq!(format!("{}", IROp::Push(42)), "push 42");
        assert_eq!(format!("{}", IROp::Add), "add");
        assert_eq!(format!("{}", IROp::Call("main".into())), "call main");
        assert_eq!(format!("{}", IROp::Pop(3)), "pop 3");
        assert_eq!(format!("{}", IROp::Dup(0)), "dup 0");
        assert_eq!(format!("{}", IROp::Swap(5)), "swap 5");
    }

    #[test]
    fn test_irop_structural_display() {
        let op = IROp::IfElse {
            then_body: vec![IROp::Push(1), IROp::Add],
            else_body: vec![IROp::Push(0)],
        };
        assert_eq!(format!("{}", op), "if_else(then=2, else=1)");

        let op = IROp::Loop {
            label: "loop_1".into(),
            body: vec![IROp::Pop(1)],
        };
        assert_eq!(format!("{}", op), "loop loop_1(body=1)");
    }

    #[test]
    fn test_irop_clone() {
        let ops = vec![
            IROp::Push(10),
            IROp::Push(20),
            IROp::Add,
            IROp::IfElse {
                then_body: vec![IROp::WriteIo(1)],
                else_body: vec![IROp::Pop(1)],
            },
        ];
        let cloned = ops.clone();
        assert_eq!(ops.len(), cloned.len());
    }

    #[test]
    fn test_irop_all_variants_construct() {
        // Verify every variant can be constructed without panic
        let _ops: Vec<IROp> = vec![
            IROp::Push(0),
            IROp::PushNegOne,
            IROp::Pop(1),
            IROp::Dup(0),
            IROp::Swap(1),
            IROp::Add,
            IROp::Mul,
            IROp::Eq,
            IROp::Lt,
            IROp::And,
            IROp::Xor,
            IROp::DivMod,
            IROp::Invert,
            IROp::Split,
            IROp::Log2,
            IROp::Pow,
            IROp::PopCount,
            IROp::XbMul,
            IROp::XInvert,
            IROp::XxDotStep,
            IROp::XbDotStep,
            IROp::ReadIo(1),
            IROp::WriteIo(1),
            IROp::Divine(1),
            IROp::ReadMem(1),
            IROp::WriteMem(1),
            IROp::Hash,
            IROp::SpongeInit,
            IROp::SpongeAbsorb,
            IROp::SpongeSqueeze,
            IROp::SpongeAbsorbMem,
            IROp::MerkleStep,
            IROp::MerkleStepMem,
            IROp::Assert,
            IROp::AssertVector,
            IROp::EmitEvent {
                name: "Transfer".into(),
                tag: 0,
                field_count: 2,
            },
            IROp::SealEvent {
                name: "Nullifier".into(),
                tag: 1,
                field_count: 1,
            },
            IROp::StorageRead { width: 1 },
            IROp::StorageWrite { width: 1 },
            IROp::HashDigest,
            IROp::Call("f".into()),
            IROp::Return,
            IROp::Halt,
            IROp::IfElse {
                then_body: vec![],
                else_body: vec![],
            },
            IROp::IfOnly { then_body: vec![] },
            IROp::Loop {
                label: "l".into(),
                body: vec![],
            },
            IROp::Label("x".into()),
            IROp::FnStart("main".into()),
            IROp::FnEnd,
            IROp::Preamble("main".into()),
            IROp::BlankLine,
            IROp::Comment("test".into()),
            IROp::RawAsm {
                lines: vec!["nop".into()],
                effect: 0,
            },
        ];
    }
}
