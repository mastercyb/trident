//! Lowering: consumes `Vec<IROp>` and produces target assembly text.
//!
//! Each target implements `Lowering` to control instruction selection
//! and control-flow structure.

use super::IROp;

/// Lowers IR operations into target assembly lines.
pub trait Lowering {
    /// Convert a sequence of IR operations into assembly text lines.
    fn lower(&self, ops: &[IROp]) -> Vec<String>;
}

// ─── Triton VM Lowering ───────────────────────────────────────────

/// A deferred subroutine block collected during lowering.
struct DeferredBlock {
    label: String,
    ops: Vec<IROp>,
    /// If true, this is a "then" branch: pop the flag on entry, push 0 on exit.
    clears_flag: bool,
}

/// Triton VM lowering — produces TASM from IR.
///
/// Structural control flow (`IfElse`, `IfOnly`, `Loop`) is lowered to
/// Triton's deferred-subroutine pattern with `skiz` + `call` branching.
pub struct TritonLowering {
    /// Collected deferred blocks (flushed after each function).
    deferred: Vec<DeferredBlock>,
    /// Label counter for generating unique deferred block labels.
    label_counter: u32,
}

impl TritonLowering {
    pub fn new() -> Self {
        Self {
            deferred: Vec::new(),
            label_counter: 0,
        }
    }

    fn fresh_label(&mut self, prefix: &str) -> String {
        self.label_counter += 1;
        format!("__{}__{}", prefix, self.label_counter)
    }

    /// Format a plain label name into Triton's label format.
    fn format_label(&self, name: &str) -> String {
        format!("__{}", name)
    }

