use super::*;

fn parse_program(source: &str) -> File {
    crate::parse_source_silent(source, "test.tri").unwrap()
}

// -- Accumulation pattern --

#[test]
fn test_accumulation_pattern() {
    let source = r#"program test
fn sum_loop() -> Field {
let mut acc: Field = 0
for i in 0..10 {
    acc = acc + i
}
acc
}
fn main() {}
"#;
    let file = parse_program(source);
    let specs = synthesize_specs(&file);
    let acc_specs: Vec<_> = specs.iter().filter(|s| s.function == "sum_loop").collect();
    assert!(
        !acc_specs.is_empty(),
        "should synthesize specs for accumulation pattern"
    );
    let has_loop_inv = acc_specs
        .iter()
        .any(|s| matches!(&s.kind, SpecKind::LoopInvariant { .. }));
    assert!(has_loop_inv, "should produce a loop invariant");
    let has_acc_inv = acc_specs
        .iter()
        .any(|s| s.expression.contains("acc") && s.expression.contains(">= 0"));
    assert!(has_acc_inv, "should produce acc >= 0 invariant");
}

// -- Counting pattern --

#[test]
fn test_counting_pattern() {
    let source = r#"program test
fn count_loop() -> Field {
let mut count: Field = 0
for i in 0..10 {
    if i == 5 {
        count = count + 1
    }
}
count
}
fn main() {}
"#;
    let file = parse_program(source);
    let specs = synthesize_specs(&file);
    let count_specs: Vec<_> = specs
        .iter()
        .filter(|s| s.function == "count_loop")
        .collect();
    assert!(
        !count_specs.is_empty(),
        "should synthesize specs for counting pattern"
    );
    let has_count_bound = count_specs
        .iter()
        .any(|s| s.expression.contains("count") && s.expression.contains("<= i"));
    assert!(has_count_bound, "should produce count <= i loop invariant");
    let has_post = count_specs.iter().any(|s| {
        s.kind == SpecKind::Postcondition
            && s.expression.contains("count")
            && s.expression.contains("<= 10")
    });
    assert!(has_post, "should produce count <= N postcondition");
}

// -- Postcondition inference --

#[test]
fn test_postcondition_simple_return() {
    let source = r#"program test
fn add(a: Field, b: Field) -> Field {
a + b
}
fn main() {}
"#;
    let file = parse_program(source);
    let specs = synthesize_specs(&file);
    let add_specs: Vec<_> = specs.iter().filter(|s| s.function == "add").collect();
    let has_post = add_specs
        .iter()
        .any(|s| s.kind == SpecKind::Postcondition && s.expression.contains("result == a + b"));
    assert!(has_post, "should infer postcondition result == a + b");
}

// -- Precondition inference --

#[test]
fn test_precondition_from_assert() {
    let source = r#"program test
fn guarded(x: Field) {
assert(x == 0)
}
fn main() {}
"#;
    let file = parse_program(source);
    let specs = synthesize_specs(&file);
    let guard_specs: Vec<_> = specs.iter().filter(|s| s.function == "guarded").collect();
    let has_pre = guard_specs
        .iter()
        .any(|s| s.kind == SpecKind::Precondition && s.expression.contains("x"));
    assert!(has_pre, "should infer precondition from assert(x == 0)");
}

#[test]
fn test_precondition_from_as_u32() {
    let source = r#"program test
fn range_check(val: Field) -> Field {
let x: U32 = as_u32(val)
val
}
fn main() {}
"#;
    let file = parse_program(source);
    let specs = synthesize_specs(&file);
    let rc_specs: Vec<_> = specs
        .iter()
        .filter(|s| s.function == "range_check")
        .collect();
    let has_range_pre = rc_specs
        .iter()
        .any(|s| s.kind == SpecKind::Precondition && s.expression.contains("val <= 4294967295"));
    assert!(
        has_range_pre,
        "should infer U32 range precondition from as_u32(val)"
    );
}

// -- CEGIS basic --

#[test]
fn test_cegis_verifies_true_candidate() {
    // A trivially safe program: assert(true)
    let source = "program test\nfn main() {\n    assert(true)\n}\n";
    let result = verify_candidate(source, "true");
    assert!(result, "should verify that assert(true) is safe");
}

#[test]
fn test_cegis_rejects_false_candidate() {
    // A violated program: assert(false)
    let source = "program test\nfn main() {\n    assert(false)\n}\n";
    let result = verify_candidate(source, "false");
    assert!(!result, "should reject program with assert(false)");
}

// -- Trivial programs --

