//! Miden VM lowering — produces MASM from TIR.

use super::Lowering;
use crate::tir::TIROp;

/// Miden VM lowering — produces MASM from IR.
///
/// Uses inline `if.true/else/end` for conditionals and `proc/end` for
/// functions, matching Miden's structured control flow model.
pub struct MidenLowering {
    /// Indentation depth for nested control flow.
    indent: usize,
}

impl Default for MidenLowering {
    fn default() -> Self {
        Self { indent: 1 }
    }
}

/// Goldilocks field: p - 1 = 2^64 - 2^32 + 1 - 1
const MIDEN_NEG_ONE: u64 = 18446744069414584320;

impl MidenLowering {
    pub fn new() -> Self {
        Self::default()
    }

    fn indent_str(&self) -> String {
        "    ".repeat(self.indent)
    }

    fn emit(&self, out: &mut Vec<String>, s: &str) {
        out.push(format!("{}{}", self.indent_str(), s));
    }

    fn lower_op(&mut self, op: &TIROp, out: &mut Vec<String>) {
        match op {
            // ── Stack ──
            TIROp::Push(v) => self.emit(out, &format!("push.{}", v)),
            TIROp::PushNegOne => self.emit(out, &format!("push.{}", MIDEN_NEG_ONE)),
            TIROp::Pop(n) => {
                for _ in 0..*n {
                    self.emit(out, "drop");
                }
            }
            TIROp::Dup(d) => self.emit(out, &format!("dup.{}", d)),
            TIROp::Swap(d) => {
                if *d == 1 {
                    self.emit(out, "swap");
                } else {
                    self.emit(out, &format!("movup.{}", d));
                }
            }

            // ── Arithmetic ──
            TIROp::Add => self.emit(out, "add"),
            TIROp::Mul => self.emit(out, "mul"),
            TIROp::Eq => self.emit(out, "eq"),
            TIROp::Lt => self.emit(out, "u32lt"),
            TIROp::And => self.emit(out, "u32and"),
            TIROp::Xor => self.emit(out, "u32xor"),
            TIROp::DivMod => self.emit(out, "u32divmod"),
            TIROp::Invert => self.emit(out, "inv"),
            TIROp::Split => self.emit(out, "u32split"),
            TIROp::Log2 => self.emit(out, "ilog2"),
            TIROp::Pow => self.emit(out, "exp"),
            TIROp::PopCount => self.emit(out, "u32popcnt"),

            // ── Recursion — extension field & FRI (not yet supported on Miden) ──
            TIROp::ExtMul => self.emit(out, "# ext_mul (recursion: not yet supported)"),
            TIROp::ExtInvert => self.emit(out, "# ext_invert (recursion: not yet supported)"),
            TIROp::FoldExt => self.emit(out, "# fold_ext (recursion: not yet supported)"),
            TIROp::FoldBase => self.emit(out, "# fold_base (recursion: not yet supported)"),

            // ── I/O ──
            TIROp::ReadIo(n) => {
                for _ in 0..*n {
                    self.emit(out, "adv_push.1");
                }
            }
            TIROp::WriteIo(n) => {
                for _ in 0..*n {
                    self.emit(out, "drop  # write_io");
                }
            }
            TIROp::Divine(n) => {
                for _ in 0..*n {
                    self.emit(out, "adv_push.1");
                }
            }

            // ── Memory ──
            TIROp::ReadMem(n) => self.emit(out, &format!("mem_load  # read {}", n)),
            TIROp::WriteMem(n) => self.emit(out, &format!("mem_store  # write {}", n)),

            // ── Crypto ──
            TIROp::Hash => self.emit(out, "hperm"),
            TIROp::SpongeInit => self.emit(out, "# sponge_init (use hperm sequence)"),
            TIROp::SpongeAbsorb => self.emit(out, "hperm  # absorb"),
            TIROp::SpongeSqueeze => self.emit(out, "hperm  # squeeze"),
            TIROp::SpongeLoad => self.emit(out, "# sponge_load (miden: custom)"),
            TIROp::MerkleStep => self.emit(out, "mtree_get  # merkle_step"),
            TIROp::MerkleLoad => self.emit(out, "mtree_get  # merkle_load"),

            // ── Assertions ──
            TIROp::Assert => self.emit(out, "assert"),
            TIROp::AssertVector => self.emit(out, "assert  # assert_vector (4 words)"),

            // ── Abstract operations (Miden lowering) ──
            TIROp::Open {
                name, field_count, ..
            } => {
                self.emit(out, &format!("# open {} ({} fields)", name, field_count));
                for _ in 0..*field_count {
                    self.emit(out, "drop  # event field");
                }
            }
            TIROp::Seal {
                name, field_count, ..
            } => {
                self.emit(out, &format!("# seal {} ({} fields)", name, field_count));
                let padding = 7usize.saturating_sub(*field_count as usize);
                for _ in 0..padding {
                    self.emit(out, "push.0");
                }
                self.emit(out, "hperm");
                for _ in 0..4 {
                    self.emit(out, "drop  # seal digest");
                }
            }
            TIROp::ReadStorage { width } => {
                self.emit(out, &format!("mem_load  # read {}", width));
            }
            TIROp::WriteStorage { width } => {
                self.emit(out, &format!("mem_store  # write {}", width));
            }
            TIROp::HashDigest => {
                self.emit(out, "hperm");
            }

            // ── Control flow (flat) ──
            TIROp::Call(label) => self.emit(out, &format!("exec.{}", label)),
            TIROp::Return => { /* Miden uses end, handled by FnEnd */ }
            TIROp::Halt => { /* Miden uses end, handled by program structure */ }

            // ── Control flow (structural) ──
            TIROp::IfElse {
                then_body,
                else_body,
            } => {
                self.emit(out, "if.true");
                self.indent += 1;
                for body_op in then_body {
                    self.lower_op(body_op, out);
                }
                self.indent -= 1;
                self.emit(out, "else");
                self.indent += 1;
                for body_op in else_body {
                    self.lower_op(body_op, out);
                }
                self.indent -= 1;
                self.emit(out, "end");
            }
            TIROp::IfOnly { then_body } => {
                self.emit(out, "if.true");
                self.indent += 1;
                for body_op in then_body {
                    self.lower_op(body_op, out);
                }
                self.indent -= 1;
                self.emit(out, "end");
            }
            TIROp::Loop { label: _, body } => {
                self.emit(out, "dup.0");
                self.emit(out, "push.0");
                self.emit(out, "eq");
                self.emit(out, "if.true");
                self.indent += 1;
                self.emit(out, "drop");
                self.indent -= 1;
                self.emit(out, "else");
                self.indent += 1;
                self.emit(out, &format!("push.{}", MIDEN_NEG_ONE));
                self.emit(out, "add");
                for body_op in body {
                    self.lower_op(body_op, out);
                }
                self.emit(out, "exec.self");
                self.indent -= 1;
                self.emit(out, "end");
            }

            // ── Program structure ──
            TIROp::Label(name) => {
                out.push(format!("proc.{}", name));
            }
            TIROp::FnStart(name) => {
                out.push(format!("proc.{}", name));
                self.indent = 1;
            }
            TIROp::FnEnd => {
                self.indent = 0;
                out.push("end".to_string());
                out.push(String::new());
            }
            TIROp::Preamble(main_label) => {
                out.push("begin".to_string());
                out.push(format!("    exec.{}", main_label));
                out.push("end".to_string());
                out.push(String::new());
            }
            TIROp::BlankLine => {
                out.push(String::new());
            }

            // ── Passthrough ──
            TIROp::Comment(text) => {
                self.emit(out, &format!("# {}", text));
            }
            TIROp::Asm { lines, .. } => {
                for line in lines {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        self.emit(out, trimmed);
                    }
                }
            }
        }
    }
}

impl Lowering for MidenLowering {
    fn lower(&self, ops: &[TIROp]) -> Vec<String> {
        let mut lowerer = MidenLowering::new();
        let mut out = Vec::new();
        for op in ops {
            lowerer.lower_op(op, &mut out);
        }
        out
    }
}
