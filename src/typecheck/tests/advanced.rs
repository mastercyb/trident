//! TypeChecker unit tests.

use super::{check, check_err};
#[test]
fn test_match_integer_pattern_on_bool_error() {
    let result = check("program test\nfn main() {\n    let b: Bool = pub_read() == pub_read()\n    match b {\n        0 => { pub_write(0) }\n        _ => { pub_write(1) }\n    }\n}");
    assert!(
        result.is_err(),
        "integer pattern on Bool scrutinee should fail"
    );
}

#[test]
fn test_match_unreachable_after_wildcard() {
    let result = check("program test\nfn main() {\n    let x: Field = pub_read()\n    match x {\n        _ => { pub_write(0) }\n        0 => { pub_write(1) }\n    }\n}");
    assert!(
        result.is_err(),
        "pattern after wildcard should be unreachable"
    );
}

#[test]
fn test_match_struct_pattern_valid() {
    let result = check(
        "program test\nstruct Point { x: Field, y: Field }\nfn main() {\n    let p = Point { x: 1, y: 2 }\n    match p {\n        Point { x, y } => { pub_write(x) }\n    }\n}",
    );
    assert!(
        result.is_ok(),
        "struct pattern match should pass: {:?}",
        result.err()
    );
}

#[test]
fn test_match_struct_pattern_wrong_type() {
    let result = check(
        "program test\nstruct Point { x: Field, y: Field }\nstruct Pair { a: Field, b: Field }\nfn main() {\n    let p = Point { x: 1, y: 2 }\n    match p {\n        Pair { a, b } => { pub_write(a) }\n    }\n}",
    );
    assert!(
        result.is_err(),
        "struct pattern with wrong type should fail"
    );
}

#[test]
fn test_match_struct_pattern_unknown_field() {
    let result = check(
        "program test\nstruct Point { x: Field, y: Field }\nfn main() {\n    let p = Point { x: 1, y: 2 }\n    match p {\n        Point { x, z } => { pub_write(x) }\n    }\n}",
    );
    assert!(
        result.is_err(),
        "struct pattern with unknown field should fail"
    );
}

#[test]
fn test_match_struct_pattern_unknown_struct() {
    let result = check(
        "program test\nfn main() {\n    let x: Field = pub_read()\n    match x {\n        Foo { a } => { pub_write(a) }\n    }\n}",
    );
    assert!(
        result.is_err(),
        "struct pattern with unknown struct should fail"
    );
}

#[test]
fn test_match_struct_pattern_with_literal_field() {
    let result = check(
        "program test\nstruct Pair { a: Field, b: Field }\nfn main() {\n    let p = Pair { a: 1, b: 2 }\n    match p {\n        Pair { a: 0, b } => { pub_write(b) }\n        _ => { pub_write(0) }\n    }\n}",
    );
    assert!(
        result.is_ok(),
        "struct pattern with literal field should pass: {:?}",
        result.err()
    );
}

// --- #[test] function validation ---

#[test]
fn test_test_fn_valid() {
    let result =
        check("program test\n#[test]\nfn check_math() {\n    assert(1 == 1)\n}\nfn main() {}");
    assert!(
        result.is_ok(),
        "valid test fn should pass: {:?}",
        result.err()
    );
}

#[test]
fn test_test_fn_with_params_rejected() {
    let result = check(
        "program test\n#[test]\nfn bad_test(x: Field) {\n    assert(x == x)\n}\nfn main() {}",
    );
    assert!(result.is_err(), "test fn with params should fail");
}

#[test]
fn test_test_fn_with_return_rejected() {
    let result = check("program test\n#[test]\nfn bad_test() -> Field {\n    42\n}\nfn main() {}");
    assert!(result.is_err(), "test fn with return type should fail");
}

#[test]
fn test_test_fn_not_emitted_in_normal_build() {
    // Test functions should type-check but not interfere with normal compilation
    let result = check("program test\n#[test]\nfn check() {\n    assert(true)\n}\nfn main() {\n    pub_write(pub_read())\n}");
    assert!(result.is_ok());
}

// --- Error path tests: message quality ---

#[test]
fn test_error_binary_op_type_mismatch() {
    let diags = check_err(
        "program test\nfn main() {\n    let a: Field = pub_read()\n    let b: Bool = a == a\n    let c: Field = a + b\n}",
    );
    assert!(!diags.is_empty(), "should error on Field + Bool");
    let msg = &diags[0].message;
    assert!(
        msg.contains("Field") && msg.contains("Bool"),
        "should show both types in mismatch, got: {}",
        msg
    );
}

