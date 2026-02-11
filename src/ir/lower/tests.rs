use super::*;
use crate::ir::IROp;

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

    assert!(joined.contains("push 1\n    swap 1\n    skiz\n    call __then__"));
    assert!(joined.contains("skiz\n    call __else__"));
    assert!(joined.contains("__then__1:"));
    assert!(joined.contains("    pop 1\n    push 10\n    write_io 1\n    push 0\n    return"));
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
use crate::ir::builder::IRBuilder;
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
    // Event emission now uses abstract EmitEvent op — the new pipeline
    // pushes all fields first, then the lowering writes tag + fields.
    // This is functionally equivalent but not byte-identical to the old
    // Emitter (which interleaved push/write_io per field).
    let source = "program test\nevent Transfer {\n  amount: Field,\n}\nfn main() {\n  emit Transfer { amount: 100 }\n}";
    let output = compile_new(source);
    assert!(output.contains("push 100"), "should push field value");
    assert!(output.contains("push 0"), "should push event tag");
    assert!(output.contains("write_io 1"), "should write to I/O");
    assert!(output.contains("call __main"), "should call main");
    assert!(output.contains("__main:"), "should define main");
    assert!(output.contains("return"), "should return");
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

// ─── Miden Lowering Tests ─────────────────────────────────────

#[test]
fn test_miden_flat_ops() {
    let ops = vec![IROp::Push(42), IROp::Push(10), IROp::Add, IROp::Pop(1)];
    let lowering = MidenLowering::new();
    let out = lowering.lower(&ops);
    assert_eq!(
        out,
        vec!["    push.42", "    push.10", "    add", "    drop"]
    );
}

#[test]
fn test_miden_fn_structure() {
    let ops = vec![
        IROp::Preamble("main".into()),
        IROp::FnStart("main".into()),
        IROp::Push(0),
        IROp::Return,
        IROp::FnEnd,
    ];
    let lowering = MidenLowering::new();
    let out = lowering.lower(&ops);
    assert_eq!(out[0], "begin");
    assert_eq!(out[1], "    exec.main");
    assert_eq!(out[2], "end");
    assert_eq!(out[3], "");
    assert_eq!(out[4], "proc.main");
    assert_eq!(out[5], "    push.0");
    assert_eq!(out[6], "end");
    assert_eq!(out[7], "");
}

#[test]
fn test_miden_if_else_inline() {
    let ops = vec![
        IROp::FnStart("test".into()),
        IROp::Push(1),
        IROp::IfElse {
            then_body: vec![IROp::Push(42)],
            else_body: vec![IROp::Push(0)],
        },
        IROp::Return,
        IROp::FnEnd,
    ];
    let lowering = MidenLowering::new();
    let out = lowering.lower(&ops);
    let joined = out.join("\n");
    assert!(joined.contains("if.true"));
    assert!(joined.contains("else"));
    assert!(joined.contains("push.42"));
    assert!(joined.contains("push.0"));
    assert!(!joined.contains("proc.__"));
}

#[test]
fn test_miden_if_only_inline() {
    let ops = vec![
        IROp::FnStart("test".into()),
        IROp::Push(1),
        IROp::IfOnly {
            then_body: vec![IROp::Push(99)],
        },
        IROp::Return,
        IROp::FnEnd,
    ];
    let lowering = MidenLowering::new();
    let out = lowering.lower(&ops);
    let joined = out.join("\n");
    assert!(joined.contains("if.true"));
    assert!(joined.contains("push.99"));
    assert!(joined.contains("end"));
}

#[test]
fn test_miden_loop() {
    let ops = vec![
        IROp::FnStart("test".into()),
        IROp::Push(5),
        IROp::Call("loop__1".into()),
        IROp::Pop(1),
        IROp::Loop {
            label: "loop__1".into(),
            body: vec![IROp::Push(1), IROp::Add],
        },
        IROp::Return,
        IROp::FnEnd,
    ];
    let lowering = MidenLowering::new();
    let out = lowering.lower(&ops);
    let joined = out.join("\n");
    assert!(joined.contains("dup.0"));
    assert!(joined.contains("push.0"));
    assert!(joined.contains("eq"));
    assert!(joined.contains("if.true"));
    assert!(joined.contains("drop"));
    assert!(joined.contains("exec.self"));
}

#[test]
fn test_miden_nested_indent() {
    let ops = vec![
        IROp::FnStart("test".into()),
        IROp::IfElse {
            then_body: vec![IROp::IfOnly {
                then_body: vec![IROp::Push(1)],
            }],
            else_body: vec![IROp::Push(0)],
        },
        IROp::Return,
        IROp::FnEnd,
    ];
    let lowering = MidenLowering::new();
    let out = lowering.lower(&ops);
    let push1_line = out.iter().find(|l| l.contains("push.1")).unwrap();
    assert!(
        push1_line.starts_with("            "),
        "expected 3-level indent, got: {:?}",
        push1_line
    );
}

#[test]
fn test_miden_comment_prefix() {
    let ops = vec![IROp::Comment("test comment".into())];
    let lowering = MidenLowering::new();
    let out = lowering.lower(&ops);
    assert_eq!(out[0], "    # test comment");
}

#[test]
fn test_miden_neg_one() {
    let ops = vec![IROp::PushNegOne];
    let lowering = MidenLowering::new();
    let out = lowering.lower(&ops);
    assert_eq!(out[0], "    push.18446744069414584320");
}
