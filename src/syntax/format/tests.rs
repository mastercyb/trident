use crate::lexer::Lexer;
use crate::parser::Parser;

use super::expr::format_type;
use super::*;

/// Helper: parse source and format it back.
fn fmt(source: &str) -> String {
    let (tokens, comments, lex_errors) = Lexer::new(source, 0).tokenize();
    assert!(lex_errors.is_empty(), "lex errors: {:?}", lex_errors);
    let file = Parser::new(tokens).parse_file().unwrap();
    format_file(&file, &comments)
}

// --- Basic formatting ---

#[test]
fn test_minimal_program() {
    let src = "program test\n\nfn main() {\n    pub_write(pub_read())\n}\n";
    assert_eq!(fmt(src), src);
}

#[test]
fn test_module_header() {
    let src = "module math\n\npub fn add(a: Field, b: Field) -> Field {\n    a + b\n}\n";
    assert_eq!(fmt(src), src);
}

#[test]
fn test_normalizes_whitespace() {
    let input = "program   test\n\n\n\nfn main() {\n    pub_write(pub_read())\n}\n";
    let output = fmt(input);
    assert!(output.starts_with("program test\n"));
    assert!(!output.contains("\n\n\n"));
}

#[test]
fn test_const_formatting() {
    let src =
        "program test\n\npub const MAX: U32 = 100\n\nfn main() {\n    pub_write(pub_read())\n}\n";
    assert_eq!(fmt(src), src);
}

#[test]
fn test_struct_formatting() {
    let src = "program test\n\nstruct Point {\n    x: Field,\n    y: Field,\n}\n\nfn main() {\n    pub_write(pub_read())\n}\n";
    assert_eq!(fmt(src), src);
}

#[test]
fn test_pub_struct_formatting() {
    let src = "program test\n\npub struct Config {\n    pub owner: Digest,\n    value: Field,\n}\n\nfn main() {\n    pub_write(pub_read())\n}\n";
    assert_eq!(fmt(src), src);
}

#[test]
fn test_event_formatting() {
    let src = "program test\n\nevent Transfer {\n    from: Field,\n    to: Field,\n}\n\nfn main() {\n    pub_write(pub_read())\n}\n";
    assert_eq!(fmt(src), src);
}

// --- Statements ---

#[test]
fn test_let_binding() {
    let src = "program test\n\nfn main() {\n    let x: Field = pub_read()\n    pub_write(x)\n}\n";
    assert_eq!(fmt(src), src);
}

#[test]
fn test_let_mut() {
    let src = "program test\n\nfn main() {\n    let mut x: Field = pub_read()\n    x = x + 1\n    pub_write(x)\n}\n";
    assert_eq!(fmt(src), src);
}

#[test]
fn test_tuple_destructure() {
    let src = "program test\n\nfn main() {\n    let (a, b) = split(pub_read())\n    pub_write(as_field(a))\n}\n";
    assert_eq!(fmt(src), src);
}

#[test]
fn test_if_else() {
    let src = "program test\n\nfn main() {\n    let x: Field = pub_read()\n    if x == 0 {\n        pub_write(0)\n    } else {\n        pub_write(1)\n    }\n}\n";
    assert_eq!(fmt(src), src);
}

#[test]
fn test_for_loop() {
    let src = "program test\n\nfn main() {\n    let mut s: Field = 0\n    for i in 0..10 bounded 10 {\n        s = s + 1\n    }\n    pub_write(s)\n}\n";
    assert_eq!(fmt(src), src);
}

#[test]
fn test_return_statement() {
    let src = "program test\n\nfn helper(x: Field) -> Field {\n    return x + 1\n}\n\nfn main() {\n    pub_write(helper(pub_read()))\n}\n";
    assert_eq!(fmt(src), src);
}

#[test]
fn test_reveal_statement() {
    let src = "program test\n\nevent Log {\n    value: Field,\n}\n\nfn main() {\n    reveal Log { value: pub_read() }\n}\n";
    assert_eq!(fmt(src), src);
}

