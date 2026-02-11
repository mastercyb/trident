//! LIR — Low-level Intermediate Representation.
//!
//! Three-address form with virtual registers and flat control flow.
//! Designed for register-machine targets (x86-64, ARM64, RISC-V).
//!
//! The LIR mirrors TIR's 4-tier structure:
//!   Tier 0: Structure (control flow, program structure, passthrough)
//!   Tier 1: Universal (arithmetic, I/O, memory, assertions, hash, events, storage)
//!   Tier 2: Provable (sponge, merkle)
//!   Tier 3: Recursion (extension field, FRI folding)
//!
//! Key differences from TIR:
//!   - Explicit virtual registers (`Reg`) instead of implicit stack
//!   - Three-address: `Add(dst, src1, src2)` instead of stack consumption
//!   - Flat control flow: `Branch`/`Jump`/`LabelDef` instead of nested bodies
//!   - No Dup/Swap/Pop — register machines don't need stack manipulation

pub mod convert;
pub mod lower;

use std::fmt;

// ─── Virtual Register ─────────────────────────────────────────────

/// A virtual register. Physical mapping is decided per-target during
/// register allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Reg(pub u32);

impl fmt::Display for Reg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "v{}", self.0)
    }
}

// ─── Label ────────────────────────────────────────────────────────

/// A control-flow label for flat branch/jump targets.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Label(pub String);

impl Label {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }
}

impl fmt::Display for Label {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ─── LIR Operations ──────────────────────────────────────────────

/// 51 LIR operations. Higher tier = narrower target set.
///
/// **Tier 0 — Structure** (every program, every target)
///   Control flow (5), Program structure (5), Passthrough (2) = 12
///
/// **Tier 1 — Universal** (compiles to every target)
///   Register (2), Arithmetic (12), I/O (3), Memory (4),
///   Assertions (2), Hash (2), Events (2), Storage (2) = 29
///
/// **Tier 2 — Provable** (requires a proof-capable target)
///   Sponge (4), Merkle (2) = 6
///
/// **Tier 3 — Recursion** (requires recursive verification capability)
///   Extension field (2), FRI folding (2) = 4
///
/// Total: 12 + 29 + 6 + 4 = 51 variants
#[derive(Debug, Clone)]
pub enum LIROp {
    // ═══════════════════════════════════════════════════════════════
    // Tier 0 — Structure
    // The scaffolding. Present in every program, on every target.
    // ═══════════════════════════════════════════════════════════════

    // ── Control flow (5) ──
    /// Direct call to a named function.
    Call(String),
    /// Return from the current function.
    Return,
    /// Halt execution.
    Halt,
    /// Conditional branch: if `cond` is nonzero jump to `if_true`, else `if_false`.
    Branch {
        cond: Reg,
        if_true: Label,
        if_false: Label,
    },
    /// Unconditional jump.
    Jump(Label),

    // ── Program structure (5) ──
    /// Label definition (branch/jump target).
    LabelDef(Label),
    /// Function entry point.
    FnStart(String),
    /// Function end marker.
    FnEnd,
    /// Program entry preamble.
    Preamble(String),
    /// Blank line in output.
    BlankLine,

    // ── Passthrough (2) ──
    /// Comment text (lowering adds target-specific prefix).
    Comment(String),
    /// Inline assembly passed through verbatim.
    RawAsm { lines: Vec<String> },

    // ═══════════════════════════════════════════════════════════════
    // Tier 1 — Universal
    // Compiles to every target. Register primitives, arithmetic,
    // I/O, memory, hashing, events, storage.
    // ═══════════════════════════════════════════════════════════════

    // ── Register (2) ──
    /// Load an immediate value into a register.
    LoadImm(Reg, u64),
    /// Register-to-register move.
    Move(Reg, Reg),