#[test]
fn test_error_function_arity_mismatch() {
    let diags = check_err(
        "program test\nfn add(a: Field, b: Field) -> Field {\n    a + b\n}\nfn main() {\n    let x: Field = add(1)\n}",
    );
    assert!(!diags.is_empty(), "should error on wrong argument count");
    let msg = &diags[0].message;
    assert!(
        msg.contains("expects 2 arguments") && msg.contains("got 1"),
        "should show expected and actual arity, got: {}",
        msg
    );
}

#[test]
fn test_error_assign_to_immutable() {
    let diags =
        check_err("program test\nfn main() {\n    let x: Field = pub_read()\n    x = 42\n}");
    assert!(!diags.is_empty(), "should error on assigning to immutable");
    let msg = &diags[0].message;
    assert!(
        msg.contains("immutable"),
        "should mention immutability, got: {}",
        msg
    );
    assert!(
        diags[0].help.as_deref().unwrap().contains("let mut"),
        "help should suggest `let mut`"
    );
}

#[test]
fn test_error_return_type_mismatch() {
    // pub_read() returns Field, but let binding declares U32 -- a type mismatch
    let diags = check_err("program test\nfn main() {\n    let x: U32 = pub_read()\n}");
    assert!(!diags.is_empty(), "should error on Field assigned to U32");
    let msg = &diags[0].message;
    assert!(
        msg.contains("U32") && msg.contains("Field"),
        "should show both expected and actual types, got: {}",
        msg
    );
}

#[test]
fn test_error_undefined_event() {
    let diags = check_err("program test\nfn main() {\n    reveal NoSuchEvent { x: 1 }\n}");
    assert!(!diags.is_empty(), "should error on undefined event");
    assert!(
        diags[0].message.contains("undefined event 'NoSuchEvent'"),
        "should name the undefined event, got: {}",
        diags[0].message
    );
}

