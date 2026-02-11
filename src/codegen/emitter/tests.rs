use super::*;
use crate::lexer::Lexer;
use crate::parser::Parser;

fn compile(source: &str) -> String {
    let (tokens, _, _) = Lexer::new(source, 0).tokenize();
    let file = Parser::new(tokens).parse_file().unwrap();
    Emitter::new().emit_file(&file)
}

#[test]
fn test_minimal_program() {
    let tasm = compile("program test\nfn main() {\n}");
    assert!(tasm.contains("call __main"));
    assert!(tasm.contains("halt"));
    assert!(tasm.contains("__main:"));
    assert!(tasm.contains("return"));
}

#[test]
fn test_pub_read_write() {
    let tasm =
        compile("program test\nfn main() {\n    let a: Field = pub_read()\n    pub_write(a)\n}");
    assert!(tasm.contains("read_io 1"));
    assert!(tasm.contains("dup 0")); // access a
    assert!(tasm.contains("write_io 1"));
}

#[test]
fn test_field_arithmetic_stack_correctness() {
    let tasm = compile(
        "program test\nfn main() {\n    let a: Field = pub_read()\n    let b: Field = pub_read()\n    let c: Field = a + b\n    pub_write(c)\n}",
    );
    let lines: Vec<&str> = tasm.lines().collect();
    let read_io_count = lines.iter().filter(|l| l.contains("read_io 1")).count();
    assert_eq!(read_io_count, 2);
    assert!(tasm.contains("add"));
    assert!(tasm.contains("write_io 1"));
}

#[test]
fn test_assert_eq() {
    let tasm = compile(
        "program test\nfn main() {\n    let a: Field = pub_read()\n    let b: Field = divine()\n    assert(a == b)\n}",
    );
    assert!(tasm.contains("read_io 1"));
    assert!(tasm.contains("divine 1"));
    assert!(tasm.contains("eq"));
    assert!(tasm.contains("assert"));
}

#[test]
fn test_sum_check_program() {
    let tasm = compile(
        "program sum_check\nfn main() {\n    let a: Field = pub_read()\n    let b: Field = pub_read()\n    let sum: Field = a + b\n    pub_write(sum)\n    let expected: Field = divine()\n    assert(sum == expected)\n}",
    );
    eprintln!("=== TASM output ===\n{}", tasm);
    assert!(tasm.contains("read_io 1"));
    assert!(tasm.contains("add"));
    assert!(tasm.contains("write_io 1"));
    assert!(tasm.contains("divine 1"));
    assert!(tasm.contains("eq"));
    assert!(tasm.contains("assert"));
}

#[test]
fn test_user_function_call() {
    let tasm = compile(
        "program test\nfn add(a: Field, b: Field) -> Field {\n    a + b\n}\nfn main() {\n    let x: Field = pub_read()\n    let y: Field = pub_read()\n    let z: Field = add(x, y)\n    pub_write(z)\n}",
    );
    assert!(tasm.contains("call __add"));
    assert!(tasm.contains("__add:"));
}

#[test]
fn test_function_return_via_tail_expr() {
    let tasm = compile(
        "program test\nfn double(x: Field) -> Field {\n    x + x\n}\nfn main() {\n    let a: Field = pub_read()\n    let b: Field = double(a)\n    pub_write(b)\n}",
    );
    assert!(tasm.contains("__double:"));
    assert!(tasm.contains("add"));
    assert!(tasm.contains("swap 1"));
}

#[test]
fn test_cross_module_call_emission() {
    let tasm = compile(
        "program test\nfn main() {\n    let a: Field = pub_read()\n    let b: Field = helpers.double(a)\n    pub_write(b)\n}",
    );
    assert!(tasm.contains("call __helpers__double"));
}

#[test]
fn test_struct_init_emission() {
    let tasm = compile(
        "program test\nstruct Point {\n    x: Field,\n    y: Field,\n}\nfn main() {\n    let a: Field = pub_read()\n    let b: Field = pub_read()\n    let p: Point = Point { x: a, y: b }\n    pub_write(p.x)\n}",
    );
    eprintln!("=== struct TASM ===\n{}", tasm);
    assert!(tasm.contains("read_io 1"));
    assert!(tasm.contains("write_io 1"));
}

