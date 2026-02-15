//! TypeChecker unit tests.

use std::collections::HashSet;

use crate::diagnostic::Diagnostic;
use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::typecheck::{ModuleExports, TypeChecker};

use super::check;

#[test]
fn test_valid_field_arithmetic() {
    let result = check("program test\nfn main() {\n    let a: Field = pub_read()\n    let b: Field = pub_read()\n    let c: Field = a + b\n    pub_write(c)\n}");
    assert!(result.is_ok());
}

#[test]
fn test_type_mismatch() {
    let result = check("program test\nfn main() {\n    let a: U32 = pub_read()\n}");
    assert!(result.is_err());
}

#[test]
fn test_undefined_variable() {
    let result = check("program test\nfn main() {\n    pub_write(x)\n}");
    assert!(result.is_err());
}

#[test]
fn test_assert_with_eq() {
    let result = check("program test\nfn main() {\n    let a: Field = pub_read()\n    let b: Field = divine()\n    assert(a == b)\n}");
    assert!(result.is_ok());
}

#[test]
fn test_function_call() {
    let result = check("program test\nfn add(a: Field, b: Field) -> Field {\n    a + b\n}\nfn main() {\n    let x: Field = pub_read()\n    let y: Field = pub_read()\n    let z: Field = add(x, y)\n}");
    assert!(result.is_ok());
}

#[test]
fn test_struct_init_and_field_access() {
    let result = check("program test\nstruct Point {\n    x: Field,\n    y: Field,\n}\nfn main() {\n    let a: Field = pub_read()\n    let b: Field = pub_read()\n    let p: Point = Point { x: a, y: b }\n    pub_write(p.x)\n}");
    assert!(result.is_ok());
}

#[test]
fn test_struct_missing_field() {
    let result = check("program test\nstruct Point {\n    x: Field,\n    y: Field,\n}\nfn main() {\n    let p: Point = Point { x: pub_read() }\n}");
    assert!(result.is_err());
}

#[test]
fn test_array_init_and_index() {
    let result = check("program test\nfn main() {\n    let arr: [Field; 3] = [pub_read(), pub_read(), pub_read()]\n    pub_write(arr[0])\n}");
    assert!(result.is_ok());
}

#[test]
fn test_tuple_destructuring() {
    let result = check("program test\nfn pair() -> (Field, Field) {\n    (pub_read(), pub_read())\n}\nfn main() {\n    let (a, b): (Field, Field) = pair()\n    pub_write(a)\n    pub_write(b)\n}");
    assert!(result.is_ok());
}

#[test]
fn test_tuple_destructure_arity_mismatch() {
    let result = check("program test\nfn main() {\n    let (a, b, c): (Field, Field) = (pub_read(), pub_read())\n}");
    assert!(result.is_err());
}

#[test]
fn test_reveal_valid() {
    let result = check("program test\nevent Transfer { from: Field, to: Field, amount: Field }\nfn main() {\n    reveal Transfer { from: pub_read(), to: pub_read(), amount: pub_read() }\n}");
    assert!(result.is_ok());
}

#[test]
fn test_seal_valid() {
    let result = check("program test\nevent Nullifier { id: Field, nonce: Field }\nfn main() {\n    seal Nullifier { id: pub_read(), nonce: pub_read() }\n}");
    assert!(result.is_ok());
}

#[test]
fn test_reveal_undefined_event() {
    let result = check("program test\nfn main() {\n    reveal Missing { x: pub_read() }\n}");
    assert!(result.is_err());
}

#[test]
fn test_reveal_missing_field() {
    let result = check("program test\nevent Ev { x: Field, y: Field }\nfn main() {\n    reveal Ev { x: pub_read() }\n}");
    assert!(result.is_err());
}

#[test]
fn test_reveal_extra_field() {
    let result = check("program test\nevent Ev { x: Field }\nfn main() {\n    reveal Ev { x: pub_read(), y: pub_read() }\n}");
    assert!(result.is_err());
}

#[test]
fn test_event_max_9_fields() {
    let result = check("program test\nevent Big { f0: Field, f1: Field, f2: Field, f3: Field, f4: Field, f5: Field, f6: Field, f7: Field, f8: Field, f9: Field }\nfn main() {\n}");
    assert!(result.is_err()); // 10 fields > max 9
}

