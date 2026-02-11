# Trident Language Reference

Write once. Prove anywhere.

**Version 0.5** | File extension: `.tri` | Compiler: `trident`

For the compiler IR see [ir.md](ir.md). For execution targets see [targets.md](targets.md).
For error explanations see [errors.md](errors.md). For a step-by-step guide see
[tutorial.md](../tutorials/tutorial.md).

---

## Agent Briefing

Machine-optimized compact format for AI code generation.

### Language Identity

```
Name:      Trident
Extension: .tri
Paradigm:  Imperative, bounded, first-order, no heap, no recursion
Domain:    Zero-knowledge provable computation
Targets:   Triton VM, Miden VM, SP1, OpenVM, Cairo (see targets.md)
Compiler:  trident build <file.tri>
All arithmetic is modular (mod p where p depends on the target).
There is no subtraction operator — use sub(a, b).
```

### File Structure

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
                        Universal — all targets
Field       1 elem      Field element (target-dependent modulus)
Bool        1 elem      Constrained to {0, 1}
U32         1 elem      Range-checked 0..2^32
Digest      D elems     Hash digest [Field; D], D = target digest width
[T; N]      N*w         Fixed array, N compile-time (supports: [Field; M+N], [Field; N*2])
(T1, T2)    w1+w2       Tuple (max 16 elements)
struct S    sum          Named product type

                        Tier 2 — extension field targets only
XField      E elems     Extension field, E = extension degree (3 on Triton, 0 = unavailable on most)
```

Digest is universal — every target has a hash function and produces digests.
The width D varies by target (5 on Triton, 4 on Miden, 8 on SP1/OpenVM, 1 on Cairo).
XField is Tier 2 only. See [targets.md](targets.md).

NO: enums, sum types, references, pointers, strings, floats, Option, Result.
NO: implicit conversions between types.

### Operators (complete)

```
                                                 Tier 1 — all targets
a + b       Field,Field -> Field     Addition mod p
a * b       Field,Field -> Field     Multiplication mod p
a == b      Field,Field -> Bool      Equality
a < b       U32,U32 -> Bool          Less-than (U32 only)
a & b       U32,U32 -> U32           Bitwise AND
a ^ b       U32,U32 -> U32           Bitwise XOR
a /% b      U32,U32 -> (U32,U32)    Divmod (quotient, remainder)

                                                 Tier 2 — XField targets only
a *. s      XField,Field -> XField   Scalar multiply
```

NO: `-`, `/`, `!=`, `>`, `<=`, `>=`, `&&`, `||`, `!`, `%`, `>>`, `<<`.
Use `sub(a, b)` for subtraction. `neg(a)` for negation. `inv(a)` for inverse.

### Declarations

```
let x: Field = 42                              // Immutable
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
fn concat<M, N>(a: [Field; M], b: [Field; N]) -> [Field; M+N] { ... }
#[pure] fn compute(x: Field) -> Field { x * x }          // No I/O allowed
#[test] fn test_add() { assert_eq(add(1, 2), 3) }        // Test
#[cfg(debug)] fn debug_helper() { }                       // Conditional
```

NO: closures, function pointers, type generics (only size generics),
default parameters, variadic arguments, method syntax.

### Builtins (complete)

```
// Tier 1 — all targets
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
// Assert
assert(cond: Bool)                      assert_eq(a: Field, b: Field)
assert_digest(a: Digest, b: Digest)
// RAM
ram_read(addr) -> Field                 ram_write(addr, val)
ram_read_block(addr) -> [Field; D]      ram_write_block(addr, vals)