    // ── Arithmetic (12) ──
    /// dst = src1 + src2 (mod p)
    Add(Reg, Reg, Reg),
    /// dst = src1 * src2 (mod p)
    Mul(Reg, Reg, Reg),
    /// dst = (src1 == src2) ? 1 : 0
    Eq(Reg, Reg, Reg),
    /// dst = (src1 < src2) ? 1 : 0
    Lt(Reg, Reg, Reg),
    /// dst = src1 & src2 (bitwise)
    And(Reg, Reg, Reg),
    /// dst = src1 ^ src2 (bitwise)
    Xor(Reg, Reg, Reg),
    /// (dst_quot, dst_rem) = divmod(src1, src2)
    DivMod {
        dst_quot: Reg,
        dst_rem: Reg,
        src1: Reg,
        src2: Reg,
    },
    /// dst = multiplicative inverse of src (in the field)
    Invert(Reg, Reg),
    /// (dst_hi, dst_lo) = split(src) — decompose into two limbs
    Split { dst_hi: Reg, dst_lo: Reg, src: Reg },
    /// dst = floor(log2(src))
    Log2(Reg, Reg),
    /// dst = base ^ exp
    Pow(Reg, Reg, Reg),
    /// dst = popcount(src)
    PopCount(Reg, Reg),

    // ── I/O (3) ──
    /// Read `count` values from public input into consecutive regs starting at `dst`.
    ReadIo { dst: Reg, count: u32 },
    /// Write `count` values from consecutive regs starting at `src` to public output.
    WriteIo { src: Reg, count: u32 },
    /// Read `count` nondeterministic values into consecutive regs starting at `dst`.
    Divine { dst: Reg, count: u32 },

    // ── Memory (4) ──
    /// dst = mem[base + offset]
    Load { dst: Reg, base: Reg, offset: i32 },
    /// mem[base + offset] = src
    Store { src: Reg, base: Reg, offset: i32 },
    /// Load `width` consecutive words from mem[base] into regs starting at `dst`.
    LoadMulti { dst: Reg, base: Reg, width: u32 },
    /// Store `width` consecutive words from regs starting at `src` to mem[base].
    StoreMulti { src: Reg, base: Reg, width: u32 },

    // ── Assertions (2) ──
    /// Assert that `src` is nonzero.
    Assert(Reg),
    /// Assert that `count` consecutive regs starting at `src` are all nonzero.
    AssertVector { src: Reg, count: u32 },

    // ── Hash (2) ──
    /// dst = hash(src..src+count)
    Hash { dst: Reg, src: Reg, count: u32 },
    /// dst = hash_digest(src..src+count)
    HashDigest { dst: Reg, src: Reg, count: u32 },

    // ── Events (2) ──
    /// Emit an observable event. Fields in consecutive regs starting at `src`.
    EmitEvent {
        name: String,
        tag: u64,
        src: Reg,
        field_count: u32,
    },
    /// Emit a sealed (hashed) event commitment.
    SealEvent {
        name: String,
        tag: u64,
        src: Reg,
        field_count: u32,
    },

    // ── Storage (2) ──
    /// Read from persistent storage. Key in `key`, result in `dst`.
    StorageRead { dst: Reg, key: Reg, width: u32 },
    /// Write to persistent storage. Key in `key`, value in `src`.
    StorageWrite { key: Reg, src: Reg, width: u32 },

    // ═══════════════════════════════════════════════════════════════
    // Tier 2 — Provable
    // Requires a proof-capable target. Sponge construction and Merkle
    // authentication have no meaningful equivalent on conventional VMs.
    // ═══════════════════════════════════════════════════════════════

    // ── Sponge (4) ──
    /// Initialize sponge state in `dst`.
    SpongeInit(Reg),
    /// Absorb `src` into sponge `state`.
    SpongeAbsorb { state: Reg, src: Reg },
    /// Squeeze output from sponge `state` into `dst`.
    SpongeSqueeze { dst: Reg, state: Reg },
    /// Absorb from memory address `addr` into sponge `state`.
    SpongeAbsorbMem { state: Reg, addr: Reg },

    // ── Merkle (2) ──
    /// One Merkle authentication step.
    MerkleStep { dst: Reg, node: Reg, sibling: Reg },
    /// Merkle step reading sibling from memory at `addr`.
    MerkleStepMem { dst: Reg, node: Reg, addr: Reg },