#[test]
fn test_array_index_emission() {
    let tasm = compile(
        "program test\nfn main() {\n    let a: Field = pub_read()\n    let b: Field = pub_read()\n    let c: Field = pub_read()\n    let arr: [Field; 3] = [a, b, c]\n    pub_write(arr[0])\n}",
    );
    eprintln!("=== array TASM ===\n{}", tasm);
    assert!(tasm.contains("read_io 1"));
    assert!(tasm.contains("write_io 1"));
}

#[test]
fn test_module_no_entry_wrapper() {
    let (tokens, _, _) = Lexer::new(
        "module helpers\npub fn add(a: Field, b: Field) -> Field {\n    a + b\n}",
        0,
    )
    .tokenize();
    let file = Parser::new(tokens).parse_file().unwrap();
    let tasm = Emitter::new().emit_file(&file);
    let first_line = tasm.lines().next().unwrap_or("").trim();
    assert_ne!(first_line, "call __main");
    assert!(!tasm.starts_with("    call __main"));
}

#[test]
fn test_digest_variable_access() {
    // Digest variables (width 5) should dup all 5 elements
    let tasm = compile(
        "program test\nfn main() {\n    let d: Digest = divine5()\n    let e: Digest = pub_read5()\n    assert_digest(d, e)\n}",
    );
    eprintln!("=== digest TASM ===\n{}", tasm);
    assert!(tasm.contains("divine 5"));
    assert!(tasm.contains("read_io 5"));
    // Accessing d (width 5) should produce 5 dup instructions, not 1
    let dup_count = tasm.lines().filter(|l| l.trim().starts_with("dup")).count();
    assert!(
        dup_count >= 10,
        "expected at least 10 dups for two Digest variable accesses, got {}",
        dup_count
    );
    assert!(tasm.contains("assert_vector"));
}

#[test]
fn test_user_fn_returning_digest() {
    // User function returning Digest should have correct return width
    let tasm = compile(
        "program test\nfn make_digest() -> Digest {\n    divine5()\n}\nfn main() {\n    let d: Digest = make_digest()\n    let e: Digest = divine5()\n    assert_digest(d, e)\n}",
    );
    eprintln!("=== fn-digest TASM ===\n{}", tasm);
    assert!(tasm.contains("call __make_digest"));
    assert!(tasm.contains("assert_vector"));
}

#[test]
fn test_spill_with_many_variables() {
    // Create a program with >16 live Field variables to trigger stack spilling
    let mut src = String::from("program test\nfn main() {\n");
    for i in 0..18 {
        src.push_str(&format!("    let v{}: Field = pub_read()\n", i));
    }
    // Access an early variable after many others to trigger reload
    src.push_str("    pub_write(v0)\n");
    src.push_str("}\n");

    let tasm = compile(&src);
    eprintln!("=== spill TASM ===\n{}", tasm);

    // Should contain spill instructions (write_mem to high RAM addresses)
    assert!(
        tasm.contains("write_mem"),
        "expected spill write_mem instructions"
    );
    // The output should still have all 18 read_io instructions
    let read_count = tasm.lines().filter(|l| l.contains("read_io 1")).count();
    assert_eq!(read_count, 18);
    // And the final write_io
    assert!(tasm.contains("write_io 1"));
}

#[test]
fn test_emit_tasm() {
    let tasm = compile(
        "program test\nevent Transfer { from: Field, to: Field, amount: Field }\nfn main() {\n    let a: Field = pub_read()\n    let b: Field = pub_read()\n    let c: Field = pub_read()\n    emit Transfer { from: a, to: b, amount: c }\n}",
    );
    eprintln!("=== emit TASM ===\n{}", tasm);
    // Tag (push 0) + write_io 1 for tag + 3 field write_io 1s = 4 write_io 1s total
    let write_io_1_count = tasm.lines().filter(|l| l.trim() == "write_io 1").count();
    assert!(
        write_io_1_count >= 4,
        "expected at least 4 write_io 1 for emit, got {}",
        write_io_1_count
    );
    // No hash instruction for open emit
    let hash_in_main = tasm
        .lines()
        .skip_while(|l| !l.contains("__main:"))
        .take_while(|l| !l.trim().starts_with("return"))
        .filter(|l| l.trim() == "hash")
        .count();
    assert_eq!(hash_in_main, 0, "open emit should not hash");
}

