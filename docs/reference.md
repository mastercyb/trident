# Trident Quick Reference

Version 0.3 -- Cheat sheet for the Trident language targeting Triton VM.

File extension: `.tri` | Compiler: `trident` | Field: Goldilocks (p = 2^64 - 2^32 + 1)

---

## 1. Types

| Type | Width (field elements) | Description | Literal examples |
|------|----------------------:|-------------|------------------|
| `Field` | 1 | Base field element mod p | `0`, `42`, `18446744069414584321` |
| `XField` | 3 | Extension field element (F_p[X]/<X^3-X+1>) | `xfield(1, 2, 3)` |
| `Bool` | 1 | Field constrained to {0, 1} | `true`, `false` |
| `U32` | 1 | Unsigned 32-bit integer (range-checked) | `0`, `4294967295` |
| `Digest` | 5 | Tip5 hash digest ([Field; 5]) | `divine5()` |
| `[T; N]` | N * width(T) | Fixed-size array, N compile-time known | `[1, 2, 3]` |
| `(T1, T2)` | width(T1) + width(T2) | Tuple (max 16 elements) | `(a, b)` |
| `struct S` | sum of field widths | Named product type | `S { x: 1, y: 2 }` |

No enums. No sum types. No references. No pointers. No implicit conversions.

---

## 2. Operators

| Operator | Operand types | Result type | TASM | Description |
|----------|---------------|-------------|------|-------------|
| `a + b` | Field, Field | Field | `add` | Field addition |
| `a + N` | Field, literal | Field | `addi N` | Immediate addition |
| `a * b` | Field, Field | Field | `mul` | Field multiplication |
| `a == b` | Field, Field | Bool | `eq` | Field equality |
| `a < b` | U32, U32 | Bool | `lt` | Unsigned less-than |
| `a & b` | U32, U32 | U32 | `and` | Bitwise AND |
| `a ^ b` | U32, U32 | U32 | `xor` | Bitwise XOR |
| `a /% b` | U32, U32 | (U32, U32) | `div_mod` | Division + remainder |
| `a *. s` | XField, Field | XField | `xb_mul` | Scalar multiplication |

No subtraction operator (`-`). No division operator (`/`). No comparison operators
other than `<` and `==`. No `!=`, `>`, `<=`, `>=` -- compose from `==`, `<`, and
`std.logic.not()`. No `&&`, `||`, `!` -- use `std.logic.*`.

---

## 3. Builtin Functions

### I/O and Non-Deterministic Input

| Signature | TASM | Cost (cc/hash/u32) | Description |
|-----------|------|---------------------|-------------|
| `pub_read() -> Field` | `read_io 1` | 1/0/0 | Read 1 public input |
| `pub_read{2,3,4,5}()` | `read_io N` | 1/0/0 | Read N public inputs |
| `pub_write(v: Field)` | `write_io 1` | 1/0/0 | Write 1 public output |
| `pub_write{2,3,4,5}(...)` | `write_io N` | 1/0/0 | Write N public outputs |
| `divine() -> Field` | `divine 1` | 1/0/0 | Read 1 secret input |
| `divine3() -> (Field, Field, Field)` | `divine 3` | 1/0/0 | Read 3 secret inputs |
| `divine5() -> Digest` | `divine 5` | 1/0/0 | Read 5 secret inputs |

### Field Arithmetic

| Signature | TASM | Cost (cc/hash/u32) | Description |
|-----------|------|---------------------|-------------|
| `inv(a: Field) -> Field` | `invert` | 1/0/0 | Multiplicative inverse |
| `neg(a: Field) -> Field` | `push -1; mul` | 2/0/0 | Additive inverse (p - a) |
| `sub(a: Field, b: Field) -> Field` | `push -1; mul; add` | 3/0/0 | Field subtraction (a + (p - b)) |

Also available as `std.field.inv`, `std.field.neg`, `std.field.sub`, `std.field.mul`, `std.field.add`.

### U32 Operations

| Signature | TASM | Cost (cc/hash/u32) | Description |
|-----------|------|---------------------|-------------|
| `split(a: Field) -> (U32, U32)` | `split` | 1/0/33 | Split field to (hi, lo) u32 pair |
| `log2(a: U32) -> U32` | `log_2_floor` | 1/0/33 | Floor of log base 2 |
| `pow(base: U32, exp: U32) -> U32` | `pow` | 1/0/33 | Exponentiation |
| `popcount(a: U32) -> U32` | `pop_count` | 1/0/33 | Hamming weight (bit count) |
| `as_u32(a: Field) -> U32` | `split; pop 1` | 2/0/33 | Range-checked conversion |
| `as_field(a: U32) -> Field` | (no-op) | 0/0/0 | Type cast (zero cost) |