    // ═══════════════════════════════════════════════════════════════
    // Tier 3 — Recursion
    // STARK-in-STARK verification primitives. Extension field
    // arithmetic and FRI folding steps.
    // ═══════════════════════════════════════════════════════════════

    // ── Extension field (2) ──
    /// dst = src1 * src2 in the extension field.
    ExtMul(Reg, Reg, Reg),
    /// dst = inverse of src in the extension field.
    ExtInvert(Reg, Reg),

    // ── FRI folding (2) ──
    /// FRI fold step.
    FriFold { dst: Reg, src1: Reg, src2: Reg },
    /// FRI base fold step.
    FriBaseFold { dst: Reg, src1: Reg, src2: Reg },
}

// ─── Display ──────────────────────────────────────────────────────

impl fmt::Display for LIROp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // Tier 0
            LIROp::Call(label) => write!(f, "call {}", label),
            LIROp::Return => write!(f, "ret"),
            LIROp::Halt => write!(f, "halt"),
            LIROp::Branch {
                cond,
                if_true,
                if_false,
            } => {
                write!(f, "br {}, {}, {}", cond, if_true, if_false)
            }
            LIROp::Jump(label) => write!(f, "jmp {}", label),
            LIROp::LabelDef(label) => write!(f, "{}:", label),
            LIROp::FnStart(name) => write!(f, "fn {}:", name),
            LIROp::FnEnd => write!(f, "fn_end"),
            LIROp::Preamble(main) => write!(f, "preamble {}", main),
            LIROp::BlankLine => write!(f, ""),
            LIROp::Comment(text) => write!(f, "// {}", text),
            LIROp::RawAsm { lines } => write!(f, "raw_asm({} lines)", lines.len()),

            // Tier 1
            LIROp::LoadImm(dst, val) => write!(f, "li {}, {}", dst, val),
            LIROp::Move(dst, src) => write!(f, "mv {}, {}", dst, src),
            LIROp::Add(d, a, b) => write!(f, "add {}, {}, {}", d, a, b),
            LIROp::Mul(d, a, b) => write!(f, "mul {}, {}, {}", d, a, b),
            LIROp::Eq(d, a, b) => write!(f, "eq {}, {}, {}", d, a, b),
            LIROp::Lt(d, a, b) => write!(f, "lt {}, {}, {}", d, a, b),
            LIROp::And(d, a, b) => write!(f, "and {}, {}, {}", d, a, b),
            LIROp::Xor(d, a, b) => write!(f, "xor {}, {}, {}", d, a, b),
            LIROp::DivMod {
                dst_quot,
                dst_rem,
                src1,
                src2,
            } => {
                write!(f, "divmod {}, {}, {}, {}", dst_quot, dst_rem, src1, src2)
            }
            LIROp::Invert(d, s) => write!(f, "inv {}, {}", d, s),
            LIROp::Split {
                dst_hi,
                dst_lo,
                src,
            } => {
                write!(f, "split {}, {}, {}", dst_hi, dst_lo, src)
            }
            LIROp::Log2(d, s) => write!(f, "log2 {}, {}", d, s),
            LIROp::Pow(d, b, e) => write!(f, "pow {}, {}, {}", d, b, e),
            LIROp::PopCount(d, s) => write!(f, "popcnt {}, {}", d, s),
            LIROp::ReadIo { dst, count } => write!(f, "read_io {}, {}", dst, count),
            LIROp::WriteIo { src, count } => write!(f, "write_io {}, {}", src, count),
            LIROp::Divine { dst, count } => write!(f, "divine {}, {}", dst, count),
            LIROp::Load { dst, base, offset } => {
                write!(f, "ld {}, [{}+{}]", dst, base, offset)
            }
            LIROp::Store { src, base, offset } => {
                write!(f, "st {}, [{}+{}]", src, base, offset)
            }
            LIROp::LoadMulti { dst, base, width } => {
                write!(f, "ldm {}, [{}], {}", dst, base, width)
            }
            LIROp::StoreMulti { src, base, width } => {
                write!(f, "stm {}, [{}], {}", src, base, width)
            }
            LIROp::Assert(s) => write!(f, "assert {}", s),
            LIROp::AssertVector { src, count } => {
                write!(f, "assert_vec {}, {}", src, count)
            }
            LIROp::Hash { dst, src, count } => {
                write!(f, "hash {}, {}, {}", dst, src, count)
            }
            LIROp::HashDigest { dst, src, count } => {
                write!(f, "hash_digest {}, {}, {}", dst, src, count)
            }
            LIROp::EmitEvent {
                name,
                src,
                field_count,
                ..
            } => {
                write!(f, "emit_event {}({}, {})", name, src, field_count)
            }
            LIROp::SealEvent {
                name,
                src,
                field_count,
                ..
            } => {
                write!(f, "seal_event {}({}, {})", name, src, field_count)
            }
            LIROp::StorageRead { dst, key, width } => {
                write!(f, "storage_read {}, {}, {}", dst, key, width)
            }
            LIROp::StorageWrite { key, src, width } => {
                write!(f, "storage_write {}, {}, {}", key, src, width)
            }