// Tier 2 — provable targets (R = hash rate, D = digest width; see targets.md)
// Hash
hash(fields: Field x R) -> Digest      sponge_init()
sponge_absorb(fields: Field x R)       sponge_absorb_mem(ptr: Field)
sponge_squeeze() -> [Field; R]
// Merkle
merkle_step(idx: U32, d: Digest) -> (U32, Digest)
merkle_step_mem(ptr, idx, d) -> (Field, U32, Digest)
// Extension field (XField targets only)
xfield(x0, ..., xE) -> XField          xinvert(a: XField) -> XField
xx_dot_step(acc, ptr_a, ptr_b) -> (XField, Field, Field)
xb_dot_step(acc, ptr_a, ptr_b) -> (XField, Field, Field)
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
```

---

# Part I — Universal Language (Tier 0 + Tier 1)

Everything here works on every target. A program that uses only Part I
features compiles for Triton VM, Miden VM, SP1, OpenVM, Cairo, and any
future target.

---

## 1. Programs and Modules

Every `.tri` file starts with exactly one of:

```
program my_program      // Executable — must have fn main()
module my_module        // Library — no fn main, provides reusable items
```

### Imports

```
use merkle                      // import module
use crypto.sponge               // nested module (directory-based)
```

Rules:
- `use` imports a module by name, accessed via dot notation (`merkle.verify(...)`)
- No wildcard imports (`use merkle.*` is forbidden)
- No renaming (`use merkle as m` is forbidden)
- No re-exports — if A uses B, C cannot access B through A
- No circular dependencies — the dependency graph must be a DAG

### Visibility

Two levels only:
- **`pub`** — visible to any module that imports this one
- **default** — private to this module

No `pub(crate)`, no `friend`, no `internal`.

```
module wallet

pub struct Balance {
    pub owner: Digest,      // visible to importers
    amount: Field,          // private to this module
}

pub fn create(owner: Digest, amount: Field) -> Balance {
    Balance { owner, amount }
}
```

### Project Layout

```
my_project/
├── main.tri            // program entry point
├── merkle.tri          // module merkle
├── crypto/
│   └── sponge.tri      // module crypto.sponge
└── trident.toml        // project manifest
```

**trident.toml:**

```toml
[project]
name = "my_project"
version = "0.1.0"
entry = "main.tri"
```

---

## 2. Types

### Primitive Types

| Type | Width | Description |
|------|------:|-------------|
| `Field` | 1 | Native field element of the target VM |
| `Bool` | 1 | Field constrained to {0, 1} |
| `U32` | 1 | Unsigned 32-bit integer, range-checked |
| `Digest` | D | Hash digest `[Field; D]` — universal content identifier |

`Field` means "element of the target VM's native field." Programs reason about
field arithmetic abstractly; the target implements it concretely.

`Digest` is universal — every target has a hash function and produces digests.
It is a content identifier: the fixed-width fingerprint of arbitrary data. The
width D varies by target (5 on Triton, 4 on Miden, 8 on SP1/OpenVM, 1 on Cairo).

No implicit conversions. `Field` and `U32` do not auto-convert. Use `as_field()`
and `as_u32()` (the latter inserts a range check).

For extension field types, see [Part II: Provable Types](#11-provable-types).

### Composite Types

| Type | Width | Description |
|------|-------|-------------|
| `[T; N]` | N * width(T) | Fixed-size array, N compile-time known |
| `(T1, T2, ...)` | sum of widths | Tuple (max 16 elements) |
| `struct S { ... }` | sum of field widths | Named product type |

Array sizes support compile-time expressions: `[Field; N]`, `[Field; M+N]`,
`[Field; N*2]`.

No enums. No sum types. No references. No pointers. All values are passed by
copy on the stack. Structs are flattened to sequential stack/RAM elements.

### Type Widths

All types have a compile-time-known width measured in field elements.
Widths marked with a variable are resolved from the target configuration.

| Type | Width | Notes |
|------|-------|-------|
| `Field` | 1 | |
| `Bool` | 1 | |
| `U32` | 1 | |
| `Digest` | D | D = `digest_width` from target config |
| `[T; N]` | N * width(T) | |
| `(T1, T2)` | width(T1) + width(T2) | |
| `struct` | sum of field widths | |

---

## 3. Declarations

### Functions

```
fn private_fn(x: Field) -> Field { x + 1 }
pub fn public_fn(x: Field) -> Field { x + 1 }
```

- No default arguments, no variadic arguments
- No function overloading, no closures
- No recursion — call graph must be a DAG
- Maximum 16 parameters (stack depth)
- Tail expression is the return value

### Size-Generic Functions

```
fn sum<N>(arr: [Field; N]) -> Field {
    let mut total: Field = 0
    for i in 0..N { total = total + arr[i] }
    total
}