    /// Lower a single IROp to output lines, collecting deferred blocks.
    fn lower_op(&mut self, op: &IROp, out: &mut Vec<String>) {
        match op {
            // ── Stack ──
            IROp::Push(v) => out.push(format!("    push {}", v)),
            IROp::PushNegOne => out.push("    push -1".to_string()),
            IROp::Pop(n) => out.push(format!("    pop {}", n)),
            IROp::Dup(d) => out.push(format!("    dup {}", d)),
            IROp::Swap(d) => out.push(format!("    swap {}", d)),

            // ── Arithmetic ──
            IROp::Add => out.push("    add".to_string()),
            IROp::Mul => out.push("    mul".to_string()),
            IROp::Eq => out.push("    eq".to_string()),
            IROp::Lt => out.push("    lt".to_string()),
            IROp::And => out.push("    and".to_string()),
            IROp::Xor => out.push("    xor".to_string()),
            IROp::DivMod => out.push("    div_mod".to_string()),
            IROp::Invert => out.push("    invert".to_string()),
            IROp::Split => out.push("    split".to_string()),
            IROp::Log2 => out.push("    log_2_floor".to_string()),
            IROp::Pow => out.push("    pow".to_string()),
            IROp::PopCount => out.push("    pop_count".to_string()),

            // ── Extension field ──
            IROp::XbMul => out.push("    xb_mul".to_string()),
            IROp::XInvert => out.push("    x_invert".to_string()),
            IROp::XxDotStep => out.push("    xx_dot_step".to_string()),
            IROp::XbDotStep => out.push("    xb_dot_step".to_string()),

            // ── I/O ──
            IROp::ReadIo(n) => out.push(format!("    read_io {}", n)),
            IROp::WriteIo(n) => out.push(format!("    write_io {}", n)),
            IROp::Divine(n) => out.push(format!("    divine {}", n)),

            // ── Memory ──
            IROp::ReadMem(n) => out.push(format!("    read_mem {}", n)),
            IROp::WriteMem(n) => out.push(format!("    write_mem {}", n)),

            // ── Crypto ──
            IROp::Hash => out.push("    hash".to_string()),
            IROp::SpongeInit => out.push("    sponge_init".to_string()),
            IROp::SpongeAbsorb => out.push("    sponge_absorb".to_string()),
            IROp::SpongeSqueeze => out.push("    sponge_squeeze".to_string()),
            IROp::SpongeAbsorbMem => out.push("    sponge_absorb_mem".to_string()),
            IROp::MerkleStep => out.push("    merkle_step".to_string()),
            IROp::MerkleStepMem => out.push("    merkle_step_mem".to_string()),

            // ── Assertions ──
            IROp::Assert => out.push("    assert".to_string()),
            IROp::AssertVector => out.push("    assert_vector".to_string()),

            // ── Control flow (flat) ──
            IROp::Call(label) => {
                let formatted = if label.starts_with("__") {
                    label.clone()
                } else {
                    self.format_label(label)
                };
                out.push(format!("    call {}", formatted));
            }
            IROp::Return => out.push("    return".to_string()),
            IROp::Halt => out.push("    halt".to_string()),

            // ── Control flow (structural) ──
            IROp::IfElse {
                then_body,
                else_body,
            } => {
                // Triton pattern: push 1; swap 1; skiz; call then; skiz; call else
                // Then and else bodies become deferred subroutines.
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
            IROp::IfOnly { then_body } => {
                // Triton pattern: skiz; call then
                let then_label = self.fresh_label("then");

                out.push("    skiz".to_string());
                out.push(format!("    call {}", then_label));

                self.deferred.push(DeferredBlock {
                    label: then_label,
                    ops: then_body.clone(),
                    clears_flag: false,
                });
            }
            IROp::Loop { label, body } => {
                // Triton pattern: the loop is emitted as a labeled subroutine.
                // The call site already emitted `call label; pop 1`.
                // Here we emit the subroutine itself:
                //   label:
                //     dup 0; push 0; eq; skiz; return  (exit if counter == 0)
                //     push -1; add                      (decrement)
                //     {body}
                //     recurse
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

                // Lower loop body inline
                for body_op in body {
                    self.lower_op(body_op, out);
                }

                out.push("    recurse".to_string());
                out.push(String::new());
            }

            // ── Program structure ──
            IROp::Label(name) => {
                let formatted = if name.starts_with("__") {
                    name.clone()
                } else {
                    self.format_label(name)
                };
                out.push(format!("{}:", formatted));
            }
            IROp::FnStart(name) => {
                let formatted = if name.starts_with("__") {
                    name.clone()
                } else {
                    self.format_label(name)
                };
                out.push(format!("{}:", formatted));
            }
            IROp::FnEnd => {
                // Trailing indented blank line after function epilogue
                // (matches Emitter's function_epilogue() which returns ["return", ""])
                out.push("    ".to_string());
                // Flush all deferred blocks collected during this function.
                self.flush_deferred(out);
            }
            IROp::Preamble(main_label) => {
                let formatted = if main_label.starts_with("__") {
                    main_label.clone()
                } else {
                    self.format_label(main_label)
                };
                out.push(format!("    call {}", formatted));
                out.push("    halt".to_string());
                out.push(String::new());
            }
            IROp::BlankLine => {
                out.push(String::new());
            }

            // ── Passthrough ──
            IROp::Comment(text) => {
                out.push(format!("    // {}", text));
            }
            IROp::RawAsm { lines, .. } => {
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
    /// Deferred blocks can create new deferred blocks (nested if/else),
    /// so we loop until empty.
    fn flush_deferred(&mut self, out: &mut Vec<String>) {
        while !self.deferred.is_empty() {
            let blocks = std::mem::take(&mut self.deferred);
            for block in blocks {
                // Label
                out.push(format!("{}:", block.label));

                // Prologue: if clears_flag, pop the flag
                if block.clears_flag {
                    out.push("    pop 1".to_string());
                }

                // Body
                for op in &block.ops {
                    self.lower_op(op, out);
                }

                // Epilogue: if clears_flag, push 0 to clear it; then return
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
    fn lower(&self, ops: &[IROp]) -> Vec<String> {
        // We need mutability for deferred block collection, so create a fresh instance.
        let mut lowerer = TritonLowering::new();
        let mut out = Vec::new();
        for op in ops {
            lowerer.lower_op(op, &mut out);
        }
        out
    }
}

// ─── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lower_flat_ops() {
        let ops = vec![IROp::Push(42), IROp::Push(10), IROp::Add, IROp::Pop(1)];
        let lowering = TritonLowering::new();
        let out = lowering.lower(&ops);
        assert_eq!(
            out,
            vec!["    push 42", "    push 10", "    add", "    pop 1",]
        );
    }

    #[test]
    fn test_lower_fn_structure() {
        let ops = vec![
            IROp::Preamble("main".into()),
            IROp::FnStart("main".into()),
            IROp::Push(0),
            IROp::Return,
            IROp::FnEnd,
        ];
        let lowering = TritonLowering::new();
        let out = lowering.lower(&ops);
        assert_eq!(out[0], "    call __main");
        assert_eq!(out[1], "    halt");
        assert_eq!(out[2], "");
        assert_eq!(out[3], "__main:");
        assert_eq!(out[4], "    push 0");
        assert_eq!(out[5], "    return");
    }

    #[test]
    fn test_lower_if_else() {
        let ops = vec![
            IROp::FnStart("test".into()),
            IROp::Push(1), // condition
            IROp::IfElse {
                then_body: vec![IROp::Push(10), IROp::WriteIo(1)],
                else_body: vec![IROp::Push(20), IROp::WriteIo(1)],
            },
            IROp::Return,
            IROp::FnEnd,
        ];
        let lowering = TritonLowering::new();
        let out = lowering.lower(&ops);
        let joined = out.join("\n");

        // Should have skiz + call pattern
        assert!(joined.contains("push 1\n    swap 1\n    skiz\n    call __then__"));
        assert!(joined.contains("skiz\n    call __else__"));

        // Should have deferred then block with pop 1 ... push 0 ... return
        assert!(joined.contains("__then__1:"));
        assert!(joined.contains("    pop 1\n    push 10\n    write_io 1\n    push 0\n    return"));

        // Should have deferred else block without pop/push (no flag clearing)
        assert!(joined.contains("__else__2:"));
        assert!(joined.contains("    push 20\n    write_io 1\n    return"));
    }

    #[test]
    fn test_lower_if_only() {
        let ops = vec![
            IROp::Push(1),
            IROp::IfOnly {
                then_body: vec![IROp::Push(42), IROp::WriteIo(1)],
            },
            IROp::FnEnd,
        ];
        let lowering = TritonLowering::new();
        let out = lowering.lower(&ops);
        let joined = out.join("\n");

        assert!(joined.contains("skiz\n    call __then__"));
        assert!(joined.contains("push 42\n    write_io 1\n    return"));
    }

    #[test]
    fn test_lower_loop() {
        let ops = vec![IROp::Loop {
            label: "loop__1".into(),
            body: vec![IROp::Push(1), IROp::Add],
        }];
        let lowering = TritonLowering::new();
        let out = lowering.lower(&ops);
        let joined = out.join("\n");

        assert!(joined.contains("__loop__1:"));
        assert!(joined.contains("dup 0\n    push 0\n    eq\n    skiz\n    return"));
        assert!(joined.contains("push -1\n    add"));
        assert!(joined.contains("push 1\n    add\n    recurse"));
    }

    #[test]
    fn test_lower_label_formatting() {
        let ops = vec![
            IROp::Label("my_func".into()),
            IROp::Call("other_func".into()),
        ];
        let lowering = TritonLowering::new();
        let out = lowering.lower(&ops);
        assert_eq!(out[0], "__my_func:");
        assert_eq!(out[1], "    call __other_func");
    }

    #[test]
    fn test_lower_comment_and_raw() {
        let ops = vec![
            IROp::Comment("test comment".into()),
            IROp::RawAsm {
                lines: vec!["nop".into(), "nop".into()],
                effect: 0,
            },
        ];
        let lowering = TritonLowering::new();
        let out = lowering.lower(&ops);
        assert_eq!(out[0], "    // test comment");
        assert_eq!(out[1], "    nop");
        assert_eq!(out[2], "    nop");
    }

    #[test]
    fn test_lower_crypto_ops() {
        let ops = vec![
            IROp::Hash,
            IROp::SpongeInit,
            IROp::SpongeAbsorb,
            IROp::SpongeSqueeze,
            IROp::MerkleStep,
        ];
        let lowering = TritonLowering::new();
        let out = lowering.lower(&ops);
        assert_eq!(
            out,
            vec![
                "    hash",
                "    sponge_init",
                "    sponge_absorb",
                "    sponge_squeeze",
                "    merkle_step",
            ]
        );
    }

    #[test]
    fn test_lower_already_prefixed_labels() {
        // Labels that already have __ prefix should not be double-prefixed
        let ops = vec![
            IROp::Call("__main".into()),
            IROp::Label("__my_label".into()),
        ];
        let lowering = TritonLowering::new();
        let out = lowering.lower(&ops);
        assert_eq!(out[0], "    call __main");
        assert_eq!(out[1], "__my_label:");
    }

    // ─── Comparison Tests: IRBuilder + TritonLowering == Emitter ─────

    use crate::codegen::emitter::Emitter;
    use crate::codegen::ir::builder::IRBuilder;
    use crate::lexer::Lexer;
    use crate::parser::Parser;
    use crate::target::TargetConfig;

    /// Compile with old Emitter path.
    fn compile_old(source: &str) -> String {
        let (tokens, _, _) = Lexer::new(source, 0).tokenize();
        let file = Parser::new(tokens).parse_file().unwrap();
        Emitter::new().emit_file(&file)
    }

    /// Compile with new IRBuilder + TritonLowering path.
    fn compile_new(source: &str) -> String {
        let (tokens, _, _) = Lexer::new(source, 0).tokenize();
        let file = Parser::new(tokens).parse_file().unwrap();
        let config = TargetConfig::triton();
        let ir = IRBuilder::new(config).build_file(&file);
        let lowering = TritonLowering::new();
        let lines = lowering.lower(&ir);
        lines.join("\n")
    }

    /// Assert both paths produce identical output, with diff on failure.
    fn assert_identical(source: &str, test_name: &str) {
        let old = compile_old(source);
        let new = compile_new(source);
        if old != new {
            let old_lines: Vec<&str> = old.lines().collect();
            let new_lines: Vec<&str> = new.lines().collect();
            let max = old_lines.len().max(new_lines.len());
            let mut first_diff = None;
            for i in 0..max {
                let ol = old_lines.get(i).unwrap_or(&"<missing>");
                let nl = new_lines.get(i).unwrap_or(&"<missing>");
                if ol != nl && first_diff.is_none() {
                    first_diff = Some(i);
                }
            }
            let diff_line = first_diff.unwrap_or(0);
            let start = diff_line.saturating_sub(3);
            let end = (diff_line + 5).min(max);
            let mut context = String::new();
            for i in start..end {
                let marker = if i == diff_line { ">>>" } else { "   " };
                let ol = old_lines.get(i).unwrap_or(&"<missing>");
                let nl = new_lines.get(i).unwrap_or(&"<missing>");
                context.push_str(&format!(
                    "{} L{}: old={:?} new={:?}\n",
                    marker,
                    i + 1,
                    ol,
                    nl
                ));
            }
            panic!(
                "[{}] IR output differs from Emitter at line {}.\n\
                 Old lines: {}, New lines: {}\n\n{}",
                test_name,
                diff_line + 1,
                old_lines.len(),
                new_lines.len(),
                context
            );
        }
    }

    #[test]
    fn test_compare_minimal_program() {
        assert_identical("program test\nfn main() {\n}", "minimal_program");
    }

    #[test]
    fn test_compare_arithmetic() {
        assert_identical(
            "program test\nfn main() {\n  let a: Field = 10\n  let b: Field = 20\n  let c: Field = a + b\n  pub_write(c)\n}",
            "arithmetic",
        );
    }

    #[test]
    fn test_compare_if_else() {
        assert_identical(
            "program test\nfn main() {\n  let x: Field = pub_read()\n  if x == 0 {\n    pub_write(1)\n  } else {\n    pub_write(2)\n  }\n}",
            "if_else",
        );
    }

    #[test]
    fn test_compare_if_only() {
        assert_identical(
            "program test\nfn main() {\n  let x: Field = pub_read()\n  if x == 1 {\n    pub_write(42)\n  }\n}",
            "if_only",
        );
    }

    #[test]
    fn test_compare_for_loop() {
        assert_identical(
            "program test\nfn main() {\n  let n: Field = 5\n  for i in 0..n bounded 10 {\n    pub_write(i)\n  }\n}",
            "for_loop",
        );
    }

    #[test]
    fn test_compare_function_call() {
        assert_identical(
            "program test\nfn double(x: Field) -> Field {\n  x * 2\n}\nfn main() {\n  let r: Field = double(21)\n  pub_write(r)\n}",
            "function_call",
        );
    }

    #[test]
    fn test_compare_multiple_functions() {
        assert_identical(
            "program test\nfn add(a: Field, b: Field) -> Field {\n  a + b\n}\nfn sub(a: Field, b: Field) -> Field {\n  a + b\n}\nfn main() {\n  let x: Field = add(10, 20)\n  let y: Field = sub(30, 10)\n  pub_write(x + y)\n}",
            "multiple_functions",
        );
    }

    #[test]
    fn test_compare_mutable_variable() {
        assert_identical(
            "program test\nfn main() {\n  let mut x: Field = 0\n  x = x + 1\n  x = x + 2\n  pub_write(x)\n}",
            "mutable_variable",
        );
    }

    #[test]
    fn test_compare_nested_if() {
        assert_identical(
            "program test\nfn main() {\n  let x: Field = pub_read()\n  if x == 0 {\n    if x == 0 {\n      pub_write(1)\n    }\n  } else {\n    pub_write(2)\n  }\n}",
            "nested_if",
        );
    }

    #[test]
    fn test_compare_struct() {
        assert_identical(
            "program test\nstruct Point {\n  x: Field,\n  y: Field,\n}\nfn origin() -> Point {\n  Point { x: 0, y: 0 }\n}\nfn main() {\n  let p: Point = origin()\n  pub_write(p.x)\n  pub_write(p.y)\n}",
            "struct",
        );
    }

    #[test]
    fn test_compare_event() {
        assert_identical(
            "program test\nevent Transfer {\n  amount: Field,\n}\nfn main() {\n  emit Transfer { amount: 100 }\n}",
            "event",
        );
    }

    #[test]
    fn test_compare_fib() {
        assert_identical(
            "program test\nfn fib(n: Field) -> Field {\n  let mut a: Field = 0\n  let mut b: Field = 1\n  for i in 0..n bounded 20 {\n    let t: Field = b\n    b = a + b\n    a = t\n  }\n  a\n}\nfn main() {\n  let r: Field = fib(10)\n  pub_write(r)\n}",
            "fib",
        );
    }

    #[test]
    fn test_nested_if_else_deferred() {
        // If/else inside if/else — should produce nested deferred blocks
        let ops = vec![
            IROp::FnStart("test".into()),
            IROp::Push(1),
            IROp::IfElse {
                then_body: vec![
                    IROp::Push(1),
                    IROp::IfOnly {
                        then_body: vec![IROp::Push(99), IROp::WriteIo(1)],
                    },
                ],
                else_body: vec![IROp::Push(0)],
            },
            IROp::Return,
            IROp::FnEnd,
        ];
        let lowering = TritonLowering::new();
        let out = lowering.lower(&ops);

        // Should have nested deferred: outer then/else, inner then
        // Count deferred labels (they all end with ":")
        let label_count = out
            .iter()
            .filter(|l| l.ends_with(':') && l.starts_with("__"))
            .count();
        assert!(
            label_count >= 3,
            "expected at least 3 deferred labels, got {}",
            label_count
        );
    }
}