#[test]
fn test_seal_tasm() {
    let tasm = compile(
        "program test\nevent Nullifier { id: Field, nonce: Field }\nfn main() {\n    let x: Field = pub_read()\n    let y: Field = pub_read()\n    seal Nullifier { id: x, nonce: y }\n}",
    );
    eprintln!("=== seal TASM ===\n{}", tasm);
    // Seal should produce hash + write_io 5
    assert!(tasm.contains("hash"), "seal should contain hash");
    assert!(
        tasm.contains("write_io 5"),
        "seal should write_io 5 for digest"
    );
}

#[test]
fn test_multi_width_array_element() {
    // Array of Digest (width 5 per element)
    let tasm = compile(
        "program test\nfn main() {\n    let a: Digest = divine5()\n    let b: Digest = divine5()\n    let arr: [Digest; 2] = [a, b]\n    let first: Digest = arr[0]\n    let second: Digest = arr[1]\n    assert_digest(first, second)\n}",
    );
    eprintln!("=== multi-width array TASM ===\n{}", tasm);
    assert!(!tasm.contains("ERROR"), "should not have errors");
    assert!(tasm.contains("assert_vector"));
}

#[test]
fn test_runtime_array_index() {
    // Access array element with a runtime-computed index
    let tasm = compile(
        "program test\nfn main() {\n    let a: Field = pub_read()\n    let b: Field = pub_read()\n    let c: Field = pub_read()\n    let arr: [Field; 3] = [a, b, c]\n    let idx: Field = pub_read()\n    let val: Field = arr[idx]\n    pub_write(val)\n}",
    );
    eprintln!("=== runtime index TASM ===\n{}", tasm);
    assert!(!tasm.contains("ERROR"), "should not have errors");
    // Runtime indexing uses RAM: write_mem to store, read_mem to load
    assert!(tasm.contains("write_mem"));
    assert!(tasm.contains("read_mem"));
}

#[test]
fn test_deep_variable_access_spill() {
    // Access a variable when the stack is deeply loaded (>16 elements)
    // The stack manager should spill/reload automatically
    let tasm = compile(
        "program test\nfn main() {\n    let a: Field = pub_read()\n    let b: Field = pub_read()\n    let c: Field = pub_read()\n    let d: Field = pub_read()\n    let e: Field = pub_read()\n    let f: Field = pub_read()\n    let g: Field = pub_read()\n    let h: Field = pub_read()\n    let i: Field = pub_read()\n    let j: Field = pub_read()\n    let k: Field = pub_read()\n    let l: Field = pub_read()\n    let m: Field = pub_read()\n    let n: Field = pub_read()\n    let o: Field = pub_read()\n    let p: Field = pub_read()\n    let q: Field = pub_read()\n    pub_write(a)\n    pub_write(q)\n}",
    );
    eprintln!("=== deep access TASM ===\n{}", tasm);
    assert!(
        !tasm.contains("ERROR"),
        "deep variable should be accessible via spill/reload"
    );
    // Should have spill instructions (write_mem for eviction)
    assert!(tasm.contains("write_mem"), "expected spill to RAM");
}

#[test]
fn test_struct_field_from_fn_return() {
    // Struct field access on a value returned from a function call
    let tasm = compile(
        "program test\nstruct Point {\n    x: Field,\n    y: Field,\n}\nfn make_point(a: Field, b: Field) -> Point {\n    Point { x: a, y: b }\n}\nfn main() {\n    let a: Field = pub_read()\n    let b: Field = pub_read()\n    let p: Point = make_point(a, b)\n    pub_write(p.x)\n    pub_write(p.y)\n}",
    );
    eprintln!("=== struct fn return TASM ===\n{}", tasm);
    assert!(!tasm.contains("ERROR"), "should not have unresolved field");
    assert!(tasm.contains("write_io 1"));
}

