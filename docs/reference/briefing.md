# 🤖 Agent Briefing

Machine-optimized compact format for AI code generation.

[Language Reference](language.md) | [Standard Library](stdlib.md) | [CLI](cli.md)

---

### Language Identity

```trident
Name:      Trident
Extension: .tri
Paradigm:  Imperative, bounded, first-order, no heap, no recursion
Domain:    Zero-knowledge provable computation
Targets:   Designed for 20 VMs — provable (TRITON, MIDEN, NOCK, SP1, OPENVM, RISCZERO, JOLT, CAIRO, AVM, AZTEC), blockchain (EVM, WASM, SBPF, MOVEVM, TVM, CKB, POLKAVM), native (X86-64, ARM64, RISCV). Today: Triton VM. See targets.md
Compiler:  trident build <file.tri>
All arithmetic is modular (mod p where p depends on the target).
There is no subtraction operator — use sub(a, b).
```

### File Structure

```trident
program <name>      // Executable (has fn main)
module <name>       // Library (no fn main)
```

Then imports, then items (constants, structs, events, functions).

```trident
program my_program

use vm.crypto.hash
use vm.io.mem

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

```trident
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
The width D varies by target (5 on TRITON, 4 on MIDEN, 8 on SP1/OPENVM, 1 on CAIRO).
XField is Tier 2 only. See [targets.md](targets.md).

NO: enums, sum types, references, pointers, strings, floats, Option, Result.
NO: implicit conversions between types.

### Operators (complete)

```text
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

```trident
let x: Field = 42                              // Immutable
let mut counter: U32 = 0                       // Mutable
let (hi, lo): (U32, U32) = split(x)           // Tuple destructuring
```

### Control Flow

```trident
if condition { body } else { body }            // No else-if; nest instead
for i in 0..32 { body }                        // Constant bound
for i in 0..n bounded 64 { body }             // Runtime bound, declared max
match value { 0 => { } 1 => { } _ => { } }    // Integer/bool/struct patterns + wildcard
return expr                                     // Explicit return or tail expression
```

NO: `while`, `loop`, `break`, `continue`, `else if`, recursion.

### Functions

```trident
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

```text
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

```trident
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

## 🔗 See Also

- [Language Reference](language.md) — Types, operators, builtins, grammar, sponge, Merkle, extension field, proof composition
- [Standard Library](stdlib.md) — `std.*` modules
- [CLI Reference](cli.md) — Compiler commands and flags
- [Grammar](grammar.md) — EBNF grammar
- [OS Reference](os.md) — OS concepts, `os.*` gold standard, extensions
- [Target Reference](targets.md) — OS model, integration tracking, how-to-add checklists