#[test]
fn test_seal_statement() {
    let src = "program test\n\nevent Commit {\n    value: Field,\n}\n\nfn main() {\n    seal Commit { value: pub_read() }\n}\n";
    assert_eq!(fmt(src), src);
}

// --- Expressions ---

#[test]
fn test_binary_precedence() {
    let src = "program test\n\nfn main() {\n    let x: Field = 1 + 2 * 3\n    pub_write(x)\n}\n";
    assert_eq!(fmt(src), src);
}

#[test]
fn test_array_init() {
    let src =
        "program test\n\nfn main() {\n    let a: [Field; 3] = [1, 2, 3]\n    pub_write(a[0])\n}\n";
    assert_eq!(fmt(src), src);
}

#[test]
fn test_struct_init_expr() {
    let src = "program test\n\nstruct Pt {\n    x: Field,\n    y: Field,\n}\n\nfn main() {\n    let p: Pt = Pt { x: 1, y: 2 }\n    pub_write(p.x)\n}\n";
    assert_eq!(fmt(src), src);
}

#[test]
fn test_field_access() {
    let src = "program test\n\nstruct Pt {\n    x: Field,\n    y: Field,\n}\n\nfn main() {\n    let p: Pt = Pt { x: 1, y: 2 }\n    pub_write(p.x + p.y)\n}\n";
    assert_eq!(fmt(src), src);
}

// --- Comments ---

#[test]
fn test_comment_preservation() {
    let src = "program test\n\n// Main entry point\nfn main() {\n    // Read input\n    let x: Field = pub_read()\n    pub_write(x)\n}\n";
    let out = fmt(src);
    assert!(
        out.contains("// Main entry point"),
        "leading comment preserved"
    );
    assert!(out.contains("// Read input"), "inline comment preserved");
}

#[test]
fn test_trailing_comment() {
    let src = "program test\n\nfn main() {\n    let x: Field = pub_read() // read value\n    pub_write(x)\n}\n";
    let out = fmt(src);
    assert!(out.contains("// read value"), "trailing comment preserved");
}

// --- Idempotency ---

#[test]
fn test_idempotent_simple() {
    let src = "program test\n\nfn main() {\n    let x: Field = pub_read()\n    let y: Field = x + 1\n    pub_write(y)\n}\n";
    let first = fmt(src);
    let second = fmt(&first);
    assert_eq!(first, second, "formatting should be idempotent");
}

#[test]
fn test_idempotent_complex() {
    let src = r#"program token

use std.hash
use std.assert

struct Config {
    owner: Digest,
    supply: Field,
}

event Transfer {
    from: Field,
    to: Field,
    amount: Field,
}

const MAX_SUPPLY: Field = 1000000

// Main function
fn main() {
    let cfg: Config = Config { owner: divine5(), supply: 100 }
    let x: Field = pub_read()
    if x == 0 {
        pub_write(cfg.supply)
    } else {
        let (hi, lo) = split(x)
        pub_write(as_field(lo))
    }
    for i in 0..5 bounded 5 {
        pub_write(i)
    }
    reveal Transfer { from: 0, to: 1, amount: x }
}
"#;
    let first = fmt(src);
    let second = fmt(&first);
    assert_eq!(first, second, "complex formatting should be idempotent");
}

// --- Use declarations ---

#[test]
fn test_use_declarations() {
    let src = "program test\n\nuse std.hash\n\nuse std.field\n\nfn main() {\n    pub_write(pub_read())\n}\n";
    assert_eq!(fmt(src), src);
}

// --- Types ---

