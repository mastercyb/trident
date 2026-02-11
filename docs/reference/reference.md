# Trident Language Reference

Version 0.3 -- Definitive reference for the Trident provable-computation language.

File extension: `.tri` | Compiler: `trident` | Default target: Triton VM (Goldilocks field, p = 2^64 - 2^32 + 1)

For full details see [spec.md](spec.md). For a step-by-step guide see [tutorial.md](tutorial.md).
For cost reduction strategies see [optimization.md](optimization.md). For error explanations see [errors.md](errors.md).

---

## LLM Quick Reference

Machine-optimized compact format for AI code generation. Each subsection is
self-contained with complete code patterns.

### Language Identity

```
Name:      Trident
Extension: .tri
Paradigm:  Imperative, bounded, first-order, no heap, no recursion
Domain:    Zero-knowledge provable computation
Field:     Goldilocks (p = 2^64 - 2^32 + 1) on Triton VM target
Compiler:  trident build <file.tri>
All arithmetic is modular (mod p). There is no subtraction operator.
```

### File Structure

Every `.tri` file starts with exactly one of:
```
program <name>      // Executable (has fn main)
module <name>       // Library (no fn main)
```

Then imports, then items (constants, structs, events, functions).

```
program my_program

use std.crypto.hash
use std.io.mem

const MAX: U32 = 32

struct Point { x: Field, y: Field }

event Transfer { from: Digest, to: Digest, amount: Field }

fn helper(a: Field) -> Field { a + 1 }

fn main() {
    let x: Field = pub_read()
    pub_write(helper(x))
}
```

### Types (complete)

```
Field       1 elem   Field element mod p
Bool        1 elem   Constrained to {0, 1}
U32         1 elem   Range-checked 0..2^32
XField      3 elems  Extension field (Triton)
Digest      5 elems  Hash digest [Field; 5]
[T; N]      N*w      Fixed array, N compile-time (supports const generic exprs: [Field; M+N])
(T1, T2)    w1+w2    Tuple (max 16 elements)
struct S    sum      Named product type
```

NO: enums, sum types, references, pointers, strings, floats, Option, Result.
NO: implicit conversions between types.

### Operators (complete)

```
a + b       Field,Field -> Field     Addition mod p
a * b       Field,Field -> Field     Multiplication mod p
a == b      Field,Field -> Bool      Equality
a < b       U32,U32 -> Bool          Less-than (U32 only)
a & b       U32,U32 -> U32           Bitwise AND
a ^ b       U32,U32 -> U32           Bitwise XOR
a /% b      U32,U32 -> (U32,U32)    Divmod (quotient, remainder)
a *. s      XField,Field -> XField   Scalar multiply
```

NO: `-`, `/`, `!=`, `>`, `<=`, `>=`, `&&`, `||`, `!`, `%`, `>>`, `<<`.
Use `sub(a, b)` for subtraction. `neg(a)` for negation. `inv(a)` for inverse.
`(a == b) == false` for not-equal. `b < a` for greater-than (U32 only).

### Declarations

```
let x: Field = 42                              // Immutable (type annotation required)
let mut counter: U32 = 0                       // Mutable
let (hi, lo): (U32, U32) = split(x)           // Tuple destructuring
```

### Control Flow

```
if condition { body } else { body }            // No else-if; nest instead
for i in 0..32 { body }                        // Constant bound
for i in 0..n bounded 64 { body }             // Runtime bound, declared max
match value { 0 => { } 1 => { } _ => { } }    // Integer/bool/struct patterns + wildcard
return expr                                     // Explicit return or tail expression
```

NO: `while`, `loop`, `break`, `continue`, `else if`, recursion.

### Functions

```
fn add(a: Field, b: Field) -> Field { a + b }            // Private
pub fn add(a: Field, b: Field) -> Field { a + b }        // Public
fn sum<N>(arr: [Field; N]) -> Field { ... }               // Size-generic
fn concat<M, N>(a: [Field; M], b: [Field; N]) -> [Field; M+N] { ... }  // Const generic expr
#[pure] fn compute(x: Field) -> Field { x * x }          // No I/O allowed
#[test] fn test_add() { assert_eq(add(1, 2), 3) }        // Test
#[cfg(debug)] fn debug_helper() { }                       // Conditional
```

