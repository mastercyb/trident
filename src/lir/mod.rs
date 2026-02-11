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

/// 53 LIR operations. Higher tier = narrower target set.
///
/// **Tier 0 — Structure** (every program, every target)
///   Control flow (5), Program structure (4), Passthrough (2) = 11
///
/// **Tier 1 — Universal** (compiles to every target)
///   Register (2), Arithmetic (15), I/O (3), Memory (4),
///   Assertions (1), Hash (1), Events (2), Storage (2) = 30
///
/// **Tier 2 — Provable** (requires a proof-capable target)
///   Sponge (4), Merkle (2) = 6
///
/// **Tier 3 — Recursion** (requires recursive verification capability)
///   Extension field (2), Folding (2), Verification (2) = 6
///
/// Total: 11 + 30 + 6 + 6 = 53 variants
#[derive(Debug, Clone)]
pub enum LIROp {
    // ═══════════════════════════════════════════════════════════════
    // Tier 0 — Structure (11)
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

    // ── Program structure (4) ──
    /// Label definition (branch/jump target).
    LabelDef(Label),
    /// Function entry point.
    FnStart(String),
    /// Function end marker.
    FnEnd,
    /// Program entry point.
    Entry(String),

    // ── Passthrough (2) ──
    /// Comment text (lowering adds target-specific prefix).
    Comment(String),
    /// Inline assembly passed through verbatim.
    Asm { lines: Vec<String> },

    // ═══════════════════════════════════════════════════════════════
    // Tier 1 — Universal (30)
    // Compiles to every target. Register primitives, arithmetic,
    // I/O, memory, hashing, events, storage.
    // ═══════════════════════════════════════════════════════════════

    // ── Register (2) ──
    /// Load an immediate value into a register.
    LoadImm(Reg, u64),
    /// Register-to-register move.
    Move(Reg, Reg),

    // ── Arithmetic (15) ──
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
    /// dst = src1 | src2 (bitwise)
    Or(Reg, Reg, Reg),
    /// dst = src1 ^ src2 (bitwise)
    Xor(Reg, Reg, Reg),
    /// (dst_quot, dst_rem) = divmod(src1, src2)
    DivMod {
        dst_quot: Reg,
        dst_rem: Reg,
        src1: Reg,
        src2: Reg,
    },
    /// dst = src1 << src2
    Shl(Reg, Reg, Reg),
    /// dst = src1 >> src2
    Shr(Reg, Reg, Reg),
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
    /// Read `count` nondeterministic hint values into consecutive regs starting at `dst`.
    Hint { dst: Reg, count: u32 },

    // ── Memory (4) ──
    /// dst = mem[base + offset]
    Load { dst: Reg, base: Reg, offset: i32 },
    /// mem[base + offset] = src
    Store { src: Reg, base: Reg, offset: i32 },
    /// Load `width` consecutive words from mem[base] into regs starting at `dst`.
    LoadMulti { dst: Reg, base: Reg, width: u32 },
    /// Store `width` consecutive words from regs starting at `src` to mem[base].
    StoreMulti { src: Reg, base: Reg, width: u32 },

    // ── Assertions (1) ──
    /// Assert `count` consecutive regs starting at `src` are all nonzero.
    Assert { src: Reg, count: u32 },

    // ── Hash (1) ──
    /// dst = hash(src..src+count). Width is metadata for optimization.
    Hash { dst: Reg, src: Reg, count: u32 },

    // ── Events (2) ──
    /// Reveal an observable event. Fields in consecutive regs starting at `src`.
    Reveal {
        name: String,
        tag: u64,
        src: Reg,
        field_count: u32,
    },
    /// Seal (hash-commit) an event.
    Seal {
        name: String,
        tag: u64,
        src: Reg,
        field_count: u32,
    },