#[test]
fn test_digest_destructuring() {
    let result = check("program test\nfn main() {\n    let d: Digest = divine5()\n    let (f0, f1, f2, f3, f4) = d\n    pub_write(f0)\n    pub_write(f4)\n}");
    assert!(result.is_ok());
}

#[test]
fn test_digest_destructuring_wrong_arity() {
    let result =
        check("program test\nfn main() {\n    let d: Digest = divine5()\n    let (a, b, c) = d\n}");
    assert!(result.is_err());
}

#[test]
fn test_digest_destructuring_inline() {
    // Destructure directly from hash() call
    let result = check("program test\nfn main() {\n    let (f0, f1, f2, f3, f4) = hash(0, 0, 0, 0, 0, 0, 0, 0, 0, 0)\n    pub_write(f0)\n}");
    assert!(result.is_ok());
}

#[test]
fn test_intrinsic_rejected_outside_std() {
    let result = check("program test\n#[intrinsic(hash)] fn foo() -> Digest {\n}\nfn main() {\n}");
    assert!(result.is_err());
}

#[test]
fn test_intrinsic_allowed_in_std_module() {
    let result = check("module std.test\n#[intrinsic(hash)] pub fn foo(x0: Field, x1: Field, x2: Field, x3: Field, x4: Field, x5: Field, x6: Field, x7: Field, x8: Field, x9: Field) -> Digest\n");
    assert!(result.is_ok());
}

#[test]
fn test_direct_recursion_rejected() {
    let result = check("program test\nfn loop_forever() {\n    loop_forever()\n}\nfn main() {\n}");
    assert!(result.is_err());
}

#[test]
fn test_mutual_recursion_rejected() {
    let result = check("program test\nfn a() {\n    b()\n}\nfn b() {\n    a()\n}\nfn main() {\n}");
    assert!(result.is_err());
}

#[test]
fn test_no_false_positive_recursion() {
    // a calls b, b calls c — no cycle
    let result = check("program test\nfn c() {\n    pub_write(1)\n}\nfn b() {\n    c()\n}\nfn a() {\n    b()\n}\nfn main() {\n    a()\n}");
    assert!(result.is_ok());
}

#[test]
fn test_dead_code_after_return() {
    let result = check(
        "program test\nfn foo() -> Field {\n    return 1\n    pub_write(2)\n}\nfn main() {\n}",
    );
    assert!(result.is_err());
}

#[test]
fn test_dead_code_after_assert_false() {
    let result =
        check("program test\nfn foo() {\n    assert(false)\n    pub_write(1)\n}\nfn main() {\n}");
    assert!(result.is_err());
}

#[test]
fn test_no_false_positive_dead_code() {
    let result = check("program test\nfn foo() -> Field {\n    let x: Field = pub_read()\n    pub_write(x)\n    x\n}\nfn main() {\n}");
    assert!(result.is_ok());
}

#[test]
fn test_unused_import_warning() {
    // Unused import should produce a warning but still succeed (it's not an error)
    let result = check("module test_mod\nuse std.hash\npub fn foo() -> Field {\n    42\n}");
    // Should succeed (warnings don't fail compilation)
    assert!(result.is_ok());
    // But should contain a warning
    let exports = result.unwrap();
    assert!(
        !exports.warnings.is_empty(),
        "expected unused import warning"
    );
}

#[test]
fn test_used_import_no_warning() {
    // We can't test cross-module calls in unit tests (no import_module),
    // but we can verify the module prefix collection works by checking
    // that a module with no imports produces no warnings.
    let result = check("module test_mod\npub fn foo() -> Field {\n    42\n}");
    assert!(result.is_ok());
    let exports = result.unwrap();
    assert!(
        exports.warnings.is_empty(),
        "no warning expected for module with no imports, got: {:?}",
        exports.warnings
    );
}

#[test]
fn test_h0003_redundant_as_u32() {
    // First as_u32(a) proves a is in U32 range.
    // Second as_u32(a) is redundant — should warn.
    let result = check(
        "program test\nfn main() {\n    let a: Field = pub_read()\n    let b: U32 = as_u32(a)\n    let c: U32 = as_u32(a)\n}",
    );
    assert!(result.is_ok());
    let exports = result.unwrap();
    let h0003 = exports.warnings.iter().any(|w| w.message.contains("H0003"));
    assert!(
        h0003,
        "expected H0003 warning for redundant as_u32, got: {:?}",
        exports.warnings
    );
}