NO: closures, function pointers, type generics (only size generics),
default parameters, variadic arguments, method syntax.

### Builtins (complete)

```
// I/O
pub_read() -> Field                    pub_write(v: Field)
pub_read{2,3,4,5}()                    pub_write{2,3,4,5}(...)
divine() -> Field                      divine3() -> (Field,Field,Field)
divine5() -> Digest

// Field arithmetic
sub(a: Field, b: Field) -> Field       neg(a: Field) -> Field
inv(a: Field) -> Field

// U32
split(a: Field) -> (U32, U32)          as_u32(a: Field) -> U32
as_field(a: U32) -> Field              log2(a: U32) -> U32
pow(base: U32, exp: U32) -> U32        popcount(a: U32) -> U32

// Hash
hash(a..j: Field x10) -> Digest        sponge_init()
sponge_absorb(a..j: Field x10)         sponge_absorb_mem(ptr: Field)
sponge_squeeze() -> [Field; 10]

// Merkle
merkle_step(idx: U32, d: Digest) -> (U32, Digest)
merkle_step_mem(ptr, idx, d) -> (Field, U32, Digest)

// Assert
assert(cond: Bool)                      assert_eq(a: Field, b: Field)
assert_digest(a: Digest, b: Digest)

// RAM
ram_read(addr) -> Field                 ram_write(addr, val)
ram_read_block(addr) -> [Field; 5]      ram_write_block(addr, vals)

// Extension field (Triton only)
xfield(x0, x1, x2) -> XField           xinvert(a: XField) -> XField
xx_dot_step(acc, ptr_a, ptr_b) -> (XField, Field, Field)
xb_dot_step(acc, ptr_a, ptr_b) -> (XField, Field, Field)
```

### Structs and Events

```
struct Config { max_depth: U32, root: Digest }
pub struct PubConfig { pub max_depth: U32, pub root: Digest }
let cfg = Config { max_depth: 32, root: my_digest }
let d: U32 = cfg.max_depth

event Transfer { from: Digest, to: Digest, amount: Field }
emit Transfer { from: sender, to: receiver, amount: value }    // Public
seal Transfer { from: sender, to: receiver, amount: value }    // Private (hash commitment)
```

### Annotations

```
#[pure]                                // No I/O side effects
#[test]                                // Test function
#[cfg(debug)]                          // Conditional compilation
#[requires(amount > 0)]               // Precondition (verified with trident verify)
#[ensures(result == balance - amount)] // Postcondition
```

### Inline Assembly

```
asm { dup 0 add }                      // Zero net stack effect
asm(+1) { push 42 }                   // Pushes 1 element
asm(-2) { pop 1 pop 1 }               // Pops 2 elements
asm(triton)(+1) { push 42 }           // Target-tagged + effect
```

### Common Errors to Avoid

```
WRONG: a - b           ->  sub(a, b)
WRONG: a / b           ->  a * inv(b)
WRONG: a != b          ->  (a == b) == false
WRONG: a > b           ->  b < a  (U32 only)
WRONG: while cond {}   ->  for i in 0..n bounded N {}
WRONG: let x = 5       ->  let x: Field = 5  (type required)
WRONG: else if          ->  else { if ... }
WRONG: recursive calls  ->  not allowed; call graph must be acyclic
WRONG: pub_read() + pub_read()  ->  bind each to let first
WRONG: for i in 0..n {}         ->  must declare: bounded N
```

### Common Patterns

```
// Read-compute-write
fn main() {
    let a: Field = pub_read()
    let b: Field = pub_read()
    pub_write(a + b)
}

// Accumulator
fn sum<N>(arr: [Field; N]) -> Field {
    let mut total: Field = 0
    for i in 0..N { total = total + arr[i] }
    total
}

// Nested conditionals (no else-if)
fn classify(x: U32) -> Field {
    if x < 10 { 1 } else { if x < 100 { 2 } else { 3 } }
}

// Match dispatch
fn dispatch(op: Field, a: Field, b: Field) -> Field {
    match op {
        0 => { a + b }
        1 => { a * b }
        2 => { sub(a, b) }
        _ => { assert(false) 0 }
    }
}
```

