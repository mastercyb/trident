//! Miden VM lowering — produces MASM from IR.

use super::Lowering;
use crate::ir::IROp;

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

    fn lower_op(&mut self, op: &IROp, out: &mut Vec<String>) {
        match op {
            // ── Stack ──
            IROp::Push(v) => self.emit(out, &format!("push.{}", v)),
            IROp::PushNegOne => self.emit(out, &format!("push.{}", MIDEN_NEG_ONE)),
            IROp::Pop(n) => {
                for _ in 0..*n {
                    self.emit(out, "drop");
                }
            }
            IROp::Dup(d) => self.emit(out, &format!("dup.{}", d)),
            IROp::Swap(d) => {
                if *d == 1 {
                    self.emit(out, "swap");
                } else {
                    self.emit(out, &format!("movup.{}", d));
                }
            }

            // ── Arithmetic ──
            IROp::Add => self.emit(out, "add"),
            IROp::Mul => self.emit(out, "mul"),
            IROp::Eq => self.emit(out, "eq"),
            IROp::Lt => self.emit(out, "u32lt"),
            IROp::And => self.emit(out, "u32and"),
            IROp::Xor => self.emit(out, "u32xor"),
            IROp::DivMod => self.emit(out, "u32divmod"),
            IROp::Invert => self.emit(out, "inv"),
            IROp::Split => self.emit(out, "u32split"),
            IROp::Log2 => self.emit(out, "ilog2"),
            IROp::Pow => self.emit(out, "exp"),
            IROp::PopCount => self.emit(out, "u32popcnt"),

            // ── Extension field ──
            IROp::XbMul => self.emit(out, "# xb_mul (unsupported on miden)"),
            IROp::XInvert => self.emit(out, "# x_invert (unsupported on miden)"),
            IROp::XxDotStep => self.emit(out, "# xx_dot_step (unsupported on miden)"),
            IROp::XbDotStep => self.emit(out, "# xb_dot_step (unsupported on miden)"),

            // ── I/O ──
            IROp::ReadIo(n) => {
                for _ in 0..*n {
                    self.emit(out, "adv_push.1");
                }
            }
            IROp::WriteIo(n) => {
                for _ in 0..*n {
                    self.emit(out, "drop  # write_io");
                }
            }
            IROp::Divine(n) => {
                for _ in 0..*n {
                    self.emit(out, "adv_push.1");
                }
            }

            // ── Memory ──
            IROp::ReadMem(n) => self.emit(out, &format!("mem_load  # read {}", n)),
            IROp::WriteMem(n) => self.emit(out, &format!("mem_store  # write {}", n)),

            // ── Crypto ──
            IROp::Hash => self.emit(out, "hperm"),
            IROp::SpongeInit => self.emit(out, "# sponge_init (use hperm sequence)"),
            IROp::SpongeAbsorb => self.emit(out, "hperm  # absorb"),
            IROp::SpongeSqueeze => self.emit(out, "hperm  # squeeze"),
            IROp::SpongeAbsorbMem => self.emit(out, "# sponge_absorb_mem (miden: custom)"),
            IROp::MerkleStep => self.emit(out, "mtree_get  # merkle_step"),
            IROp::MerkleStepMem => self.emit(out, "mtree_get  # merkle_step_mem"),

            // ── Assertions ──
            IROp::Assert => self.emit(out, "assert"),
            IROp::AssertVector => self.emit(out, "assert  # assert_vector (4 words)"),

            // ── Abstract operations (Miden lowering) ──
            IROp::EmitEvent {
                name, field_count, ..
            } => {
                self.emit(out, &format!("# emit {} ({} fields)", name, field_count));
                for _ in 0..*field_count {
                    self.emit(out, "drop  # event field");
                }
            }
            IROp::SealEvent {
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
            IROp::StorageRead { width } => {
                self.emit(out, &format!("mem_load  # read {}", width));
            }
            IROp::StorageWrite { width } => {
                self.emit(out, &format!("mem_store  # write {}", width));
            }
            IROp::HashDigest => {
                self.emit(out, "hperm");
            }

            // ── Control flow (flat) ──
            IROp::Call(label) => self.emit(out, &format!("exec.{}", label)),
            IROp::Return => { /* Miden uses end, handled by FnEnd */ }
            IROp::Halt => { /* Miden uses end, handled by program structure */ }

            // ── Control flow (structural) ──
            IROp::IfElse {
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
            IROp::IfOnly { then_body } => {
                self.emit(out, "if.true");
                self.indent += 1;
                for body_op in then_body {
                    self.lower_op(body_op, out);
                }
                self.indent -= 1;
                self.emit(out, "end");
            }
            IROp::Loop { label: _, body } => {
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
            IROp::Label(name) => {
                out.push(format!("proc.{}", name));
            }
            IROp::FnStart(name) => {
                out.push(format!("proc.{}", name));
                self.indent = 1;
            }
            IROp::FnEnd => {
                self.indent = 0;
                out.push("end".to_string());
                out.push(String::new());
            }
            IROp::Preamble(main_label) => {
                out.push("begin".to_string());
                out.push(format!("    exec.{}", main_label));
                out.push("end".to_string());
                out.push(String::new());
            }
            IROp::BlankLine => {
                out.push(String::new());
            }

            // ── Passthrough ──
            IROp::Comment(text) => {
                self.emit(out, &format!("# {}", text));
            }
            IROp::RawAsm { lines, .. } => {
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
    fn lower(&self, ops: &[IROp]) -> Vec<String> {
        let mut lowerer = MidenLowering::new();
        let mut out = Vec::new();
        for op in ops {
            lowerer.lower_op(op, &mut out);
        }
        out
    }
}
