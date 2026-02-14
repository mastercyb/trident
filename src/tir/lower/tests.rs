use super::*;
use crate::tir::TIROp;

#[test]
fn test_lower_flat_ops() {
    let ops = vec![TIROp::Push(42), TIROp::Push(10), TIROp::Add, TIROp::Pop(1)];
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
        TIROp::Entry("main".into()),
        TIROp::FnStart("main".into()),
        TIROp::Push(0),
        TIROp::Return,
        TIROp::FnEnd,
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
        TIROp::FnStart("test".into()),
        TIROp::Push(1), // condition
        TIROp::IfElse {
            then_body: vec![TIROp::Push(10), TIROp::WriteIo(1)],
            else_body: vec![TIROp::Push(20), TIROp::WriteIo(1)],
        },
        TIROp::Return,
        TIROp::FnEnd,
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
        TIROp::Push(1),
        TIROp::IfOnly {
            then_body: vec![TIROp::Push(42), TIROp::WriteIo(1)],
        },
        TIROp::FnEnd,
    ];
    let lowering = TritonLowering::new();
    let out = lowering.lower(&ops);
    let joined = out.join("\n");

    assert!(joined.contains("skiz\n    call __then__"));
    assert!(joined.contains("push 42\n    write_io 1\n    return"));
}

#[test]
fn test_lower_loop() {
    let ops = vec![TIROp::Loop {
        label: "loop__1".into(),
        body: vec![TIROp::Push(1), TIROp::Add],
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
        TIROp::FnStart("my_func".into()),
        TIROp::Call("other_func".into()),
    ];
    let lowering = TritonLowering::new();
    let out = lowering.lower(&ops);
    assert_eq!(out[0], "__my_func:");
    assert_eq!(out[1], "    call __other_func");
}

#[test]
fn test_lower_comment_and_raw() {
    let ops = vec![
        TIROp::Comment("test comment".into()),
        TIROp::Asm {
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
        TIROp::Hash { width: 0 },
        TIROp::SpongeInit,
        TIROp::SpongeAbsorb,
        TIROp::SpongeSqueeze,
        TIROp::MerkleStep,
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
        TIROp::Call("__main".into()),
        TIROp::FnStart("__my_label".into()),
    ];
    let lowering = TritonLowering::new();
    let out = lowering.lower(&ops);
    assert_eq!(out[0], "    call __main");
    assert_eq!(out[1], "__my_label:");
}

// ─── End-to-end regression tests ──────────────────────────────

use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::target::TargetConfig;
use crate::tir::builder::TIRBuilder;

/// Compile source through the TIR pipeline to TASM.
fn compile_to_tasm(source: &str) -> String {
    let (tokens, _, _) = Lexer::new(source, 0).tokenize();
    let file = Parser::new(tokens).parse_file().unwrap();
    let config = TargetConfig::triton();
    let ir = TIRBuilder::new(config).build_file(&file);
    let lowering = TritonLowering::new();
    lowering.lower(&ir).join("\n")
}

#[test]
fn test_regression_event_emission() {
    let source = "program test\nevent Transfer {\n  amount: Field,\n}\nfn main() {\n  reveal Transfer { amount: 100 }\n}";
    let output = compile_to_tasm(source);
    assert!(output.contains("push 100"), "should push field value");
    assert!(output.contains("push 0"), "should push event tag");
    assert!(output.contains("write_io 1"), "should write to I/O");
    assert!(output.contains("call __main"), "should call main");
    assert!(output.contains("__main:"), "should define main");
    assert!(output.contains("return"), "should return");
}

#[test]
fn test_nested_if_else_deferred() {
    let ops = vec![
        TIROp::FnStart("test".into()),
        TIROp::Push(1),
        TIROp::IfElse {
            then_body: vec![
                TIROp::Push(1),
                TIROp::IfOnly {
                    then_body: vec![TIROp::Push(99), TIROp::WriteIo(1)],
                },
            ],
            else_body: vec![TIROp::Push(0)],
        },
        TIROp::Return,
        TIROp::FnEnd,
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