---

## 1. Types

| Type | Width (field elements) | Description | Literal examples |
|------|----------------------:|-------------|------------------|
| `Field` | 1 | Base field element mod p | `0`, `42`, `18446744069414584321` |
| `XField` | 3 | Extension field element (F_p[X]/<X^3-X+1>) -- Triton VM target | `xfield(1, 2, 3)` |
| `Bool` | 1 | Field constrained to {0, 1} | `true`, `false` |
| `U32` | 1 | Unsigned 32-bit integer (range-checked) | `0`, `4294967295` |
| `Digest` | 5 | Tip5 hash digest ([Field; 5]) | `divine5()` |
| `[T; N]` | N * width(T) | Fixed-size array, N compile-time known. Supports const generic expressions: `[Field; M+N]`, `[Field; N*2]` | `[1, 2, 3]` |
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
`not()`. No `&&`, `||`, `!` -- use boolean combinators from `std.core.bool`.

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

Also available as `std.core.field.inv`, `std.core.field.neg`, `std.core.field.sub`, `std.core.field.mul`, `std.core.field.add`.

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

### Merkle (Triton VM target)

Native Merkle tree instructions. On other targets, use `std.crypto.merkle` which
provides a portable implementation via hash-loop fallback.

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

### Extension Field (Triton VM target)

These builtins are specific to Triton VM's cubic extension field. In multi-target
projects, access them via `ext.triton.xfield`.

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

// Match (integer/bool patterns, struct patterns, wildcard)
match value {
    0 => { handle_zero() }
    1 => { handle_one() }
    _ => { handle_default() }
}