### Hash Operations

| Signature | TASM | Cost (cc/hash/u32) | Description |
|-----------|------|---------------------|-------------|
| `hash(a..j: Field x10) -> Digest` | `hash` | 1/6/0 | Tip5 hash of 10 fields |
| `sponge_init()` | `sponge_init` | 1/6/0 | Initialize sponge state |
| `sponge_absorb(a..j: Field x10)` | `sponge_absorb` | 1/6/0 | Absorb 10 fields |
| `sponge_absorb_mem(ptr: Field)` | `sponge_absorb_mem` | 1/6/0 | Absorb 10 fields from RAM |
| `sponge_squeeze() -> [Field; 10]` | `sponge_squeeze` | 1/6/0 | Squeeze 10 fields |

### Merkle

| Signature | TASM | Cost (cc/hash/u32) | Description |
|-----------|------|---------------------|-------------|
| `merkle_step(idx: U32, d: Digest) -> (U32, Digest)` | `merkle_step` | 1/6/33 | One tree level up |
| `merkle_step_mem(ptr, idx, d) -> (Field, U32, Digest)` | `merkle_step_mem` | 1/6/33 | Tree level from RAM |

### Assertions

| Signature | TASM | Cost (cc/hash/u32) | Description |
|-----------|------|---------------------|-------------|
| `assert(cond: Bool)` | `assert` | 1/0/0 | Crash VM if false |
| `assert_eq(a: Field, b: Field)` | `eq; assert` | 2/0/0 | Assert equality |
| `assert_digest(a: Digest, b: Digest)` | `assert_vector; pop 5` | 2/0/0 | Assert digest equality |

### RAM

| Signature | TASM | Cost (cc/hash/u32/ram) | Description |
|-----------|------|------------------------|-------------|
| `ram_read(addr) -> Field` | `read_mem 1; pop 1` | 2/0/0/1 | Read 1 word |
| `ram_write(addr, val)` | `write_mem 1; pop 1` | 2/0/0/1 | Write 1 word |
| `ram_read_block(addr) -> [Field; 5]` | `read_mem 5; pop 1` | 2/0/0/5 | Read 5 words |
| `ram_write_block(addr, vals)` | `write_mem 5; pop 1` | 2/0/0/5 | Write 5 words |

### Extension Field

| Signature | TASM | Cost | Description |
|-----------|------|------|-------------|
| `xfield(x0, x1, x2) -> XField` | (stack layout) | 0 | Construct XField |
| `xinvert(a: XField) -> XField` | `x_invert` | 1/0/0 | XField inverse |
| `xx_dot_step(acc, ptr_a, ptr_b) -> (XField, Field, Field)` | `xx_dot_step` | 1/0/0/6 | XField dot product step |
| `xb_dot_step(acc, ptr_a, ptr_b) -> (XField, Field, Field)` | `xb_dot_step` | 1/0/0/4 | Mixed dot product step |

---

## 4. Control Flow

```
// If / else (no else-if; nest instead)
if condition {
    // body
} else {
    // body
}

// Bounded for-loop (constant bound)
for i in 0..32 {
    // exactly 32 iterations
}

// Bounded for-loop (runtime count, declared max)
for i in 0..n bounded 64 {
    // at most 64 iterations; cost computed from bound
}

// Match (integer patterns + wildcard)
match value {
    0 => { handle_zero() }
    1 => { handle_one() }
    _ => { handle_default() }
}

// Early return
fn foo(x: Field) -> Field {
    if x == 0 {
        return 1
    }
    x + x
}

// Tail expression (last expression is return value)
fn bar(x: Field) -> Field {
    x * x
}
```

No `while`. No `loop`. No `break`. No `continue`. No `else if`.

---

## 5. Declarations

```
// Program entry point (exactly one per project)
program my_program

// Library module (no main)
module my_module

// Imports (no wildcards, no renaming)
use merkle
use crypto.sponge

// Public / private functions
fn private_fn(x: Field) -> Field { x }
pub fn public_fn(x: Field) -> Field { x }

// Size-generic functions
fn sum<N>(arr: [Field; N]) -> Field { ... }

// Variables
let x: Field = 42
let mut counter: U32 = 0

// Structs
struct Point { x: Field, y: Field }
pub struct PubPoint { pub x: Field, pub y: Field }

// Constants (inlined at compile time)
const MAX_DEPTH: U32 = 32
pub const ZERO: Field = 0

// I/O declarations (program modules only)
pub input:  [Field; 3]
pub output: Field
sec input:  [Field; 5]
sec ram: { 17: Field, 42: Field }

// Events
event Transfer { from: Digest, to: Digest, amount: Field }
emit Transfer { from: sender, to: receiver, amount: value }
seal Transfer { from: sender, to: receiver, amount: value }

// Test functions
#[test]
fn test_something() { assert(1 == 1) }

// Conditional compilation
#[cfg(debug)]
fn debug_only() { ... }
```