    // ── Storage (2) ──
    /// Read from persistent storage. Key in `key`, result in `dst`.
    ReadStorage { dst: Reg, key: Reg, width: u32 },
    /// Write to persistent storage. Key in `key`, value in `src`.
    WriteStorage { key: Reg, src: Reg, width: u32 },

    // ═══════════════════════════════════════════════════════════════
    // Tier 2 — Provable (6)
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
    SpongeLoad { state: Reg, addr: Reg },

    // ── Merkle (2) ──
    /// One Merkle authentication step.
    MerkleStep { dst: Reg, node: Reg, sibling: Reg },
    /// Merkle step reading sibling from memory at `addr`.
    MerkleLoad { dst: Reg, node: Reg, addr: Reg },

    // ═══════════════════════════════════════════════════════════════
    // Tier 3 — Recursion (6)
    // STARK-in-STARK verification primitives. Extension field
    // arithmetic, FRI folding steps, and proof verification blocks.
    // ═══════════════════════════════════════════════════════════════

    // ── Extension field (2) ──
    /// dst = src1 * src2 in the extension field.
    ExtMul(Reg, Reg, Reg),
    /// dst = inverse of src in the extension field.
    ExtInvert(Reg, Reg),

    // ── Folding (2) ──
    /// Fold extension field elements.
    FoldExt { dst: Reg, src1: Reg, src2: Reg },
    /// Fold base field elements.
    FoldBase { dst: Reg, src1: Reg, src2: Reg },

    // ── Verification (2) ──
    /// Recursive proof verification block start marker.
    /// The verification ops follow until ProofBlockEnd.
    ProofBlock { program_hash: String },
    /// End of a proof verification block.
    ProofBlockEnd,
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
            LIROp::Entry(main) => write!(f, "entry {}", main),
            LIROp::Comment(text) => write!(f, "// {}", text),
            LIROp::Asm { lines } => write!(f, "asm({} lines)", lines.len()),

            // Tier 1
            LIROp::LoadImm(dst, val) => write!(f, "li {}, {}", dst, val),
            LIROp::Move(dst, src) => write!(f, "mv {}, {}", dst, src),
            LIROp::Add(d, a, b) => write!(f, "add {}, {}, {}", d, a, b),
            LIROp::Mul(d, a, b) => write!(f, "mul {}, {}, {}", d, a, b),
            LIROp::Eq(d, a, b) => write!(f, "eq {}, {}, {}", d, a, b),
            LIROp::Lt(d, a, b) => write!(f, "lt {}, {}, {}", d, a, b),
            LIROp::And(d, a, b) => write!(f, "and {}, {}, {}", d, a, b),
            LIROp::Or(d, a, b) => write!(f, "or {}, {}, {}", d, a, b),
            LIROp::Xor(d, a, b) => write!(f, "xor {}, {}, {}", d, a, b),
            LIROp::DivMod {
                dst_quot,
                dst_rem,
                src1,
                src2,
            } => {
                write!(f, "divmod {}, {}, {}, {}", dst_quot, dst_rem, src1, src2)
            }
            LIROp::Shl(d, a, b) => write!(f, "shl {}, {}, {}", d, a, b),
            LIROp::Shr(d, a, b) => write!(f, "shr {}, {}, {}", d, a, b),
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
            LIROp::Hint { dst, count } => write!(f, "hint {}, {}", dst, count),
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
            LIROp::Assert { src, count } => {
                write!(f, "assert {}, {}", src, count)
            }
            LIROp::Hash { dst, src, count } => {
                write!(f, "hash {}, {}, {}", dst, src, count)
            }
            LIROp::Reveal {
                name,
                src,
                field_count,
                ..
            } => {
                write!(f, "reveal {}({}, {})", name, src, field_count)
            }
            LIROp::Seal {
                name,
                src,
                field_count,
                ..
            } => {
                write!(f, "seal {}({}, {})", name, src, field_count)
            }
            LIROp::ReadStorage { dst, key, width } => {
                write!(f, "read_storage {}, {}, {}", dst, key, width)
            }
            LIROp::WriteStorage { key, src, width } => {
                write!(f, "write_storage {}, {}, {}", key, src, width)
            }