#[test]
fn test_sec_ram_emission() {
    // sec ram declarations should produce metadata comments in TASM
    let tasm = compile(
        "program test\n\nsec ram: {\n    17: Field,\n    42: Field,\n}\n\nfn main() {\n    let v: Field = ram_read(17)\n    pub_write(v)\n}",
    );
    eprintln!("=== sec ram TASM ===\n{}", tasm);
    assert!(tasm.contains("sec ram"), "should have sec ram comment");
    assert!(tasm.contains("ram[17]"), "should document address 17");
    assert!(tasm.contains("ram[42]"), "should document address 42");
}

#[test]
fn test_digest_destructuring() {
    // Decompose a Digest into 5 individual Field variables
    let tasm = compile(
        "program test\nfn main() {\n    let d: Digest = divine5()\n    let (f0, f1, f2, f3, f4) = d\n    pub_write(f0)\n    pub_write(f4)\n}",
    );
    eprintln!("=== digest destructure TASM ===\n{}", tasm);
    assert!(tasm.contains("divine 5"));
    // After destructuring, each field should be accessible as width-1 var
    assert!(tasm.contains("write_io 1"));
}

#[test]
fn test_digest_destructure_and_pass_to_hash() {
    // Decompose a Digest, then pass fields to hash()
    let tasm = compile(
        "program test\nfn main() {\n    let d: Digest = divine5()\n    let (f0, f1, f2, f3, f4) = d\n    let h: Digest = hash(f0, f1, f2, f3, f4, 0, 0, 0, 0, 0)\n    let e: Digest = divine5()\n    assert_digest(h, e)\n}",
    );
    eprintln!("=== digest decompose+hash TASM ===\n{}", tasm);
    assert!(tasm.contains("divine 5"));
    assert!(tasm.contains("hash"));
    assert!(tasm.contains("assert_vector"));
}

#[test]
fn test_asm_block_emits_raw_tasm() {
    let tasm = compile(
        "program test\nfn main() {\n    asm(+1) { push 42 }\n    asm(-1) { write_io 1 }\n}",
    );
    eprintln!("=== asm TASM ===\n{}", tasm);
    assert!(tasm.contains("push 42"), "raw asm should appear in output");
    assert!(
        tasm.contains("write_io 1"),
        "raw asm should appear in output"
    );
}

#[test]
fn test_asm_block_with_negative_push() {
    // TASM allows `push -1` but Trident doesn't have negative literals
    let tasm = compile("program test\nfn main() {\n    asm { push -1\nadd }\n}");
    eprintln!("=== asm negative TASM ===\n{}", tasm);
    assert!(tasm.contains("push -1"));
    assert!(tasm.contains("add"));
}

#[test]
fn test_asm_spills_variables_before_block() {
    // Variables should be spilled to RAM before asm block executes
    let mut src = String::from("program test\nfn main() {\n");
    for i in 0..5 {
        src.push_str(&format!("    let v{}: Field = pub_read()\n", i));
    }
    src.push_str("    asm { push 99 }\n");
    src.push_str("}\n");

    let tasm = compile(&src);
    eprintln!("=== asm spill TASM ===\n{}", tasm);
    // The asm instruction should be present
    assert!(tasm.contains("push 99"));
    // Variables should be spilled before the asm block
    assert!(tasm.contains("write_mem"), "expected spill before asm");
}

#[test]
fn test_asm_net_zero_effect() {
    // asm with net-zero effect: stack model unchanged
    let tasm = compile(
        "program test\nfn main() {\n    let x: Field = pub_read()\n    asm { dup 0\npop 1 }\n    pub_write(x)\n}",
    );
    eprintln!("=== asm zero-effect TASM ===\n{}", tasm);
    assert!(tasm.contains("dup 0"));
    assert!(!tasm.contains("ERROR"));
}

// --- Size-generic function emission tests ---

/// Full pipeline: parse → typecheck → emit (needed for generic functions).
fn compile_full(source: &str) -> String {
    crate::compile(source, "test.tri").expect("compilation should succeed")
}

#[test]
fn test_generic_fn_emits_mangled_label() {
    let tasm = compile_full(
        "program test\nfn first<N>(arr: [Field; N]) -> Field {\n    arr[0]\n}\nfn main() {\n    let a: [Field; 3] = [1, 2, 3]\n    let s: Field = first<3>(a)\n    pub_write(s)\n}",
    );
    eprintln!("=== generic TASM ===\n{}", tasm);
    // Should have mangled label for first with N=3
    assert!(
        tasm.contains("__first__N3:"),
        "should emit mangled label __first__N3"
    );
    assert!(
        tasm.contains("call __first__N3"),
        "should call mangled label"
    );
}

