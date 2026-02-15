use super::*;

fn parse(source: &str) -> File {
    crate::parse_source_silent(source, "test.tri").unwrap()
}

// --- Trivially equivalent (same body, different names) ---

#[test]
fn test_trivially_equivalent() {
    let source = r#"program test

fn add_v1(a: Field, b: Field) -> Field {
a + b
}

fn add_v2(a: Field, b: Field) -> Field {
a + b
}

fn main() { }
"#;
    let file = parse(source);
    let result = check_equivalence(&file, "add_v1", "add_v2");
    assert_eq!(result.verdict, EquivalenceVerdict::Equivalent);
    assert!(
        result.method.contains("hash") || result.method.contains("polynomial"),
        "should use fast path; got: {}",
        result.method
    );
}

// --- Alpha-equivalent (different variable names) ---

#[test]
fn test_alpha_equivalent() {
    let source = r#"program test

fn add_xy(x: Field, y: Field) -> Field {
x + y
}

fn add_ab(a: Field, b: Field) -> Field {
a + b
}

fn main() { }
"#;
    let file = parse(source);
    let result = check_equivalence(&file, "add_xy", "add_ab");
    assert_eq!(result.verdict, EquivalenceVerdict::Equivalent);
    assert!(
        result.method.contains("hash"),
        "alpha-equivalent functions should match by hash; got: {}",
        result.method
    );
}

// --- Equivalent with different computation paths (commutativity) ---

#[test]
fn test_commutative_equivalent() {
    let source = r#"program test

fn f(x: Field, y: Field) -> Field {
x + y
}

fn g(x: Field, y: Field) -> Field {
y + x
}

fn main() { }
"#;
    let file = parse(source);
    let result = check_equivalence(&file, "f", "g");
    assert_eq!(result.verdict, EquivalenceVerdict::Equivalent);
}

// --- Non-equivalent functions (counterexample expected) ---

#[test]
fn test_not_equivalent() {
    let source = r#"program test

fn f(x: Field, y: Field) -> Field {
x + y
}

fn g(x: Field, y: Field) -> Field {
x * y
}

fn main() { }
"#;
    let file = parse(source);
    let result = check_equivalence(&file, "f", "g");
    assert_eq!(result.verdict, EquivalenceVerdict::NotEquivalent);
}

// --- Signature mismatch ---

#[test]
fn test_signature_mismatch() {
    let source = r#"program test

fn f(x: Field) -> Field {
x
}

fn g(x: Field, y: Field) -> Field {
x + y
}

fn main() { }
"#;
    let file = parse(source);
    let result = check_equivalence(&file, "f", "g");
    assert_eq!(result.verdict, EquivalenceVerdict::Unknown);
    assert!(result.method.contains("mismatch"));
}

// --- Function not found ---

#[test]
fn test_function_not_found() {
    let source = r#"program test

fn f(x: Field) -> Field {
x
}

fn main() { }
"#;
    let file = parse(source);
    let result = check_equivalence(&file, "f", "nonexistent");
    assert_eq!(result.verdict, EquivalenceVerdict::Unknown);
    assert!(result.method.contains("not found"));
}

// --- Polynomial normalization ---

#[test]
fn test_polynomial_normalization_const() {
    let val = SymValue::Const(42);
    let poly = normalize_polynomial(&val).unwrap();
    assert_eq!(poly, vec![(42, vec![])]);
}

#[test]
fn test_polynomial_normalization_var() {
    let val = SymValue::Var(crate::sym::SymVar {
        name: "x".to_string(),
        version: 0,
    });
    let poly = normalize_polynomial(&val).unwrap();
    assert_eq!(poly, vec![(1, vec!["x".to_string()])]);
}

#[test]
fn test_polynomial_normalization_add() {
    // x + y
    let x = SymValue::Var(crate::sym::SymVar {
        name: "x".to_string(),
        version: 0,
    });
    let y = SymValue::Var(crate::sym::SymVar {
        name: "y".to_string(),
        version: 0,
    });
    let val = SymValue::Add(Box::new(x), Box::new(y));
    let poly = normalize_polynomial(&val).unwrap();
    assert_eq!(
        poly,
        vec![(1, vec!["x".to_string()]), (1, vec!["y".to_string()]),]
    );
}

#[test]
fn test_polynomial_normalization_mul() {
    // x * y
    let x = SymValue::Var(crate::sym::SymVar {
        name: "x".to_string(),
        version: 0,
    });
    let y = SymValue::Var(crate::sym::SymVar {
        name: "y".to_string(),
        version: 0,
    });
    let val = SymValue::Mul(Box::new(x), Box::new(y));
    let poly = normalize_polynomial(&val).unwrap();
    assert_eq!(poly, vec![(1, vec!["x".to_string(), "y".to_string()])]);
}

#[test]
fn test_polynomial_commutativity() {
    // x + y vs y + x should produce the same polynomial
    let x = SymValue::Var(crate::sym::SymVar {
        name: "x".to_string(),
        version: 0,
    });
    let y = SymValue::Var(crate::sym::SymVar {
        name: "y".to_string(),
        version: 0,
    });
    let sum1 = SymValue::Add(Box::new(x.clone()), Box::new(y.clone()));
    let sum2 = SymValue::Add(Box::new(y), Box::new(x));
    let poly1 = normalize_polynomial(&sum1).unwrap();
    let poly2 = normalize_polynomial(&sum2).unwrap();
    assert_eq!(poly1, poly2);
}

#[test]
fn test_polynomial_normalization_complex() {
    // (x + y) * x  =  x*x + x*y
    let x = SymValue::Var(crate::sym::SymVar {
        name: "x".to_string(),
        version: 0,
    });
    let y = SymValue::Var(crate::sym::SymVar {
        name: "y".to_string(),
        version: 0,
    });
    let sum = SymValue::Add(Box::new(x.clone()), Box::new(y.clone()));
    let prod = SymValue::Mul(Box::new(sum), Box::new(x));
    let poly = normalize_polynomial(&prod).unwrap();
    assert_eq!(
        poly,
        vec![
            (1, vec!["x".to_string(), "x".to_string()]),
            (1, vec!["x".to_string(), "y".to_string()]),
        ]
    );
}

// --- Equivalent with rearranged arithmetic ---

#[test]
fn test_equivalent_rearranged() {
    let source = r#"program test

fn f(x: Field, y: Field) -> Field {
x + y + x
}

fn g(x: Field, y: Field) -> Field {
x * 2 + y
}

fn main() { }
"#;
    let file = parse(source);
    let result = check_equivalence(&file, "f", "g");
    // Both reduce to 2*x + y, so polynomial normalization catches it.
    assert_eq!(
        result.verdict,
        EquivalenceVerdict::Equivalent,
        "report: {}",
        result.format_report()
    );
}

// --- Differential testing catches non-equivalence ---

#[test]
fn test_differential_not_equivalent() {
    let source = r#"program test

fn f(x: Field) -> Field {
x + 1
}

fn g(x: Field) -> Field {
x + 2
}

fn main() { }
"#;
    let file = parse(source);
    let result = check_equivalence(&file, "f", "g");
    assert_eq!(result.verdict, EquivalenceVerdict::NotEquivalent);
}

// --- Void functions ---

#[test]
fn test_void_functions_equivalent() {
    let source = r#"program test

fn f(x: Field) {
assert(true)
}

fn g(x: Field) {
assert(true)
}

fn main() { }
"#;
    let file = parse(source);
    let result = check_equivalence(&file, "f", "g");
    // Void functions with same assertions should be equivalent.
    assert_eq!(result.verdict, EquivalenceVerdict::Equivalent);
}