#[test]
fn test_error_struct_unknown_field() {
    let diags = check_err(
        "program test\nstruct Point { x: Field, y: Field }\nfn main() {\n    let p: Point = Point { x: 1, y: 2, z: 3 }\n}",
    );
    assert!(!diags.is_empty(), "should error on unknown struct field");
    let has_unknown = diags
        .iter()
        .any(|d| d.message.contains("unknown field 'z'"));
    assert!(
        has_unknown,
        "should report unknown field 'z', got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn test_error_recursion_has_help() {
    let diags =
        check_err("program test\nfn loop_forever() {\n    loop_forever()\n}\nfn main() {\n}");
    assert!(!diags.is_empty(), "should detect recursion");
    assert!(
        diags[0].message.contains("recursive call cycle"),
        "should report cycle, got: {}",
        diags[0].message
    );
    assert!(
        diags[0].help.is_some(),
        "recursion error should have help text explaining alternative"
    );
}

#[test]
fn test_error_non_exhaustive_match_has_help() {
    let diags = check_err(
        "program test\nfn main() {\n    let x: Field = pub_read()\n    match x {\n        0 => { pub_write(0) }\n    }\n}",
    );
    assert!(!diags.is_empty(), "should detect non-exhaustive match");
    assert!(
        diags[0].message.contains("non-exhaustive"),
        "should report non-exhaustive match, got: {}",
        diags[0].message
    );
    assert!(
        diags[0].help.as_deref().unwrap().contains("_ =>"),
        "help should suggest wildcard arm"
    );
}

#[test]
fn test_error_unreachable_code_has_help() {
    let diags = check_err(
        "program test\nfn foo() -> Field {\n    return 1\n    pub_write(2)\n}\nfn main() {\n}",
    );
    assert!(!diags.is_empty(), "should detect unreachable code");
    let unreachable_diag = diags.iter().find(|d| d.message.contains("unreachable"));
    assert!(
        unreachable_diag.is_some(),
        "should report unreachable code, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    assert!(
        unreachable_diag.unwrap().help.is_some(),
        "unreachable code error should have help text"
    );
}

#[test]
fn test_error_undefined_variable_has_help() {
    let diags = check_err("program test\nfn main() {\n    pub_write(xyz)\n}");
    assert!(!diags.is_empty(), "should error on undefined variable");
    assert!(
        diags[0].message.contains("undefined variable 'xyz'"),
        "should name the variable, got: {}",
        diags[0].message
    );
    assert!(
        diags[0].help.is_some(),
        "undefined variable error should have help text"
    );
}

#[test]
fn test_error_undefined_function_has_help() {
    let diags = check_err("program test\nfn main() {\n    let x: Field = no_such_fn()\n}");
    assert!(!diags.is_empty(), "should error on undefined function");
    assert!(
        diags[0].message.contains("undefined function 'no_such_fn'"),
        "should name the function, got: {}",
        diags[0].message
    );
    assert!(
        diags[0].help.is_some(),
        "undefined function error should have help text"
    );
}

#[test]
fn test_error_loop_bound_has_help() {
    let diags = check_err(
        "program test\nfn main() {\n    let n: Field = pub_read()\n    for i in 0..n {\n        pub_write(0)\n    }\n}",
    );
    assert!(!diags.is_empty(), "should error on non-constant loop bound");
    let msg = &diags[0].message;
    assert!(
        msg.contains("compile-time constant") || msg.contains("bound"),
        "should explain the loop bound requirement, got: {}",
        msg
    );
    assert!(
        diags[0].help.as_deref().unwrap().contains("bounded"),
        "help should suggest `bounded` keyword"
    );
}

#[test]
fn test_error_lt_requires_u32() {
    let diags = check_err(
        "program test\nfn main() {\n    let a: Field = pub_read()\n    let b: Field = pub_read()\n    assert(a < b)\n}",
    );
    assert!(!diags.is_empty(), "should error on Field < Field");
    let msg = &diags[0].message;
    assert!(
        msg.contains("U32") && msg.contains("Field"),
        "should show required U32 and actual Field types, got: {}",
        msg
    );
}

#[test]
fn test_error_field_access_on_non_struct() {
    let diags = check_err(
        "program test\nfn main() {\n    let x: Field = pub_read()\n    pub_write(x.y)\n}",
    );
    assert!(
        !diags.is_empty(),
        "should error on field access of non-struct"
    );
    // The parser treats `x.y` as a dotted variable, so the error is
    // "undefined variable 'x.y'" since x is Field, not a struct with field y
    let has_error = diags
        .iter()
        .any(|d| d.message.contains("undefined variable") || d.message.contains("field"));
    assert!(
        has_error,
        "should report variable/field error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn test_error_messages_have_spans() {
    // All type checker errors should have non-dummy spans
    let diags = check_err("program test\nfn main() {\n    pub_write(undefined_var)\n}");
    assert!(!diags.is_empty());
    for d in &diags {
        assert!(
            d.span.start != d.span.end || d.span.start > 0,
            "error '{}' should have a meaningful span, got: {:?}",
            d.message,
            d.span
        );
    }
}

// --- #[pure] annotation tests ---

#[test]
fn test_pure_fn_no_io_compiles() {
    let result = check(
        "program test\n#[pure]\nfn add(a: Field, b: Field) -> Field {\n    a + b\n}\nfn main() {}",
    );
    assert!(
        result.is_ok(),
        "pure fn without I/O should pass: {:?}",
        result.err()
    );
}

#[test]
fn test_pure_fn_rejects_pub_read() {
    let diags =
        check_err("program test\n#[pure]\nfn f() -> Field {\n    pub_read()\n}\nfn main() {}");
    assert!(diags
        .iter()
        .any(|d| d.message.contains("#[pure]") && d.message.contains("pub_read")));
}

#[test]
fn test_pure_fn_rejects_pub_write() {
    let diags =
        check_err("program test\n#[pure]\nfn f(x: Field) {\n    pub_write(x)\n}\nfn main() {}");
    assert!(diags
        .iter()
        .any(|d| d.message.contains("#[pure]") && d.message.contains("pub_write")));
}

#[test]
fn test_pure_fn_rejects_divine() {
    let diags =
        check_err("program test\n#[pure]\nfn f() -> Field {\n    divine()\n}\nfn main() {}");
    assert!(diags
        .iter()
        .any(|d| d.message.contains("#[pure]") && d.message.contains("divine")));
}

#[test]
fn test_pure_fn_allows_assert() {
    // assert is not I/O — it's a control flow operation
    let result =
        check("program test\n#[pure]\nfn f(x: Field) {\n    assert(x == 0)\n}\nfn main() {}");
    assert!(
        result.is_ok(),
        "assert should be allowed in pure fn: {:?}",
        result.err()
    );
}

#[test]
fn test_pure_fn_allows_hash() {
    // hash is a deterministic pure computation (same inputs → same outputs)
    let result = check("program test\n#[pure]\nfn f(a: Field, b: Field, c: Field, d: Field, e: Field, f2: Field, g: Field, h: Field, i: Field, j: Field) -> Digest {\n    hash(a, b, c, d, e, f2, g, h, i, j)\n}\nfn main() {}");
    assert!(
        result.is_ok(),
        "hash should be allowed in pure fn: {:?}",
        result.err()
    );
}

#[test]
fn test_pure_fn_rejects_sponge_init() {
    let diags = check_err("program test\n#[pure]\nfn f() {\n    sponge_init()\n}\nfn main() {}");
    assert!(diags
        .iter()
        .any(|d| d.message.contains("#[pure]") && d.message.contains("sponge_init")));
}
