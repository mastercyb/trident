# Patterns and Exclusions

[← Language Reference](language.md)

---

## Permanent Exclusions

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

## Common Patterns

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

- [Language Reference](language.md) — Core language (types, operators, statements)
- [Provable Computation](provable.md) — Hash, Merkle, extension field, proof composition
- [Standard Library](stdlib.md) — `std.*` modules and OS extensions
- [CLI Reference](cli.md) — Compiler commands and flags
- [Grammar](grammar.md) — EBNF grammar