#[test]
fn test_generic_fn_two_instantiations() {
    let tasm = compile_full(
        "program test\nfn first<N>(arr: [Field; N]) -> Field {\n    arr[0]\n}\nfn main() {\n    let a: [Field; 3] = [1, 2, 3]\n    let b: [Field; 5] = [1, 2, 3, 4, 5]\n    let x: Field = first<3>(a)\n    let y: Field = first<5>(b)\n    pub_write(x + y)\n}",
    );
    eprintln!("=== two instantiations TASM ===\n{}", tasm);
    assert!(tasm.contains("__first__N3:"), "should emit first<3>");
    assert!(tasm.contains("__first__N5:"), "should emit first<5>");
    assert!(tasm.contains("call __first__N3"), "should call first<3>");
    assert!(tasm.contains("call __first__N5"), "should call first<5>");
}

#[test]
fn test_generic_fn_inferred_emission() {
    let tasm = compile_full(
        "program test\nfn first<N>(arr: [Field; N]) -> Field {\n    arr[0]\n}\nfn main() {\n    let a: [Field; 3] = [1, 2, 3]\n    let s: Field = first(a)\n    pub_write(s)\n}",
    );
    eprintln!("=== inferred generic TASM ===\n{}", tasm);
    // Inferred N=3 from [Field; 3]
    assert!(
        tasm.contains("__first__N3:"),
        "should emit __first__N3 via inference"
    );
    assert!(tasm.contains("call __first__N3"), "should call __first__N3");
}

#[test]
fn test_generic_fn_not_emitted_as_regular() {
    let tasm = compile_full(
        "program test\nfn first<N>(arr: [Field; N]) -> Field {\n    arr[0]\n}\nfn main() {\n    let a: [Field; 3] = [1, 2, 3]\n    let s: Field = first<3>(a)\n    pub_write(s)\n}",
    );
    // Generic function should NOT be emitted with the un-mangled label
    assert!(
        !tasm.contains("\n__first:"),
        "generic fn should not emit un-mangled label"
    );
}

// ─── Cross-Target Tests ────────────────────────────────────────

fn compile_with_target(source: &str, target_name: &str) -> String {
    let config = TargetConfig::resolve(target_name).unwrap_or_else(|_| TargetConfig::triton());
    let backend = create_backend(target_name);
    let (tokens, _, _) = Lexer::new(source, 0).tokenize();
    let file = Parser::new(tokens).parse_file().unwrap();
    Emitter::with_backend(backend, config).emit_file(&file)
}

#[test]
fn test_backend_factory_triton() {
    let backend = create_backend("triton");
    assert_eq!(backend.target_name(), "triton");
    assert_eq!(backend.output_extension(), ".tasm");
}

#[test]
fn test_backend_factory_miden() {
    let backend = create_backend("miden");
    assert_eq!(backend.target_name(), "miden");
    assert_eq!(backend.output_extension(), ".masm");
}

#[test]
fn test_backend_factory_openvm() {
    let backend = create_backend("openvm");
    assert_eq!(backend.target_name(), "openvm");
    assert_eq!(backend.output_extension(), ".S");
}

#[test]
fn test_backend_factory_sp1() {
    let backend = create_backend("sp1");
    assert_eq!(backend.target_name(), "sp1");
    assert_eq!(backend.output_extension(), ".S");
}

#[test]
fn test_backend_factory_cairo() {
    let backend = create_backend("cairo");
    assert_eq!(backend.target_name(), "cairo");
    assert_eq!(backend.output_extension(), ".sierra");
}

#[test]
fn test_backend_factory_unknown_falls_back() {
    let backend = create_backend("unknown");
    assert_eq!(backend.target_name(), "triton");
}

