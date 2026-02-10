# Writing a Program

This is the first stage of the Trident program lifecycle: **Writing** > Compiling > Running > Deploying > Generating Proofs > Verifying Proofs. It covers everything you need to write a valid Trident program -- file structure, types, control flow, functions, modules, and the key differences from conventional languages. For a step-by-step walkthrough, see the [Tutorial](tutorial.md). For a complete lookup table, see the [Reference](reference.md).

---

## Program Structure

Every `.tri` file starts with either a `program` or a `module` declaration.

A **program** has an entry point (`fn main()`) and compiles to an executable:

```
program my_app

fn main() {
    let x: Field = pub_read()
    pub_write(x + x)
}
```

A **module** is a library with no entry point. Its public items are available to other files:

```
module helpers

pub fn double(x: Field) -> Field {
    x + x
}
```

A project has exactly one `program` file and zero or more `module` files. The typical layout:

```
my_project/
  trident.toml    # Project configuration
  main.tri        # program (entry point)
  helpers.tri     # module
  crypto/
    auth.tri      # module (use crypto.auth)
```

---

## Types

All types have compile-time known widths measured in field elements. There are no dynamically sized types.

| Type | Width | Description |
|------|------:|-------------|
| `Field` | 1 | Base field element mod p (Goldilocks: p = 2^64 - 2^32 + 1) |
| `Bool` | 1 | Field constrained to 0 or 1 |
| `U32` | 1 | Unsigned 32-bit integer, range-checked by the VM |
| `Digest` | 5 | Tip5 hash digest (5 field elements) |
| `XField` | 3 | Extension field element (Triton VM target) |
| `[T; N]` | N * width(T) | Fixed-size array, N known at compile time |
| `(T, U)` | width(T) + width(U) | Tuple (up to 16 elements) |
| `struct S` | sum of field widths | Named product type |

`Field` is the native type -- the one the VM operates on directly. Everything else is built from field elements. See the [Reference](reference.md) for the complete type table, operators, and cost per instruction.

### Structs

Define named data types with `struct`:

```
struct Account {
    pub id: Field,
    pub balance: Field,
    nonce: Field,
}

fn new_account(id: Field) -> Account {
    Account { id: id, balance: 0, nonce: 0 }
}

fn get_balance(acc: Account) -> Field {
    acc.balance
}
```

Fields marked `pub` are accessible from other modules. Unmarked fields are private to the defining module.

---

## Variables and Expressions

### Let Bindings

Variables are immutable by default:

```
let x: Field = 42
let flag: Bool = true
let arr: [Field; 3] = [1, 2, 3]
```

Use `mut` for mutable variables:

```
let mut counter: Field = 0
counter = counter + 1
```

### Constants

Module-level constants are inlined at every use site:

```
const MAX_DEPTH: U32 = as_u32(32)
pub const ZERO: Field = 0
```

### Tuple Destructuring

```
let (quot, rem) = divmod(17, 5)
```

### Operators

Field arithmetic uses `+` and `*`. There is no `-` operator -- in a prime field, `1 - 2` produces `p - 1`, not `-1`. Subtraction is explicit via `std.core.field.sub`:

```
use std.core.field

let sum: Field = a + b
let product: Field = a * b
let difference: Field = std.core.field.sub(a, b)
```

Comparisons produce `Bool`:

```
let equal: Bool = a == b       // Field or U32
let less: Bool = x < y         // U32 only
```

There are no `!=`, `>`, `<=`, or `>=` operators. Compose them from `==`, `<`, and `not()`. There are no `&&` or `||` -- use boolean combinators from `std.core.bool`. This is deliberate: fewer operators means fewer things to audit in provable code.

---

## Control Flow

### If / Else

```
if condition {
    do_something()
} else {
    do_other()
}
```

If/else works as an expression (tail expression returns a value):

```
let result: Field = if flag { 1 } else { 0 }
```

There is no `else if`. Nest instead:

```
if a {
    handle_a()
} else {
    if b {
        handle_b()
    } else {
        handle_default()
    }
}
```

### Bounded For Loops

All loops require a compile-time bound:

```
for i in 0..10 bounded 10 {
    process(i)
}
```

**Why bounds are required.** Provable VMs execute a fixed-length trace. The prover must know the worst-case iteration count before execution begins. The `bounded N` annotation declares this maximum. The compiler uses the bound -- not the runtime count -- to compute proving cost, so `bounded 100` always costs 100 iterations in the trace even if the loop exits earlier.

Dynamic ranges work with `bounded`:

```
let n: Field = pub_read()
for i in 0..n bounded 100 {
    // runs at most 100 iterations
    process(i)
}
```

When the range is a constant (e.g., `0..10`), the bound can be omitted -- the compiler infers it:

```
for i in 0..10 {
    process(i)
}
```

### Match Expressions

Pattern matching over integer and boolean values:

```
match op_code {
    0 => { handle_pay() }
    1 => { handle_lock() }
    2 => { handle_update() }
    _ => { reject() }
}
```

The wildcard `_` arm is required unless all values are covered. For `Bool`, covering both arms is sufficient:

```
match flag {
    true  => { accept() }
    false => { reject() }
}
```

### Early Return

```
fn early_exit(x: Field) -> Field {
    if x == 0 {
        return 0
    }
    x * x
}
```

There is no `while`, `loop`, `break`, or `continue`.

---

## Functions

### Declaration

Functions are declared with `fn`. The last expression in the body is the return value:

```
fn add_three(a: Field, b: Field, c: Field) -> Field {
    a + b + c
}
```

Functions with no return value omit the `->` annotation:

```
fn log_value(x: Field) {
    pub_write(x)
}
```

### Visibility

Functions are private by default. Mark them `pub` to export from a module:

```
module utils

pub fn public_fn() -> Field { 42 }    // accessible from other modules
fn private_fn() -> Field { 99 }        // internal only
```

### Multiple Return Values

Return tuples and destructure at the call site:

```
fn divmod(a: Field, b: Field) -> (Field, Field) {
    a /% b
}

let (q, r) = divmod(17, 5)
```

### Size-Generic Functions

Functions can be generic over array sizes using `<N>`:

```
fn sum<N>(arr: [Field; N]) -> Field {
    let mut total: Field = 0
    for i in 0..N bounded N {
        total = total + arr[i]
    }
    total
}
```

The size parameter `N` is inferred from the argument or specified explicitly:

```
let a: [Field; 3] = [1, 2, 3]
let s: Field = sum(a)           // N inferred as 3
let t: Field = sum<5>(b)        // N specified as 5
```

Size generics are monomorphized at compile time -- each distinct `N` produces a separate function in the output.

---

## Modules and Imports

### Use Declarations

Import modules with `use`:

```
program my_app

use helpers
use crypto.auth
use std.crypto.hash
```

Call functions with the module prefix:

```
let d: Digest = std.crypto.hash.tip5(x, 0, 0, 0, 0, 0, 0, 0, 0, 0)
let result: Field = helpers.double(x)
```

### Module Resolution

| Import | Resolves to |
|--------|-------------|
| `use helpers` | `helpers.tri` in the project directory |
| `use crypto.auth` | `crypto/auth.tri` in the project directory |
| `use std.crypto.hash` | `crypto/hash.tri` in the standard library |
| `use ext.triton.xfield` | `triton/xfield.tri` in the extensions directory |

### Standard Library Layers

The standard library is organized in three universal layers plus backend extensions:

| Layer | Modules | Purpose |
|-------|---------|---------|
| `std.core` | `field`, `convert`, `u32`, `assert`, `bool` | Arithmetic, conversions, assertions |
| `std.io` | `io`, `mem`, `storage` | Public/secret I/O, RAM, persistent storage |
| `std.crypto` | `hash`, `merkle`, `auth` | Tip5 hashing, Merkle proofs, authorization |
| `ext.triton` | `xfield`, `kernel`, `utxo`, `storage` | Triton VM-specific operations |