// Struct pattern matching
match p {
    Point { x: 0, y } => { handle_origin_x(y) }
    Point { x, y: 0 } => { handle_origin_y(x) }
    _ => { handle_general(p.x, p.y) }
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

// Const generic expressions in signatures
fn concat<M, N>(a: [Field; M], b: [Field; N]) -> [Field; M+N] { ... }
fn double<N>(arr: [Field; N]) -> [Field; N*2] { ... }

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

// Annotations
#[pure]
fn compute(a: Field, b: Field) -> Field { a * b + a }

#[test]
fn test_something() { assert(1 == 1) }

#[cfg(debug)]
fn debug_only() { ... }

// Specification annotations (verified with trident verify)
#[requires(amount > 0)]
#[ensures(result == sub(balance, amount))]
fn withdraw(balance: Field, amount: Field) -> Field {
    sub(balance, amount)
}
```

Visibility: `pub` (cross-module) or default (private). Two levels only.

The `#[pure]` annotation enforces that a function performs no I/O side effects
(no `pub_read`, `pub_write`, `divine`, `sponge_init`, etc.). This enables
more aggressive formal verification reasoning.

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
| `asm(target) { ... }` | verbatim target assembly | varies |

---

## 7. Cost Per Instruction

The cost table below shows Triton VM proving costs. Each instruction contributes
rows to multiple [Triton VM](https://triton-vm.org/) tables simultaneously.
Proving cost is determined by the **tallest** table (padded to next power of 2).
Other targets have different cost models -- use `trident build --target <t> --costs`
to see target-specific costs. See [How STARK Proofs Work](stark-proofs.md) Section 4
for why there are six tables, and the [Optimization Guide](optimization.md) for
strategies to reduce the dominant table.

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

## 8. Proof Composition

Trident supports recursive proof verification through the `ext.triton.proof` module.
This enables proof-of-proof composition: verifying STARK proofs inside STARK proofs.

### Key Types and Functions

| Function | Description |
|----------|-------------|
| `proof.parse_claim() -> Claim` | Read a Claim (program digest + I/O counts) from public input |
| `proof.hash_public_io(claim) -> Digest` | Hash all public I/O into a binding commitment |
| `proof.derive_fiat_shamir_seed(claim, io_hash) -> Digest` | Compute initial Fiat-Shamir challenge |
| `proof.fri_verify(commitment, seed, rounds) -> Digest` | Run full FRI verification chain |
| `proof.verify_ood(seed) -> (Digest, Digest)` | Verify out-of-domain evaluation |
| `proof.combine_constraints(ptr_c, ptr_w, num) -> Digest` | Inner product for AIR constraint combination |
| `proof.verify_inner_proof(num_fri_rounds)` | Verify a single inner proof end-to-end |
| `proof.aggregate_proofs(num_proofs, num_fri_rounds)` | Verify N inner proofs sequentially |

### Neptune Transaction Validation

Neptune transactions use recursive proof composition. Each transaction carries
STARK proofs for lock scripts (input authorization) and type scripts (conservation
laws). The transaction validator verifies these proofs recursively:

```
use ext.triton.proof
use ext.triton.kernel

fn verify_lock_scripts(num_inputs: Field, num_fri_rounds: Field) {
    for i in 0..num_inputs bounded 16 {
        proof.verify_inner_proof(num_fri_rounds)
    }
}

fn verify_type_scripts(num_type_scripts: Field, num_fri_rounds: Field) {
    for i in 0..num_type_scripts bounded 8 {
        proof.verify_inner_proof(num_fri_rounds)
    }
}
```

See `examples/neptune/transaction_validation.tri` for the complete implementation.

---

## 9. Inline Assembly

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

### Target-Tagged Assembly Blocks

For multi-target builds, `asm` blocks can be tagged with a target name so the
compiler knows which backend should process them:

```
// Triton VM assembly (explicit target tag)
asm(triton) {
    dup 0
    add
    swap 5 pop 1
}

// Miden VM assembly
asm(miden) {
    dup.0
    add
    movdn.5 drop
}

// Cairo assembly
asm(cairo) {
    [ap] = [ap-1] + [ap-2]; ap++
}
```

A bare `asm { ... }` (no target tag) is treated as `asm(triton) { ... }` for
backward compatibility. In multi-target projects, bare `asm` blocks emit a
deprecation warning -- use `asm(triton) { ... }` explicitly instead.

Target-tagged blocks can be combined with stack-effect annotations:

```
asm(triton)(+1) { push 42 }
```

Blocks tagged for a different target than the current `--target` are silently
skipped during compilation. Combine with `#[cfg(target)]` guards for
conditional logic around the assembly.

---

## 10. CLI Reference

### trident build

Compile `.tri` to TASM.

```
trident build <file>                        # Output to <file>.tasm
trident build <file> -o <out.tasm>          # Custom output path
trident build <file> --target triton        # Compile for Triton VM (default)
trident build <file> --target miden         # Compile for Miden VM
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
trident verify <file>                       # Symbolic verification
trident verify <file> --json                # JSON verification report
trident verify <file> --z3                  # Formal verification via Z3
trident doc <file>                          # Generate docs to stdout
trident doc <file> -o <docs.md>             # Generate docs to file
trident hash <file>                         # Show function content hashes
trident generate <spec.tri>                 # Generate scaffold from spec
trident init <name>                         # Create new program project
trident init --lib <name>                   # Create new library project
trident lsp                                 # Start LSP server
```

---

## 11. Standard Library Modules

### Universal (`std.*`)

| Module | Key functions |
|--------|---------------|
| `std.core.field` | `add`, `sub`, `mul`, `neg`, `inv` |
| `std.core.convert` | `as_u32`, `as_field`, `split` |
| `std.core.u32` | `log2`, `pow`, `popcount` |
| `std.core.assert` | `is_true`, `eq`, `digest` |
| `std.io.io` | `pub_read`, `pub_write`, `divine` |
| `std.io.mem` | `read`, `write`, `read_block`, `write_block` |
| `std.io.storage` | `read`, `write`, `read_digest`, `write_digest` |
| `std.crypto.hash` | `hash`, `sponge_init`, `sponge_absorb`, `sponge_squeeze` |
| `std.crypto.merkle` | `verify1`..`verify4`, `authenticate_leaf3` |
| `std.crypto.auth` | `verify_preimage`, `verify_digest_preimage` |

### Triton VM Extensions (`ext.triton.*`)

These modules are available only when compiling with `--target triton` (the default).
Programs that import `ext.triton.*` are bound to the Triton VM backend.

| Module | Key functions / types |
|--------|----------------------|
| `ext.triton.xfield` | `XField` type, `xx_add`, `xx_mul`, `x_invert`, `xx_dot_step`, `xb_dot_step` |
| `ext.triton.kernel` | `authenticate_field`, `tree_height` (Neptune kernel interface) |
| `ext.triton.utxo` | `authenticate` (UTXO verification) |
| `ext.triton.proof` | `verify_inner_proof`, `aggregate_proofs`, `parse_claim` (proof composition) |

Import with `use ext.triton.xfield`, etc. The compiler enforces target
consistency -- importing an `ext.triton.*` module while targeting a different
backend is a compile error.

---

## 12. Grammar (EBNF)

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
item          = const_decl | struct_def | fn_def | event_def ;
const_decl    = "pub"? "const" IDENT ":" type "=" expr ;
struct_def    = "pub"? "struct" IDENT "{" struct_fields "}" ;
struct_fields = struct_field ("," struct_field)* ","? ;
struct_field  = "pub"? IDENT ":" type ;
fn_def        = "pub"? attribute* "fn" IDENT type_params?
                "(" params? ")" ("->" type)? block ;
event_def     = "event" IDENT "{" struct_fields "}" ;
type_params   = "<" IDENT ("," IDENT)* ">" ;
attribute     = "#[" IDENT ("(" attr_args ")")? "]" ;
attr_args     = IDENT | expr ;
params        = param ("," param)* ;
param         = IDENT ":" type ;

(* Types *)
type          = "Field" | "XField" | "Bool" | "U32" | "Digest"
              | "[" type ";" array_size "]"
              | "(" type ("," type)* ")"
              | module_path ;
array_size    = const_expr ;
const_expr    = INTEGER | IDENT | const_expr ("+" | "*") const_expr ;

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
match_arm     = pattern "=>" block ;
pattern       = literal | "_" | struct_pattern ;
struct_pattern = IDENT "{" (IDENT (":" (literal | IDENT))? ",")* "}" ;
assert_stmt   = "assert" "(" expr ")"
              | "assert_eq" "(" expr "," expr ")"
              | "assert_digest" "(" expr "," expr ")" ;
asm_stmt      = "asm" asm_target? asm_effect? "{" TASM_BODY "}" ;
asm_target    = "(" IDENT ")" ;
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
generic_args  = "<" const_expr ("," const_expr)* ">" ;
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

## 13. Project Structure

```
my_project/
  trident.toml        # Project manifest
  main.tri            # Entry point (program)
  utils.tri           # Helper module
  std/                # Standard library (auto-resolved)
  ext/triton/         # Triton-specific extensions
```

trident.toml:
```toml
[project]
name = "my_project"
version = "0.1.0"
entry = "main.tri"
```

---

## See Also

- [Language Specification](spec.md) -- Complete language reference (sections 1-18)
- [Tutorial](tutorial.md) -- Step-by-step developer guide
- [Compiling a Program](compiling-a-program.md) -- Build pipeline, CLI flags, cost analysis
- [Programming Model](programming-model.md) -- Execution model (Triton VM default)
- [Universal Design](universal-design.md) -- Multi-target architecture and backend extensions
- [Formal Verification](formal-verification.md) -- `#[requires]`, `#[ensures]`, `#[pure]` and the verification pipeline
- [Content-Addressed Code](content-addressed.md) -- Function hashing, UCM codebase manager, verification caching
- [Optimization Guide](optimization.md) -- Cost reduction strategies
- [How STARK Proofs Work](stark-proofs.md) -- Section 4 (six tables), Section 11 (proving cost formula)
- [Error Catalog](errors.md) -- All error messages with explanations
- [For Developers](for-developers.md) -- Zero-knowledge concepts for conventional programmers
- [For Blockchain Devs](for-blockchain-devs.md) -- Mental model migration from Solidity/Anchor/CosmWasm
- [Vision](vision.md) -- Why Trident exists and what you can build
- [Comparative Analysis](analysis.md) -- Trident vs. Cairo, Leo, Noir, Vyper
