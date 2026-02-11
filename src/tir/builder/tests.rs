//! TIRBuilder unit tests.

use super::*;
use crate::ast::*;
use crate::span::{Span, Spanned};

use self::helpers::parse_spill_effect;

fn dummy_span() -> Span {
    Span::dummy()
}

fn sp<T>(node: T) -> Spanned<T> {
    Spanned::new(node, dummy_span())
}

fn minimal_program(items: Vec<Item>) -> File {
    File {
        kind: FileKind::Program,
        name: sp("test".to_string()),
        uses: vec![],
        declarations: vec![],
        items: items.into_iter().map(|i| sp(i)).collect(),
    }
}

fn make_builder() -> TIRBuilder {
    TIRBuilder::new(TargetConfig::triton())
}

// ── Test: minimal program produces Entry + FnStart + FnEnd ──

#[test]
fn test_minimal_program() {
    let file = minimal_program(vec![Item::Fn(FnDef {
        is_pub: false,
        cfg: None,
        intrinsic: None,
        is_test: false,
        is_pure: false,
        requires: vec![],
        ensures: vec![],
        name: sp("main".to_string()),
        type_params: vec![],
        params: vec![],
        return_ty: None,
        body: Some(sp(Block {
            stmts: vec![],
            tail_expr: None,
        })),
    })]);

    let ops = make_builder().build_file(&file);

    assert!(
        ops.iter().any(|op| matches!(op, TIROp::Entry(_))),
        "expected Entry op"
    );
    assert!(
        ops.iter()
            .any(|op| matches!(op, TIROp::FnStart(n) if n == "main")),
        "expected FnStart(main)"
    );
    assert!(
        ops.iter().any(|op| matches!(op, TIROp::Return)),
        "expected Return"
    );
    assert!(
        ops.iter().any(|op| matches!(op, TIROp::FnEnd)),
        "expected FnEnd"
    );
}

// ── Test: if/else produces TIROp::IfElse ──

#[test]
fn test_if_else_produces_structural_op() {
    let file = minimal_program(vec![Item::Fn(FnDef {
        is_pub: false,
        cfg: None,
        intrinsic: None,
        is_test: false,
        is_pure: false,
        requires: vec![],
        ensures: vec![],
        name: sp("main".to_string()),
        type_params: vec![],
        params: vec![],
        return_ty: None,
        body: Some(sp(Block {
            stmts: vec![sp(Stmt::If {
                cond: sp(Expr::Literal(Literal::Bool(true))),
                then_block: sp(Block {
                    stmts: vec![sp(Stmt::Expr(sp(Expr::Call {
                        path: sp(ModulePath::single("pub_write".to_string())),
                        generic_args: vec![],
                        args: vec![sp(Expr::Literal(Literal::Integer(1)))],
                    })))],
                    tail_expr: None,
                }),
                else_block: Some(sp(Block {
                    stmts: vec![sp(Stmt::Expr(sp(Expr::Call {
                        path: sp(ModulePath::single("pub_write".to_string())),
                        generic_args: vec![],
                        args: vec![sp(Expr::Literal(Literal::Integer(0)))],
                    })))],
                    tail_expr: None,
                })),
            })],
            tail_expr: None,
        })),
    })]);

    let ops = make_builder().build_file(&file);

    let has_if_else = ops.iter().any(|op| matches!(op, TIROp::IfElse { .. }));
    assert!(has_if_else, "expected TIROp::IfElse in output");
}

// ── Test: for loop produces TIROp::Loop ──

#[test]
fn test_for_loop_produces_loop_op() {
    let file = minimal_program(vec![Item::Fn(FnDef {
        is_pub: false,
        cfg: None,
        intrinsic: None,
        is_test: false,
        is_pure: false,
        requires: vec![],
        ensures: vec![],
        name: sp("main".to_string()),
        type_params: vec![],
        params: vec![],
        return_ty: None,
        body: Some(sp(Block {
            stmts: vec![sp(Stmt::For {
                var: sp("i".to_string()),
                start: sp(Expr::Literal(Literal::Integer(0))),
                end: sp(Expr::Literal(Literal::Integer(5))),
                bound: Some(5),
                body: sp(Block {
                    stmts: vec![],
                    tail_expr: None,
                }),
            })],
            tail_expr: None,
        })),
    })]);

    let ops = make_builder().build_file(&file);

    let has_loop = ops.iter().any(|op| matches!(op, TIROp::Loop { .. }));
    assert!(has_loop, "expected TIROp::Loop in output");
}

// ── Test: arithmetic produces the right instruction sequence ──

