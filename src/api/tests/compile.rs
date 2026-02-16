use crate::*;

#[test]
fn test_compile_valid_program() {
    let source =
        "program test\nfn main() {\n    let x: Field = pub_read()\n    pub_write(x + 1)\n}";
    let result = compile(source, "test.tri");
    assert!(result.is_ok());
    let tasm = result.unwrap();
    assert!(tasm.contains("read_io 1"));
    assert!(tasm.contains("write_io 1"));
}

#[test]
fn test_compile_type_error_returns_err() {
    let source = "program test\nfn main() {\n    let x: U32 = pub_read()\n}";
    let result = compile(source, "test.tri");
    assert!(result.is_err());
}

#[test]
fn test_deeply_nested_if() {
    let source = r#"program test
fn main() {
let x: Field = pub_read()
if x == 0 {
    if x == 1 {
        if x == 2 {
            if x == 3 {
                if x == 4 {
                    pub_write(x)
                }
            }
        }
    }
}
}
"#;
    assert!(compile(source, "test.tri").is_ok());
}

#[test]
fn test_deeply_nested_for() {
    let source = r#"program test
fn main() {
let mut s: Field = 0
for i in 0..3 bounded 3 {
    for j in 0..3 bounded 3 {
        for k in 0..3 bounded 3 {
            s = s + 1
        }
    }
}
pub_write(s)
}
"#;
    let result = compile(source, "test.tri");
    assert!(result.is_ok());
    let tasm = result.unwrap();
    assert!(tasm.contains("write_io 1"));
}

#[test]
fn test_many_variables_spill() {
    // Force stack spilling by having many live variables
    let source = r#"program test
fn main() {
let a: Field = pub_read()
let b: Field = pub_read()
let c: Field = pub_read()
let d: Field = pub_read()
let e: Field = pub_read()
let f: Field = pub_read()
let g: Field = pub_read()
let h: Field = pub_read()
let i: Field = pub_read()
let j: Field = pub_read()
let k: Field = pub_read()
let l: Field = pub_read()
let m: Field = pub_read()
let n: Field = pub_read()
let o: Field = pub_read()
let p: Field = pub_read()
let q: Field = pub_read()
let r: Field = pub_read()
pub_write(a + b + c + d + e + f + g + h + i + j + k + l + m + n + o + p + q + r)
}
"#;
    let result = compile(source, "test.tri");
    assert!(
        result.is_ok(),
        "should handle 18 live variables with spilling"
    );
    let tasm = result.unwrap();
    // All 18 variables contribute to the output — verify the sum reaches write_io
    assert!(
        tasm.contains("write_io 1"),
        "18-variable sum should produce write_io"
    );
}

#[test]
fn test_chain_of_function_calls() {
    let source = r#"program test
fn add1(x: Field) -> Field {
x + 1
}

fn add2(x: Field) -> Field {
add1(add1(x))
}

fn add4(x: Field) -> Field {
add2(add2(x))
}

fn main() {
let x: Field = pub_read()
pub_write(add4(add4(x)))
}
"#;
    let result = compile(source, "test.tri");
    assert!(result.is_ok());
}

#[test]
fn test_all_binary_operators() {
    let source = r#"program test
fn main() {
let a: Field = pub_read()
let b: Field = pub_read()
let sum: Field = a + b
let prod: Field = a * b
let eq: Bool = a == b
let (hi, lo) = split(a)
let lt: Bool = hi < lo
let band: U32 = hi & lo
let bxor: U32 = hi ^ lo
let (q, r) = hi /% lo
pub_write(sum)
pub_write(prod)
}
"#;
    assert!(compile(source, "test.tri").is_ok());
}

#[test]
fn test_struct_with_digest_field() {
    let source = r#"program test
struct AuthData {
owner: Digest,
nonce: Field,
}

fn main() {
let d: Digest = divine5()
let auth: AuthData = AuthData { owner: d, nonce: 42 }
pub_write(auth.nonce)
}
"#;
    assert!(compile(source, "test.tri").is_ok());
}

#[test]
fn test_xfield_operations() {
    // *. operator is XField * Field -> XField (scalar multiplication)
    let source = r#"program test
fn main() {
let a: XField = xfield(1, 2, 3)
let s: Field = pub_read()
let c: XField = a *. s
let d: XField = xinvert(c)
pub_write(0)
}
"#;
    assert!(compile(source, "test.tri").is_ok());
}

#[test]
fn test_tail_expression() {
    let source = r#"program test
fn double(x: Field) -> Field {
x + x
}

fn main() {
pub_write(double(pub_read()))
}
"#;
    assert!(compile(source, "test.tri").is_ok());
}

#[test]
fn test_multiple_return_paths() {
    let source = r#"program test
fn abs_diff(a: Field, b: Field) -> Field {
if a == b {
    return 0
}
a + b
}

fn main() {
pub_write(abs_diff(pub_read(), pub_read()))
}
"#;
    assert!(compile(source, "test.tri").is_ok());
}