fn concat<M, N>(a: [Field; M], b: [Field; N]) -> [Field; M+N] { ... }
```

Size parameters appear in angle brackets. Each unique combination of size arguments
produces a monomorphized copy at compile time.

```
let a: [Field; 3] = [1, 2, 3]
let total: Field = sum(a)       // N=3 inferred from argument type
let total: Field = sum<3>(a)    // N=3 explicit
```

Only integer size parameters — no type-level generics.

### Structs

```
struct Point { x: Field, y: Field }
pub struct PubPoint { pub x: Field, pub y: Field }

let p = Point { x: 1, y: 2 }
let x: Field = p.x
```

### Events

```
event Transfer { from: Digest, to: Digest, amount: Field }
```

Events are declared at module scope. Fields must be `Field`-width types.
Maximum 9 fields. Events are emitted with `reveal` (public) or `seal`
(committed) — see [Part II: Events](#15-events).

### Constants

```
const MAX_DEPTH: U32 = 32
pub const ZERO: Field = 0
```

Inlined at compile time. No runtime cost.

### I/O Declarations (program modules only)

```
pub input:  [Field; 3]      // public input (visible to verifier)
pub output: Field            // public output
sec input:  [Field; 5]      // secret input (prover only)
sec ram: { 17: Field, 42: Field }   // pre-initialized RAM slots
```

---

## 4. Expressions and Operators

### Operator Table

| Operator | Operand types | Result type | Description |
|----------|---------------|-------------|-------------|
| `a + b` | Field, Field | Field | Field addition |
| `a + N` | Field, literal | Field | Immediate addition |
| `a * b` | Field, Field | Field | Field multiplication |
| `a == b` | Field, Field | Bool | Field equality |
| `a < b` | U32, U32 | Bool | Unsigned less-than |
| `a & b` | U32, U32 | U32 | Bitwise AND |
| `a ^ b` | U32, U32 | U32 | Bitwise XOR |
| `a /% b` | U32, U32 | (U32, U32) | Division + remainder |

No subtraction operator (`-`). No division operator (`/`). No `!=`, `>`, `<=`,
`>=`. No `&&`, `||`, `!`. Use builtins: `sub(a, b)`, `neg(a)`, `inv(a)`.

For extension field operators, see [Part II: Provable Operators](#12-provable-operators).

### Other Expressions

```
p.x                             // field access
arr[i]                          // array indexing
Point { x: 1, y: 2 }           // struct initialization
[1, 2, 3]                       // array literal
(a, b)                          // tuple literal
{ let x: Field = 1; x + 1 }    // block with tail expression
```

---

## 5. Statements

### Let Bindings

```
let x: Field = 42                          // immutable
let mut counter: U32 = 0                   // mutable
let (hi, lo): (U32, U32) = split(x)       // tuple destructuring
```

### Assignment

```
counter = counter + 1
p.x = 42
arr[i] = value
(a, b) = some_function()                   // tuple assignment
```

### If / Else

```
if condition {
    // body
} else {
    // body
}
```

No `else if` — use nested `if/else`. Condition must be `Bool` or `Field`
(0 = false, nonzero = true).

### For Loops

```
for i in 0..32 { body }               // constant bound — exactly 32 iterations
for i in 0..n bounded 64 { body }     // runtime bound — at most 64 iterations
```

All loops must have a compile-time-known or declared upper bound. This guarantees
the compiler can compute exact trace length.

No `while`. No `loop`. No `break`. No `continue`.

### Match

```
match value {
    0 => { handle_zero() }
    1 => { handle_one() }
    _ => { handle_default() }
}
```

Patterns: integer literals, `true`, `false`, struct destructuring, `_` (wildcard).
Exhaustiveness is enforced — wildcard `_` arm is required unless all values are covered.

```
// Struct pattern matching
match p {
    Point { x: 0, y } => { handle_origin_x(y) }
    Point { x, y: 0 } => { handle_origin_y(x) }
    _ => { handle_general(p.x, p.y) }
}
```

### Return

```
fn foo(x: Field) -> Field {
    if x == 0 { return 1 }
    x + x                      // tail expression — implicit return
}
```

---

## 6. Builtin Functions

### I/O and Non-Deterministic Input

| Signature | Description |
|-----------|-------------|
| `pub_read() -> Field` | Read 1 public input |
| `pub_read2()` ... `pub_read5()` | Read N public inputs |
| `pub_write(v: Field)` | Write 1 public output |
| `pub_write2(...)` ... `pub_write5(...)` | Write N public outputs |
| `divine() -> Field` | Read 1 secret input (prover only) |
| `divine3() -> (Field, Field, Field)` | Read 3 secret inputs |
| `divine5() -> Digest` | Read D secret inputs as Digest |

### Field Arithmetic

| Signature | Description |
|-----------|-------------|
| `sub(a: Field, b: Field) -> Field` | Subtraction: a + (p - b) |
| `neg(a: Field) -> Field` | Additive inverse: p - a |
| `inv(a: Field) -> Field` | Multiplicative inverse |

### U32 Operations

| Signature | Description |
|-----------|-------------|
| `split(a: Field) -> (U32, U32)` | Split field to (hi, lo) u32 pair |
| `as_u32(a: Field) -> U32` | Range-checked conversion |
| `as_field(a: U32) -> Field` | Type cast (zero cost) |
| `log2(a: U32) -> U32` | Floor of log base 2 |
| `pow(base: U32, exp: U32) -> U32` | Exponentiation |
| `popcount(a: U32) -> U32` | Hamming weight (bit count) |

### Assertions

| Signature | Description |
|-----------|-------------|
| `assert(cond: Bool)` | Crash VM if false — proof generation impossible |
| `assert_eq(a: Field, b: Field)` | Assert equality |
| `assert_digest(a: Digest, b: Digest)` | Assert digest equality |

### Memory

| Signature | Description |
|-----------|-------------|
| `ram_read(addr) -> Field` | Read 1 word |
| `ram_write(addr, val)` | Write 1 word |
| `ram_read_block(addr) -> [Field; D]` | Read D words (D = digest width) |
| `ram_write_block(addr, vals)` | Write D words |

For hash, sponge, Merkle, and extension field builtins, see
[Part II: Provable Computation](#part-ii--provable-computation-tier-2).

---

## 7. Attributes

| Attribute | Meaning |
|-----------|---------|
| `#[cfg(flag)]` | Conditional compilation |
| `#[test]` | Test function — run with `trident test` |
| `#[pure]` | No I/O side effects allowed |
| `#[intrinsic(name)]` | Maps to target instruction (std modules only) |
| `#[requires(predicate)]` | Precondition — checked by `trident verify` |
| `#[ensures(predicate)]` | Postcondition — `result` refers to return value |