#[test]
fn test_triton_instructions() {
    let b = TritonBackend;
    assert_eq!(b.inst_push(42), "push 42");
    assert_eq!(b.inst_pop(1), "pop 1");
    assert_eq!(b.inst_dup(0), "dup 0");
    assert_eq!(b.inst_swap(1), "swap 1");
    assert_eq!(b.inst_add(), "add");
    assert_eq!(b.inst_mul(), "mul");
    assert_eq!(b.inst_call("foo"), "call foo");
    assert_eq!(b.inst_return(), "return");
}

#[test]
fn test_miden_instructions() {
    let b = MidenBackend;
    assert_eq!(b.inst_push(42), "push.42");
    assert_eq!(b.inst_pop(1), "drop");
    assert_eq!(b.inst_dup(0), "dup.0");
    assert!(b.inst_swap(3).contains("movup"));
    assert_eq!(b.inst_add(), "add");
    assert_eq!(b.inst_mul(), "mul");
    assert_eq!(b.inst_call("foo"), "exec.foo");
    assert_eq!(b.inst_return(), "end");
}

#[test]
fn test_openvm_instructions() {
    let b = OpenVMBackend;
    assert!(b.inst_push(42).contains("li"));
    assert!(b.inst_add().contains("add"));
    assert!(b.inst_call("foo").contains("jal"));
}

#[test]
fn test_sp1_instructions() {
    let b = SP1Backend;
    assert!(b.inst_push(42).contains("li"));
    assert!(b.inst_add().contains("add"));
    assert_eq!(b.inst_return(), "ret");
}

#[test]
fn test_cairo_instructions() {
    let b = CairoBackend;
    assert!(b.inst_push(42).contains("felt252_const<42>"));
    assert!(b.inst_add().contains("felt252_add"));
    assert!(b.inst_mul().contains("felt252_mul"));
    assert!(b.inst_call("foo").contains("function_call"));
    assert_eq!(b.inst_return(), "return([0])");
}

#[test]
fn test_compile_minimal_triton() {
    let out = compile_with_target("program test\nfn main() {\n}", "triton");
    assert!(out.contains("call __main"));
}

#[test]
fn test_compile_minimal_miden() {
    let out = compile_with_target("program test\nfn main() {\n}", "miden");
    assert!(out.contains("exec.main"));
}

#[test]
fn test_compile_minimal_openvm() {
    let out = compile_with_target("program test\nfn main() {\n}", "openvm");
    assert!(out.contains("jal ra, __main"));
}

#[test]
fn test_compile_minimal_sp1() {
    let out = compile_with_target("program test\nfn main() {\n}", "sp1");
    assert!(out.contains("jal ra, __main"));
}

#[test]
fn test_compile_minimal_cairo() {
    let out = compile_with_target("program test\nfn main() {\n}", "cairo");
    assert!(out.contains("function_call<__main>"));
}

#[test]
fn test_all_targets_produce_output() {
    let source = "program test\nfn main() {\n  let x: Field = 42\n}";
    for target in &["triton", "miden", "openvm", "sp1", "cairo"] {
        let out = compile_with_target(source, target);
        assert!(!out.is_empty(), "target {} produced empty output", target);
    }
}

// ─── Cross-Target Integration Tests ─────────────────────────────

#[test]
fn test_cross_target_arithmetic() {
    let source = "program test\nfn main() {\n  let a: Field = 10\n  let b: Field = 20\n  let c: Field = a + b\n  let d: Field = a * b\n  let e: Field = d + c\n  pub_write(e)\n}";
    for target in &["triton", "miden", "openvm", "sp1", "cairo"] {
        let out = compile_with_target(source, target);
        assert!(!out.is_empty(), "{}: empty output for arithmetic", target);
        // Each target should have its add and mul instructions
        match *target {
            "triton" => {
                assert!(out.contains("add"), "{}: missing add", target);
                assert!(out.contains("mul"), "{}: missing mul", target);
            }
            "miden" => {
                assert!(out.contains("add"), "{}: missing add", target);
                assert!(out.contains("mul"), "{}: missing mul", target);
            }
            "openvm" | "sp1" => {
                assert!(out.contains("add"), "{}: missing add", target);
                assert!(out.contains("mul"), "{}: missing mul", target);
            }
            "cairo" => {
                assert!(out.contains("felt252_add"), "{}: missing add", target);
                assert!(out.contains("felt252_mul"), "{}: missing mul", target);
            }
            _ => {}
        }
    }
}

