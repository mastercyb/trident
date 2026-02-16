//! TIRBuilder advanced unit tests (pass-through, multi-width).

use crate::ast::*;
use crate::ir::tir::builder::*;
use crate::span::{Span, Spanned};

fn dummy_span() -> Span {
    Span::dummy()
}

fn sp<T>(node: T) -> Spanned<T> {
    Spanned::new(node, dummy_span())
}

fn make_builder() -> TIRBuilder {
    TIRBuilder::new(TargetConfig::triton())
}

// ── Test: pass-through intrinsic emits minimal ops ──

#[test]
fn pass_through_hash_emits_minimal_ops() {
    // fn wrapper(a..j: Field) -> Digest { hash(a, b, c, d, e, f, g, h, i, j) }
    let params: Vec<Param> = (0..10)
        .map(|i| Param {
            name: sp(format!("p{}", i)),
            ty: sp(Type::Field),
        })
        .collect();
    let args: Vec<Spanned<Expr>> = (0..10).map(|i| sp(Expr::Var(format!("p{}", i)))).collect();

    let file = File {
        kind: FileKind::Module,
        name: sp("test".to_string()),
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
            name: sp("wrapper".to_string()),
            type_params: vec![],
            params,
            return_ty: Some(sp(Type::Digest)),
            body: Some(sp(Block {
                stmts: vec![],
                tail_expr: Some(Box::new(sp(Expr::Call {
                    path: sp(ModulePath::single("hash".to_string())),
                    generic_args: vec![],
                    args,
                }))),
            })),
        }))],
    };

    let mut builder = make_builder();
    builder
        .intrinsic_map
        .insert("hash".to_string(), "hash".to_string());
    let ops = builder.build_file(&file);

    // Should be: FnStart, Hash, Return, FnEnd — 4 ops total.
    let fn_ops: Vec<&TIROp> = ops
        .iter()
        .filter(|op| !matches!(op, TIROp::Comment(_)))
        .collect();
    assert_eq!(fn_ops.len(), 4, "expected 4 ops, got: {:?}", fn_ops);
    assert!(matches!(fn_ops[0], TIROp::FnStart(_)));
    assert!(matches!(fn_ops[1], TIROp::Hash { .. }));
    assert!(matches!(fn_ops[2], TIROp::Return));
    assert!(matches!(fn_ops[3], TIROp::FnEnd));
}

// ── Test: non-pass-through uses normal path ──

#[test]
fn non_pass_through_still_compiles_normally() {
    // fn add(a: Field, b: Field) -> Field { a + b }
    let file = File {
        kind: FileKind::Module,
        name: sp("test".to_string()),
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
            name: sp("add".to_string()),
            type_params: vec![],
            params: vec![
                Param {
                    name: sp("a".to_string()),
                    ty: sp(Type::Field),
                },
                Param {
                    name: sp("b".to_string()),
                    ty: sp(Type::Field),
                },
            ],
            return_ty: Some(sp(Type::Field)),
            body: Some(sp(Block {
                stmts: vec![],
                tail_expr: Some(Box::new(sp(Expr::BinOp {
                    op: BinOp::Add,
                    lhs: Box::new(sp(Expr::Var("a".to_string()))),
                    rhs: Box::new(sp(Expr::Var("b".to_string()))),
                }))),
            })),
        }))],
    };

    let ops = make_builder().build_file(&file);

    // Should contain dup and add — NOT the pass-through shortcut.
    let flat: Vec<String> = ops.iter().map(|op| format!("{}", op)).collect();
    assert!(
        flat.iter().any(|s| s.contains("dup")),
        "expected dup for variable access, got: {:?}",
        flat
    );
    assert!(
        flat.iter().any(|s| s == "add"),
        "expected add instruction, got: {:?}",
        flat
    );
}

// ── Test: pass-through user-defined call ──

