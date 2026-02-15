use super::*;
use crate::sym;

fn parse_and_verify(source: &str) -> VerificationReport {
    let file = crate::parse_source(source, "test.tri").unwrap();
    let system = sym::analyze(&file);
    verify(&system)
}

#[test]
fn test_trivial_safe_program() {
    let report = parse_and_verify("program test\nfn main() {\n    assert(true)\n}\n");
    assert!(report.is_safe());
    assert_eq!(report.verdict, Verdict::Safe);
}

#[test]
fn test_trivial_violated_program() {
    let report = parse_and_verify("program test\nfn main() {\n    assert(false)\n}\n");
    assert!(!report.is_safe());
    assert_eq!(report.verdict, Verdict::StaticViolation);
}

#[test]
fn test_constant_equality_safe() {
    let report = parse_and_verify("program test\nfn main() {\n    assert_eq(42, 42)\n}\n");
    assert!(report.is_safe());
}

#[test]
fn test_constant_equality_violated() {
    let report = parse_and_verify("program test\nfn main() {\n    assert_eq(1, 2)\n}\n");
    assert!(!report.is_safe());
}

#[test]
fn test_arithmetic_identity() {
    // x + 0 == x should always hold
    let report = parse_and_verify(
        "program test\nfn main() {\n    let x: Field = pub_read()\n    assert_eq(x + 0, x)\n}\n",
    );
    assert!(report.is_safe());
}

#[test]
fn test_field_arithmetic_safe() {
    // (x + y) * 1 == x + y
    let report = parse_and_verify(
        "program test\nfn main() {\n    let x: Field = pub_read()\n    let y: Field = pub_read()\n    let z: Field = x + y\n    assert_eq(z * 1, z)\n}\n",
    );
    assert!(report.is_safe());
}

#[test]
fn test_counterexample_for_false_assert() {
    let report = parse_and_verify("program test\nfn main() {\n    assert(false)\n}\n");
    assert!(!report.static_violations.is_empty());
}

#[test]
fn test_random_solver_catches_violation() {
    // assert_eq(x, 0) is not always true — random testing should find a counterexample
    let report = parse_and_verify(
        "program test\nfn main() {\n    let x: Field = pub_read()\n    assert_eq(x, 0)\n}\n",
    );
    // Random testing should catch this since most random x != 0
    assert!(!report.random_result.all_passed || !report.bmc_result.all_passed);
}

#[test]
fn test_divine_and_assert() {
    // divine() value with no constraint is unchecked
    let report = parse_and_verify(
        "program test\nfn main() {\n    let x: Field = divine()\n    assert(true)\n}\n",
    );
    assert!(report.is_safe());
}

#[test]
fn test_field_operations() {
    // Test field arithmetic helpers
    assert_eq!(field_add(1, 2), 3);
    assert_eq!(field_mul(3, 4), 12);
    assert_eq!(field_sub(5, 3), 2);
    assert_eq!(field_sub(0, 1), GOLDILOCKS_P - 1);
    assert_eq!(field_neg(0), 0);
    assert_eq!(field_neg(1), GOLDILOCKS_P - 1);
    assert_eq!(field_mul(field_inv(7), 7), 1);
}

#[test]
fn test_interesting_values_coverage() {
    let values = interesting_field_values(8);
    assert!(values.contains(&0));
    assert!(values.contains(&1));
    assert!(values.contains(&(GOLDILOCKS_P - 1)));
}

#[test]
fn test_bmc_empty_system() {
    let system = ConstraintSystem::new();
    let result = bounded_check(&system, &BmcConfig::default());
    assert!(result.all_passed);
}

#[test]
fn test_format_constraint_display() {
    let c = Constraint::Equal(SymValue::Const(1), SymValue::Const(2));
    let s = format_constraint(&c);
    assert!(s.contains("1"));
    assert!(s.contains("2"));
}

#[test]
fn test_solver_with_if_else() {
    let report = parse_and_verify(
        "program test\nfn main() {\n    let x: Field = pub_read()\n    if x == 0 {\n        assert(true)\n    } else {\n        assert(true)\n    }\n}\n",
    );
    assert!(report.is_safe());
}

#[test]
fn test_inlined_function_verification() {
    let report = parse_and_verify(
        "program test\nfn check(x: Field) {\n    assert_eq(x + 0, x)\n}\nfn main() {\n    let a: Field = pub_read()\n    check(a)\n}\n",
    );
    assert!(report.is_safe());
}
