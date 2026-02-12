# Trident Language Reference

[IR Reference](ir.md) | [Target Reference](targets.md) | [Error Catalog](errors.md) | [Agent Briefing](briefing.md)

Trident is a programming language for provable computation. One source
file compiles to 20 virtual machines — from zero-knowledge proof systems
to EVM, WASM, and native x86-64. Write once. Prove anywhere.

**Version 0.5** | File extension: `.tri` | Compiler: `trident`

---

# Part I — Universal Language (Tier 0 + Tier 1)

Everything here works on every target. A program that uses only Part I
features compiles for TRITON, MIDEN, SP1, OPENVM, CAIRO, and any
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
width D varies by target (5 on TRITON, 4 on MIDEN, 8 on SP1/OPENVM, 1 on CAIRO).

No implicit conversions. `Field` and `U32` do not auto-convert. Use `as_field()`
and `as_u32()` (the latter inserts a range check).

For extension field types, see [provable.md](provable.md).

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

| Type | Width |
|------|-------|
| `Field` | 1 |
| `Bool` | 1 |
| `U32` | 1 |
| `Digest` | D (`digest_width` from target config) |
| `[T; N]` | N * width(T) |
| `(T1, T2)` | width(T1) + width(T2) |
| `struct` | sum of field widths |

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

For extension field operators, see [provable.md](provable.md).

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

### Hash

| Signature | Description |
|-----------|-------------|
| `hash(fields: Field x R) -> Digest` | Hash R field elements into a Digest (R = target hash rate) |

`hash()` is the Tier 1 hash operation — available on every target. The rate R
and digest width D are target-dependent. The user-facing function name varies
by target: `std.crypto.hash.tip5()` on TRITON, with other targets providing
their native hash function. All compile to the `Hash` TIR operation internally.
See [targets.md](targets.md) for per-VM hash functions.

For sponge, Merkle, and extension field builtins (Tier 2-3), see
[provable.md](provable.md).

### Portable OS (`std.os.*`)

The `std.os.*` modules provide portable OS interaction — neuron identity,
signals, state, and time. They are not builtins (they're standard library
functions), but they compile to target-specific lowerings just like
builtins do.

| Module | Key functions | Available when |
|--------|---------------|----------------|
| `std.os.neuron` | `id() -> Digest`, `verify(expected: Digest) -> Bool`, `auth(credential: Digest) -> ()` | Target has identity |
| `std.os.signal` | `send(from: Digest, to: Digest, amount: Field)`, `balance(neuron: Digest) -> Field` | Target has native value |
| `std.os.state` | `read(key: Field) -> Field`, `write(key, value)`, `exists(key)` | Target has persistent state |
| `std.os.time` | `now() -> Field`, `block_height() -> Field` | All targets |

These sit between `std.*` (pure computation, all targets) and `ext.<os>.*`
(OS-native, one target). A program using only `std.*` + `std.os.*` compiles
to any OS that supports the required concepts. The compiler emits clear
errors when targeting an OS that lacks a concept (e.g., `std.os.neuron.id()`
on UTXO chains, `std.os.signal.send()` on journal targets).

For full API specifications, see [stdlib.md](stdlib.md). For per-OS lowering
tables, see [targets.md](targets.md).

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
asm(miden) { dup.0 add }           // MIDEN assembly
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
On provable targets, events are how programs talk to the OS. On native
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

## See Also

- [Agent Briefing](briefing.md) — AI-optimized compact cheat-sheet
- [Provable Computation](provable.md) — Hash, sponge, Merkle, extension field, proof composition (Tier 2-3)
- [Standard Library](stdlib.md) — `std.*` modules and OS extensions (`ext.<os>.*`)
- [CLI Reference](cli.md) — Compiler commands and flags
- [Grammar](grammar.md) — EBNF grammar
- [Patterns and Exclusions](patterns.md) — Common patterns and permanent exclusions
- [IR Reference](ir.md) — Compiler intermediate representation (54 ops, 4 tiers)
- [Target Reference](targets.md) — OS model, target profiles, cost models
- [Error Catalog](errors.md) — All compiler error messages explained
- [Tutorial](../tutorials/tutorial.md) — Step-by-step developer guide

---

*Trident v0.5 — Write once. Prove anywhere.*