            // Tier 2
            LIROp::SpongeInit(d) => write!(f, "sponge_init {}", d),
            LIROp::SpongeAbsorb { state, src } => {
                write!(f, "sponge_absorb {}, {}", state, src)
            }
            LIROp::SpongeSqueeze { dst, state } => {
                write!(f, "sponge_squeeze {}, {}", dst, state)
            }
            LIROp::SpongeAbsorbMem { state, addr } => {
                write!(f, "sponge_absorb_mem {}, {}", state, addr)
            }
            LIROp::MerkleStep { dst, node, sibling } => {
                write!(f, "merkle_step {}, {}, {}", dst, node, sibling)
            }
            LIROp::MerkleStepMem { dst, node, addr } => {
                write!(f, "merkle_step_mem {}, {}, {}", dst, node, addr)
            }

            // Tier 3
            LIROp::ExtMul(d, a, b) => write!(f, "ext_mul {}, {}, {}", d, a, b),
            LIROp::ExtInvert(d, s) => write!(f, "ext_inv {}, {}", d, s),
            LIROp::FriFold { dst, src1, src2 } => {
                write!(f, "fri_fold {}, {}, {}", dst, src1, src2)
            }
            LIROp::FriBaseFold { dst, src1, src2 } => {
                write!(f, "fri_base_fold {}, {}, {}", dst, src1, src2)
            }
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reg_display() {
        assert_eq!(format!("{}", Reg(0)), "v0");
        assert_eq!(format!("{}", Reg(42)), "v42");
    }

    #[test]
    fn test_label_display() {
        assert_eq!(format!("{}", Label::new("loop_1")), "loop_1");
    }

    #[test]
    fn test_reg_equality() {
        assert_eq!(Reg(0), Reg(0));
        assert_ne!(Reg(0), Reg(1));
    }

    #[test]
    fn test_label_equality() {
        assert_eq!(Label::new("a"), Label::new("a"));
        assert_ne!(Label::new("a"), Label::new("b"));
    }

    #[test]
    fn test_lirop_display() {
        let r0 = Reg(0);
        let r1 = Reg(1);
        let r2 = Reg(2);

        assert_eq!(format!("{}", LIROp::LoadImm(r0, 42)), "li v0, 42");
        assert_eq!(format!("{}", LIROp::Add(r0, r1, r2)), "add v0, v1, v2");
        assert_eq!(format!("{}", LIROp::Move(r0, r1)), "mv v0, v1");
        assert_eq!(format!("{}", LIROp::Call("main".into())), "call main");
        assert_eq!(format!("{}", LIROp::Return), "ret");
    }

    #[test]
    fn test_lirop_branch_display() {
        let op = LIROp::Branch {
            cond: Reg(0),
            if_true: Label::new("then"),
            if_false: Label::new("else"),
        };
        assert_eq!(format!("{}", op), "br v0, then, else");
    }