```
#[pure]
fn compute(a: Field, b: Field) -> Field { a * b + a }

#[requires(amount > 0)]
#[ensures(result == sub(balance, amount))]
fn withdraw(balance: Field, amount: Field) -> Field {
    sub(balance, amount)
}

#[test]
fn test_withdraw() {
    assert_eq(withdraw(100, 50), 50)
}
```

---

## 8. Memory Model

### Stack

The operational stack has 16 directly accessible elements. The compiler manages
stack layout automatically — variables are assigned stack positions, and when more
than 16 are live, the compiler spills to RAM via an LRU policy.

The developer does not manage the stack.

### RAM

Word-addressed memory. Each cell holds one Field element.

```
ram_write(17, value)
let v: Field = ram_read(17)
```

RAM is non-deterministic on first read — if an address hasn't been written,
reading returns whatever the prover supplies. Constrain with assertions.

### No Heap

No dynamic allocation. No `alloc`, no `free`, no garbage collector. All data
structures have compile-time-known sizes. This guarantees deterministic memory
usage and predictable trace length.

---

## 9. Inline Assembly

```
asm { dup 0 add }                   // zero net stack effect (default)
asm(+1) { push 42 }                // pushes 1 element
asm(-2) { pop 1 pop 1 }            // pops 2 elements
asm(triton)(+1) { push 42 }        // target-tagged + effect
asm(miden) { dup.0 add }           // Miden VM assembly
```

Target-tagged blocks are skipped when compiling for a different target.
A bare `asm { ... }` is treated as `asm(triton) { ... }` for backward
compatibility.

The compiler does not parse, validate, or optimize assembly contents. The effect
annotation `(+N)` / `(-N)` is the contract between hand-written assembly and
the compiler's stack model.