#[test]
fn test_arithmetic_sequence() {
    let file = minimal_program(vec![Item::Fn(FnDef {
        is_pub: false,
        cfg: None,
        intrinsic: None,
        is_test: false,
        is_pure: false,
        requires: vec![],
        ensures: vec![],
        name: sp("main".to_string()),
        type_params: vec![],
        params: vec![],
        return_ty: Some(sp(Type::Field)),
        body: Some(sp(Block {
            stmts: vec![],
            tail_expr: Some(Box::new(sp(Expr::BinOp {
                op: BinOp::Add,
                lhs: Box::new(sp(Expr::Literal(Literal::Integer(2)))),
                rhs: Box::new(sp(Expr::BinOp {
                    op: BinOp::Mul,
                    lhs: Box::new(sp(Expr::Literal(Literal::Integer(3)))),
                    rhs: Box::new(sp(Expr::Literal(Literal::Integer(4)))),
                })),
            }))),
        })),
    })]);

    let ops = make_builder().build_file(&file);

    let flat: Vec<String> = ops.iter().map(|op| format!("{}", op)).collect();
    let joined = flat.join(" | ");

    assert!(
        joined.contains("push 2"),
        "expected push 2, got: {}",
        joined
    );
    assert!(
        joined.contains("push 3"),
        "expected push 3, got: {}",
        joined
    );
    assert!(
        joined.contains("push 4"),
        "expected push 4, got: {}",
        joined
    );
    assert!(joined.contains("mul"), "expected mul, got: {}", joined);
    assert!(joined.contains("add"), "expected add, got: {}", joined);

    let push3_pos = flat.iter().position(|s| s == "push 3").unwrap();
    let push4_pos = flat.iter().position(|s| s == "push 4").unwrap();
    let mul_pos = flat.iter().position(|s| s == "mul").unwrap();
    let push2_pos = flat.iter().position(|s| s == "push 2").unwrap();
    let add_pos = flat.iter().position(|s| s == "add").unwrap();

    assert!(push3_pos < push4_pos, "push 3 should precede push 4");
    assert!(push4_pos < mul_pos, "push 4 should precede mul");
    assert!(push2_pos < add_pos, "push 2 should precede add");
    assert!(mul_pos < add_pos, "mul should precede add");
}

// ── Test: parse_spill_effect ──

#[test]
fn test_parse_spill_effect() {
    assert!(matches!(parse_spill_effect("    push 42"), TIROp::Push(42)));
    assert!(matches!(parse_spill_effect("    swap 5"), TIROp::Swap(5)));
    assert!(matches!(parse_spill_effect("    pop 1"), TIROp::Pop(1)));
    assert!(matches!(
        parse_spill_effect("    write_mem 1"),
        TIROp::WriteMem(1)
    ));
    assert!(matches!(
        parse_spill_effect("    read_mem 1"),
        TIROp::ReadMem(1)
    ));
    assert!(matches!(parse_spill_effect("  dup 3"), TIROp::Dup(3)));
}

// ── Test: module (not program) omits preamble ──

#[test]
fn test_module_no_preamble() {
    let file = File {
        kind: FileKind::Module,
        name: sp("mylib".to_string()),
        uses: vec![],
        declarations: vec![],
        items: vec![sp(Item::Fn(FnDef {
            is_pub: true,
            cfg: None,
            intrinsic: None,
            is_test: false,
            is_pure: false,
            requires: vec![],
            ensures: vec![],
            name: sp("helper".to_string()),
            type_params: vec![],
            params: vec![],
            return_ty: None,
            body: Some(sp(Block {
                stmts: vec![],
                tail_expr: None,
            })),
        }))],
    };

    let ops = make_builder().build_file(&file);

    assert!(
        !ops.iter().any(|op| matches!(op, TIROp::Entry(_))),
        "module should not produce Entry"
    );
    assert!(
        ops.iter()
            .any(|op| matches!(op, TIROp::FnStart(n) if n == "helper")),
        "expected FnStart(helper)"
    );
}

// ── Test: if-only (no else) produces IfOnly ──

#[test]
fn test_if_only_produces_structural_op() {
    let file = minimal_program(vec![Item::Fn(FnDef {
        is_pub: false,
        cfg: None,
        intrinsic: None,
        is_test: false,
        is_pure: false,
        requires: vec![],
        ensures: vec![],
        name: sp("main".to_string()),
        type_params: vec![],
        params: vec![],
        return_ty: None,
        body: Some(sp(Block {
            stmts: vec![sp(Stmt::If {
                cond: sp(Expr::Literal(Literal::Bool(true))),
                then_block: sp(Block {
                    stmts: vec![],
                    tail_expr: None,
                }),
                else_block: None,
            })],
            tail_expr: None,
        })),
    })]);

    let ops = make_builder().build_file(&file);
    let has_if_only = ops.iter().any(|op| matches!(op, TIROp::IfOnly { .. }));
    assert!(has_if_only, "expected TIROp::IfOnly in output");
}