#[test]
fn test_all_types_formatted() {
    use crate::ast::{ArraySize, Type};
    assert_eq!(format_type(&Type::Field), "Field");
    assert_eq!(format_type(&Type::XField), "XField");
    assert_eq!(format_type(&Type::Bool), "Bool");
    assert_eq!(format_type(&Type::U32), "U32");
    assert_eq!(format_type(&Type::Digest), "Digest");
    assert_eq!(
        format_type(&Type::Array(Box::new(Type::Field), ArraySize::Literal(10))),
        "[Field; 10]"
    );
    assert_eq!(
        format_type(&Type::Tuple(vec![Type::Field, Type::U32])),
        "(Field, U32)"
    );
}

// --- Intrinsic attribute ---

#[test]
fn test_intrinsic_function() {
    let src = "module std.hash\n\n#[intrinsic(hash)]\npub fn tip5(a: Field, b: Field, c: Field, d: Field, e: Field, f: Field, g: Field, h: Field, i: Field, j: Field) -> Digest\n";
    let out = fmt(src);
    assert!(out.contains("#[intrinsic(hash)]"), "intrinsic preserved");
    assert!(out.contains("pub fn tip5"), "function name preserved");
}

// --- Line wrapping ---

#[test]
fn test_long_signature_wraps() {
    let src = "program test\n\nfn long_function(aaa: Field, bbb: Field, ccc: Field, ddd: Field, eee: Field, fff: Field) -> Field {\n    aaa\n}\n";
    let out = fmt(src);
    assert!(out.contains("fn long_function"));
    assert!(out.contains("-> Field"));
}

// --- Round-trip: parse -> format -> parse produces same AST items ---

#[test]
fn test_round_trip_preserves_ast() {
    let src = r#"program test

struct Pair {
    a: Field,
    b: Field,
}

const LIMIT: Field = 42

event Tick {
    seq: Field,
}

fn helper(x: Field) -> Field {
    x + 1
}

fn main() {
    let p: Pair = Pair { a: 1, b: 2 }
    let mut sum: Field = p.a + p.b
    if sum == 3 {
        sum = helper(sum)
    }
    for i in 0..5 bounded 5 {
        sum = sum + i
    }
    reveal Tick { seq: sum }
    pub_write(sum)
}
"#;
    let formatted = fmt(src);
    let (tok2, _, lex2) = Lexer::new(&formatted, 0).tokenize();
    assert!(lex2.is_empty(), "formatted source should lex cleanly");
    let file2 = Parser::new(tok2).parse_file().unwrap();

    let (tok1, _, _) = Lexer::new(src, 0).tokenize();
    let file1 = Parser::new(tok1).parse_file().unwrap();

    assert_eq!(file1.items.len(), file2.items.len(), "item count mismatch");

    for (a, b) in file1.items.iter().zip(file2.items.iter()) {
        let kind_a = match &a.node {
            Item::Fn(_) => "fn",
            Item::Struct(_) => "struct",
            Item::Const(_) => "const",
            Item::Event(_) => "event",
        };
        let kind_b = match &b.node {
            Item::Fn(_) => "fn",
            Item::Struct(_) => "struct",
            Item::Const(_) => "const",
            Item::Event(_) => "event",
        };
        assert_eq!(kind_a, kind_b, "item kind mismatch");
    }
}

// --- Edge cases ---

#[test]
fn test_empty_function_body() {
    let src = "program test\n\nfn main() {\n}\n";
    let out = fmt(src);
    assert!(out.contains("fn main()"));
}

#[test]
fn test_single_trailing_newline() {
    let src = "program test\n\nfn main() {\n    pub_write(0)\n}\n";
    let out = fmt(src);
    assert!(out.ends_with("}\n"), "should end with exactly one newline");
    assert!(
        !out.ends_with("}\n\n"),
        "should not end with double newline"
    );
}

#[test]
fn test_sec_ram_formatting() {
    let src = "program test\n\nsec ram: {\n    0: Field,\n    5: Digest,\n}\n\nfn main() {\n    pub_write(ram_read(0))\n}\n";
    let out = fmt(src);
    assert!(out.contains("sec ram:"));
    assert!(out.contains("0: Field"));
    assert!(out.contains("5: Digest"));
}