#[test]
fn pass_through_user_call_emits_call_and_return() {
    // fn wrapper(a: Field) -> Field { target(a) }
    let file = File {
        kind: FileKind::Module,
        name: sp("test".to_string()),
        uses: vec![],
        declarations: vec![],
        items: vec![
            sp(Item::Fn(FnDef {
                is_pub: true,
                cfg: None,
                intrinsic: None,
                is_test: false,
                is_pure: false,
                requires: vec![],
                ensures: vec![],
                name: sp("target".to_string()),
                type_params: vec![],
                params: vec![Param {
                    name: sp("x".to_string()),
                    ty: sp(Type::Field),
                }],
                return_ty: Some(sp(Type::Field)),
                body: Some(sp(Block {
                    stmts: vec![],
                    tail_expr: Some(Box::new(sp(Expr::Var("x".to_string())))),
                })),
            })),
            sp(Item::Fn(FnDef {
                is_pub: true,
                cfg: None,
                intrinsic: None,
                is_test: false,
                is_pure: false,
                requires: vec![],
                ensures: vec![],
                name: sp("wrapper".to_string()),
                type_params: vec![],
                params: vec![Param {
                    name: sp("a".to_string()),
                    ty: sp(Type::Field),
                }],
                return_ty: Some(sp(Type::Field)),
                body: Some(sp(Block {
                    stmts: vec![],
                    tail_expr: Some(Box::new(sp(Expr::Call {
                        path: sp(ModulePath::single("target".to_string())),
                        generic_args: vec![],
                        args: vec![sp(Expr::Var("a".to_string()))],
                    }))),
                })),
            })),
        ],
    };

    let ops = make_builder().build_file(&file);

    // Find wrapper's ops: between FnStart("wrapper") and FnEnd.
    let wrapper_start = ops
        .iter()
        .position(|op| matches!(op, TIROp::FnStart(n) if n == "wrapper"))
        .expect("expected FnStart(wrapper)");
    let wrapper_end = ops[wrapper_start..]
        .iter()
        .position(|op| matches!(op, TIROp::FnEnd))
        .map(|i| i + wrapper_start)
        .expect("expected FnEnd after wrapper");
    let wrapper_ops = &ops[wrapper_start..=wrapper_end];

    // Should be: FnStart, Call(target), Return, FnEnd — 4 ops.
    assert_eq!(
        wrapper_ops.len(),
        4,
        "expected 4 ops for wrapper, got: {:?}",
        wrapper_ops
    );
    assert!(matches!(wrapper_ops[1], TIROp::Call(ref n) if n == "target"));
}

// ── Test: multi-width pass-through (Digest params) ──

#[test]
fn pass_through_multi_width_params_emits_minimal_ops() {
    // fn wrapper(a: Digest, b: Digest) -> Digest { target(a, b) }
    // Digest is 5 elements wide. Without multi-width pass-through, the
    // compiler would dup/register all 10 stack elements, then rebuild them
    // for the call. With the optimization, it emits just Call + Return.
    let file = File {
        kind: FileKind::Module,
        name: sp("test".to_string()),
        uses: vec![],
        declarations: vec![],
        items: vec![
            sp(Item::Fn(FnDef {
                is_pub: true,
                cfg: None,
                intrinsic: None,
                is_test: false,
                is_pure: false,
                requires: vec![],
                ensures: vec![],
                name: sp("target".to_string()),
                type_params: vec![],
                params: vec![
                    Param {
                        name: sp("a".to_string()),
                        ty: sp(Type::Digest),
                    },
                    Param {
                        name: sp("b".to_string()),
                        ty: sp(Type::Digest),
                    },
                ],
                return_ty: Some(sp(Type::Digest)),
                body: Some(sp(Block {
                    stmts: vec![],
                    tail_expr: Some(Box::new(sp(Expr::Var("a".to_string())))),
                })),
            })),
            sp(Item::Fn(FnDef {
                is_pub: true,
                cfg: None,
                intrinsic: None,
                is_test: false,
                is_pure: false,
                requires: vec![],
                ensures: vec![],
                name: sp("wrapper".to_string()),
                type_params: vec![],
                params: vec![
                    Param {
                        name: sp("a".to_string()),
                        ty: sp(Type::Digest),
                    },
                    Param {
                        name: sp("b".to_string()),
                        ty: sp(Type::Digest),
                    },
                ],
                return_ty: Some(sp(Type::Digest)),
                body: Some(sp(Block {
                    stmts: vec![],
                    tail_expr: Some(Box::new(sp(Expr::Call {
                        path: sp(ModulePath::single("target".to_string())),
                        generic_args: vec![],
                        args: vec![
                            sp(Expr::Var("a".to_string())),
                            sp(Expr::Var("b".to_string())),
                        ],
                    }))),
                })),
            })),
        ],
    };

    let ops = make_builder().build_file(&file);

    let wrapper_start = ops
        .iter()
        .position(|op| matches!(op, TIROp::FnStart(n) if n == "wrapper"))
        .expect("expected FnStart(wrapper)");
    let wrapper_end = ops[wrapper_start..]
        .iter()
        .position(|op| matches!(op, TIROp::FnEnd))
        .map(|i| i + wrapper_start)
        .expect("expected FnEnd after wrapper");
    let wrapper_ops = &ops[wrapper_start..=wrapper_end];

    // Should be: FnStart, Call(target), Return, FnEnd — 4 ops.
    assert_eq!(
        wrapper_ops.len(),
        4,
        "expected 4 ops for multi-width pass-through, got: {:?}",
        wrapper_ops
    );
    assert!(matches!(wrapper_ops[1], TIROp::Call(ref n) if n == "target"));
}
