//! Triton VM lowering — produces TASM from TIR.

use super::Lowering;
use crate::tir::TIROp;

/// A deferred subroutine block collected during lowering.
struct DeferredBlock {
    label: String,
    ops: Vec<TIROp>,
    /// If true, this is a "then" branch: pop the flag on entry, push 0 on exit.
    clears_flag: bool,
}

/// Triton VM lowering — produces TASM from IR.
///
/// Structural control flow (`IfElse`, `IfOnly`, `Loop`) is lowered to
/// Triton's deferred-subroutine pattern with `skiz` + `call` branching.
#[derive(Default)]
pub struct TritonLowering {
    /// Collected deferred blocks (flushed after each function).
    deferred: Vec<DeferredBlock>,
    /// Label counter for generating unique deferred block labels.
    label_counter: u32,
}

impl TritonLowering {
    pub fn new() -> Self {
        Self::default()
    }

    fn fresh_label(&mut self, prefix: &str) -> String {
        self.label_counter += 1;
        format!("__{}__{}", prefix, self.label_counter)
    }

    /// Format a plain label name into Triton's label format.
    fn format_label(&self, name: &str) -> String {
        format!("__{}", name)
    }

    /// Lower a single TIROp to output lines, collecting deferred blocks.
    fn lower_op(&mut self, op: &TIROp, out: &mut Vec<String>) {
        match op {
            // ── Stack ──
            TIROp::Push(v) => out.push(format!("    push {}", v)),
            TIROp::PushNegOne => out.push("    push -1".to_string()),
            TIROp::Pop(n) => out.push(format!("    pop {}", n)),
            TIROp::Dup(d) => out.push(format!("    dup {}", d)),
            TIROp::Swap(d) => out.push(format!("    swap {}", d)),

            // ── Arithmetic ──
            TIROp::Add => out.push("    add".to_string()),
            TIROp::Mul => out.push("    mul".to_string()),
            TIROp::Eq => out.push("    eq".to_string()),
            TIROp::Lt => out.push("    lt".to_string()),
            TIROp::And => out.push("    and".to_string()),
            TIROp::Xor => out.push("    xor".to_string()),
            TIROp::DivMod => out.push("    div_mod".to_string()),
            TIROp::Invert => out.push("    invert".to_string()),
            TIROp::Split => out.push("    split".to_string()),
            TIROp::Log2 => out.push("    log_2_floor".to_string()),
            TIROp::Pow => out.push("    pow".to_string()),
            TIROp::PopCount => out.push("    pop_count".to_string()),

            // ── Recursion — extension field & FRI ──
            TIROp::ExtMul => out.push("    xb_mul".to_string()),
            TIROp::ExtInvert => out.push("    x_invert".to_string()),
            TIROp::FoldExt => out.push("    xx_dot_step".to_string()),
            TIROp::FoldBase => out.push("    xb_dot_step".to_string()),

            // ── I/O ──
            TIROp::ReadIo(n) => out.push(format!("    read_io {}", n)),
            TIROp::WriteIo(n) => out.push(format!("    write_io {}", n)),
            TIROp::Divine(n) => out.push(format!("    divine {}", n)),

            // ── Memory ──
            TIROp::ReadMem(n) => out.push(format!("    read_mem {}", n)),
            TIROp::WriteMem(n) => out.push(format!("    write_mem {}", n)),

            // ── Crypto ──
            TIROp::Hash => out.push("    hash".to_string()),
            TIROp::SpongeInit => out.push("    sponge_init".to_string()),
            TIROp::SpongeAbsorb => out.push("    sponge_absorb".to_string()),
            TIROp::SpongeSqueeze => out.push("    sponge_squeeze".to_string()),
            TIROp::SpongeLoad => out.push("    sponge_absorb_mem".to_string()),
            TIROp::MerkleStep => out.push("    merkle_step".to_string()),
            TIROp::MerkleLoad => out.push("    merkle_step_mem".to_string()),

            // ── Assertions ──
            TIROp::Assert => out.push("    assert".to_string()),
            TIROp::AssertVector => out.push("    assert_vector".to_string()),

            // ── Abstract operations (Triton lowering) ──
            TIROp::Open {
                tag, field_count, ..
            } => {
                // Triton: write tag then each field to public output.
                out.push(format!("    push {}", tag));
                out.push("    write_io 1".to_string());
                for _ in 0..*field_count {
                    out.push("    write_io 1".to_string());
                }
            }
            TIROp::Seal {
                tag, field_count, ..
            } => {
                // Triton: pad to rate=10, hash, write 5-element digest.
                let padding = 9usize.saturating_sub(*field_count as usize);
                for _ in 0..padding {
                    out.push("    push 0".to_string());
                }
                out.push(format!("    push {}", tag));
                out.push("    hash".to_string());
                out.push("    write_io 5".to_string());
            }
            TIROp::ReadStorage { width } => {
                // Triton: read_mem + pop address.
                out.push(format!("    read_mem {}", width));
                out.push("    pop 1".to_string());
            }
            TIROp::WriteStorage { width } => {
                // Triton: write_mem + pop address.
                out.push(format!("    write_mem {}", width));
                out.push("    pop 1".to_string());
            }
            TIROp::HashDigest => {
                // Triton: hash instruction (consumes 10, produces 5).
                out.push("    hash".to_string());
            }

            // ── Control flow (flat) ──
            TIROp::Call(label) => {
                let formatted = if label.starts_with("__") {
                    label.clone()
                } else {
                    self.format_label(label)
                };
                out.push(format!("    call {}", formatted));
            }
            TIROp::Return => out.push("    return".to_string()),
            TIROp::Halt => out.push("    halt".to_string()),

            // ── Control flow (structural) ──
            TIROp::IfElse {
                then_body,
                else_body,
            } => {
                let then_label = self.fresh_label("then");
                let else_label = self.fresh_label("else");

                out.push("    push 1".to_string());
                out.push("    swap 1".to_string());
                out.push("    skiz".to_string());
                out.push(format!("    call {}", then_label));
                out.push("    skiz".to_string());
                out.push(format!("    call {}", else_label));

                self.deferred.push(DeferredBlock {
                    label: then_label,
                    ops: then_body.clone(),
                    clears_flag: true,
                });
                self.deferred.push(DeferredBlock {
                    label: else_label,
                    ops: else_body.clone(),
                    clears_flag: false,
                });
            }
            TIROp::IfOnly { then_body } => {
                let then_label = self.fresh_label("then");

                out.push("    skiz".to_string());
                out.push(format!("    call {}", then_label));

                self.deferred.push(DeferredBlock {
                    label: then_label,
                    ops: then_body.clone(),
                    clears_flag: false,
                });
            }
            TIROp::Loop { label, body } => {
                let formatted_label = if label.starts_with("__") {
                    label.clone()
                } else {
                    self.format_label(label)
                };
                out.push(format!("{}:", formatted_label));
                out.push("    dup 0".to_string());
                out.push("    push 0".to_string());
                out.push("    eq".to_string());
                out.push("    skiz".to_string());
                out.push("    return".to_string());
                out.push("    push -1".to_string());
                out.push("    add".to_string());

                for body_op in body {
                    self.lower_op(body_op, out);
                }

                out.push("    recurse".to_string());
                out.push(String::new());
            }

            // ── Program structure ──
            TIROp::Label(name) => {
                let formatted = if name.starts_with("__") {
                    name.clone()
                } else {
                    self.format_label(name)
                };
                out.push(format!("{}:", formatted));
            }
            TIROp::FnStart(name) => {
                let formatted = if name.starts_with("__") {
                    name.clone()
                } else {
                    self.format_label(name)
                };
                out.push(format!("{}:", formatted));
            }
            TIROp::FnEnd => {
                out.push("    ".to_string());
                self.flush_deferred(out);
            }
            TIROp::Preamble(main_label) => {
                let formatted = if main_label.starts_with("__") {
                    main_label.clone()
                } else {
                    self.format_label(main_label)
                };
                out.push(format!("    call {}", formatted));
                out.push("    halt".to_string());
                out.push(String::new());
            }
            TIROp::BlankLine => {
                out.push(String::new());
            }

            // ── Passthrough ──
            TIROp::Comment(text) => {
                out.push(format!("    // {}", text));
            }
            TIROp::Asm { lines, .. } => {
                for line in lines {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        out.push(format!("    {}", trimmed));
                    }
                }
            }
        }
    }

    /// Flush all deferred blocks, emitting them as labeled subroutines.
    fn flush_deferred(&mut self, out: &mut Vec<String>) {
        while !self.deferred.is_empty() {
            let blocks = std::mem::take(&mut self.deferred);
            for block in blocks {
                out.push(format!("{}:", block.label));

                if block.clears_flag {
                    out.push("    pop 1".to_string());
                }

                for op in &block.ops {
                    self.lower_op(op, out);
                }

                if block.clears_flag {
                    out.push("    push 0".to_string());
                }
                out.push("    return".to_string());
                out.push(String::new());
            }
        }
    }
}

impl Lowering for TritonLowering {
    fn lower(&self, ops: &[TIROp]) -> Vec<String> {
        let mut lowerer = TritonLowering::new();
        let mut out = Vec::new();
        for op in ops {
            lowerer.lower_op(op, &mut out);
        }
        out
    }
}