The `std.*` modules are target-agnostic and work across all backends. The `ext.triton.*` modules are available only when compiling with `--target triton` (the default). Importing an `ext.*` module while targeting a different backend is a compile error.

See the [Reference](reference.md) for a complete list of standard library functions, and the [Programming Model](programming-model.md) for how I/O interacts with the prover and verifier.

---

## Events

Events record structured data in the proof trace. Declare the event, then `emit` or `seal` it.

### Declaration

```
event Transfer {
    from: Digest,
    to: Digest,
    amount: Field,
}
```

### Emit (Open)

All fields are visible to the verifier:

```
emit Transfer {
    from: sender,
    to: receiver,
    amount: value,
}
```

### Seal (Hashed)

Fields are hashed; only the digest is visible to the verifier:

```
seal Transfer {
    from: sender,
    to: receiver,
    amount: value,
}
```

Use `emit` for public audit trails. Use `seal` when field values must remain private but their commitment must be verifiable.

---

## Inline Assembly

For operations not covered by the language, embed raw TASM instructions in `asm` blocks.

### Basic Form

The effect annotation (`+N` or `-N`) declares the net stack depth change. The compiler trusts it to track stack layout:

```
fn custom_op(a: Field, b: Field) -> Field {
    asm(-1) {
        add
    }
}
```

`asm(-1)` means the block consumes one net element (two inputs become one output via `add`). An incorrect annotation produces broken output -- the compiler does not validate the contents of `asm` blocks.

### Target-Tagged Blocks

For multi-target projects, tag the block with a backend name. Blocks for non-active targets are silently skipped:

```
fn target_specific(a: Field, b: Field) -> Field {
    asm(triton, -1) {
        add
    }
}
```

A bare `asm { ... }` (no target tag) is treated as `asm(triton) { ... }` for backward compatibility. In multi-target projects, prefer the explicit tag.

### Combining with Stack Effects

```
asm(triton, +1) { push 42 }         // pushes one element
asm(-2) { pop 1 pop 1 }             // pops two elements
asm { dup 0 add }                    // zero net effect (default)
```

Named variables in scope are spilled to RAM before the block executes and restored after. See the [Reference](reference.md) for the full TASM instruction set mapping.

---

## Key Differences from Conventional Languages

These are not limitations -- they are properties required for provable computation.

| Conventional expectation | Trident | Why |
|--------------------------|---------|-----|
| Heap allocation | No heap. All data is stack or RAM with static addressing. | The prover must know memory layout at trace generation time. |
| Recursion | No recursion. Use bounded loops. | Recursive call depth is unbounded, which prevents static trace sizing. |
| Unbounded loops | All loops require a `bounded` annotation. | The proof trace has a fixed length determined before execution. |
| Strings | No string type. | Strings are variable-length; all types must have compile-time known widths. |
| Floating point | No floats. `Field` is the native numeric type. | The VM operates over a prime field. Floats have no representation. |
| Subtraction operator | No `-`. Use `std.core.field.sub()`. | `1 - 2` in a prime field is `p - 1`, not `-1`. Explicit subtraction prevents this footgun. |
| Many comparison operators | Only `==` and `<`. No `!=`, `>`, `<=`, `>=`. | Fewer primitives means a smaller, more auditable instruction set. |
| Garbage collection | No GC. All lifetimes are lexical. | There is no runtime; the program is a static trace. |

These constraints make every Trident program a fixed, bounded computation -- exactly what a STARK prover requires. For a deeper explanation of the execution model, see the [Programming Model](programming-model.md). For zero-knowledge concepts explained from first principles, see [For Developers](for-developers.md).

---

## Next Step

Once your program compiles with `trident check`, move to [Compiling a Program](compiling-a-program.md) to learn about build targets, cost analysis, and optimization flags.