Visibility: `pub` (cross-module) or default (private). Two levels only.

---

## 6. TASM Instruction Mapping

| Trident construct | TASM instruction(s) | Trace rows |
|-------------------|---------------------|------------|
| `a + b` | `add` | 1 |
| `a + 42` | `addi 42` | 1 |
| `a * b` | `mul` | 1 |
| `inv(a)` | `invert` | 1 |
| `a == b` | `eq` | 1 |
| `a < b` | `lt` | 1 |
| `a & b` | `and` | 1 |
| `a ^ b` | `xor` | 1 |
| `a /% b` | `div_mod` | 1 |
| `split(a)` | `split` | 1 |
| `log2(a)` | `log_2_floor` | 1 |
| `pow(a, b)` | `pow` | 1 |
| `popcount(a)` | `pop_count` | 1 |
| `hash(...)` | `hash` | 1 |
| `sponge_init()` | `sponge_init` | 1 |
| `sponge_absorb(...)` | `sponge_absorb` | 1 |
| `sponge_squeeze()` | `sponge_squeeze` | 1 |
| `sponge_absorb_mem(p)` | `sponge_absorb_mem` | 1 |
| `merkle_step(i, d)` | `merkle_step` | 1 |
| `merkle_step_mem(...)` | `merkle_step_mem` | 1 |
| `divine()` | `divine 1` | 1 |
| `divine5()` | `divine 5` | 1 |
| `pub_read()` | `read_io 1` | 1 |
| `pub_write(v)` | `write_io 1` | 1 |
| `assert(x)` | `assert` | 1 |
| `assert_digest(a, b)` | `assert_vector` | 1 |
| `xx_dot_step(...)` | `xx_dot_step` | 1 |
| `xb_dot_step(...)` | `xb_dot_step` | 1 |
| `fn call / return` | `call` + `return` | body + 2 |
| `for ... (N iters)` | loop + N * body | N * body + 8 |
| `if cond { }` | `skiz` + deferred call | body + 3 |
| `if/else` | `push 1; swap 1; skiz; call; skiz; call` | max(then, else) + 3 |
| `module.fn()` | `call` (resolved address) | body + 2 |
| `fn_name<N>(...)` | `call` (monomorphized label) | body + 2 |
| `asm { ... }` | verbatim TASM | varies |

---

## 7. Cost Per Instruction

Each instruction contributes rows to multiple Triton VM tables simultaneously.
Proving cost is determined by the **tallest** table (padded to next power of 2).

