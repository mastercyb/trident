# Trident Tutorial

A step-by-step guide to writing provable programs with Trident for [Triton VM](https://triton-vm.org/).

## Prerequisites

Build the compiler from source:

```bash
cd trident
cargo build --release
```

The binary is at `target/release/trident`. Add it to your PATH or use it directly.

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

This program reads two public field elements, adds them, and writes the result. The verifier sees both inputs and the output.

Build it:

```bash
trident build hello.tri -o hello.tasm
```

Check it (type-check without emitting TASM):

```bash
trident check hello.tri
```

## 2. Types

Trident has five primitive types, all with known compile-time widths:

### Field

The base type. A prime field element modulo p = 2^64 - 2^32 + 1 (the [Goldilocks prime](https://xn--2-umb.com/22/goldilocks/)). Supports `+`, `*`, `==`.

```
let x: Field = 42
let y: Field = x + x
```

There is no `-` operator. Use `sub(a, b)` from `std.field`:

```
use std.field

fn example() {
    let diff: Field = std.field.sub(10, 3)
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

Extension field element (3 base field elements). Used for [FRI](https://eccc.weizmann.ac.il/report/2017/134/) and IPA operations.

```
let x: XField = std.xfield.new(1, 0, 0)
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

## 3. Variables and Mutability

Variables are immutable by default:

```
let x: Field = 42
// x = 100  -- ERROR: cannot assign to immutable variable
```

Use `mut` for mutable variables:

```
let mut counter: Field = 0
counter = counter + 1
```

### Constants

Module-level constants use `const`:

```
const MAX_SUPPLY: Field = 1000000
const TREE_HEIGHT: U32 = as_u32(3)
```

Constants are inlined at every use site.

## 4. Functions

Functions are declared with `fn`. The return type follows `->`:

```
fn add_three(a: Field, b: Field, c: Field) -> Field {
    a + b + c
}
```

The last expression in a block is the return value (tail expression). You can also use explicit `return`:

```
fn abs_diff(a: Field, b: Field) -> Field {
    if a == b {
        return 0
    }
    std.field.sub(a, b)
}
```

### Multiple Return Values

Return tuples and destructure at the call site:

```
fn divmod(a: Field, b: Field) -> (Field, Field) {
    a /% b
}

fn example() {
    let (quot, rem) = divmod(17, 5)
}
```

### Size-Generic Functions

Functions can be generic over array sizes:

```
fn sum<N>(arr: [Field; N]) -> Field {
    let mut total: Field = 0
    for i in 0..N bounded N {
        total = total + arr[i]
    }
    total
}

fn example() {
    let a: [Field; 3] = [1, 2, 3]
    let s: Field = sum(a)          // N inferred as 3

    let b: [Field; 5] = [1, 2, 3, 4, 5]
    let t: Field = sum(b)          // N inferred as 5
}
```

Explicit size arguments use angle brackets:

```
let s: Field = sum<3>(a)
```

## 5. Structs

Define named data types with `struct`:

```
struct Account {
    pub id: Field,
    pub balance: Field,
    nonce: Field,
}
```

Fields marked `pub` are accessible from other modules. Private fields are only accessible within the defining module.

Create instances with struct literal syntax:

```
fn new_account(id: Field) -> Account {
    Account {
        id: id,
        balance: 0,
        nonce: 0,
    }
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

## 6. Arrays

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

## 7. Control Flow

### If/Else

```
if condition {
    do_something()
} else {
    do_other()
}
```

If/else can be used as expressions (tail expressions):

```
let result: Field = if flag { 1 } else { 0 }
```

### For Loops

All loops require a compile-time bound:

```
for i in 0..10 bounded 10 {
    process(i)
}
```

The `bounded N` annotation tells the compiler the maximum number of iterations. This is required because [Triton VM](https://triton-vm.org/) cannot execute unbounded loops. The loop variable `i` has type `Field`.

Dynamic ranges are allowed with `bounded`:

```
let n: Field = pub_read()
for i in 0..n bounded 100 {
    // Runs at most 100 iterations
    process(i)
}
```

### Match

Pattern matching over integer and boolean values:

```
match op_code {
    0 => { handle_pay() }
    1 => { handle_lock() }
    2 => { handle_update() }
    _ => { reject() }
}
```

The wildcard `_` arm is required unless all values are covered (for Bool: both `true` and `false`).

```
match flag {
    true => { accept() }
    false => { reject() }
}
```

### Return

Explicit return exits the function immediately:

```
fn early_exit(x: Field) -> Field {
    if x == 0 {
        return 0
    }
    x * x
}
```

## 8. The Module System

### Programs and Modules

Every `.tri` file is either a `program` (with a `main()` entry point) or a `module` (a library):

```
// main.tri
program my_app

use helpers
use std.hash

fn main() {
    let x: Field = pub_read()
    let d: Digest = std.hash.tip5(x, 0, 0, 0, 0, 0, 0, 0, 0, 0)
    pub_write(d.0)
}
```

```
// helpers.tri
module helpers

pub fn double(x: Field) -> Field {
    x + x
}
```

### Module Resolution

- `use helpers` looks for `helpers.tri` in the project directory
- `use crypto.hash` looks for `crypto/hash.tri`
- `use std.hash` looks for `hash.tri` in the standard library

### Calling Module Functions

Prefix the function name with the module name:

```
let result: Field = helpers.double(x)
let d: Digest = std.hash.tip5(x, 0, 0, 0, 0, 0, 0, 0, 0, 0)
```

### Visibility

Only items marked `pub` are visible to other modules:

```
module utils

pub fn public_fn() -> Field { 42 }   // accessible
fn private_fn() -> Field { 99 }       // not accessible outside
```

## 9. I/O and Secret Input

### Public I/O

Public inputs are visible to the verifier:

```
let x: Field = pub_read()         // read one field element
pub_write(x)                       // write one field element

let (a, b) = pub_read2()           // read two elements
pub_write5(d.0, d.1, d.2, d.3, d.4)  // write five elements
```

### Secret Input (Divine)

Secret inputs are known to the prover but not the verifier:

```
let secret: Field = divine()        // one field element
let (a, b, c) = divine3()           // three field elements
let d: Digest = divine5()           // five field elements (Digest)
```

The program must verify divine values are correct:

```
let claimed_root: Digest = divine5()
let actual_root: Digest = compute_root(data)
std.assert.digest(claimed_root, actual_root)
```

## 10. Hashing

[Tip5](https://eprint.iacr.org/2023/107) is the native hash function (10 inputs, 5 outputs):

```
use std.hash

fn hash_pair(a: Field, b: Field) -> Digest {
    std.hash.tip5(a, b, 0, 0, 0, 0, 0, 0, 0, 0)
}
```

For streaming data, use the sponge API:

```
fn hash_stream() -> Digest {
    std.hash.sponge_init()
    std.hash.sponge_absorb(a, b, c, d, e, f, g, h, i, j)
    std.hash.sponge_absorb(k, l, m, n, o, p, q, r, s, t)
    std.hash.sponge_squeeze()
}
```

## 11. Events

Events record data in the proof trace. Two kinds:

### Emit (Open Events)

All fields are visible to the verifier:

```
event Transfer {
    from: Digest,
    to: Digest,
    amount: Field,
}

fn pay(sender: Digest, receiver: Digest, value: Field) {
    emit Transfer {
        from: sender,
        to: receiver,
        amount: value,
    }
}
```

### Seal (Hashed Events)

Fields are hashed; only the digest is visible:

```
fn pay_private(sender: Digest, receiver: Digest, value: Field) {
    seal Transfer {
        from: sender,
        to: receiver,
        amount: value,
    }
}
```

## 12. Conditional Compilation

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

Build with a target:

```bash
trident build main.tri --target debug     # includes debug_log
trident build main.tri --target release   # excludes debug_log
```

Define custom targets in `trident.toml`:

```toml
[targets.testnet]
flags = ["testnet", "debug"]
```

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

Test functions are excluded from production builds.

## 14. Cost Analysis

Every operation in [Triton VM](https://triton-vm.org/) has a measurable proving cost. Use the build flags to analyze:

```bash
# Full cost report
trident build main.tri --costs

# Top cost contributors
trident build main.tri --hotspots

# Optimization suggestions
trident build main.tri --hints

# Per-line cost annotations
trident build main.tri --annotate
```

Track costs across builds:

```bash
# Save baseline
trident build main.tri --save-costs baseline.json

# After changes, compare
trident build main.tri --compare baseline.json
```

See the [Optimization Guide](optimization.md) for strategies to reduce proving cost.

## 15. Inline Assembly

For operations not yet covered by the language, use inline [TASM](https://triton-vm.org/spec/):

```
fn custom_op(a: Field, b: Field) -> Field {
    asm(-1) {
        add
    }
}
```

The effect annotation (`+N` or `-N`) declares the net stack change. `asm(-1)` means the block consumes one net stack element (two inputs, one output from `add`).

Use with care: the compiler trusts the effect annotation. An incorrect annotation will produce broken TASM.

## Next Steps

- [Language Specification](spec.md) -- Complete reference for all language constructs
- [Programming Model](programming-model.md) -- How programs run in [Triton VM](https://triton-vm.org/)
- [Optimization Guide](optimization.md) -- Strategies to reduce proving cost
- [Error Catalog](errors.md) -- All error messages explained
- [Triton VM specification](https://triton-vm.org/spec/) -- Target VM instruction set
- [tasm-lib](https://github.com/TritonVM/tasm-lib) -- Reusable TASM snippets