    #[test]
    fn test_lirop_memory_display() {
        assert_eq!(
            format!(
                "{}",
                LIROp::Load {
                    dst: Reg(0),
                    base: Reg(1),
                    offset: 8
                }
            ),
            "ld v0, [v1+8]"
        );
        assert_eq!(
            format!(
                "{}",
                LIROp::Store {
                    src: Reg(0),
                    base: Reg(1),
                    offset: 0
                }
            ),
            "st v0, [v1+0]"
        );
    }

    #[test]
    fn test_lirop_all_variants_construct() {
        let r0 = Reg(0);
        let r1 = Reg(1);
        let r2 = Reg(2);
        let r3 = Reg(3);
        let _ops: Vec<LIROp> = vec![
            // Tier 0
            LIROp::Call("f".into()),
            LIROp::Return,
            LIROp::Halt,
            LIROp::Branch {
                cond: r0,
                if_true: Label::new("t"),
                if_false: Label::new("f"),
            },
            LIROp::Jump(Label::new("x")),
            LIROp::LabelDef(Label::new("x")),
            LIROp::FnStart("main".into()),
            LIROp::FnEnd,
            LIROp::Preamble("main".into()),
            LIROp::BlankLine,
            LIROp::Comment("test".into()),
            LIROp::RawAsm {
                lines: vec!["nop".into()],
            },
            // Tier 1
            LIROp::LoadImm(r0, 0),
            LIROp::Move(r0, r1),
            LIROp::Add(r0, r1, r2),
            LIROp::Mul(r0, r1, r2),
            LIROp::Eq(r0, r1, r2),
            LIROp::Lt(r0, r1, r2),
            LIROp::And(r0, r1, r2),
            LIROp::Xor(r0, r1, r2),
            LIROp::DivMod {
                dst_quot: r0,
                dst_rem: r1,
                src1: r2,
                src2: r3,
            },
            LIROp::Invert(r0, r1),
            LIROp::Split {
                dst_hi: r0,
                dst_lo: r1,
                src: r2,
            },
            LIROp::Log2(r0, r1),
            LIROp::Pow(r0, r1, r2),
            LIROp::PopCount(r0, r1),
            LIROp::ReadIo { dst: r0, count: 1 },
            LIROp::WriteIo { src: r0, count: 1 },
            LIROp::Divine { dst: r0, count: 1 },
            LIROp::Load {
                dst: r0,
                base: r1,
                offset: 0,
            },
            LIROp::Store {
                src: r0,
                base: r1,
                offset: 0,
            },
            LIROp::LoadMulti {
                dst: r0,
                base: r1,
                width: 4,
            },
            LIROp::StoreMulti {
                src: r0,
                base: r1,
                width: 4,
            },
            LIROp::Assert(r0),
            LIROp::AssertVector { src: r0, count: 4 },
            LIROp::Hash {
                dst: r0,
                src: r1,
                count: 1,
            },
            LIROp::HashDigest {
                dst: r0,
                src: r1,
                count: 1,
            },
            LIROp::EmitEvent {
                name: "Transfer".into(),
                tag: 0,
                src: r0,
                field_count: 2,
            },
            LIROp::SealEvent {
                name: "Nullifier".into(),
                tag: 1,
                src: r0,
                field_count: 1,
            },
            LIROp::StorageRead {
                dst: r0,
                key: r1,
                width: 1,
            },
            LIROp::StorageWrite {
                key: r0,
                src: r1,
                width: 1,
            },
            // Tier 2
            LIROp::SpongeInit(r0),
            LIROp::SpongeAbsorb { state: r0, src: r1 },
            LIROp::SpongeSqueeze { dst: r0, state: r1 },
            LIROp::SpongeAbsorbMem {
                state: r0,
                addr: r1,
            },
            LIROp::MerkleStep {
                dst: r0,
                node: r1,
                sibling: r2,
            },
            LIROp::MerkleStepMem {
                dst: r0,
                node: r1,
                addr: r2,
            },
            // Tier 3
            LIROp::ExtMul(r0, r1, r2),
            LIROp::ExtInvert(r0, r1),
            LIROp::FriFold {
                dst: r0,
                src1: r1,
                src2: r2,
            },
            LIROp::FriBaseFold {
                dst: r0,
                src1: r1,
                src2: r2,
            },
        ];
    }
}