| Trident construct | TASM | Processor | Hash | U32 | OpStack | RAM |
|-------------------|------|----------:|-----:|----:|--------:|----:|
| `a + b` | `add` | 1 | 0 | 0 | 1 | 0 |
| `a * b` | `mul` | 1 | 0 | 0 | 1 | 0 |
| `inv(a)` | `invert` | 1 | 0 | 0 | 0 | 0 |
| `a == b` | `eq` | 1 | 0 | 0 | 1 | 0 |
| `a < b` | `lt` | 1 | 0 | 33* | 1 | 0 |
| `a & b` | `and` | 1 | 0 | 33* | 1 | 0 |
| `a ^ b` | `xor` | 1 | 0 | 33* | 1 | 0 |
| `split(a)` | `split` | 1 | 0 | 33* | 1 | 0 |
| `a /% b` | `div_mod` | 1 | 0 | 33* | 0 | 0 |
| `pow(b, e)` | `pow` | 1 | 0 | 33* | 1 | 0 |
| `log2(a)` | `log_2_floor` | 1 | 0 | 33* | 0 | 0 |
| `popcount(a)` | `pop_count` | 1 | 0 | 33* | 0 | 0 |
| `hash(...)` | `hash` | 1 | **6** | 0 | 1 | 0 |
| `sponge_init()` | `sponge_init` | 1 | **6** | 0 | 0 | 0 |
| `sponge_absorb(...)` | `sponge_absorb` | 1 | **6** | 0 | 1 | 0 |
| `sponge_squeeze()` | `sponge_squeeze` | 1 | **6** | 0 | 1 | 0 |
| `sponge_absorb_mem(p)` | `sponge_absorb_mem` | 1 | **6** | 0 | 1 | 10 |
| `merkle_step(i, d)` | `merkle_step` | 1 | **6** | 33* | 0 | 0 |
| `merkle_step_mem(...)` | `merkle_step_mem` | 1 | **6** | 33* | 0 | 5 |
| `divine()` | `divine 1` | 1 | 0 | 0 | 1 | 0 |
| `pub_read()` | `read_io 1` | 1 | 0 | 0 | 1 | 0 |
| `pub_write(v)` | `write_io 1` | 1 | 0 | 0 | 1 | 0 |
| `ram_read(addr)` | `read_mem 1` | 2 | 0 | 0 | 2 | 1 |
| `ram_write(addr, v)` | `write_mem 1` | 2 | 0 | 0 | 2 | 1 |
| `xx_dot_step(...)` | `xx_dot_step` | 1 | 0 | 0 | 0 | 6 |
| `xb_dot_step(...)` | `xb_dot_step` | 1 | 0 | 0 | 0 | 4 |
| `assert(x)` | `assert` | 1 | 0 | 0 | 1 | 0 |
| `assert_digest(a, b)` | `assert_vector` | 2 | 0 | 0 | 2 | 0 |
| fn call | `call` | 1 | 0 | 0 | 0 | 1 |
| fn return | `return` | 1 | 0 | 0 | 0 | 1 |
| fn call+return overhead | -- | 2 | 0 | 0 | 0 | 2 |
| if/else overhead | -- | 3 | 0 | 0 | 2 | 0 |
| for-loop overhead | -- | 8 | 0 | 0 | 4 | 0 |

`*` U32 table rows depend on operand bit-width; 33 is the worst-case (32-bit) estimate
used by the static analyzer.

Hash table: 6 rows per hash op (Tip5 = 5 rounds + 1 setup).

---

## 8. Grammar (EBNF)

```ebnf
(* Top-level *)
file          = program_decl | module_decl ;
program_decl  = "program" IDENT use_stmt* declaration* item* ;
module_decl   = "module" IDENT use_stmt* item* ;

(* Imports *)
use_stmt      = "use" module_path ;
module_path   = IDENT ("." IDENT)* ;

(* Declarations *)
declaration   = pub_input | pub_output | sec_input | sec_ram ;
pub_input     = "pub" "input" ":" type ;
pub_output    = "pub" "output" ":" type ;
sec_input     = "sec" "input" ":" type ;
sec_ram       = "sec" "ram" ":" "{" (INTEGER ":" type ",")* "}" ;

(* Items *)
item          = const_decl | struct_def | fn_def ;
const_decl    = "pub"? "const" IDENT ":" type "=" expr ;
struct_def    = "pub"? "struct" IDENT "{" struct_fields "}" ;
struct_fields = struct_field ("," struct_field)* ","? ;
struct_field  = "pub"? IDENT ":" type ;
fn_def        = "pub"? attribute? "fn" IDENT type_params?
                "(" params? ")" ("->" type)? block ;
type_params   = "<" IDENT ("," IDENT)* ">" ;
attribute     = "#[" IDENT "(" IDENT ")" "]" ;
params        = param ("," param)* ;
param         = IDENT ":" type ;

(* Types *)
type          = "Field" | "XField" | "Bool" | "U32" | "Digest"
              | "[" type ";" array_size "]"
              | "(" type ("," type)* ")"
              | module_path ;
array_size    = INTEGER | IDENT ;

(* Blocks and Statements *)
block         = "{" statement* expr? "}" ;
statement     = let_stmt | assign_stmt | if_stmt | for_stmt
              | assert_stmt | asm_stmt | match_stmt
              | emit_stmt | seal_stmt | expr_stmt | return_stmt ;
let_stmt      = "let" "mut"? (IDENT | "(" IDENT ("," IDENT)* ")")
                (":" type)? "=" expr ;
assign_stmt   = place "=" expr ;
place         = IDENT | place "." IDENT | place "[" expr "]" ;
if_stmt       = "if" expr block ("else" block)? ;
for_stmt      = "for" IDENT "in" expr ".." expr ("bounded" INTEGER)? block ;
match_stmt    = "match" expr "{" match_arm* "}" ;
match_arm     = (literal | "_") "=>" block ;
assert_stmt   = "assert" "(" expr ")"
              | "assert_eq" "(" expr "," expr ")"
              | "assert_digest" "(" expr "," expr ")" ;
asm_stmt      = "asm" asm_effect? "{" TASM_BODY "}" ;
asm_effect    = "(" ("+" | "-") INTEGER ")" ;
emit_stmt     = "emit" IDENT "{" (IDENT ":" expr ",")* "}" ;
seal_stmt     = "seal" IDENT "{" (IDENT ":" expr ",")* "}" ;
return_stmt   = "return" expr? ;
expr_stmt     = expr ;

(* Expressions *)
expr          = literal | place | bin_op | call | struct_init
              | array_init | tuple_expr | block ;
bin_op        = expr ("+" | "*" | "==" | "<" | "&" | "^" | "/%"
              | "*." ) expr ;
call          = module_path generic_args? "(" (expr ("," expr)*)? ")" ;
generic_args  = "<" array_size ("," array_size)* ">" ;
struct_init   = module_path "{" (IDENT ":" expr ",")* "}" ;
array_init    = "[" (expr ("," expr)*)? "]" ;
tuple_expr    = "(" expr ("," expr)+ ")" ;

(* Literals *)
literal       = INTEGER | "true" | "false" ;
INTEGER       = [0-9]+ ;
IDENT         = [a-zA-Z_][a-zA-Z0-9_]* ;
comment       = "//" .* NEWLINE ;
```