---

## 10. Events

Events are structured data output — the universal communication mechanism.
On provable targets, events are how programs talk to the chain. On conventional
targets, they're structured logging (like `console.log`).

### Declaration

Events are declared at module scope (see [Section 3](#3-declarations)):

```
event Transfer { from: Digest, to: Digest, amount: Field }
```

Fields must be `Field`-width types. Maximum 9 fields.

### Reveal (Public Output)

```
reveal Transfer { from: sender, to: receiver, amount: value }
```

Each field is written to public output. The verifier sees all data.
`reveal` is Tier 1 — it works on every target.

### Seal (Committed Secret)

```
seal Transfer { from: sender, to: receiver, amount: value }
```

Fields are hashed via the sponge construction. Only the commitment digest is
written to public output. The verifier sees the commitment, not the data.
`seal` requires sponge support (Tier 2).

---

## 11. Type Checking Rules

- No implicit conversions between any types
- No recursion — the compiler rejects call cycles across all modules
- Exhaustive match required (wildcard or all cases covered)
- `#[pure]` functions cannot perform I/O (`pub_read`, `pub_write`, `divine`,
  `sponge_init`, etc.)
- `#[intrinsic]` only allowed in std modules
- `asm` blocks tagged for a different target are rejected
- Dead code after unconditional halt/assert is rejected
- Unused imports produce warnings

---

# Part II — Provable Computation (Tier 2)

Proof-capable targets only. No meaningful equivalent on conventional machines.

Three capabilities: cryptographic hashing (sponge + Merkle), non-deterministic
witness input, and extension field arithmetic. Programs using any Part II
feature cannot compile for Tier 1-only targets (SP1, OpenVM, Cairo).
See [targets.md](targets.md) for tier compatibility.

---

## 12. Hash and Sponge

These builtins require a target with native hash coprocessor support. The
argument counts (rate R, digest width D) are target-dependent. On Triton VM:
R = 10, D = 5. On Miden: R = 8, D = 4. See [targets.md](targets.md).

### Hash

| Signature | Description |
|-----------|-------------|
| `hash(fields: Field x R) -> Digest` | Hash R field elements into a Digest |

### Sponge

| Signature | Description |
|-----------|-------------|
| `sponge_init()` | Initialize sponge state |
| `sponge_absorb(fields: Field x R)` | Absorb R fields |
| `sponge_absorb_mem(ptr: Field)` | Absorb R fields from RAM |
| `sponge_squeeze() -> [Field; R]` | Squeeze R fields |

The sponge API enables incremental hashing of data larger than R fields.
Initialize, absorb in chunks, squeeze the result.

---

## 13. Merkle Authentication

| Signature | Description |
|-----------|-------------|
| `merkle_step(idx: U32, d: Digest) -> (U32, Digest)` | One tree level up |
| `merkle_step_mem(ptr, idx, d) -> (Field, U32, Digest)` | Tree level from RAM |

`merkle_step` authenticates one level of a Merkle tree. Call it in a loop
to verify a full Merkle path:

```
pub fn verify(root: Digest, leaf: Digest, index: U32, depth: U32) {
    let mut idx = index
    let mut current = leaf
    for _ in 0..depth bounded 64 {
        (idx, current) = merkle_step(idx, current)
    }
    assert_digest(current, root)
}
```

---

## 14. Extension Field

The extension field extends `Field` to degree E (E = 3 on Triton VM).
Only available on targets where `xfield_width > 0`.

### Type

| Type | Width | Description |
|------|------:|-------------|
| `XField` | E | Extension field element (E = `xfield_width` from target config) |

### Operator

| Operator | Operand types | Result type | Description |
|----------|---------------|-------------|-------------|
| `a *. s` | XField, Field | XField | Scalar multiplication |

### Builtins

| Signature | Description |
|-----------|-------------|
| `xfield(x0, ..., xE) -> XField` | Construct from E base field elements |
| `xinvert(a: XField) -> XField` | Multiplicative inverse |
| `xx_dot_step(acc, ptr_a, ptr_b) -> (XField, Field, Field)` | XField dot product step |
| `xb_dot_step(acc, ptr_a, ptr_b) -> (XField, Field, Field)` | Mixed dot product step |

The dot-step builtins are building blocks for inner product arguments and FRI
verification — the core of recursive proof composition.

---

# Part III — Recursive Verification (Tier 3)

Proofs that verify other proofs. **Triton VM only.**

---

## 15. Proof Composition

Tier 3 enables a program to verify another program's proof inside its own
execution. This is STARK-in-STARK recursion: the verifier circuit runs as
part of the prover's trace.

```
// Verify a proof of program_hash and use its public output
proof_block(program_hash) {
    // verification circuit runs here
    // public outputs of the inner proof become available
}
```

Tier 3 uses the extension field builtins from [Section 16](#16-extension-field-builtins)
plus dedicated IR operations:

- **ProofBlock** — Wraps a recursive verification circuit
- **FoldExt / FoldBase** — FRI folding over extension / base field
- **ExtMul / ExtInvert** — Extension field arithmetic for the verifier

See [ir.md Part I, Tier 3](ir.md) for the full list of 5 recursive operations.

Only Triton VM supports Tier 3. Programs using proof composition cannot
compile for any other target.

---

# Part IV — Reference

---

## 16. Standard Library

### Universal Modules (`std.*`)

| Module | Key functions |
|--------|---------------|
| `std.core.field` | `add`, `sub`, `mul`, `neg`, `inv` |
| `std.core.convert` | `as_u32`, `as_field`, `split` |
| `std.core.u32` | U32 arithmetic helpers |
| `std.core.assert` | `is_true`, `eq`, `digest` |
| `std.io.io` | `pub_read`, `pub_write`, `divine` |
| `std.io.mem` | `read`, `write`, `read_block`, `write_block` |
| `std.io.storage` | Persistent storage helpers |
| `std.crypto.hash` | `hash`, `sponge_init`, `sponge_absorb`, `sponge_squeeze` |
| `std.crypto.merkle` | `verify1`..`verify4`, `authenticate_leaf3` |
| `std.crypto.auth` | `verify_preimage`, `verify_digest_preimage` |

### Target Extensions (`ext.<target>.*`)

| Module | Description |
|--------|-------------|
| `ext.triton.xfield` | XField ops, `xx_dot_step`, `xb_dot_step` |
| `ext.triton.kernel` | Neptune kernel interface |
| `ext.triton.proof` | Recursive proof composition |

Importing `ext.<target>.*` binds the program to that target — the compiler
rejects cross-target imports.

---

## 17. CLI Reference

```bash
# Build
trident build <file>                    # Compile to target assembly
trident build <file> --target triton    # Target selection (default: triton)
trident build <file> --target miden     # Miden VM → .masm
trident build <file> --costs            # Print cost analysis
trident build <file> --hotspots         # Top cost contributors
trident build <file> --hints            # Optimization hints (H0001-H0004)
trident build <file> --annotate         # Per-line cost annotations
trident build <file> -o <out>           # Custom output path

# Check
trident check <file>                    # Type-check only
trident check <file> --costs            # Type-check + cost analysis

# Format
trident fmt <file>                      # Format in place
trident fmt <dir>/                      # Format all .tri in directory
trident fmt <file> --check              # Check only (exit 1 if unformatted)

# Test
trident test <file>                     # Run #[test] functions

# Verify
trident verify <file>                   # Verify #[requires]/#[ensures]
trident verify <file> --z3              # Formal verification via Z3

# Docs
trident doc <file>                      # Generate documentation
trident doc <file> -o <docs.md>         # Generate to file

# Project
trident init <name>                     # Create new program project
trident init --lib <name>               # Create new library project
trident hash <file>                     # Show function content hashes
trident lsp                             # Start LSP server
```

---

## 18. Grammar (EBNF)

```ebnf
(* Top-level *)
file          = program_decl | module_decl ;
program_decl  = "program" IDENT use_stmt* declaration* item* ;
module_decl   = "module" IDENT use_stmt* item* ;

(* Imports *)
use_stmt      = "use" module_path ;
module_path   = IDENT ("." IDENT)* ;

(* Declarations — program modules only *)
declaration   = pub_input | pub_output | sec_input | sec_ram ;
pub_input     = "pub" "input" ":" type ;
pub_output    = "pub" "output" ":" type ;
sec_input     = "sec" "input" ":" type ;
sec_ram       = "sec" "ram" ":" "{" (INTEGER ":" type ",")* "}" ;

(* Items *)
item          = const_decl | struct_def | event_def | fn_def ;
const_decl    = "pub"? "const" IDENT ":" type "=" expr ;
struct_def    = "pub"? "struct" IDENT "{" struct_fields "}" ;
struct_fields = struct_field ("," struct_field)* ","? ;
struct_field  = "pub"? IDENT ":" type ;
event_def     = "event" IDENT "{" event_fields "}" ;
event_fields  = event_field ("," event_field)* ","? ;
event_field   = IDENT ":" type ;
fn_def        = "pub"? attribute* "fn" IDENT type_params?
                "(" params? ")" ("->" type)? block ;
type_params   = "<" IDENT ("," IDENT)* ">" ;
attribute     = "#[" IDENT ("(" attr_arg ")")? "]" ;
attr_arg      = IDENT | expr ;
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
              | reveal_stmt | seal_stmt
              | expr_stmt | return_stmt ;
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
asm_stmt      = "asm" asm_annotation? "{" TASM_BODY "}" ;
asm_annotation = "(" asm_target ("," asm_effect)? ")"
               | "(" asm_effect ")" ;
asm_target    = IDENT ;
asm_effect    = ("+" | "-") INTEGER ;
reveal_stmt   = "reveal" IDENT "{" (IDENT ":" expr ",")* "}" ;
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

## 19. Permanent Exclusions

These are design decisions, not roadmap items.

| Feature | Reason |
|---------|--------|
| Strings | No string operations in any target VM ISA |
| Dynamic arrays | Unpredictable trace length |
| Heap allocation | Non-deterministic memory, no GC |
| Recursion | Unbounded trace; use bounded loops |
| Closures | Requires dynamic dispatch |
| Type-level generics | Compile-time complexity, audit difficulty |
| Operator overloading | Hides costs |
| Inheritance / Traits | Complexity without benefit |
| Exceptions | Use assert; failure = no proof |
| Floating point | Not supported by field arithmetic |
| Macros | Source-level complexity |
| Concurrency | VM is single-threaded |
| Wildcard imports | Obscures dependencies |
| Circular dependencies | Prevents deterministic compilation |

---

## 20. Common Patterns

### Read-Compute-Write (Universal)

```
fn main() {
    let a: Field = pub_read()
    let b: Field = pub_read()
    pub_write(a + b)
}
```

### Accumulator (Universal)

```
fn sum<N>(arr: [Field; N]) -> Field {
    let mut total: Field = 0
    for i in 0..N { total = total + arr[i] }
    total
}
```

### Non-Deterministic Verification (Universal)

```
fn prove_sqrt(x: Field) {
    let s: Field = divine()      // prover injects sqrt(x)
    assert(s * s == x)           // verifier checks s^2 = x
}
```

### Merkle Proof Verification (Tier 2)

```
module merkle

pub fn verify(root: Digest, leaf: Digest, index: U32, depth: U32) {
    let mut idx = index
    let mut current = leaf
    for _ in 0..depth bounded 64 {
        (idx, current) = merkle_step(idx, current)
    }
    assert_digest(current, root)
}
```

### Event Emission (Tier 2)

```
event Transfer { from: Digest, to: Digest, amount: Field }

fn process(sender: Digest, receiver: Digest, value: Field) {
    // ... validation ...
    reveal Transfer { from: sender, to: receiver, amount: value }
}
```

---

## See Also

- [IR Reference](ir.md) — Compiler intermediate representation (54 ops, 4 tiers)
- [Target Reference](targets.md) — OS model, target profiles, cost models
- [Error Catalog](errors.md) — All compiler error messages explained
- [Tutorial](../tutorials/tutorial.md) — Step-by-step developer guide
- [For Developers](../tutorials/for-developers.md) — ZK concepts for conventional programmers
- [For Blockchain Devs](../tutorials/for-blockchain-devs.md) — Mental model migration
- [Optimization Guide](../guides/optimization.md) — Cost reduction strategies

---

*Trident v0.5 — Write once. Prove anywhere.*
