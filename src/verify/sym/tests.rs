use super::*;

fn parse_program(source: &str) -> File {
    crate::parse_source(source, "test.tri").unwrap()
}

#[test]
fn test_simple_assert() {
    let file = parse_program("program test\nfn main() {\n    assert(true)\n}\n");
    let system = analyze(&file);
    assert!(!system.constraints.is_empty(), "should have constraints");
    assert!(system.violated_constraints().is_empty());
}

#[test]
fn test_assert_false_violated() {
    let file = parse_program("program test\nfn main() {\n    assert(false)\n}\n");
    let system = analyze(&file);
    assert!(!system.violated_constraints().is_empty());
}

#[test]
fn test_pub_read_symbolic() {
    let file = parse_program(
        "program test\nfn main() {\n    let x: Field = pub_read()\n    pub_write(x)\n}\n",
    );
    let system = analyze(&file);
    assert_eq!(system.pub_inputs.len(), 1);
    assert_eq!(system.pub_outputs.len(), 1);
}

#[test]
fn test_assert_eq_constants() {
    let file = parse_program("program test\nfn main() {\n    assert_eq(42, 42)\n}\n");
    let system = analyze(&file);
    // Should have a constraint that is trivially true
    assert!(system.violated_constraints().is_empty());
}

#[test]
fn test_assert_eq_constants_violated() {
    let file = parse_program("program test\nfn main() {\n    assert_eq(1, 2)\n}\n");
    let system = analyze(&file);
    assert!(!system.violated_constraints().is_empty());
}

#[test]
fn test_divine_input_tracking() {
    let file = parse_program(
        "program test\nfn main() {\n    let x: Field = divine()\n    let y: Field = divine()\n}\n",
    );
    let system = analyze(&file);
    assert_eq!(system.divine_inputs.len(), 2);
}

#[test]
fn test_arithmetic_simplification() {
    let v =
        SymValue::Add(Box::new(SymValue::Const(3)), Box::new(SymValue::Const(4))).simplify();
    assert_eq!(v, SymValue::Const(7));
}

#[test]
fn test_mul_by_zero() {
    let v = SymValue::Mul(
        Box::new(SymValue::Const(0)),
        Box::new(SymValue::Var(SymVar {
            name: "x".to_string(),
            version: 0,
        })),
    )
    .simplify();
    assert_eq!(v, SymValue::Const(0));
}

#[test]
fn test_add_zero_identity() {
    let x = SymValue::Var(SymVar {
        name: "x".to_string(),
        version: 0,
    });
    let v = SymValue::Add(Box::new(SymValue::Const(0)), Box::new(x.clone())).simplify();
    assert_eq!(v, x);
}

#[test]
fn test_range_u32_constraint() {
    let file = parse_program(
        "program test\nfn main() {\n    let x: Field = pub_read()\n    let y: U32 = as_u32(x)\n}\n",
    );
    let system = analyze(&file);
    let has_range = system
        .constraints
        .iter()
        .any(|c| matches!(c, Constraint::RangeU32(_)));
    assert!(has_range);
}

#[test]
fn test_verify_file_safe() {
    let file = parse_program(
        "program test\nfn main() {\n    let x: Field = pub_read()\n    pub_write(x)\n}\n",
    );
    let result = verify_file(&file);
    assert!(result.is_safe());
}

#[test]
fn test_verify_file_violated() {
    let file = parse_program("program test\nfn main() {\n    assert(false)\n}\n");
    let result = verify_file(&file);
    assert!(!result.is_safe());
}

#[test]
fn test_function_inlining() {
    let file = parse_program(
        "program test\nfn helper() {\n    assert(true)\n}\nfn main() {\n    helper()\n}\n",
    );
    let system = analyze(&file);
    // The inlined assert(true) should produce a constraint
    assert!(!system.constraints.is_empty());
}

#[test]
fn test_if_else_symbolic() {
    let file = parse_program(
        "program test\nfn main() {\n    let x: Field = pub_read()\n    if x == 0 {\n        assert(true)\n    } else {\n        assert(true)\n    }\n}\n",
    );
    let system = analyze(&file);
    assert!(system.violated_constraints().is_empty());
}