---

## 9. CLI Reference

### trident build

Compile `.tri` to TASM.

```
trident build <file>                        # Output to <file>.tasm
trident build <file> -o <out.tasm>          # Custom output path
trident build <file> --target release       # Release target (cfg flags)
trident build <file> --costs                # Print cost analysis table
trident build <file> --hotspots             # Show top cost contributors
trident build <file> --hints                # Show optimization hints (H0001-H0004)
trident build <file> --annotate             # Per-line cost annotations
trident build <file> --save-costs <f.json>  # Save costs as JSON
trident build <file> --compare <f.json>     # Diff costs with previous build
```

### Other subcommands

```
trident check <file>                        # Type-check only (no TASM output)
trident check <file> --costs                # Type-check + cost analysis
trident fmt <file>                          # Format source in place
trident fmt <dir>/                          # Format all .tri in directory
trident fmt <file> --check                  # Check only (exit 1 if unformatted)
trident test <file>                         # Run #[test] functions
trident doc <file>                          # Generate docs to stdout
trident doc <file> -o <docs.md>             # Generate docs to file
trident init <name>                         # Create new program project
trident init --lib <name>                   # Create new library project
trident lsp                                 # Start LSP server
```

---

## 10. Inline Assembly

```
// Zero net stack effect (default)
asm { dup 0 add }

// Positive effect: pushes N elements
asm(+1) { push 42 }

// Negative effect: pops N elements
asm(-2) { pop 1 pop 1 }

// Multi-line
asm(-1) {
    hash
    swap 5 pop 1
    swap 4 pop 1
    swap 3 pop 1
    swap 2 pop 1
    swap 1 pop 1
}
```

The `(+N)` / `(-N)` annotation declares the net stack depth change. The compiler
trusts it to track stack layout across `asm` boundaries. Named variables are
spilled to RAM before the block executes.

Raw TASM instructions are emitted verbatim -- no parsing, validation, or
optimization by the compiler.

---

## Standard Library Modules

| Module | Key functions |
|--------|---------------|
| `std.io` | `pub_read`, `pub_write`, `divine` |
| `std.hash` | `tip5`, `sponge_init`, `sponge_absorb`, `sponge_squeeze` |
| `std.field` | `add`, `sub`, `mul`, `neg`, `inv` |
| `std.convert` | `as_u32`, `as_field`, `split` |
| `std.u32` | `log2`, `pow`, `popcount` |
| `std.assert` | `is_true`, `eq`, `digest` |
| `std.xfield` | `new`, `inv` |
| `std.mem` | `read`, `write`, `read_block`, `write_block` |
| `std.storage` | `read`, `write`, `read_digest`, `write_digest` |
| `std.merkle` | `verify1`..`verify4`, `authenticate_leaf3` |
| `std.auth` | `verify_preimage`, `verify_digest_preimage` |
| `std.kernel` | `authenticate_field`, `tree_height` |
| `std.utxo` | `authenticate` |

---

## See Also

- [Language Specification](spec.md) -- Complete language reference (sections 1-18)
- [Tutorial](tutorial.md) -- Step-by-step developer guide
- [Programming Model](programming-model.md) -- Triton VM execution model
- [Optimization Guide](optimization.md) -- Cost reduction strategies
- [Error Catalog](errors.md) -- All error messages with explanations
- [Comparative Analysis](analysis.md) -- Trident vs. Cairo, Leo, Noir, Vyper