#[test]
fn test_h0003_no_false_positive() {
    // as_u32 on a fresh Field should NOT warn
    let result = check(
        "program test\nfn main() {\n    let a: Field = pub_read()\n    let b: U32 = as_u32(a)\n}",
    );
    assert!(result.is_ok());
    let exports = result.unwrap();
    let h0003 = exports.warnings.iter().any(|w| w.message.contains("H0003"));
    assert!(!h0003, "should not warn on first as_u32 call");
}

#[test]
fn test_asm_block_type_checks() {
    // asm blocks should pass type checking without errors
    let result = check(
        "program test\nfn main() {\n    let x: Field = pub_read()\n    asm { dup 0\nadd }\n    pub_write(x)\n}",
    );
    assert!(result.is_ok(), "asm block should not cause type errors");
}

#[test]
fn test_asm_block_with_effect() {
    let result =
        check("program test\nfn main() {\n    asm(+1) { push 42 }\n    asm(-1) { pop 1 }\n}");
    assert!(result.is_ok(), "asm with effect should type check");
}

// --- Size-generic function tests ---

#[test]
fn test_generic_fn_explicit_size_arg() {
    let result = check(
        "program test\nfn sum<N>(arr: [Field; N]) -> Field {\n    arr[0]\n}\nfn main() {\n    let a: [Field; 3] = [1, 2, 3]\n    let s: Field = sum<3>(a)\n    pub_write(s)\n}",
    );
    assert!(
        result.is_ok(),
        "explicit size arg should type check: {:?}",
        result.err()
    );
}

#[test]
fn test_generic_fn_inferred_size() {
    let result = check(
        "program test\nfn first<N>(arr: [Field; N]) -> Field {\n    arr[0]\n}\nfn main() {\n    let a: [Field; 3] = [1, 2, 3]\n    let f: Field = first(a)\n    pub_write(f)\n}",
    );
    assert!(
        result.is_ok(),
        "inferred size arg should type check: {:?}",
        result.err()
    );
}

#[test]
fn test_generic_fn_wrong_size_arg() {
    // Call sum<2> with a [Field; 3] — should fail type check
    let result = check(
        "program test\nfn sum<N>(arr: [Field; N]) -> Field {\n    arr[0]\n}\nfn main() {\n    let a: [Field; 3] = [1, 2, 3]\n    let s: Field = sum<2>(a)\n}",
    );
    assert!(
        result.is_err(),
        "mismatched size arg should fail type check"
    );
}

#[test]
fn test_generic_fn_wrong_param_count() {
    // Function has 1 size param but call provides 2
    let result = check(
        "program test\nfn sum<N>(arr: [Field; N]) -> Field {\n    arr[0]\n}\nfn main() {\n    let a: [Field; 3] = [1, 2, 3]\n    let s: Field = sum<3, 5>(a)\n}",
    );
    assert!(result.is_err(), "wrong number of size params should fail");
}

#[test]
fn test_generic_fn_records_mono_instance() {
    let result = check(
        "program test\nfn id<N>(arr: [Field; N]) -> [Field; N] {\n    arr\n}\nfn main() {\n    let a: [Field; 3] = [1, 2, 3]\n    let b: [Field; 3] = id<3>(a)\n}",
    );
    assert!(result.is_ok());
    let exports = result.unwrap();
    assert_eq!(exports.mono_instances.len(), 1);
    assert_eq!(exports.mono_instances[0].name, "id");
    assert_eq!(exports.mono_instances[0].size_args, vec![3]);
}

#[test]
fn test_generic_fn_multiple_instantiations() {
    let result = check(
        "program test\nfn first<N>(arr: [Field; N]) -> Field {\n    arr[0]\n}\nfn main() {\n    let a: [Field; 3] = [1, 2, 3]\n    let b: [Field; 5] = [1, 2, 3, 4, 5]\n    let x: Field = first<3>(a)\n    let y: Field = first<5>(b)\n    pub_write(x + y)\n}",
    );
    assert!(result.is_ok());
    let exports = result.unwrap();
    assert_eq!(
        exports.mono_instances.len(),
        2,
        "should have 2 distinct instantiations"
    );
}