#[test]
fn test_events_emit_and_seal() {
    let source = r#"program test

event Transfer {
from: Field,
to: Field,
amount: Field,
}

event Commitment {
value: Field,
}

fn main() {
let a: Field = pub_read()
let b: Field = pub_read()
let c: Field = pub_read()

// Open reveal: tag + 3 fields written directly
reveal Transfer { from: a, to: b, amount: c }

// Sealed: hash(tag, value, 0...) written as digest
seal Commitment { value: a }
}
"#;
    let tasm = compile(source, "events.tri").expect("events program should compile");

    // reveal Transfer: push 0, write_io 1, [field], write_io 1 × 3
    // Total write_io 1 from reveal: 4 (tag + 3 fields)
    let write_io_1 = tasm.lines().filter(|l| l.trim() == "write_io 1").count();
    assert!(
        write_io_1 >= 4,
        "expected >= 4 write_io 1 (reveal tag + 3 fields), got {}",
        write_io_1
    );

    // seal Commitment: hash + write_io 5
    assert!(tasm.contains("hash"), "seal should contain hash");
    assert!(tasm.contains("write_io 5"), "seal should write_io 5");

    eprintln!("Events TASM:\n{}", tasm);
}

#[test]
fn test_error_max_nesting_depth() {
    // Generate deeply nested blocks via nested if statements.
    // Each `if true { ... }` adds one nesting level; 260 > MAX_NESTING_DEPTH (256).
    // The parser recurses to depth 256 before the guard triggers, which
    // needs more stack than the default test-thread provides in debug
    // builds.  Run the actual work on a thread with an explicit 16 MB stack.
    let handle = std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            let depth = 260u32;
            let mut src = String::from("program t\nfn main() {\n");
            for _ in 0..depth {
                src.push_str("if true {\n");
            }
            src.push_str("pub_write(0)\n");
            for _ in 0..depth {
                src.push_str("}\n");
            }
            src.push_str("}\n");

            let (tokens, _comments, lex_errs) = crate::lexer::Lexer::new(&src, 0).tokenize();
            assert!(lex_errs.is_empty(), "lex errors: {:?}", lex_errs);
            let result = crate::parser::Parser::new(tokens).parse_file();
            assert!(
                result.is_err(),
                "deeply nested input should produce an error"
            );
            let diags = result.unwrap_err();
            let has_depth = diags.iter().any(|d| d.message.contains("nesting depth"));
            assert!(
                has_depth,
                "should report nesting depth exceeded, got: {:?}",
                diags.iter().map(|d| &d.message).collect::<Vec<_>>()
            );
        })
        .expect("failed to spawn test thread");
    handle.join().expect("test thread panicked");
}

#[test]
fn test_coin_compiles() {
    let path = std::path::Path::new("os/neptune/standards/coin.tri");
    if !path.exists() {
        return;
    }
    let tasm = compile_project(path).expect("coin program should compile");

    // Verify all 5 operations are in the TASM output
    assert!(tasm.contains("__pay:"), "missing pay function");
    assert!(tasm.contains("__mint:"), "missing mint function");
    assert!(tasm.contains("__burn:"), "missing burn function");
    assert!(tasm.contains("__lock:"), "missing lock function");
    assert!(tasm.contains("__update:"), "missing update function");

    // Verify helper functions
    assert!(tasm.contains("__hash_leaf:"), "missing hash_leaf function");
    assert!(
        tasm.contains("__hash_config:"),
        "missing hash_config function"
    );
    // hash_metadata is defined but never called — DCE correctly removes it
    assert!(
        tasm.contains("__verify_auth:"),
        "missing verify_auth function"
    );
    assert!(
        tasm.contains("__verify_config:"),
        "missing verify_config function"
    );

    // Verify hash operations are emitted (leaf/config/auth + seal nullifiers)
    // hash_metadata is DCE'd (unused), so count is lower than pre-DCE
    let hash_count = tasm.lines().filter(|l| l.trim() == "hash").count();
    assert!(
        hash_count >= 5,
        "expected at least 5 hash ops, got {}",
        hash_count
    );

    // Verify seal produces write_io 5 (nullifier commitments in pay and burn)
    assert!(
        tasm.contains("write_io 5"),
        "seal should produce write_io 5"
    );

    // Verify assertions are present (security checks)
    let assert_count = tasm
        .lines()
        .filter(|l| l.trim().starts_with("assert"))
        .count();
    assert!(
        assert_count >= 6,
        "expected at least 6 assertions, got {}",
        assert_count
    );

    // Verify Merkle root authentication is present
    assert!(
        tasm.contains("merkle_step"),
        "should authenticate leaves against Merkle root"
    );

    eprintln!(
        "Token TASM: {} lines, {} instructions",
        tasm.lines().count(),
        tasm.lines()
            .filter(|l| l.starts_with("    ") && !l.trim().is_empty())
            .count()
    );
}

#[test]
fn test_card_compiles() {
    let path = std::path::Path::new("os/neptune/standards/card.tri");
    if !path.exists() {
        return;
    }
    let tasm = compile_project(path).expect("card program should compile");

    // Verify all 5 PLUMB operations
    assert!(tasm.contains("__pay:"), "missing pay function");
    assert!(tasm.contains("__mint:"), "missing mint function");
    assert!(tasm.contains("__burn:"), "missing burn function");
    assert!(tasm.contains("__lock:"), "missing lock function");
    assert!(tasm.contains("__update:"), "missing update function");

    // Verify helper functions
    assert!(tasm.contains("__hash_leaf:"), "missing hash_leaf function");
    assert!(
        tasm.contains("__hash_config:"),
        "missing hash_config function"
    );
    assert!(
        tasm.contains("__verify_auth:"),
        "missing verify_auth function"
    );
    assert!(
        tasm.contains("__verify_config:"),
        "missing verify_config function"
    );

    // Verify Merkle root authentication is present
    assert!(
        tasm.contains("merkle_step"),
        "should authenticate leaves against Merkle root"
    );
}