#[test]
fn test_no_specs_for_trivial_program() {
    let source = "program test\nfn main() {}\n";
    let file = parse_program(source);
    let specs = synthesize_specs(&file);
    assert!(
        specs.is_empty(),
        "should not synthesize specs for trivial empty main"
    );
}

// -- Identity preservation --

#[test]
fn test_identity_preservation() {
    let source = r#"program test
fn identity(x: Field) -> Field {
x
}
fn main() {}
"#;
    let file = parse_program(source);
    let specs = synthesize_specs(&file);
    let id_specs: Vec<_> = specs.iter().filter(|s| s.function == "identity").collect();
    let has_identity = id_specs
        .iter()
        .any(|s| s.kind == SpecKind::Postcondition && s.expression == "result == x");
    assert!(has_identity, "should detect identity preservation");
}

// -- Range preservation --

#[test]
fn test_range_preservation() {
    let source = r#"program test
fn u32_op(a: U32, b: U32) -> U32 {
a + b
}
fn main() {}
"#;
    let file = parse_program(source);
    let specs = synthesize_specs(&file);
    let u32_specs: Vec<_> = specs.iter().filter(|s| s.function == "u32_op").collect();
    let has_range = u32_specs
        .iter()
        .any(|s| s.kind == SpecKind::Postcondition && s.expression.contains("4294967295"));
    assert!(has_range, "should suggest U32 range postcondition");
}

// -- Constant result --

#[test]
fn test_constant_result() {
    let source = r#"program test
fn always_42() -> Field {
42
}
fn main() {}
"#;
    let file = parse_program(source);
    let specs = synthesize_specs(&file);
    let const_specs: Vec<_> = specs.iter().filter(|s| s.function == "always_42").collect();
    let has_const = const_specs
        .iter()
        .any(|s| s.kind == SpecKind::Postcondition && s.expression == "result == 42");
    assert!(has_const, "should detect constant result");
}

// -- Monotonic pattern --

#[test]
fn test_monotonic_pattern() {
    let source = r#"program test
fn mono() -> Field {
let mut x: Field = 0
for i in 0..5 {
    x = x + 3
}
x
}
fn main() {}
"#;
    let file = parse_program(source);
    let specs = synthesize_specs(&file);
    let mono_specs: Vec<_> = specs.iter().filter(|s| s.function == "mono").collect();
    let has_mono = mono_specs.iter().any(|s| {
        matches!(&s.kind, SpecKind::LoopInvariant { .. }) && s.expression.contains("x >= 0")
    });
    assert!(has_mono, "should detect monotonic increase pattern");
}

// -- Symbolic postcondition inference --

#[test]
fn test_symbolic_postcondition_constant_output() {
    let source = "program test\nfn main() {\n    pub_write(42)\n}\n";
    let file = parse_program(source);
    let system = sym::analyze(&file);
    // Find the main function
    let main_fn = file
        .items
        .iter()
        .find_map(|item| {
            if let Item::Fn(f) = &item.node {
                if f.name.node == "main" {
                    return Some(f);
                }
            }
            None
        })
        .unwrap();
    let specs = infer_postconditions_from_constraints(main_fn, &system);
    let has_const_out = specs
        .iter()
        .any(|s| s.expression.contains("output[0] == 42"));
    assert!(has_const_out, "should detect constant output value");
}

// -- Weaken candidate --

#[test]
fn test_weaken_candidate_le() {
    let result = weaken_candidate("x <= 10");
    assert_eq!(result, Some("x <= 11".to_string()));
}

#[test]
fn test_weaken_candidate_ge() {
    let result = weaken_candidate("x >= 5");
    assert_eq!(result, Some("x >= 4".to_string()));
}

#[test]
fn test_weaken_candidate_ge_zero() {
    let result = weaken_candidate("x >= 0");
    assert_eq!(result, None, "cannot weaken >= 0 further");
}

// -- Format report --

#[test]
fn test_format_empty_report() {
    let report = format_report(&[]);
    assert!(report.contains("No specifications"));
}

#[test]
fn test_format_nonempty_report() {
    let specs = vec![SynthesizedSpec {
        function: "test".to_string(),
        kind: SpecKind::Postcondition,
        expression: "result == 0".to_string(),
        confidence: 90,
        explanation: "test explanation".to_string(),
    }];
    let report = format_report(&specs);
    assert!(report.contains("Synthesized 1 specification"));
    assert!(report.contains("result == 0"));
}

// -- expr_to_string --

#[test]
fn test_expr_to_string_literal() {
    let expr = Expr::Literal(Literal::Integer(42));
    assert_eq!(expr_to_string(&expr), "42");
}

#[test]
fn test_expr_to_string_var() {
    let expr = Expr::Var("x".to_string());
    assert_eq!(expr_to_string(&expr), "x");
}