#[test]
fn test_generic_fn_non_generic_with_size_args_fails() {
    // Calling a non-generic function with size args should error
    let result = check(
        "program test\nfn add(a: Field, b: Field) -> Field {\n    a + b\n}\nfn main() {\n    let x: Field = add<3>(1, 2)\n}",
    );
    assert!(
        result.is_err(),
        "non-generic fn called with size args should fail"
    );
}

// --- conditional compilation ---

fn check_with_flags(source: &str, flags: &[&str]) -> Result<ModuleExports, Vec<Diagnostic>> {
    let (tokens, _, _) = Lexer::new(source, 0).tokenize();
    let file = Parser::new(tokens).parse_file().unwrap();
    let flag_set: HashSet<String> = flags.iter().map(|s| s.to_string()).collect();
    TypeChecker::new()
        .with_cfg_flags(flag_set)
        .check_file(&file)
}

#[test]
fn test_cfg_debug_includes_debug_fn() {
    let result = check_with_flags(
        "program test\n#[cfg(debug)]\nfn check() {}\nfn main() {\n    check()\n}",
        &["debug"],
    );
    assert!(result.is_ok(), "debug fn should be available in debug mode");
}

#[test]
fn test_cfg_release_excludes_debug_fn() {
    let result = check_with_flags(
        "program test\n#[cfg(debug)]\nfn check() {}\nfn main() {\n    check()\n}",
        &["release"],
    );
    assert!(
        result.is_err(),
        "debug fn should not be available in release mode"
    );
}

#[test]
fn test_cfg_no_attr_always_available() {
    let result = check_with_flags(
        "program test\nfn helper() {}\nfn main() {\n    helper()\n}",
        &["release"],
    );
    assert!(result.is_ok(), "uncfg'd fn always available");
}

#[test]
fn test_cfg_duplicate_names_different_cfg() {
    // Two functions with same name but different cfg — only one active
    let result = check_with_flags(
        "program test\n#[cfg(debug)]\nfn mode() -> Field { 0 }\n#[cfg(release)]\nfn mode() -> Field { 1 }\nfn main() {\n    let x: Field = mode()\n}",
        &["debug"],
    );
    assert!(result.is_ok(), "should pick the debug variant");
}

#[test]
fn test_cfg_const_excluded() {
    let result = check_with_flags(
        "program test\n#[cfg(debug)]\nconst X: Field = 42\nfn main() {\n    let a: Field = X\n}",
        &["release"],
    );
    // X is cfg'd out, so it should be unknown
    assert!(result.is_err(), "const should be excluded in release");
}

#[test]
fn test_cfg_export_filtered() {
    let exports = check_with_flags(
        "module test\n#[cfg(debug)]\npub fn dbg_only() {}\npub fn always() {}",
        &["release"],
    )
    .unwrap();
    assert_eq!(exports.functions.len(), 1, "only always() exported");
    assert_eq!(exports.functions[0].0, "always");
}

// --- match statement type checking ---

#[test]
fn test_match_field_with_integers() {
    let result = check("program test\nfn main() {\n    let x: Field = pub_read()\n    match x {\n        0 => { pub_write(0) }\n        1 => { pub_write(1) }\n        _ => { pub_write(2) }\n    }\n}");
    assert!(result.is_ok(), "match on Field with integers should pass");
}

#[test]
fn test_match_bool_exhaustive() {
    let result = check("program test\nfn main() {\n    let b: Bool = pub_read() == pub_read()\n    match b {\n        true => { pub_write(1) }\n        false => { pub_write(0) }\n    }\n}");
    assert!(
        result.is_ok(),
        "match on Bool with true+false is exhaustive"
    );
}

#[test]
fn test_match_non_exhaustive_error() {
    let result = check("program test\nfn main() {\n    let x: Field = pub_read()\n    match x {\n        0 => { pub_write(0) }\n        1 => { pub_write(1) }\n    }\n}");
    assert!(
        result.is_err(),
        "match without wildcard on Field should fail"
    );
}

#[test]
fn test_match_bool_pattern_on_field_error() {
    let result = check("program test\nfn main() {\n    let x: Field = pub_read()\n    match x {\n        true => { pub_write(1) }\n        _ => { pub_write(0) }\n    }\n}");
    assert!(
        result.is_err(),
        "boolean pattern on Field scrutinee should fail"
    );
}
