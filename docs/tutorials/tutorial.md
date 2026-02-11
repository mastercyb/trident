# Trident Tutorial

This is the first stage of the Trident program lifecycle: **Writing** > [Compiling](compiling-a-program.md) > [Running](running-a-program.md) > [Deploying](deploying-a-program.md) > [Generating Proofs](generating-proofs.md) > [Verifying Proofs](verifying-proofs.md).

This tutorial covers everything you need to write a valid Trident program -- file structure, types, control flow, functions, modules, and the key differences from conventional languages. For a complete lookup table, see the [Reference](reference.md). For a formal treatment, see the [Language Specification](spec.md).

---

## Prerequisites

Build the compiler from source:

```bash
cd trident
cargo build --release
```

The binary is at `target/release/trident`. Add it to your PATH or use it directly.

---

## 1. Your First Program

Create a file `hello.tri`:

```
program hello

fn main() {
    let a: Field = pub_read()
    let b: Field = pub_read()
    pub_write(a + b)
}
```

This program reads two public field elements, adds them, and writes the result. The verifier sees both inputs and the output. For a deeper explanation of how public I/O interacts with the prover and verifier, see the [Programming Model](programming-model.md).

Build it:

```bash
trident build hello.tri --target triton -o hello.tasm
```

This compiles Trident source to [TASM](https://triton-vm.org/spec/) (Triton Assembly) -- the instruction set of [Triton VM](https://triton-vm.org/). The output `hello.tasm` is what the VM executes and proves. See [Compiling a Program](compiling-a-program.md) for the full build pipeline.

Check it (type-check without emitting TASM):

```bash
trident check hello.tri
```

---

## 2. Program Structure

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

## 3. Types

All types have compile-time known widths measured in field elements. There are no dynamically sized types. See the [Reference](reference.md) for the complete type table, operators, and cost per instruction.

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

`Field` is the native type -- the one the VM operates on directly. Everything else is built from field elements.

### Field

The base type. A prime field element modulo p = 2^64 - 2^32 + 1 (the [Goldilocks prime](https://xn--2-umb.com/22/goldilocks/)). Supports `+`, `*`, `==`.

```
let x: Field = 42
let y: Field = x + x
```

There is no `-` operator. Use `sub(a, b)` from `std.core.field`. This is deliberate -- in a prime field, `1 - 2` gives `p - 1`, not `-1`. Making subtraction explicit avoids this footgun (see the [Key Differences](#17-key-differences-from-conventional-languages) table at the end):

```
program example

use std.core.field

fn main() {
    let diff: Field = std.core.field.sub(10, 3)
    pub_write(diff)
}
```

### Bool

Boolean values. `true` or `false`. Produced by `==` and `<` comparisons.

```
let flag: Bool = x == y
if flag {
    // ...
}
```

### U32

Unsigned 32-bit integer. Range-checked by the VM. Supports `+`, `*`, `<`, bitwise `&`, `^`.

```
let n: U32 = as_u32(42)
let m: U32 = n + n
```

### XField

Extension field element (3 base field elements). Used for [FRI](https://eccc.weizmann.ac.il/report/2017/134/) and IPA operations. See [How STARK Proofs Work](stark-proofs.md) for where extension fields appear in the proof system.

```
let x: XField = std.core.xfield.new(1, 0, 0)
```

### Digest

A [Tip5](https://eprint.iacr.org/2023/107) hash digest (5 field elements). Returned by hash functions.

```
let d: Digest = tip5(a, b, c, 0, 0, 0, 0, 0, 0, 0)
```

Access individual elements with `.0`, `.1`, `.2`, `.3`, `.4`:

```
let first: Field = d.0
let last: Field = d.4
```

---

## 4. Structs

Define named data types with `struct`:

```
struct Account {
    pub id: Field,
    pub balance: Field,
    nonce: Field,
}
```

Fields marked `pub` are accessible from other modules. Unmarked fields are private to the defining module.

Create instances with struct literal syntax:

```
fn new_account(id: Field) -> Account {
    Account { id: id, balance: 0, nonce: 0 }
}
```

Access fields with dot notation:

```
let bal: Field = account.balance
```

Assign to mutable struct fields:

```
let mut acc: Account = new_account(1)
acc.balance = 100
```

### Struct Pattern Matching

Structs can be destructured in `match` arms. Each field can bind a variable, match a literal, or use `_` as a wildcard:

```
struct Point {
    x: Field,
    y: Field,
}

fn describe(p: Point) -> Field {
    match p {
        Point { x: 0, y } => { y }
        Point { x, y: 0 } => { x }
        Point { x, y }    => { x + y }
    }
}
```

You can also use wildcard fields to ignore values you don't need:

```
match p {
    Point { x: _, b } => { pub_write(b) }
}
```

---

## 5. Arrays

Fixed-size arrays with compile-time known lengths:

```
let arr: [Field; 4] = [10, 20, 30, 40]
let first: Field = arr[0]
let last: Field = arr[3]
```

Mutable arrays support element assignment:

```
let mut data: [Field; 3] = [0, 0, 0]
data[0] = 42
```

Array indexing can use runtime values (with bounds checking):

```
let idx: Field = pub_read()
let val: Field = arr[idx]
```

---

## 6. Variables, Constants, and Operators

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
const MAX_SUPPLY: Field = 1000000
const TREE_HEIGHT: U32 = as_u32(3)
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

There are no `!=`, `>`, `<=`, or `>=` operators. Compose them from `==`, `<`, and `not()`. There are no `&&` or `||` -- use boolean combinators from `std.core.bool`. This is deliberate: fewer operators means fewer things to audit in provable code. See the [Reference](reference.md) for the full operator table and per-instruction costs.

---

## 7. Control Flow

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

**Why bounds are required.** Provable VMs execute a fixed-length trace. The prover must know the worst-case iteration count before execution begins. The `bounded N` annotation declares this maximum. The compiler uses the bound -- not the runtime count -- to compute proving cost, so `bounded 100` always costs 100 iterations in the trace even if the loop exits earlier. See [How STARK Proofs Work](stark-proofs.md) Section 11 for the proving time formula, and the [Optimization Guide](optimization.md) for strategies to choose good bounds.

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

The loop variable `i` has type `Field`.

### Match Expressions

Pattern matching over integer, boolean, and struct values:

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

For struct pattern matching, see the [Structs](#4-structs) section above.

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

## 8. Functions

### Declaration

Functions are declared with `fn`. The last expression in the body is the return value (tail expression). You can also use explicit `return`:

```
fn add_three(a: Field, b: Field, c: Field) -> Field {
    a + b + c
}

fn abs_diff(a: Field, b: Field) -> Field {
    if a == b {
        return 0
    }
    std.core.field.sub(a, b)
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

### Const Generic Expressions

Size parameters can appear in arithmetic expressions in types. This enables functions that compute output sizes from input sizes:

```
fn first_of<M, N>(a: [Field; M + N]) -> Field {
    a[0]
}

fn sum_pairs<N>(a: [Field; N * 2]) -> Field {
    a[0] + a[1]
}
```

The expressions support `+` and `*` over size parameters and integer literals. Precedence follows standard arithmetic: `M + N * 2` parses as `M + (N * 2)`.

### The `#[pure]` Annotation

Mark a function `#[pure]` to declare it has no I/O side effects -- no `pub_read`, `pub_write`, `divine`, `emit`, or `seal`:

```
#[pure]
fn square(x: Field) -> Field {
    x * x
}
```

The compiler enforces the constraint: calling any I/O function inside a `#[pure]` function is a compile error. Pure functions enable more aggressive reasoning in [formal verification](formal-verification.md) and may unlock additional compiler optimizations.

---

## 9. Modules and Imports

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

## 10. I/O and Secret Input

### Public I/O

Public inputs are visible to the verifier:

```
let x: Field = pub_read()         // read one field element
pub_write(x)                       // write one field element

let (a, b) = pub_read2()           // read two elements
pub_write5(d.0, d.1, d.2, d.3, d.4)  // write five elements
```

### Secret Input (Divine)

Secret inputs are known to the prover but not the verifier. For a conceptual introduction to why this matters, see [ZK Concepts for Developers](for-developers.md).

```
let secret: Field = divine()        // one field element
let (a, b, c) = divine3()           // three field elements
let d: Digest = divine5()           // five field elements (Digest)
```

The program must verify divine values are correct:

```
let claimed_root: Digest = divine5()
let actual_root: Digest = compute_root(data)
std.core.assert.digest(claimed_root, actual_root)
```

---

## 11. Hashing and Merkle Proofs

[Tip5](https://eprint.iacr.org/2023/107) is Triton VM's native algebraic hash function (see [How STARK Proofs Work](stark-proofs.md) Section 5 for why this hash matters for proofs). It always takes exactly 10 field elements as input and produces a 5-element Digest. Pad unused inputs with zeros:

```
use std.crypto.hash

fn hash_pair(a: Field, b: Field) -> Digest {
    std.crypto.hash.tip5(a, b, 0, 0, 0, 0, 0, 0, 0, 0)
}
```

For streaming data, use the sponge API:

```
fn hash_stream() -> Digest {
    std.crypto.hash.sponge_init()
    std.crypto.hash.sponge_absorb(a, b, c, d, e, f, g, h, i, j)
    std.crypto.hash.sponge_absorb(k, l, m, n, o, p, q, r, s, t)
    std.crypto.hash.sponge_squeeze()
}
```

Merkle proofs are built from Tip5 hashes. See `std.crypto.merkle` in the [Reference](reference.md) for the Merkle authentication API.

---

## 12. Events

Events record structured data in the proof trace. Declare the event, then `emit` or `seal` it.

### Declaration

```
event Transfer {
    from: Digest,
    to: Digest,
    amount: Field,
}
```

### Emit (Open Events)

All fields are visible to the verifier:

```
fn pay(sender: Digest, receiver: Digest, value: Field) {
    emit Transfer {
        from: sender,
        to: receiver,
        amount: value,
    }
}
```

### Seal (Hashed Events)

Fields are hashed; only the digest is visible to the verifier:

```
fn pay_private(sender: Digest, receiver: Digest, value: Field) {
    seal Transfer {
        from: sender,
        to: receiver,
        amount: value,
    }
}
```

Use `emit` for public audit trails. Use `seal` when field values must remain private but their commitment must be verifiable. For how events fit into the Neptune transaction model, see the [Programming Model](programming-model.md).

---

## 13. Testing

Add `#[test]` attributes to test functions:

```
fn add(a: Field, b: Field) -> Field {
    a + b
}

#[test]
fn test_add() {
    let result: Field = add(1, 2)
    assert(result == 3)
}
```

Run tests:

```bash
trident test main.tri
```

Test functions are excluded from production builds. See the [Error Catalog](errors.md) for all assertion failure messages.

---

## 14. Cost Analysis

Every operation in [Triton VM](https://triton-vm.org/) has a measurable proving cost. Use the build flags to analyze:

```bash
# Full cost report
trident build main.tri --target triton --costs

# Top cost contributors
trident build main.tri --target triton --hotspots

# Optimization suggestions
trident build main.tri --target triton --hints

# Per-line cost annotations
trident build main.tri --target triton --annotate
```

Track costs across builds:

```bash
# Save baseline
trident build main.tri --target triton --save-costs baseline.json

# After changes, compare
trident build main.tri --target triton --compare baseline.json
```

See the [Optimization Guide](optimization.md) for strategies to reduce proving cost, and [How STARK Proofs Work](stark-proofs.md) Section 11 for the proving time formula.

---

## 15. Conditional Compilation

Use `#[cfg(...)]` to include items only for specific targets:

```
#[cfg(debug)]
fn debug_log(x: Field) {
    pub_write(x)
}

fn main() {
    let x: Field = pub_read()
    #[cfg(debug)]
    fn debug_print() {
        debug_log(x)
    }
}
```

Build with a target to activate the conditional code:

```bash
trident build main.tri --target debug     # includes debug_log
trident build main.tri --target release   # excludes debug_log
trident build main.tri                    # no target: cfg(debug) items excluded
```

Define custom targets in `trident.toml`:

```toml
[targets.testnet]
flags = ["testnet", "debug"]
```

---

## 16. Inline Assembly

For operations not covered by the language, embed raw [TASM](https://triton-vm.org/spec/) instructions in `asm` blocks. See the [Reference](reference.md) for the full TASM instruction set mapping.

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

Named variables in scope are spilled to RAM before the block executes and restored after.

---

## 17. Key Differences from Conventional Languages

These are not limitations -- they are properties required for provable computation. For a deeper explanation, see the [Programming Model](programming-model.md). For zero-knowledge concepts explained from first principles, see [ZK Concepts for Developers](for-developers.md). For migration from smart-contract languages, see [For Blockchain Devs](for-blockchain-devs.md).

| Conventional expectation | Trident | Why |
|--------------------------|---------|-----|
| Heap allocation | No heap. All data is stack or RAM with static addressing. | The prover must know memory layout at trace generation time. |
| Recursion | No recursion. Use bounded loops. | Recursive call depth is unbounded, which prevents static trace sizing. |
| Unbounded loops | All loops require a `bounded` annotation. | The proof trace has a fixed length determined before execution. |
| Strings | No string type. | Strings are variable-length; all types must have compile-time known widths. |
| Floating point | No floats. `Field` is the native numeric type. | The VM operates over a prime field. Floats have no representation. |
| Subtraction operator | No `-`. Use `std.core.field.sub()`. | `1 - 2` in a prime field is `p - 1`, not `-1`. Explicit subtraction prevents this footgun. |
| Many comparison operators | Only `==` and `<`. No `!=`, `>`, `<=`, `>=`. | Fewer primitives means a smaller, more auditable instruction set. |
| Boolean connectives | No `&&` or `||`. Use `std.core.bool` combinators. | Same rationale: fewer primitives, easier audits. |
| Garbage collection | No GC. All lifetimes are lexical. | There is no runtime; the program is a static trace. |

These constraints make every Trident program a fixed, bounded computation -- exactly what a STARK prover requires.

---

## Next Steps

- [Language Reference](reference.md) -- Quick lookup for types, operators, builtins, and grammar
- [Language Specification](spec.md) -- Complete formal reference for all language constructs
- [Programming Model](programming-model.md) -- How programs run (currently targeting [Triton VM](https://triton-vm.org/)) and the Neptune transaction model
- [Compiling a Program](compiling-a-program.md) -- Next lifecycle stage: build targets, output formats, and optimization flags
- [Optimization Guide](optimization.md) -- Strategies to reduce proving cost
- [How STARK Proofs Work](stark-proofs.md) -- The proof system behind every Trident program
- [Error Catalog](errors.md) -- All error messages explained
- [Formal Verification](formal-verification.md) -- `#[requires]`, `#[ensures]`, `#[invariant]`, and `#[pure]`
- [For Developers](for-developers.md) -- Zero-knowledge concepts explained for conventional programmers
- [For Blockchain Devs](for-blockchain-devs.md) -- Mental model migration from Solidity/Anchor/CosmWasm
- [Content-Addressed Code](content-addressed.md) -- Content-addressed code and the UCM model
- [Vision](vision.md) -- Why Trident exists and what you can build
- [Comparative Analysis](analysis.md) -- Triton VM vs. every other ZK system
- [Triton VM specification](https://triton-vm.org/spec/) -- Target VM instruction set
- [tasm-lib](https://github.com/TritonVM/tasm-lib) -- Reusable TASM snippets