#[test]
fn test_cross_target_control_flow() {
    let source = "program test\nfn main() {\n  let x: Field = pub_read()\n  if x == 0 {\n    pub_write(1)\n  } else {\n    pub_write(2)\n  }\n}";
    for target in &["triton", "miden", "openvm", "sp1", "cairo"] {
        let out = compile_with_target(source, target);
        assert!(!out.is_empty(), "{}: empty output for control flow", target);
        // All targets should have labels for branching
        assert!(
            out.contains("main"),
            "{}: missing main label in control flow",
            target
        );
    }
}

#[test]
fn test_cross_target_function_calls() {
    let source = "program test\nfn add_one(x: Field) -> Field {\n  x + 1\n}\nfn main() {\n  let r: Field = add_one(41)\n  pub_write(r)\n}";
    for target in &["triton", "miden", "openvm", "sp1", "cairo"] {
        let out = compile_with_target(source, target);
        assert!(!out.is_empty(), "{}: empty for function calls", target);
        // Should have both main and add_one labels
        assert!(out.contains("main"), "{}: missing main", target);
        assert!(out.contains("add_one"), "{}: missing add_one", target);
    }
}

#[test]
fn test_cross_target_loops() {
    let source = "program test\nfn main() {\n  let n: Field = 5\n  let mut sum: Field = 0\n  for i in 0..n bounded 10 {\n    sum = sum + i\n  }\n  pub_write(sum)\n}";
    for target in &["triton", "miden", "openvm", "sp1", "cairo"] {
        let out = compile_with_target(source, target);
        assert!(!out.is_empty(), "{}: empty for loops", target);
        // Loops desugar to labels and jumps in all targets
        assert!(
            out.contains("main"),
            "{}: missing main in loop test",
            target
        );
    }
}

#[test]
fn test_cross_target_io() {
    let source = "program test\nfn main() {\n  let x: Field = pub_read()\n  let y: Field = pub_read()\n  pub_write(x + y)\n}";
    for target in &["triton", "miden", "openvm", "sp1", "cairo"] {
        let out = compile_with_target(source, target);
        assert!(!out.is_empty(), "{}: empty for IO", target);
        match *target {
            "triton" => {
                assert!(out.contains("read_io"), "{}: missing read_io", target);
                assert!(out.contains("write_io"), "{}: missing write_io", target);
            }
            "miden" => {
                assert!(
                    out.contains("sdepth") && out.contains("drop"),
                    "{}: missing miden IO pattern",
                    target
                );
            }
            "openvm" | "sp1" => {
                assert!(out.contains("ecall"), "{}: missing ecall for IO", target);
            }
            "cairo" => {
                assert!(
                    out.contains("input") || out.contains("output"),
                    "{}: missing cairo IO",
                    target
                );
            }
            _ => {}
        }
    }
}

#[test]
fn test_cross_target_events() {
    let source = "program test\nevent Transfer {\n  amount: Field,\n}\nfn main() {\n  emit Transfer { amount: 100 }\n}";
    for target in &["triton", "miden", "openvm", "sp1", "cairo"] {
        let out = compile_with_target(source, target);
        assert!(!out.is_empty(), "{}: empty for events", target);
    }
}

#[test]
fn test_cross_target_multiple_functions() {
    let source = "program test\nfn double(x: Field) -> Field {\n  x * 2\n}\nfn triple(x: Field) -> Field {\n  x * 3\n}\nfn main() {\n  let a: Field = double(5)\n  let b: Field = triple(5)\n  pub_write(a + b)\n}";
    for target in &["triton", "miden", "openvm", "sp1", "cairo"] {
        let out = compile_with_target(source, target);
        assert!(out.contains("double"), "{}: missing double", target);
        assert!(out.contains("triple"), "{}: missing triple", target);
        assert!(out.contains("main"), "{}: missing main", target);
    }
}

#[test]
fn test_cross_target_u32_operations() {
    let source = "program test\nfn main() {\n  let a: U32 = as_u32(10)\n  let b: U32 = as_u32(20)\n  if a < b {\n    pub_write(1)\n  } else {\n    pub_write(0)\n  }\n}";
    for target in &["triton", "miden", "openvm", "sp1", "cairo"] {
        let out = compile_with_target(source, target);
        assert!(!out.is_empty(), "{}: empty for U32 ops", target);
    }
}