// ── Test: let binding + variable reference ──

#[test]
fn test_let_and_var_ref() {
    let file = minimal_program(vec![Item::Fn(FnDef {
        is_pub: false,
        cfg: None,
        intrinsic: None,
        is_test: false,
        is_pure: false,
        requires: vec![],
        ensures: vec![],
        name: sp("main".to_string()),
        type_params: vec![],
        params: vec![],
        return_ty: Some(sp(Type::Field)),
        body: Some(sp(Block {
            stmts: vec![sp(Stmt::Let {
                mutable: false,
                pattern: Pattern::Name(sp("x".to_string())),
                ty: Some(sp(Type::Field)),
                init: sp(Expr::Literal(Literal::Integer(42))),
            })],
            tail_expr: Some(Box::new(sp(Expr::Var("x".to_string())))),
        })),
    })]);

    let ops = make_builder().build_file(&file);

    let flat: Vec<String> = ops.iter().map(|op| format!("{}", op)).collect();
    assert!(flat.contains(&"push 42".to_string()), "expected push 42");
    assert!(
        flat.contains(&"dup 0".to_string()),
        "expected dup 0 for variable reference"
    );
}

// ── Test: intrinsic call dispatch ──

#[test]
fn test_intrinsic_pub_read_write() {
    let file = minimal_program(vec![Item::Fn(FnDef {
        is_pub: false,
        cfg: None,
        intrinsic: None,
        is_test: false,
        is_pure: false,
        requires: vec![],
        ensures: vec![],
        name: sp("main".to_string()),
        type_params: vec![],
        params: vec![],
        return_ty: None,
        body: Some(sp(Block {
            stmts: vec![sp(Stmt::Expr(sp(Expr::Call {
                path: sp(ModulePath::single("pub_write".to_string())),
                generic_args: vec![],
                args: vec![sp(Expr::Call {
                    path: sp(ModulePath::single("pub_read".to_string())),
                    generic_args: vec![],
                    args: vec![],
                })],
            })))],
            tail_expr: None,
        })),
    })]);

    let ops = make_builder().build_file(&file);

    let has_read = ops.iter().any(|op| matches!(op, TIROp::ReadIo(1)));
    let has_write = ops.iter().any(|op| matches!(op, TIROp::WriteIo(1)));
    assert!(has_read, "expected ReadIo(1)");
    assert!(has_write, "expected WriteIo(1)");
}

// ── Test: IfElse has non-empty nested bodies ──

#[test]
fn test_if_else_nested_bodies_have_content() {
    let file = minimal_program(vec![Item::Fn(FnDef {
        is_pub: false,
        cfg: None,
        intrinsic: None,
        is_test: false,
        is_pure: false,
        requires: vec![],
        ensures: vec![],
        name: sp("main".to_string()),
        type_params: vec![],
        params: vec![],
        return_ty: None,
        body: Some(sp(Block {
            stmts: vec![sp(Stmt::If {
                cond: sp(Expr::Literal(Literal::Bool(true))),
                then_block: sp(Block {
                    stmts: vec![sp(Stmt::Expr(sp(Expr::Call {
                        path: sp(ModulePath::single("pub_write".to_string())),
                        generic_args: vec![],
                        args: vec![sp(Expr::Literal(Literal::Integer(1)))],
                    })))],
                    tail_expr: None,
                }),
                else_block: Some(sp(Block {
                    stmts: vec![sp(Stmt::Expr(sp(Expr::Call {
                        path: sp(ModulePath::single("pub_write".to_string())),
                        generic_args: vec![],
                        args: vec![sp(Expr::Literal(Literal::Integer(0)))],
                    })))],
                    tail_expr: None,
                })),
            })],
            tail_expr: None,
        })),
    })]);

    let ops = make_builder().build_file(&file);

    for op in &ops {
        if let TIROp::IfElse {
            then_body,
            else_body,
        } = op
        {
            assert!(!then_body.is_empty(), "then_body should not be empty");
            assert!(!else_body.is_empty(), "else_body should not be empty");

            let then_has_push1 = then_body.iter().any(|o| matches!(o, TIROp::Push(1)));
            let then_has_write = then_body.iter().any(|o| matches!(o, TIROp::WriteIo(1)));
            assert!(then_has_push1, "then_body should have Push(1)");
            assert!(then_has_write, "then_body should have WriteIo(1)");

            let else_has_push0 = else_body.iter().any(|o| matches!(o, TIROp::Push(0)));
            let else_has_write = else_body.iter().any(|o| matches!(o, TIROp::WriteIo(1)));
            assert!(else_has_push0, "else_body should have Push(0)");
            assert!(else_has_write, "else_body should have WriteIo(1)");

            return;
        }
    }
    panic!("no IfElse op found");
}