// --- Fungible token round-trip ---

#[test]
fn test_coin_idempotent() {
    let src = include_str!("../../../os/neptune/standards/coin.tri");
    let first = fmt(src);
    let second = fmt(&first);
    assert_eq!(first, second, "token.tri formatting should be idempotent");
}

#[test]
fn test_asm_basic_formatting() {
    let src = "program test\n\nfn main() {\n    asm {\n        push 1\n        add\n    }\n}\n";
    assert_eq!(fmt(src), src);
}

#[test]
fn test_asm_positive_effect_formatting() {
    let src = "program test\n\nfn main() {\n    asm(+1) {\n        push 42\n    }\n}\n";
    assert_eq!(fmt(src), src);
}

#[test]
fn test_asm_negative_effect_formatting() {
    let src =
        "program test\n\nfn main() {\n    asm(-2) {\n        pop 1\n        pop 1\n    }\n}\n";
    assert_eq!(fmt(src), src);
}

#[test]
fn test_asm_idempotent() {
    let src = "program test\n\nfn main() {\n    let x: Field = pub_read()\n    asm {\n        dup 0\n        add\n    }\n    pub_write(x)\n}\n";
    let first = fmt(src);
    let second = fmt(&first);
    assert_eq!(first, second, "asm formatting should be idempotent");
}

#[test]
fn test_asm_with_negative_literal() {
    let src = "program test\n\nfn main() {\n    asm {\n        push -1\n        mul\n    }\n}\n";
    assert_eq!(fmt(src), src);
}

#[test]
fn test_match_formatting() {
    let src = "program test\n\nfn main() {\n    let x: Field = pub_read()\n    match x {\n        0 => {\n            pub_write(0)\n        }\n        1 => {\n            pub_write(1)\n        }\n        _ => {\n            pub_write(2)\n        }\n    }\n}\n";
    assert_eq!(fmt(src), src);
}

#[test]
fn test_match_bool_formatting() {
    let src = "program test\n\nfn main() {\n    let b: Bool = true\n    match b {\n        true => {\n            pub_write(1)\n        }\n        false => {\n            pub_write(0)\n        }\n    }\n}\n";
    assert_eq!(fmt(src), src);
}

#[test]
fn test_match_idempotent() {
    let src = "program test\n\nfn main() {\n    let x: Field = pub_read()\n    match x {\n        0 => {\n            pub_write(0)\n        }\n        _ => {\n            pub_write(1)\n        }\n    }\n}\n";
    let first = fmt(src);
    let second = fmt(&first);
    assert_eq!(first, second, "match formatting should be idempotent");
}

#[test]
fn test_match_struct_pattern_formatting() {
    let src = "program test\n\nstruct Point {\n    x: Field,\n    y: Field,\n}\n\nfn main() {\n    let p = Point { x: 1, y: 2 }\n    match p {\n        Point { x, y } => {\n            pub_write(x)\n        }\n    }\n}\n";
    let first = fmt(src);
    let second = fmt(&first);
    assert_eq!(
        first, second,
        "struct pattern formatting should be idempotent"
    );
    assert!(
        first.contains("Point { x, y }"),
        "should use shorthand for matching bindings"
    );
}

#[test]
fn test_match_struct_pattern_with_literal_formatting() {
    let src = "program test\n\nstruct Pair {\n    a: Field,\n    b: Field,\n}\n\nfn main() {\n    let p = Pair { a: 1, b: 2 }\n    match p {\n        Pair { a: 0, b } => {\n            pub_write(b)\n        }\n        _ => {\n            pub_write(0)\n        }\n    }\n}\n";
    let first = fmt(src);
    let second = fmt(&first);
    assert_eq!(
        first, second,
        "struct pattern with literal should be idempotent"
    );
    assert!(
        first.contains("Pair { a: 0, b }"),
        "should format literal field pattern"
    );
}