            // Tier 2
            LIROp::SpongeInit(d) => write!(f, "sponge_init {}", d),
            LIROp::SpongeAbsorb { state, src } => {
                write!(f, "sponge_absorb {}, {}", state, src)
            }
            LIROp::SpongeSqueeze { dst, state } => {
                write!(f, "sponge_squeeze {}, {}", dst, state)
            }
            LIROp::SpongeLoad { state, addr } => {
                write!(f, "sponge_load {}, {}", state, addr)
            }
            LIROp::MerkleStep { dst, node, sibling } => {
                write!(f, "merkle_step {}, {}, {}", dst, node, sibling)
            }
            LIROp::MerkleLoad { dst, node, addr } => {
                write!(f, "merkle_load {}, {}, {}", dst, node, addr)
            }

            // Tier 3
            LIROp::ExtMul(d, a, b) => write!(f, "ext_mul {}, {}, {}", d, a, b),
            LIROp::ExtInvert(d, s) => write!(f, "ext_inv {}, {}", d, s),
            LIROp::FoldExt { dst, src1, src2 } => {
                write!(f, "fold_ext {}, {}, {}", dst, src1, src2)
            }
            LIROp::FoldBase { dst, src1, src2 } => {
                write!(f, "fold_base {}, {}, {}", dst, src1, src2)
            }
            LIROp::ProofBlock { program_hash } => {
                write!(f, "proof_block {}", program_hash)
            }
            LIROp::ProofBlockEnd => write!(f, "proof_block_end"),
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
            LIROp::Entry("main".into()),
            LIROp::Comment("test".into()),
            LIROp::Asm {
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
            LIROp::Or(r0, r1, r2),
            LIROp::Xor(r0, r1, r2),
            LIROp::DivMod {
                dst_quot: r0,
                dst_rem: r1,
                src1: r2,
                src2: r3,
            },
            LIROp::Shl(r0, r1, r2),
            LIROp::Shr(r0, r1, r2),
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
            LIROp::Hint { dst: r0, count: 1 },
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
            LIROp::Assert { src: r0, count: 1 },
            LIROp::Assert { src: r0, count: 4 },
            LIROp::Hash {
                dst: r0,
                src: r1,
                count: 1,
            },
            LIROp::Reveal {
                name: "Transfer".into(),
                tag: 0,
                src: r0,
                field_count: 2,
            },
            LIROp::Seal {
                name: "Nullifier".into(),
                tag: 1,
                src: r0,
                field_count: 1,
            },
            LIROp::ReadStorage {
                dst: r0,
                key: r1,
                width: 1,
            },
            LIROp::WriteStorage {
                key: r0,
                src: r1,
                width: 1,
            },
            // Tier 2
            LIROp::SpongeInit(r0),
            LIROp::SpongeAbsorb { state: r0, src: r1 },
            LIROp::SpongeSqueeze { dst: r0, state: r1 },
            LIROp::SpongeLoad {
                state: r0,
                addr: r1,
            },
            LIROp::MerkleStep {
                dst: r0,
                node: r1,
                sibling: r2,
            },
            LIROp::MerkleLoad {
                dst: r0,
                node: r1,
                addr: r2,
            },
            // Tier 3
            LIROp::ExtMul(r0, r1, r2),
            LIROp::ExtInvert(r0, r1),
            LIROp::FoldExt {
                dst: r0,
                src1: r1,
                src2: r2,
            },
            LIROp::FoldBase {
                dst: r0,
                src1: r1,
                src2: r2,
            },
            LIROp::ProofBlock {
                program_hash: "abc123".into(),
            },
            LIROp::ProofBlockEnd,
        ];
    }
}
