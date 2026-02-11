# The Trident Language

**Correct. Bounded. Provable.**

**Version 0.5**
**February 2026**

A minimal, security-first language for provable computation on zero-knowledge virtual machines.

> **Quick lookup?** See [reference.md](reference.md) for types, operators, builtins, grammar, and CLI flags.
> **New to ZK?** Start with [for-developers.md](for-developers.md).
> **Coming from Solidity/Anchor?** See [for-blockchain-devs.md](for-blockchain-devs.md).
> **Multi-target architecture?** See [universal-design.md](universal-design.md).

---

## 1. Executive Summary

Trident is a minimal, security-first programming language for [zero-knowledge proof](https://en.wikipedia.org/wiki/Zero-knowledge_proof) systems. The language follows the [Vyper](https://docs.vyperlang.org/) philosophy: **deliberate limitation as a feature**, not a compromise.

Trident is a **universal language with pluggable backends**. The core language — types, control flow, modules, field arithmetic, I/O — is target-agnostic. Each backend implements a thin compilation layer for a specific zkVM. The primary backend targets [Triton VM](https://triton-vm.org/), compiling directly to [TASM](https://triton-vm.org/spec/) (Triton Assembly) with no intermediate representation. Additional backends (Miden VM, Cairo VM, SP1/RISC-V) follow the same architecture. See [universal-design.md](universal-design.md) for the full multi-target design.

Every language construct maps predictably to known instruction patterns in the target VM. The compiler is a thin, auditable translation layer — not an optimization engine.

The language exists to solve one problem: **writing provable programs without spending months in assembly**. It explicitly does not aim to be a general-purpose language.

> **Target abstraction principle.** `Field` means "element of the target VM's native field." Programs reason about field arithmetic abstractly; the backend implements it concretely. A program that multiplies two field elements and asserts the result means the same thing on every zkVM. Programs should never depend on the specific field modulus.

**File extension**: `.tri`
**Compiler**: `trident`

---

## 2. Design Rationale

### 2.1 Why Not an IR?

> *The rationale below applies to the Triton VM backend. Other backends may introduce a minimal IR where the target architecture requires it (e.g., register-machine targets). See [universal-design.md](universal-design.md) Section 8.*

[Triton VM's ISA](https://triton-vm.org/spec/) (~45 instructions) is already cleaner than most assembly languages. An intermediate representation would:

- Double the audit surface (source→IR mapping + IR→TASM mapping)
- Add 40-50% to compiler engineering effort
- Obscure the cost model through optimization passes
- Require ongoing maintenance by a 3-person team

Direct compilation (source → AST → type check → TASM emit) keeps the compiler small enough that a single engineer can understand the entire codebase, and a security auditor can verify the translation in days rather than months.

An IR may become justified if: (a) a second frontend language is needed, (b) optimization becomes critical, or (c) the team grows significantly. None of these conditions hold today.

### 2.2 Why [Vyper](https://docs.vyperlang.org/), Not Solidity?

Solidity's design philosophy — maximal expressiveness, familiar OOP patterns, escape hatches everywhere — optimizes for developer convenience at the cost of auditability. In zero-knowledge systems, the proving cost of every instruction is real and measurable. Hidden complexity becomes hidden cost.

[Vyper](https://docs.vyperlang.org/) demonstrated that deliberate limitation produces:

- **Auditable code**: one obvious way to do everything
- **Predictable costs**: no hidden allocations or implicit loops
- **Fewer bugs**: less surface area for mistakes
- **Faster shipping**: smaller language = smaller compiler = faster to production

For Triton VM, these properties are even more critical. Every instruction expands the algebraic execution trace. Every unnecessary abstraction inflates proving time. The language must make costs visible.

### 2.3 Why Not [Cairo](https://www.cairo-lang.org/)?

[Cairo](https://www.cairo-lang.org/) is the closest analog but carries baggage from its evolution:

- Cairo 0 → Cairo 1 was a full rewrite that split the ecosystem
- Sierra IR adds complexity justified by StarkNet's needs (gas metering, contract isolation) that Triton VM doesn't share
- Cairo's 252-bit field encourages different idioms than Triton's 64-bit field

We learn from [Cairo](https://www.cairo-lang.org/)'s successes (hint architecture, felt-aware type system, bounded execution) while avoiding its accumulated complexity.

### 2.4 Core Design Principles

1. **What you see is what you prove.** Every line maps predictably to TASM. No hidden allocations, no implicit loops, no compiler magic.

2. **One obvious way to do everything.** No function overloading, no operator overriding, no macros, no metaprogramming.

3. **Bounded everything.** All loops have compile-time-known or declared maximum bounds. No unbounded recursion. No dynamic memory. The compiler computes exact trace length before execution.

4. **ZK primitives are first-class.** Non-deterministic hints, Merkle authentication, sponge hashing are language constructs, not library hacks.

5. **Cost transparency.** The compiler annotates every function with its trace cost. No surprises.

6. **Modules enforce boundaries.** Clear interfaces between components improve auditability over monolithic single-file programs. Every module is independently auditable.

---

## 3. Type System

### 3.1 Primitive Types

```
Field           // Native field element of the target VM
XField          // Extension field element (target-dependent degree)
Bool            // 0 or 1 (a Field element, constrained)
U32             // Unsigned 32-bit integer (a Field element, range-checked)
Digest          // [Field; DIGEST_WIDTH] — a hash digest (width is target-dependent)
```

> **Triton VM target.** On Triton VM: `Field` is the Goldilocks field (integers mod p, where p = 2^64 - 2^32 + 1). `XField` is the cubic extension F_p[X] / (X^3 - X + 1), width 3. `Digest` is [Field; 5] (Tip5 hash output). `U32` exists because the Triton ISA has dedicated u32 instructions (`lt`, `and`, `xor`, `div_mod`, `log_2_floor`, `pow`, `pop_count`). Attempting u32 operations on values outside the u32 range crashes the VM.

**Design note**: There is no general integer type. The target VM operates natively on field elements. The type system prevents field/u32 misuse at compile time where possible, and the VM enforces it at runtime where not.

**No implicit conversions.** `Field` and `U32` do not auto-convert. Use explicit `as_field()` and `as_u32()` (the latter inserts a range check via `split`).

### 3.2 Composite Types

```
[T; N]          // Fixed-size array, N known at compile time
(T1, T2, ...)   // Tuple (max 16 elements due to stack depth)
struct Name { field1: T1, field2: T2 }   // Named product type
```

**No enums.** No sum types. No unions. These require dynamic dispatch or tag checking that complicates the trace. Use separate functions instead.

**No references or pointers.** All values are passed by copy on the stack. Structs are flattened to sequential stack/RAM elements.

### 3.3 Type Layout

All types have a known compile-time **width** measured in field elements:

| Type | Width |
|------|-------|
| `Field` | 1 |
| `XField` | 3 |
| `Bool` | 1 |
| `U32` | 1 |
| `Digest` | 5 |
| `[T; N]` | N × width(T) |
| `(T1, T2)` | width(T1) + width(T2) |
| `struct` | sum of field widths |

The maximum total width of all live variables in a function is bounded by available stack + RAM. The compiler tracks stack depth and rejects programs that would overflow the 16-element operational stack, spilling to RAM automatically when needed.

---

## 4. Module System

### 4.1 Design Philosophy

Modules exist to enforce **interface boundaries** and enable **independent auditability**. A 2000-line monolithic program is harder to audit than ten 200-line modules with explicit contracts between them. Modules are the unit of trust.

The module system is intentionally minimal — namespacing, visibility control, and separate compilation. No advanced features that would complicate the compilation model.

### 4.2 Module Declaration

Every `.tri` file is a module. A module is either a **program** (has `main`, produces an executable) or a **library** (no `main`, provides reusable functions and types).

```
// merkle.tri — a library module
module merkle

pub struct Proof {
    root: Digest,
    leaf_index: U32,
    depth: U32,
}

pub fn verify(root: Digest, leaf: Digest, index: U32, depth: U32) {
    let mut idx = index
    let mut current = leaf
    for _ in 0..depth bounded 64 {
        (idx, current) = merkle_step(idx, current)
    }
    assert_digest(current, root)
}

// Private — not visible outside this module
fn validate_index(index: U32, depth: U32) -> Bool {
    index < pow(2, depth)
}
```

### 4.3 Imports

```
// main.tri — a program module
program my_verifier

use merkle                      // import module
use crypto.sponge               // nested module (directory-based)

fn main() {
    let root: Digest = pub_read5()
    let leaf: Digest = divine5()
    let index: U32 = as_u32(pub_read())
    merkle.verify(root, leaf, index, 20)
}
```

**Import rules:**

- `use` imports a module by name, accessed via dot notation
- **No wildcard imports** (`use merkle.*` is forbidden)
- **No renaming** (`use merkle as m` is forbidden)
- **No re-exports** (if A uses B, C cannot access B through A)
- **No circular dependencies** — the dependency graph must be a DAG, enforced at compile time

These restrictions keep the module resolution trivially simple: topological sort of the dependency DAG, compile in order, concatenate TASM output, resolve `call` addresses. The entire module resolver is ~200 lines of compiler code.

### 4.4 Visibility

Two levels only:

- **`pub`** — visible to any module that imports this one
- **default (no keyword)** — private to this module

No `pub(crate)`, no `friend`, no `internal`. Two levels. Nothing to learn, nothing to misuse.

```
module wallet

pub struct Balance {
    pub owner: Digest,          // visible to importers
    amount: Field,              // private to this module
}

pub fn create(owner: Digest, amount: Field) -> Balance {
    Balance { owner, amount }
}

pub fn get_amount(b: Balance) -> Field {
    b.amount                    // private field, accessible within module
}

fn validate(b: Balance) -> Bool {
    // private function
    ...
}
```

### 4.5 Project Layout

```
my_project/
├── main.tri                    // program entry point
├── merkle.tri                  // module merkle
├── crypto/
│   ├── sponge.tri              // module crypto.sponge
│   └── tip5.tri                // module crypto.tip5
├── neptune/
│   ├── transaction.tri         // module neptune.transaction
│   └── mutator_set.tri         // module neptune.mutator_set
└── trident.toml                // project config (minimal)
```

**trident.toml** — deliberately minimal:

```toml
[project]
name = "my_verifier"
version = "0.1.0"
entry = "main.tri"

[dependencies]
# Local path dependencies only (no package registry in v1)
neptune_stdlib = { path = "../neptune-stdlib" }
```

No package registry in v1. Dependencies are local paths or git URLs. This avoids the complexity of dependency resolution, version conflicts, and supply chain attacks. The ecosystem is too small for a registry to be useful yet.

### 4.6 Standard Library as Modules

The standard library ships as Trident modules organized in a layered hierarchy, not as compiler built-ins:

```
// Core modules — field and integer operations
use std.core.convert  // as_u32(), as_field()
use std.core.field    // neg(), sub(), inv()
use std.core.u32      // u32 arithmetic helpers
use std.core.assert   // assertion helpers

// Cryptography modules
use std.crypto.hash   // tip5(), sponge_init(), sponge_absorb(), sponge_squeeze()
use std.crypto.merkle // verify(), step()
use std.crypto.auth   // authentication path utilities

// I/O modules
use std.io.io         // pub_read(), pub_write()
use std.io.mem        // ram_read(), ram_write()
use std.io.storage    // persistent storage helpers
```

**Legacy flat paths.** For backward compatibility, the compiler accepts flat paths and rewrites them to layered equivalents: `std.hash` resolves to `std.crypto.hash`, `std.convert` to `std.core.convert`, `std.io` to `std.io.io`, and so on. New code should use the layered paths.

These modules are backed by optimized target-specific code (e.g., [tasm-lib](https://github.com/TritonVM/tasm-lib) patterns on the Triton VM backend) but are written in Trident syntax with `#[intrinsic]` annotations that tell the compiler to emit target-appropriate instruction sequences:

```
// std/crypto/hash.tri
module std.crypto.hash

#[intrinsic(hash)]
pub fn tip5(a: Field, b: Field, c: Field, d: Field, e: Field,
            f: Field, g: Field, h: Field, i: Field, j: Field) -> Digest

#[intrinsic(sponge_init)]
pub fn sponge_init()

#[intrinsic(sponge_absorb)]
pub fn sponge_absorb(a: Field, b: Field, c: Field, d: Field, e: Field,
                     f: Field, g: Field, h: Field, i: Field, j: Field)

#[intrinsic(sponge_squeeze)]
pub fn sponge_squeeze() -> [Field; 10]
```

The `#[intrinsic]` annotation is **only** allowed in `std` modules shipped with the compiler. User code cannot use it. This is the one place where the compiler knows more than the language -- and it's explicitly marked and auditable.

### 4.7 Compilation Model

```
1. Parse all modules (topological order from entry point)
2. Type check each module independently
3. Emit TASM for each module (each becomes a labeled block)
4. Link: concatenate TASM blocks, resolve cross-module call addresses
5. Output single .tasm file
```

The linker is trivial — TASM uses absolute jump addresses, so linking is just address patching. No dynamic dispatch, no vtables, no PLT/GOT. A module's public functions become labeled TASM subroutines, called with the standard `call` instruction.

---

## 5. Program Structure

### 5.1 Program Declaration

Every Trident program has exactly one entry point:

```
program my_program

use std.io.io
use std.crypto.hash

// Public I/O declarations
pub input:  [Field; 3]
pub output: Field

// Secret input declaration
sec input:  [Field; 5]

// Optional RAM initialization
sec ram: {
    17: Field,
    42: Field,
}

fn main() {
    // program body
}
```

### 5.2 Functions

```
fn function_name(param1: Type1, param2: Type2) -> ReturnType {
    // body
}
```

- No default arguments
- No variadic arguments
- No function overloading
- No closures or higher-order functions
- No recursion (the compiler rejects call cycles across all modules)
- Maximum 16 parameters (stack depth limit)

### 5.3 Constants

```
const MAX_DEPTH: U32 = 32
const ZERO: Field = 0
const GENERATOR: Field = 7
```

Constants are inlined at compile time. No runtime cost. Constants can be `pub` for cross-module use.

### 5.4 Size-Generic Functions

Functions can be parameterized over array sizes using compile-time size parameters:

```
fn sum<N>(arr: [Field; N]) -> Field {
    let mut total: Field = 0
    for i in 0..N {
        total = total + arr[i]
    }
    total
}
```

Size parameters appear in angle brackets after the function name. They can be used anywhere an array size is expected in the function signature and body. Only integer size parameters are supported — Trident has no type-level generics (see Section 14).

**Explicit size arguments:**

```
let a: [Field; 3] = [1, 2, 3]
let total: Field = sum<3>(a)
```

**Inferred size arguments:**

```
let a: [Field; 3] = [1, 2, 3]
let total: Field = sum(a)       // N=3 inferred from argument type
```

When the size argument is omitted, the compiler infers it by matching the concrete argument types against the generic parameter types. If `arr` has type `[Field; 5]` and the parameter expects `[Field; N]`, the compiler deduces `N=5`.

**Monomorphization.** Each unique combination of size arguments produces a specialized copy of the function at compile time. Calling `sum<3>(...)` and `sum<5>(...)` in the same program emits two distinct TASM functions (`__sum__N3` and `__sum__N5`). There is no runtime dispatch.

```
fn first<N>(arr: [Field; N]) -> Field {
    arr[0]
}

fn main() {
    let a: [Field; 3] = [1, 2, 3]
    let b: [Field; 5] = [10, 20, 30, 40, 50]
    let x: Field = first(a)    // emits __first__N3
    let y: Field = first(b)    // emits __first__N5
}
```

**Multiple size parameters:**

```
fn concat<M, N>(a: [Field; M], b: [Field; N]) -> [Field; ???] {
    // not yet supported — return type cannot reference M+N
}
```

Currently, size parameters can only appear as standalone array sizes, not in arithmetic expressions. Size-dependent return types require computed size expressions, which are not yet supported.

**Design rationale:** Size-generic functions solve the most common source of code duplication in Triton VM programs — array-processing functions that differ only in length. Full type-level generics (like Rust's `<T>`) are permanently excluded (Section 14) because they would require trait resolution, vtables or monomorphization over types, and significantly complicate the compiler. Size parameters are the minimal extension that covers the practical need.

---

## 6. Expressions and Operators

> *The TASM instruction mappings shown in this section are for the Triton VM target. Other backends emit equivalent instructions in their native instruction sets. The Trident source syntax is the same across all targets.*

### 6.1 Field Arithmetic

```
a + b           // field addition    → TASM: add
a * b           // field multiply    → TASM: mul
inv(a)          // field inverse     → TASM: invert
a == b          // field equality    → TASM: eq
a + 42          // immediate add     → TASM: addi 42
```

**No subtraction operator.** Subtraction in a prime field is addition by the additive inverse. Use the built-in:

```
sub(a, b)       // field subtraction: a + (p - b)
neg(a)          // additive inverse: p - a
```

**Design rationale:** Making subtraction explicit reminds the developer they're in a [prime field](https://en.wikipedia.org/wiki/Finite_field). `(1 - 2)` doesn't give `-1` — it gives `p - 1`. This is the [Cairo](https://www.cairo-lang.org/) felt footgun, and Trident avoids it by forcing explicitness. When you write `sub(a, b)` you know exactly what's happening.

### 6.2 U32 Arithmetic

```
a < b           // u32 less than     → TASM: lt
a & b           // u32 bitwise and   → TASM: and
a ^ b           // u32 bitwise xor   → TASM: xor
a /% b          // divmod            → TASM: div_mod, returns (U32, U32)
log2(a)         // floor log base 2  → TASM: log_2_floor
pow(base, exp)  // exponentiation    → TASM: pow
popcount(a)     // hamming weight    → TASM: pop_count
split(a)        // Field→(U32, U32)  → TASM: split
```

U32 operations crash the VM if operands are not in range. The type system enforces this statically where possible.

### 6.3 Extension Field Arithmetic

```
let a: XField = xfield(x0, x1, x2)
let b: XField = xfield(y0, y1, y2)

a + b           // → TASM: xx_add
a * b           // → TASM: xx_mul
xinvert(a)      // → TASM: x_invert
a *. s          // XField × Field   → TASM: xb_mul
```

### 6.4 Boolean Logic

```
let a: Bool = (x == y)

if a { ... }
```

`Bool` is a `Field` constrained to `{0, 1}`. The `if` construct compiles to `skiz`. There is no `&&` or `||` — use the standard library:

```
use std.core.field

std.core.field.and(a, b)    // a * b
std.core.field.or(a, b)     // a + b - a * b
std.core.field.not(a)       // 1 - a
```

This is consistent with how boolean logic works in arithmetic circuits.

---

## 7. Control Flow

### 7.1 If / Else

```
if condition {
    // body
}

if condition {
    // true branch
} else {
    // false branch
}
```

`condition` must be `Bool` or `Field` (where 0 = false, nonzero = true).

Compiles to `skiz` + jump. No `else if` chains — use nested `if/else`.

### 7.2 Bounded Loops

```
for i in 0..N {     // N must be compile-time constant
    // body using i
}
```

**All loops must have a compile-time-known or explicitly declared upper bound.** This guarantees the compiler can compute exact trace length.

```
// Compile-time constant bound
for i in 0..32 {
    // exactly 32 iterations, trace cost known
}

// Variable bound with declared maximum
for i in 0..n bounded 64 {
    // at most 64 iterations
    // actual count depends on runtime n
    // trace cost computed from bound (64), not actual (n)
}
```

**No `while`.** No `loop`. No `break`. No `continue`. Every loop runs for exactly its declared iterations. For early exit, use a boolean flag:

```
let mut done: Bool = false
for i in 0..MAX {
    if std.core.field.not(done) {
        // actual work
        if exit_condition {
            done = true
        }
    }
}
```

This is intentionally verbose. It makes wasted trace rows visible. If the waste is unacceptable, restructure the algorithm.

### 7.3 Assert

```
assert(condition)                   // → TASM: assert
assert_eq(a, b)                     // → assert(a == b)
assert_digest(d1, d2)               // → TASM: assert_vector
```

Assertions are the primary verification mechanism. Failed assertions crash the VM and make proof generation impossible -- which is exactly the desired behavior.

### 7.4 Match

Pattern matching over integer and boolean values:

```
match op_code {
    0 => { handle_pay() }
    1 => { handle_lock() }
    2 => { handle_update() }
    _ => { reject() }
}
```

The wildcard `_` arm is required unless all values are covered. For `Bool`, both `true` and `false` must be covered:

```
match flag {
    true  => { accept() }
    false => { reject() }
}
```

**Semantics.** `match` is syntactic sugar over nested `if`/`else` chains. The compiler desugars it at the AST level -- there is no dedicated match instruction in the target VM. Each arm's pattern is compared with `eq`, and the corresponding block executes on match. Arms are tested in source order; the first match wins.

**Supported patterns:** integer literals, `true`, `false`, and `_` (wildcard). No struct destructuring, no nested patterns, no guards. This keeps the construct a thin translation layer over `if`/`else` rather than a complex pattern-matching engine.

**Exhaustiveness.** The compiler rejects `match` expressions that are neither exhaustive nor have a wildcard arm.

---

## 8. ZK-Native Constructs

These constructs distinguish Trident from general-purpose languages. They map to specialized instructions in the target VM.

> *The TASM mappings below are for the **Triton VM target**. Other backends provide equivalent semantics through their native instruction sets. The Trident source syntax is identical across targets.*

### 8.1 Non-Deterministic Hints

```
let value: Field = divine()                        // → TASM: divine 1
let (a, b, c): (Field, Field, Field) = divine3()   // → TASM: divine 3
let values: [Field; 5] = divine5()                  // → TASM: divine 5
```

`divine` reads from the secret input tape. The prover supplies these values; the verifier does not see them. The program must constrain divined values with assertions for soundness.

**Pattern: compute expensive, verify cheap**

```
// Prove knowledge of square root without revealing it
fn prove_sqrt(x: Field) {
    let s: Field = divine()      // prover injects sqrt(x)
    assert(s * s == x)           // verifier checks s² = x
}
```

### 8.2 Hashing

```
// Fixed-input Tip5 hash (10 field elements → Digest)
let d: Digest = hash(a, b, c, d, e, f, g, h, i, j)

// Variable-length hashing via sponge
sponge_init()
sponge_absorb(a, b, c, d, e, f, g, h, i, j)
let squeezed: [Field; 10] = sponge_squeeze()

// Absorb from RAM
sponge_absorb_mem(ptr)
```

### 8.3 [Merkle Tree](https://en.wikipedia.org/wiki/Merkle_tree) Operations

```
// Single Merkle step — one level of the tree
fn merkle_step(node_index: U32, digest: Digest) -> (U32, Digest)
    // → TASM: merkle_step
    // Reads sibling from secret input
    // Returns (parent_index, parent_digest)

// Merkle step from RAM (reusable auth paths)
fn merkle_step_mem(ptr: Field, node_index: U32, digest: Digest)
    -> (Field, U32, Digest)
    // → TASM: merkle_step_mem

// High-level verification (stdlib)
use std.crypto.merkle
std.crypto.merkle.verify(root, leaf, leaf_index, depth)
```

### 8.4 Dot Products (for [STARK](stark-proofs.md) Verification)

```
// Extension field dot product from RAM
fn xx_dot_step(acc: XField, ptr_a: Field, ptr_b: Field)
    -> (XField, Field, Field)
    // → TASM: xx_dot_step

// Mixed-field dot product from RAM
fn xb_dot_step(acc: XField, ptr_a: Field, ptr_b: Field)
    -> (XField, Field, Field)
    // → TASM: xb_dot_step
```

### 8.5 Inline TASM

For cases where hand-written assembly is needed — performance-critical inner loops, access to new VM instructions not yet exposed as builtins, or precise stack manipulation — Trident provides an inline assembly escape hatch:

```
fn double_top() {
    asm { dup 0 add }
}
```

The `asm` block contains raw TASM instructions that are emitted verbatim into the output. The compiler does not parse, validate, or optimize the assembly contents.

**Target-tagged blocks.** In a multi-target project, `asm` blocks must be tagged with the target VM name so the compiler knows which backend should process them:

```
fn double_top() {
    asm(triton) { dup 0 add }   // Triton VM assembly
}

fn double_top_miden() {
    asm(miden) { dup.0 add }    // Miden VM assembly
}
```

A bare `asm { ... }` (without a target tag) is treated as `asm(triton) { ... }` for backward compatibility, but new code in multi-target projects should always use the tagged form. The compiler rejects `asm` blocks tagged for a target other than the current compilation target.

**Stack effect annotations.** By default, the compiler assumes an `asm` block has zero net stack effect (pushes and pops cancel out). If the block changes the stack height, declare the effect explicitly:

```
fn push_magic() -> Field {
    asm(+1) { push 42 }        // pushes one element (single-target shorthand)
}

fn push_magic_tagged() -> Field {
    asm(triton, +1) { push 42 } // target tag + stack effect
}

fn consume_two(a: Field, b: Field) {
    asm(-2) { pop 1 pop 1 }    // pops two elements
}
```

The annotation `(+N)` or `(-N)` declares the net change in stack depth. When combined with a target tag, the target comes first: `asm(triton, +1) { ... }`. The compiler uses the effect annotation to track stack layout correctly across `asm` boundaries.

**Interleaving with Trident code.** Inline assembly can appear between regular statements:

```
fn example() {
    let x: Field = pub_read()
    asm { dup 0 add }           // doubles top of stack
    pub_write(x)
}
```

**When to use inline TASM:**

- Accessing VM instructions not yet exposed as Trident builtins
- Hand-optimizing a proven hot loop
- Implementing new intrinsics during language development

**When not to use it:**

- For anything the language already supports — prefer Trident syntax for auditability
- In security-critical code where formal verification of the Trident→TASM mapping matters

**Design rationale:** Every Triton VM language eventually needs an escape hatch. By providing one explicitly with mandatory stack effect declarations, the compiler can continue tracking types and stack layout across inline blocks rather than giving up entirely. The effect annotation is the minimal contract between hand-written assembly and the compiler's stack model.

### 8.6 Events

Events provide a structured way to emit data during proof execution. They serve two purposes: **public output** (data visible to the verifier via `emit`) and **committed secrets** (data hashed and sealed via `seal`).

**Event Declaration:**

```
event Transfer {
    sender: Digest,
    receiver: Digest,
    amount: Field,
}
```

Events are declared at module scope with named, typed fields. They compile to sequential I/O operations — no runtime overhead beyond the I/O itself.

**Emitting Events:**

```
fn process_transfer(sender: Digest, receiver: Digest, amount: Field) {
    // ... validation logic ...

    emit Transfer {
        sender: sender,
        receiver: receiver,
        amount: amount,
    }
}
```

`emit` writes each field to public output via `write_io`. The verifier sees the emitted data. Use `emit` for data that should be publicly observable — transaction logs, state transitions, receipts.

**Sealing Events:**

```
fn process_secret(owner: Digest, value: Field) {
    seal SecretUpdate {
        owner: owner,
        value: value,
    }
}
```

`seal` hashes the event fields via the sponge construction and writes the resulting digest to public output. The verifier sees only the commitment (digest), not the individual fields. Use `seal` for data that must be committed but kept private — secret values, authentication witnesses, proprietary logic.

**Design rationale:** Events separate "what happened" (structured data) from "how to output it" (`emit` for public, `seal` for committed). This mirrors blockchain event logs but with ZK-native semantics: `seal` provides cryptographic commitment without revealing the data, which has no analog in conventional smart contracts.

### 8.7 Verification Annotations

Functions can carry formal specifications via `#[requires]` and `#[ensures]` attributes:

```
#[requires(amount > 0)]
#[ensures(result == balance + amount)]
pub fn deposit(balance: Field, amount: Field) -> Field {
    balance + amount
}
```

- `#[requires(predicate)]` — **precondition**: must hold when the function is called
- `#[ensures(predicate)]` — **postcondition**: must hold when the function returns

In `#[ensures]`, the identifier `result` refers to the function's return value.

These annotations are checked by `trident verify`, which uses symbolic execution and optional SMT solving to prove or refute the specifications. They have no effect on compilation — `trident build` ignores them entirely.

```
#[requires(depth <= 64)]
#[requires(index < pow(2, depth))]
#[ensures(true)]  // postcondition: function does not crash
pub fn verify_merkle(root: Digest, leaf: Digest, index: U32, depth: U32) {
    // ...
}
```

See `trident verify` in Section 16.1 for the verification workflow.

### 8.8 Test Functions

Functions annotated with `#[test]` are test cases:

```
#[test]
fn test_deposit() {
    let result = deposit(100, 50)
    assert_eq(result, 150)
}
```

Test functions must take no parameters and return nothing. They are excluded from production compilation (`trident build`) and run via `trident test`. The test runner compiles each test function as a standalone program and checks that it does not crash (all assertions pass).

---

## 9. Memory Model

### 9.1 Stack

The operational stack has 16 directly accessible elements. The compiler manages stack layout automatically. Variables are assigned stack positions; when more than 16 are live, the compiler spills to RAM.

The developer does not manage the stack. Stack layout is a compiler concern.

### 9.2 RAM

RAM is word-addressed, each cell holds one `Field` element:

```
// Write to RAM
ram_write(address, value)
ram_write_block(address, values: [Field; N])

// Read from RAM
let v: Field = ram_read(address)
let vs: [Field; N] = ram_read_block(address)
```

**RAM is non-deterministic on first read.** If an address hasn't been written to, reading returns whatever the prover supplies. The program must constrain values through assertions.

### 9.3 No Heap

No dynamic memory allocation. No `alloc`, no `free`, no garbage collector. All data structures have compile-time-known sizes. This guarantees deterministic memory usage, no leaks, no use-after-free, and predictable trace length.

---

## 10. I/O Interface

### 10.1 Public Input

```
pub input: [Field; N]

let a: Field = pub_read()                         // → TASM: read_io 1
let (a, b): (Field, Field) = pub_read2()           // → TASM: read_io 2
// up to pub_read5()
```

Public input is visible to both prover and verifier. Consumed sequentially.

### 10.2 Public Output

```
pub output: [Field; M]

pub_write(value)           // → TASM: write_io 1
pub_write3(a, b, c)        // → TASM: write_io 3
// up to pub_write5()
```

### 10.3 Secret Input

```
sec input: [Field; K]

let s: Field = divine()    // → TASM: divine 1
```

Visible only to the prover. Must be constrained by assertions.

---

## 11. Compiler Behavior

### 11.1 Compilation Pipeline

```
Source (.tri files)
  → Module Resolution (topological sort of DAG)
  → Lexer → Parser → AST (per module)
  → Type Checker (per module, cross-module interface check)
  → TASM Emitter (per module)
  → Linker (concatenate, resolve addresses)
  → Output: single .tasm file
```

Single-pass emitter per module, no optimization passes. The emitter uses tasm-lib patterns for known constructs.

### 11.2 Cost Annotations

The compiler annotates every `pub` function with its **trace length**:

```
pub fn transfer(sender: Digest, receiver: Digest, amount: U32) -> Bool
// [trace: 847 rows, proving: ~2.1s @ 1GHz]
{
    ...
}
```

Computed statically from TASM output. For bounded loops, worst-case bound is used. Appears in compiler output and generated documentation.

### 11.3 Error Messages

Errors reference source location and resulting TASM:

```
error[E0042]: u32 operation on unchecked Field value
  --> wallet/transfer.tri:17:5
   |
17 |     let result = a < b
   |                  ^^^^^ `a` is Field, not U32
   |
   = note: this would emit `lt` which crashes if operands are not u32
   = help: use std.core.convert.as_u32(a) to insert a range check
```

### 11.4 What the Compiler Rejects

- Circular module dependencies
- Recursive function calls (across all modules)
- Unbounded loops
- Stack overflow without automatic RAM spill
- Type mismatches (Field vs U32 vs Bool)
- Dead code (unreachable after unconditional halt/assert)
- Unused imports (warning)
- Programs that don't end with halt
- `#[intrinsic]` in non-std modules
- `asm` blocks tagged for a different target than the current compilation target
- `emit`/`seal` referencing undeclared events
- Event field type mismatches
- Non-exhaustive `match` without wildcard arm

---

## 12. Cost Computation

> *The cost model in this section describes the **Triton VM target**. Each backend has its own cost model (e.g., Miden uses cycle counts, Cairo uses steps, RISC-V zkVMs use cycle counts). The compiler reports costs in the target's native units. The Trident cost infrastructure (static analysis, per-function annotations, `--costs` flag) works identically across all targets.*

### 12.1 Why Cost Matters

In [Triton VM](https://triton-vm.org/), proving time is directly determined by the **padded height** of the Algebraic Execution Tables (AETs). The padded height is the smallest power of 2 that is greater than or equal to the height of the tallest table. Doubling the padded height roughly doubles the proving time and memory consumption. This means:

- A program with 1,000 processor cycles and a program with 1,023 processor cycles have identical proving cost (padded to 1,024).
- A program with 1,025 processor cycles costs roughly **twice as much** to prove as one with 1,024 (padded to 2,048).
- A seemingly small code change that pushes trace height past a power-of-2 boundary can double proving time.

This makes cost computation not just useful but **essential**. Trident computes costs statically at compile time and makes them visible to the developer. The developer should never be surprised by proving cost.

### 12.2 The Multi-Table Cost Model

[Triton VM](https://triton-vm.org/)'s execution trace is spread across multiple tables. Each instruction contributes rows to different tables simultaneously. The proving cost is determined by the **tallest** table, not the sum. Understanding which table dominates is critical for optimization.

**Table overview:**

| Table | What grows it | Rows per trigger |
|-------|--------------|-----------------|
| Processor Table | Every instruction | 1 row per instruction |
| Hash Table | `hash`, `sponge_init`, `sponge_absorb`, `sponge_absorb_mem`, `sponge_squeeze`, `merkle_step`, `merkle_step_mem` + program attestation | 6 rows per hash op (Tip5 has 5 rounds + 1 setup) |
| U32 Table | `split`, `lt`, `and`, `xor`, `log_2_floor`, `pow`, `div_mod`, `pop_count`, `merkle_step`, `merkle_step_mem` | Variable: depends on operand bit-width |
| Op Stack Table | Every instruction that changes stack depth | 1 row per stack operation |
| RAM Table | `read_mem`, `write_mem`, `sponge_absorb_mem`, `merkle_step_mem`, `xx_dot_step`, `xb_dot_step` | 1 row per word read/written |
| Jump Stack Table | `call`, `return`, `recurse`, `recurse_or_return` | 1 row per jump operation |

**The critical insight**: a program that uses many hash operations may have its proving cost dominated by the Hash Table even if the Processor Table is relatively small. The Trident compiler tracks all table heights independently and reports the dominant table.

### 12.3 Cost Units

Trident uses three cost metrics:

**1. Clock cycles (cc)** — Number of processor instructions executed. This is the Processor Table height. Simple, intuitive, but potentially misleading if coprocessor tables dominate.

**2. Padded height (ph)** — The actual value that determines proving cost: `2^⌈log₂(max_table_height)⌉`. This is the single most important number.

**3. Table profile** — Heights of all six tables, showing which one dominates. This tells the developer *where* to optimize.

```
// Compiler output for a function
pub fn verify_merkle(root: Digest, leaf: Digest, index: U32, depth: U32)
// cost {
//   clock_cycles:     142
//   hash_table:       120    ← dominant (20 merkle_steps × 6 rounds)
//   u32_table:         87
//   op_stack_table:   142
//   ram_table:          0
//   jump_stack_table:   4
//   padded_height:    128    (next power of 2 above 142)
//   proving_time:     ~0.8s  (estimated @ reference hardware)
// }
```

### 12.4 Per-Instruction Cost Table

Every TASM instruction has a known, fixed contribution to each table. The compiler uses this table for static analysis:

| Trident construct | TASM | Processor | Hash | U32 | OpStack | RAM |
|-------------------|------|-----------|------|-----|---------|-----|
| `a + b` | `add` | 1 | 0 | 0 | 1 | 0 |
| `a * b` | `mul` | 1 | 0 | 0 | 1 | 0 |
| `inv(a)` | `invert` | 1 | 0 | 0 | 0 | 0 |
| `a == b` | `eq` | 1 | 0 | 0 | 1 | 0 |
| `a < b` | `lt` | 1 | 0 | * | 1 | 0 |
| `a & b` | `and` | 1 | 0 | * | 1 | 0 |
| `a ^ b` | `xor` | 1 | 0 | * | 1 | 0 |
| `split(a)` | `split` | 1 | 0 | * | 1 | 0 |
| `a /% b` | `div_mod` | 1 | 0 | * | 0 | 0 |
| `pow(b, e)` | `pow` | 1 | 0 | * | 1 | 0 |
| `log2(a)` | `log_2_floor` | 1 | 0 | * | 0 | 0 |
| `popcount(a)` | `pop_count` | 1 | 0 | * | 0 | 0 |
| `hash(...)` | `hash` | 1 | **6** | 0 | 1 | 0 |
| `sponge_init()` | `sponge_init` | 1 | **6** | 0 | 0 | 0 |
| `sponge_absorb(...)` | `sponge_absorb` | 1 | **6** | 0 | 1 | 0 |
| `sponge_squeeze()` | `sponge_squeeze` | 1 | **6** | 0 | 1 | 0 |
| `sponge_absorb_mem(p)` | `sponge_absorb_mem` | 1 | **6** | 0 | 1 | 10 |
| `merkle_step(i, d)` | `merkle_step` | 1 | **6** | * | 0 | 0 |
| `merkle_step_mem(...)` | `merkle_step_mem` | 1 | **6** | * | 0 | 5 |
| `divine()` | `divine 1` | 1 | 0 | 0 | 1 | 0 |
| `pub_read()` | `read_io 1` | 1 | 0 | 0 | 1 | 0 |
| `pub_write(v)` | `write_io 1` | 1 | 0 | 0 | 1 | 0 |
| `ram_read(addr)` | `push + read_mem 1` | 2 | 0 | 0 | 2 | 1 |
| `ram_write(addr, v)` | `push + write_mem 1` | 2 | 0 | 0 | 2 | 1 |
| `xx_dot_step(...)` | `xx_dot_step` | 1 | 0 | 0 | 0 | 6 |
| `xb_dot_step(...)` | `xb_dot_step` | 1 | 0 | 0 | 0 | 4 |
| `assert(x)` | `assert` | 1 | 0 | 0 | 1 | 0 |
| `assert_digest(a, b)` | `assert_vector` | 1 | 0 | 0 | 1 | 0 |
| fn call | `call` | 1 | 0 | 0 | 0 | 0 |
| fn return | `return` | 1 | 0 | 0 | 0 | 0 |

`*` = U32 table contribution depends on operand values (bit decomposition). The compiler uses worst-case (32-bit) for static analysis.

**Note on Hash Table rows**: The [Tip5](https://eprint.iacr.org/2023/107) permutation has 5 rounds. Together with setup, each hash-related instruction contributes 6 rows to the Hash Table. This is why `hash`, `sponge_*`, and `merkle_step` are the most expensive operations — not in clock cycles, but in their coprocessor impact.

### 12.5 Static Cost Computation Algorithm

The compiler computes cost using a straightforward traversal of the AST:

```
function compute_cost(node):
    match node:
        Literal | Variable | Constant:
            return ZERO_COST + stack_manipulation_overhead

        BinaryOp(op, left, right):
            cost_l = compute_cost(left)
            cost_r = compute_cost(right)
            cost_op = INSTRUCTION_COST_TABLE[op]
            return cost_l + cost_r + cost_op

        FunctionCall(name, args):
            cost_args = sum(compute_cost(a) for a in args)
            cost_body = compute_cost(function_body[name])
            cost_call = CALL_OVERHEAD  // call + return = 2 cc
            return cost_args + cost_body + cost_call

        ForLoop(bound, body):
            cost_body = compute_cost(body)
            cost_loop = LOOP_OVERHEAD  // setup + iteration control
            return cost_body * bound + cost_loop

        IfElse(cond, then_branch, else_branch):
            cost_cond = compute_cost(cond)
            cost_then = compute_cost(then_branch)
            cost_else = compute_cost(else_branch)
            // worst case: take the more expensive branch
            return cost_cond + max(cost_then, cost_else) + IF_OVERHEAD

        Assert(expr):
            return compute_cost(expr) + ASSERT_COST

        LetBinding(expr):
            return compute_cost(expr) + STACK_PLACEMENT_COST
```

**For bounded loops with variable iteration count** (`for i in 0..n bounded MAX`), the compiler uses the declared bound `MAX`, not the runtime value `n`. This guarantees the cost annotation is a true upper bound.

**For if/else**, the compiler uses `max(then_cost, else_cost)` — the worst-case branch. Both branches contribute to the trace regardless of which one executes (the non-taken branch is skipped via `skiz`, but the jump overhead remains). The compiler reports both branch costs when they differ significantly.

### 12.6 Padded Height Computation

After computing raw table heights, the compiler determines padded height:

```
function padded_height(program_cost):
    max_height = max(
        program_cost.processor,
        program_cost.hash_table,
        program_cost.u32_table,
        program_cost.op_stack,
        program_cost.ram_table,
        program_cost.jump_stack
    )
    // Also account for program attestation (adds to Hash Table)
    program_hash_rows = ceil(program_size / 10) * 6
    max_height = max(max_height, program_hash_rows)

    return next_power_of_two(max_height)
```

The compiler warns when the program is close to a power-of-2 boundary:

```
warning[W0017]: program is 3 rows below padded height boundary
  --> main.tri
   |
   = note: padded_height = 1024 (max table height = 1021)
   = note: adding 4+ rows to any table will double proving cost to 2048
   = help: consider optimizing to stay well below 1024
```

### 12.7 Proving Time Estimation

The compiler estimates wall-clock proving time from padded height:

```
proving_time ≈ padded_height × columns × log(padded_height) × field_op_time
```

Where:
- `columns` ≈ 300 (total columns across all tables — this is fixed by [Triton VM](https://triton-vm.org/)'s arithmetization)
- `log(padded_height)` accounts for [FRI](https://eccc.weizmann.ac.il/report/2017/134/) folding
- `field_op_time` depends on hardware (~1-5 ns per 64-bit field op)

The compiler reports estimates for reference hardware. Actual times will vary by CPU, but the **relative** costs between functions are reliable.

### 12.8 Cost-Aware Development Workflow

#### Build with costs

```bash
$ trident build --costs

Compiling merkle.tri ... done
Compiling main.tri ... done
Linking ... done

Cost report:
┌─────────────────────────────────────────────────────────────┐
│ Program: merkle_verifier                                    │
├──────────────────────┬──────┬──────┬──────┬──────┬──────────┤
│ Function             │  cc  │ hash │  u32 │  ram │ dominant │
├──────────────────────┼──────┼──────┼──────┼──────┼──────────┤
│ main                 │   22 │   12 │    4 │    0 │ proc     │
│ merkle.verify        │  142 │  120 │   87 │    0 │ proc     │
│   └─ per iteration   │    7 │    6 │    4 │    0 │ hash     │
│ TOTAL (worst case)   │  164 │  132 │   91 │    0 │ proc     │
├──────────────────────┴──────┴──────┴──────┴──────┴──────────┤
│ Padded height: 256                                          │
│ Estimated proving time: ~1.6s                               │
│ Program attestation: 18 hash rows (4 instructions / chunk)  │
└─────────────────────────────────────────────────────────────┘
```

#### Compare functions

```bash
$ trident build --costs --compare

Comparing: compute_inputs_hash vs compute_outputs_hash
┌────────────────────────┬──────┬──────┬──────┐
│                        │  cc  │ hash │  u32 │
├────────────────────────┼──────┼──────┼──────┤
│ compute_inputs_hash    │  412 │  780 │   12 │
│ compute_outputs_hash   │  412 │  780 │   12 │
│                        │  =   │  =   │  =   │
└────────────────────────┴──────┴──────┴──────┘
```

#### Identify bottlenecks

```bash
$ trident build --costs --hotspots

Top 5 cost contributors:
  1. merkle.verify:loop_body     120 hash rows (46% of hash table)
  2. compute_inputs_hash:absorb  768 hash rows (29% of hash table)  ← HOTSPOT
  3. main:divine5                  5 cc  (negligible)
  ...

Recommendation: Hash table dominates. Reduce hash operations to lower padded height.
```

### 12.9 Cost Annotations in Source

The compiler can emit annotated source showing per-line costs:

```bash
$ trident build --annotate
```

Output:

```
pub fn verify(root: Digest, leaf: Digest, index: U32, depth: U32) {
    let mut idx = index                          // cc: 1  hash: 0  u32: 0
    let mut current = leaf                       // cc: 0  hash: 0  u32: 0
    for _ in 0..depth bounded 64 {               // × 64 iterations (worst case)
        (idx, current) = merkle_step(idx, current)  // cc: 1  hash: 6  u32: ~4
    }                                            // subtotal: cc: 64  hash: 384  u32: ~256
    assert_digest(current, root)                 // cc: 1  hash: 0  u32: 0
}
// TOTAL: cc: 66  hash: 384  u32: ~256
// dominant table: hash (384 rows)
// padded height: 512
```

### 12.10 Optimization Guidance

The compiler provides actionable suggestions when it detects common cost antipatterns:

**Pattern 1: Hash table dominance**
```
hint[H0001]: hash table is 3.2x taller than processor table
  = This means processor optimizations will not reduce proving cost.
  = Consider: batching data before hashing, reducing Merkle depth,
    or using sponge_absorb_mem instead of repeated sponge_absorb.
```

**Pattern 2: Power-of-2 boundary proximity**
```
hint[H0002]: padded height is 1024, but max table height is only 519
  = You have 505 rows of headroom before the next doubling.
  = This function could be 97% more complex at zero additional proving cost.
```

**Pattern 3: Redundant range checks**
```
hint[H0003]: as_u32() on line 42 inserts a range check (split instruction)
  = The value was already proven to be U32 on line 38.
  = Removing the redundant check saves 1 cc + ~4 u32 table rows.
```

**Pattern 4: Loop bound waste**
```
hint[H0004]: loop bounded 128 but typical execution uses ~10 iterations
  = Worst-case cost is 128 × 7 = 896 cc, but expected is ~70 cc.
  = If the bound can be tightened, consider bounded 16 or bounded 32.
  = Tightening from 128 to 16 would reduce padded height from 1024 to 256.
```

### 12.11 Cost Invariants

The Trident compiler guarantees the following invariants about cost computation:

1. **Upper bound guarantee**: The reported cost is always ≥ actual cost for any valid input. The cost is exact for constant-bound loops and worst-case for variable-bound loops.

2. **Monotonicity**: Adding code never decreases reported cost. Removing code never increases it. There are no "negative-cost" optimizations that the compiler performs silently.

3. **Compositionality**: The cost of `f(g(x))` equals `cost(f) + cost(g) + call_overhead`. Costs compose linearly. There are no non-local effects.

4. **Determinism**: The same source code always produces the same cost report, regardless of compilation environment. Costs are computed from the AST, not from runtime profiling.

5. **Table-completeness**: All six Triton VM tables are tracked. No table is ignored or approximated (except U32 table, which uses worst-case bit-width estimates).

### 12.12 Cost-Driven Design Decisions

Understanding the cost model informs several architectural decisions in Trident programs:

**Prefer `sponge_absorb_mem` over `sponge_absorb`** when absorbing data from RAM. Both cost 6 Hash Table rows, but `sponge_absorb_mem` avoids the 10 `read_mem` instructions needed to get data from RAM onto the stack first. This saves ~10 processor cycles and ~10 RAM table rows per absorption.

**Minimize [Merkle tree](https://en.wikipedia.org/wiki/Merkle_tree) depth.** Each `merkle_step` costs 6 Hash Table rows + U32 table rows. A depth-20 tree verification: 120 hash rows. A depth-32 tree: 192 hash rows. If the padded height boundary is between 128 and 256, the difference between depth 20 and depth 22 could double proving cost.

**Batch U32 operations.** The U32 coprocessor table grows with bit decompositions. Multiple u32 operations on the same value share decomposition work. The compiler does not currently optimize this automatically — the developer should sequence related u32 operations together.

**Watch program size.** Program attestation hashes the entire program, adding `⌈program_size / 10⌉ × 6` rows to the Hash Table. A 1,000-instruction program adds 600 hash rows just for attestation. This is a fixed overhead that scales with code size, not execution.

---

## 13. Examples

### 13.1 Sum of Squares Proof

```
// main.tri
program sum_of_squares

use std.io.io
use std.core.convert

pub input: [Field; 1]
sec input: [Field; 3]
sec ram: { 17: Field, 42: Field }
pub output: []

fn sum_sq_secret() -> Field {
    let s1: Field = divine()
    let s2: Field = divine()
    let s3: Field = divine()
    s1 * s1 + s2 * s2 + s3 * s3
}

fn sum_sq_ram() -> Field {
    let s4: Field = ram_read(17)
    let s5: Field = ram_read(42)
    s4 * s4 + s5 * s5
}

fn main() {
    let n: Field = pub_read()
    let sum1: Field = sum_sq_secret()
    let sum2: Field = sum_sq_ram()
    assert(n == sum1 + sum2)
}
```

### 13.2 Merkle Proof Verification (Multi-Module)

```
// merkle.tri
module merkle

use std.core.convert

pub const MAX_DEPTH: U32 = 64

pub fn verify(root: Digest, leaf: Digest, index: U32, depth: U32) {
    let mut idx = index
    let mut current = leaf
    for _ in 0..depth bounded MAX_DEPTH {
        (idx, current) = merkle_step(idx, current)
    }
    assert_digest(current, root)
}

pub fn verify_mem(root: Digest, leaf: Digest, index: U32, depth: U32, path_ptr: Field) {
    let mut idx = index
    let mut current = leaf
    let mut ptr = path_ptr
    for _ in 0..depth bounded MAX_DEPTH {
        (ptr, idx, current) = merkle_step_mem(ptr, idx, current)
    }
    assert_digest(current, root)
}
```

```
// main.tri
program merkle_verifier

use merkle
use std.io.io
use std.core.convert

pub input: [Field; 6]       // root (5) + leaf_index (1)
sec input: [Field; 5]       // leaf digest
pub output: []

fn main() {
    let root: Digest = pub_read5()
    let leaf_index: U32 = std.core.convert.as_u32(pub_read())
    let leaf: Digest = divine5()

    merkle.verify(root, leaf, leaf_index, 20)
}
```

### 13.3 Neptune Transaction Validation (Sketch)

```
// neptune/transaction.tri
module neptune.transaction

use merkle
use neptune.mutator_set
use std.crypto.hash
use std.core.convert

pub struct TxKernel {
    inputs_hash: Digest,
    outputs_hash: Digest,
    fee: Field,
}

pub fn validate_kernel(kernel: TxKernel) {
    // Verify inputs hash matches committed inputs
    let computed_hash: Digest = compute_inputs_hash()
    assert_digest(kernel.inputs_hash, computed_hash)

    // Verify outputs hash matches committed outputs
    let computed_out: Digest = compute_outputs_hash()
    assert_digest(kernel.outputs_hash, computed_out)

    // Fee must be non-negative (u32 range)
    let _: U32 = std.core.convert.as_u32(kernel.fee)
}

fn compute_inputs_hash() -> Digest {
    sponge_init()
    let n_inputs: U32 = std.core.convert.as_u32(divine())
    for i in 0..n_inputs bounded 128 {
        let input: [Field; 10] = divine_10()
        sponge_absorb(
            input[0], input[1], input[2], input[3], input[4],
            input[5], input[6], input[7], input[8], input[9]
        )
    }
    let squeezed: [Field; 10] = sponge_squeeze()
    digest_from(squeezed[0], squeezed[1], squeezed[2], squeezed[3], squeezed[4])
}

fn compute_outputs_hash() -> Digest {
    // similar pattern
    sponge_init()
    let n_outputs: U32 = std.core.convert.as_u32(divine())
    for i in 0..n_outputs bounded 128 {
        let output: [Field; 10] = divine_10()
        sponge_absorb(
            output[0], output[1], output[2], output[3], output[4],
            output[5], output[6], output[7], output[8], output[9]
        )
    }
    let squeezed: [Field; 10] = sponge_squeeze()
    digest_from(squeezed[0], squeezed[1], squeezed[2], squeezed[3], squeezed[4])
}

fn divine_10() -> [Field; 10] {
    let a: [Field; 5] = divine5()
    let b: [Field; 5] = divine5()
    [a[0], a[1], a[2], a[3], a[4], b[0], b[1], b[2], b[3], b[4]]
}

fn digest_from(a: Field, b: Field, c: Field, d: Field, e: Field) -> Digest {
    // construct Digest from 5 field elements
    [a, b, c, d, e]
}
```

```
// main.tri
program neptune_tx_validator

use neptune.transaction
use std.io.io

pub input: [Field; 15]    // tx kernel fields
sec input: [Field; ?]     // all secret witness data
pub output: []

fn main() {
    let kernel = neptune.transaction.TxKernel {
        inputs_hash: pub_read5(),
        outputs_hash: pub_read5(),
        fee: pub_read(),
    }
    neptune.transaction.validate_kernel(kernel)
}
```

### 13.4 Recursive STARK Verifier (Structural Sketch)

```
// stark/verifier.tri
module stark.verifier

use stark.fri
use merkle
use std.crypto.hash
use std.core.convert

pub const NUM_ROUNDS: U32 = 32
pub const FRI_DEPTH: U32 = 16

pub struct Claim {
    program_digest: Digest,
    input_hash: Digest,
    output_hash: Digest,
}

pub fn verify(claim: Claim) {
    // 1. Read proof components from non-deterministic input
    let commitment: Digest = divine5()

    // 2. Verify Merkle commitments for each query
    for i in 0..NUM_ROUNDS {
        let idx: U32 = std.core.convert.as_u32(divine())
        let leaf: Digest = divine5()
        merkle.verify(commitment, leaf, idx, FRI_DEPTH)
    }

    // 3. Verify FRI layers
    stark.fri.verify_all_layers(FRI_DEPTH)

    // 4. Verify transition constraints evaluate to zero
    verify_transitions(claim)
}

fn verify_transitions(claim: Claim) {
    // Extension field dot products for constraint evaluation
    let mut acc: XField = xfield(0, 0, 0)
    let ptr_a: Field = divine()  // RAM pointer to constraint evaluations
    let ptr_b: Field = divine()  // RAM pointer to challenge weights

    for _ in 0..256 bounded 256 {
        (acc, ptr_a, ptr_b) = xx_dot_step(acc, ptr_a, ptr_b)
    }

    // Result must be zero
    assert(acc == xfield(0, 0, 0))
}
```

---

## 14. Permanent Exclusions

These are **design decisions**, not roadmap items:

| Feature | Reason |
|---------|--------|
| Strings | No string operations in Triton VM ISA |
| Dynamic arrays | Unpredictable trace length |
| Heap allocation | Non-deterministic memory, no GC |
| Recursion | Unbounded trace; use bounded loops |
| Closures | Requires dynamic dispatch |
| Generics (type-level) | Compile-time complexity, audit difficulty |
| Operator overloading | Hides costs |
| Inheritance / Traits | Complexity without benefit |
| Exceptions | Use assert; failure = no proof |
| Floating point | Not supported by field arithmetic |
| Macros | Source-level complexity |
| Concurrency | VM is single-threaded |
| Wildcard imports | Obscures dependencies |
| Circular dependencies | Prevents deterministic compilation |

### 14.1 Implemented Extensions

These were initially considered future work but have been implemented:

- ~~**Size-generic functions**~~: see Section 5.4
- ~~**Inline TASM**~~: see Section 8.5 (with target tags and stack effect annotations)
- ~~**Pattern matching**~~: see Section 7.4
- ~~**Events (emit/seal)**~~: see Section 8.6
- ~~**Multi-target backends**~~: see Section 16.2
- ~~**Verification annotations**~~: see Section 8.7
- ~~**Test framework**~~: see Section 8.8

### 14.2 Possible Future Additions

- **Pattern matching on structs**: `match p { Point { x: 0, y } => ... }`
- **Const generics in expressions**: `fn foo<M, N>() -> [Field; M + N]`
- **Package registry**: when the ecosystem justifies it
- **Conditional compilation**: for debug/release proving targets
- **Trait-like interfaces**: generic over hash function or backend extension
- **`#[pure]` annotation**: no I/O — enables aggressive verification

---

## 15. Grammar (EBNF)

```ebnf
(* Top-level *)
file          = program_decl | module_decl ;
program_decl  = "program" IDENT use_stmt* declaration* item* ;
module_decl   = "module" IDENT use_stmt* item* ;

(* Imports *)
use_stmt      = "use" module_path ;
module_path   = IDENT ("." IDENT)* ;

(* Declarations (program only) *)
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
event_def     = "pub"? "event" IDENT "{" event_fields "}" ;
event_fields  = event_field ("," event_field)* ","? ;
event_field   = IDENT ":" type ;
fn_def        = "pub"? attribute* "fn" IDENT type_params? "(" params? ")" ("->" type)? block ;
type_params   = "<" IDENT ("," IDENT)* ">" ;
attribute     = "#[" IDENT ("(" attr_arg ")")? "]" ;
attr_arg      = IDENT | expr ;    (* intrinsic(name), requires(pred), ensures(pred), test *)
params        = param ("," param)* ;
param         = IDENT ":" type ;

(* Types *)
type          = "Field" | "XField" | "Bool" | "U32" | "Digest"
              | "[" type ";" array_size "]"
              | "(" type ("," type)* ")"
              | module_path ;
array_size    = INTEGER | IDENT ;                  (* IDENT for size params *)

(* Blocks and Statements *)
block         = "{" statement* expr? "}" ;
statement     = let_stmt | assign_stmt | if_stmt | for_stmt
              | assert_stmt | asm_stmt | match_stmt
              | emit_stmt | seal_stmt
              | expr_stmt | return_stmt ;
emit_stmt     = "emit" IDENT "{" (IDENT ":" expr ",")* "}" ;
seal_stmt     = "seal" IDENT "{" (IDENT ":" expr ",")* "}" ;
asm_stmt      = "asm" asm_annotation? "{" TASM_BODY "}" ;
asm_annotation = "(" asm_target ("," asm_effect)? ")"
               | "(" asm_effect ")" ;
asm_target    = IDENT ;                            (* "triton", "miden", etc. *)
asm_effect    = ("+" | "-") INTEGER ;
match_stmt    = "match" expr "{" match_arm* "}" ;
match_arm     = (literal | "_") "=>" block ;
let_stmt      = "let" "mut"? IDENT (":" type)? "=" expr ;
assign_stmt   = place "=" expr ;
place         = IDENT | place "." IDENT | place "[" expr "]" ;
if_stmt       = "if" expr block ("else" block)? ;
for_stmt      = "for" IDENT "in" expr ".." expr ("bounded" INTEGER)? block ;
assert_stmt   = "assert" "(" expr ")"
              | "assert_eq" "(" expr "," expr ")"
              | "assert_digest" "(" expr "," expr ")" ;
return_stmt   = "return" expr? ;
expr_stmt     = expr ";" ;

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

(* Comments *)
comment       = "//" .* NEWLINE ;     (* single-line only *)
```

---

## 16. Tooling

### 16.1 Compiler Commands

```bash
# Build and compile
trident build                       # compile project to target assembly
trident build --costs               # compile with trace cost report
trident build --hotspots            # identify top cost contributors
trident build --hints               # show optimization suggestions (H0001-H0004)
trident build --annotate            # emit per-line cost annotations in source
trident build --save-costs FILE     # save cost report to file for comparison
trident build --compare             # compare costs between functions
trident build --target triton       # select compilation target (default: triton)
trident build -o output.tasm        # specify output file

# Type checking
trident check                       # type check without compiling
trident check --costs               # type check + cost analysis

# Formatting
trident fmt                         # format all .tri files in project
trident fmt --check                 # check formatting without modifying
trident fmt path/to/file.tri        # format specific file or directory

# Testing
trident test                        # run all #[test] functions
trident test --filter name          # run tests matching filter

# Documentation
trident doc                         # generate documentation with costs

# Formal verification
trident verify                      # verify #[requires]/#[ensures] annotations
trident verify --verbose            # verbose verification output
trident verify --json               # machine-readable JSON output
trident verify --smt PATH           # export SMT-LIB2 queries to file
trident verify --z3                 # use Z3 SMT solver (if available)
trident verify --synthesize         # attempt automatic invariant synthesis

# Content-addressed codebase
trident hash                        # show content hash of entry function
trident hash --full                 # show hashes of all functions

# Project initialization
trident init my_project             # create new project
trident init --lib my_library       # create new library

# Codebase manager
trident ucm add FILE                # parse and store definitions from file
trident ucm list                    # list all stored definitions
trident ucm view NAME               # pretty-print a definition by name
trident ucm rename OLD NEW          # rename a definition (instant, non-breaking)
trident ucm stats                   # show codebase statistics
trident ucm history NAME            # show all versions of a name
trident ucm deps NAME               # show dependency graph for a definition

# Package management
trident deps list                   # list project dependencies
trident deps fetch                  # fetch all dependencies
trident deps check                  # verify dependency integrity

# Code generation
trident generate PROMPT             # generate verified code from natural language

# Semantic analysis
trident equiv FILE1 FILE2           # check semantic equivalence of two functions

# LSP server
trident lsp                         # start Language Server Protocol server
```

### 16.2 Target Selection

The `--target` flag selects the compilation backend:

```bash
trident build --target triton       # Triton VM → .tasm (default)
trident build --target miden        # Miden VM → .masm
trident build --target openvm       # OpenVM RISC-V → .S
trident build --target sp1          # SP1 RISC-V → .S
trident build --target cairo        # Cairo VM → Sierra
```

Each target has its own cost model, instruction set, and output format. The Trident source is identical across targets — only target-tagged `asm` blocks and `ext.*` imports are target-specific.

The default target is `triton`. Projects can set a default in `trident.toml`:

```toml
[project]
name = "my_project"
target = "triton"
```

### 16.3 Integration with Triton VM

```bash
trident build -o program.tasm
triton-cli run program.tasm --input "1 2 3"
triton-cli prove program.tasm --input "1 2 3"
triton-cli verify program.tasm --input "1 2 3" --proof proof.bin
```

### 16.4 LSP Features

The `trident lsp` server implements the Language Server Protocol, providing:

- **Diagnostics** — real-time type errors and warnings as you type
- **Formatting** — format-on-save via the LSP formatting request
- **Go to definition** — jump to function/struct/constant definitions across modules
- **Hover** — type information and trace cost for any expression
- **Completions** — module members, struct fields, builtin functions
- **Signature help** — parameter hints while typing function calls
- **Document symbols** — outline of functions, structs, events, constants

Editor support: VS Code (via Zed extension), Zed (native), Helix, any LSP-compatible editor.

---

## 17. Implementation Status

The compiler is implemented and operational. Current status:

| Component | Status | Lines |
|-----------|--------|------:|
| Lexer + Parser | Complete | ~2,400 |
| Type Checker | Complete | ~2,700 |
| Emitter (5 backends) | Complete | ~2,100 |
| Cost Analyzer (4 models) | Complete | ~1,900 |
| Stack Manager | Complete | ~430 |
| Module Resolver + Linker | Complete | ~630 |
| Standard Library | Complete | 13 modules |
| Formatter | Complete | ~1,200 |
| LSP Server | Complete | ~1,600 |
| Diagnostic Engine | Complete | ~170 |
| Symbolic Verifier | Complete | ~900 |
| SMT Backend | Complete | ~600 |
| Content-Addressed UCM | Complete | ~800 |
| Package Manager | Complete | ~400 |
| CLI (14 commands) | Complete | ~650 |
| Test Suite | 670 tests | — |

Compiler is written in Rust. Total: ~37,000 lines including all backends, verification, and tooling.

---

## 18. Success Criteria

Trident is successful if:

1. Neptune Cash transaction validation can be written in Trident with trace length within 2x of hand-written TASM
2. A developer familiar with Rust can write their first Trident program within 1 hour
3. The compiler remains auditable by a single engineer (currently ~37,000 lines)
4. Security audit of the compiler takes less than 4 weeks
5. The recursive STARK verifier can be expressed in Trident
6. The same source compiles to at least 3 different zkVM targets
7. The module system allows the standard library to be maintained independently from the compiler
8. Formal verification catches real bugs in production contracts

---

## Appendix A: TASM Instruction Mapping (Triton VM Target)

| Trident | TASM | Trace Rows |
|---------|------|------------|
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
| `divine()` | `divine 1` | 1 |
| `assert(x)` | `assert` | 1 |
| `assert_digest(a,b)` | `assert_vector` | 1 |
| `merkle_step(i,d)` | `merkle_step` | 1 |
| `sponge_init()` | `sponge_init` | 1 |
| `sponge_absorb(...)` | `sponge_absorb` | 1 |
| `sponge_squeeze()` | `sponge_squeeze` | 1 |
| `pub_read()` | `read_io 1` | 1 |
| `pub_write(v)` | `write_io 1` | 1 |
| `xx_dot_step(...)` | `xx_dot_step` | 1 |
| `xb_dot_step(...)` | `xb_dot_step` | 1 |
| `for ...(N iters)` | loop + N × body | N × body + 3 |
| `fn call` | `call` + `return` | body + 2 |
| `if cond { }` | `skiz` + jump | body + 2-3 |
| `module.fn()` | `call` (resolved address) | body + 2 |
| `fn_name<N>(...)` | `call` (monomorphized label) | body + 2 |
| `match v { ... }` | `eq` + `skiz` chain (desugared) | arms + N comparisons |
| `emit Event { ... }` | `write_io` per field | 1 per field |
| `seal Event { ... }` | `sponge_init` + `sponge_absorb` + `sponge_squeeze` + `write_io 5` | 13+ (sponge overhead) |
| `asm { ... }` | verbatim TASM | varies |
| `asm(triton) { ... }` | verbatim TASM (target-tagged) | varies |

---

## Appendix B: Comparison with Related Languages

| Feature | Trident | [Cairo 1](https://www.cairo-lang.org/) | [Leo](https://leo-lang.org/) (Aleo) | [Vyper](https://docs.vyperlang.org/) | [Noir](https://noir-lang.org/) |
|---------|---------|---------|------------|-------|------|
| Target VM | Multi-target (5 backends) | Cairo (STARK) | Aleo (SNARK) | EVM | ACIR (SNARK) |
| Module system | Yes (DAG) | Yes (crates) | Yes | No | Yes (crates) |
| IR | None (direct emit) | Sierra | R1CS | None | SSA → ACIR |
| Type system | 5 primitives | Rich | Rich | Basic | Rich |
| Non-determinism | `divine()` | `extern` hints | Implicit | N/A | `oracle` |
| Merkle ops | First-class | Library | N/A | N/A | Library |
| Loop model | Bounded only | Bounded + gas | Bounded | Unbounded | Bounded |
| Heap | No | Yes | No | No | No |
| Recursion | No | Yes | No | No | No |
| Generics | Size only | Yes | Yes | No | Yes |
| Formal verify | Built-in | No | No | External | No |
| Events | `emit`/`seal` | Yes | No | Yes | No |
| Post-quantum | Yes (STARK) | Partial | No | No | No |
| Cost visible | Yes (per-table) | Yes (gas) | No | Yes (gas) | No |
| Inline asm | Yes (target-tagged) | No | No | No | No |
| LSP | Built-in | Plugin | Plugin | External | Plugin |

---

## See Also

- [Language Reference](reference.md) -- Quick lookup: types, operators, builtins, grammar, CLI flags
- [Tutorial](tutorial.md) -- Step-by-step developer guide
- [Programming Model](programming-model.md) -- Triton VM execution model, Neptune transaction model
- [Optimization Guide](optimization.md) -- Cost reduction strategies for all six tables
- [How STARK Proofs Work](stark-proofs.md) -- The proof system underlying every Trident program
- [Error Catalog](errors.md) -- All compiler error messages explained
- [For Developers](for-developers.md) -- Zero-knowledge concepts for conventional programmers
- [For Blockchain Devs](for-blockchain-devs.md) -- Mental model migration from Solidity/Anchor/CosmWasm
- [Vision](vision.md) -- Why Trident exists and what you can build
- [Comparative Analysis](analysis.md) -- Trident vs. Cairo, Leo, Noir, Vyper

---

## Appendix C: The Three Prongs

The name **Trident** reflects the language's three non-negotiable guarantees — the three prongs of its design:

**Prong I — Correct.**
If a program compiles and a proof is generated, the computation is correct. The type system prevents category errors. Assertions enforce constraints. The VM crashes on any violation. There is no "undefined behavior."

**Prong II — Bounded.**
Every program has a compile-time-computable upper bound on its execution trace. No unbounded loops, no dynamic allocation, no recursion. The cost of proving is known before the prover runs. There are no surprises.

**Prong III — Provable.**
Every valid execution produces a STARK proof. The proof is [zero-knowledge](https://en.wikipedia.org/wiki/Zero-knowledge_proof) (secret inputs remain hidden), succinct (logarithmic in trace length), and post-quantum secure (no elliptic curves, no trusted setup). The proof can verify itself — enabling recursive composition.

---

*Trident v0.5 — Correct. Bounded. Provable.*
*This specification is a living document. Extensions are driven by ecosystem needs, not theoretical completeness.*