#[test]
fn test_cross_target_output_size_comparison() {
    // Benchmark: same program compiled to all targets — compare sizes
    let source = "program test\nfn fib(n: Field) -> Field {\n  let mut a: Field = 0\n  let mut b: Field = 1\n  for i in 0..n bounded 20 {\n    let t: Field = b\n    b = a + b\n    a = t\n  }\n  a\n}\nfn main() {\n  let r: Field = fib(10)\n  pub_write(r)\n}";
    let mut sizes: Vec<(&str, usize)> = Vec::new();
    for target in &["triton", "miden", "openvm", "sp1", "cairo"] {
        let out = compile_with_target(source, target);
        assert!(!out.is_empty(), "{}: empty for fib benchmark", target);
        sizes.push((target, out.len()));
    }
    // All targets should produce non-trivial output
    for (target, size) in &sizes {
        assert!(*size > 50, "{}: output too small ({})", target, size);
    }
    // Sanity: outputs should differ between target families
    let triton_size = sizes[0].1;
    let cairo_size = sizes[4].1;
    assert_ne!(
        triton_size, cairo_size,
        "triton and cairo should produce different-sized output"
    );
}

#[test]
fn test_cross_target_nested_calls() {
    let source = "program test\nfn inc(x: Field) -> Field {\n  x + 1\n}\nfn add_two(x: Field) -> Field {\n  inc(inc(x))\n}\nfn main() {\n  pub_write(add_two(40))\n}";
    for target in &["triton", "miden", "openvm", "sp1", "cairo"] {
        let out = compile_with_target(source, target);
        assert!(out.contains("inc"), "{}: missing inc", target);
        assert!(out.contains("add_two"), "{}: missing add_two", target);
    }
}

#[test]
fn test_cross_target_struct() {
    let source = "program test\nstruct Point {\n  x: Field,\n  y: Field,\n}\nfn origin() -> Point {\n  Point { x: 0, y: 0 }\n}\nfn main() {\n  let p: Point = origin()\n  pub_write(p.x)\n  pub_write(p.y)\n}";
    for target in &["triton", "miden", "openvm", "sp1", "cairo"] {
        let out = compile_with_target(source, target);
        assert!(!out.is_empty(), "{}: empty for struct test", target);
        assert!(out.contains("origin"), "{}: missing origin", target);
    }
}

#[test]
fn test_cross_target_mutable_variables() {
    let source = "program test\nfn main() {\n  let mut x: Field = 0\n  x = x + 1\n  x = x + 2\n  x = x + 3\n  pub_write(x)\n}";
    for target in &["triton", "miden", "openvm", "sp1", "cairo"] {
        let out = compile_with_target(source, target);
        assert!(!out.is_empty(), "{}: empty for mutable vars", target);
    }
}

#[test]
fn test_cross_target_divine() {
    let source = "program test\nfn main() {\n  let secret: Field = divine()\n  let d: Digest = divine5()\n  let (a, b, c, e, f) = d\n  pub_write(secret + a)\n}";
    for target in &["triton", "miden", "openvm", "sp1", "cairo"] {
        let out = compile_with_target(source, target);
        assert!(!out.is_empty(), "{}: empty for divine test", target);
    }
}

#[test]
fn test_cross_target_hash() {
    let source = "program test\nfn main() {\n  let d: Digest = hash(1, 2, 3, 4, 5, 6, 7, 8, 9, 0)\n  let (a, b, c, e, f) = d\n  pub_write(a)\n}";
    for target in &["triton", "miden", "openvm", "sp1", "cairo"] {
        let out = compile_with_target(source, target);
        assert!(!out.is_empty(), "{}: empty for hash test", target);
    }
}

#[test]
fn test_cross_target_seal() {
    let source =
        "program test\nevent Secret {\n  val: Field,\n}\nfn main() {\n  seal Secret { val: 42 }\n}";
    for target in &["triton", "miden", "openvm", "sp1", "cairo"] {
        let out = compile_with_target(source, target);
        assert!(!out.is_empty(), "{}: empty for seal test", target);
    }
}
