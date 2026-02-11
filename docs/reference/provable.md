# Provable Computation (Tier 2) and Recursive Verification (Tier 3)

[← Language Reference](language.md) | [IR Reference](ir.md) | [Target Reference](targets.md)

---

Proof-capable targets only. No meaningful equivalent on non-provable targets.

Three capabilities: cryptographic hashing (sponge + Merkle), non-deterministic
witness input, and extension field arithmetic. Programs using any Tier 2
feature cannot compile for Tier 1-only targets (SP1, OpenVM, Cairo).
See [targets.md](targets.md) for tier compatibility.

---

## Hash and Sponge

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

## Merkle Authentication

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

## Extension Field

The extension field extends `Field` to degree E (E = 3 on Triton VM and Nock).
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

## Proof Composition (Tier 3)

Proofs that verify other proofs. **Triton VM and Nock only.**

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

Tier 3 uses the extension field builtins above plus dedicated IR operations:

- **ProofBlock** — Wraps a recursive verification circuit
- **FoldExt / FoldBase** — FRI folding over extension / base field
- **ExtMul / ExtInvert** — Extension field arithmetic for the verifier

See [ir.md Part I, Tier 3](ir.md) for the full list of 5 recursive operations.

Only Triton VM and Nock support Tier 3. Programs using proof composition
cannot compile for any other target.

---

## See Also

- [Language Reference](language.md) — Core language (types, operators, statements)
- [Standard Library](stdlib.md) — `std.*` modules and OS extensions
- [CLI Reference](cli.md) — Compiler commands and flags
- [Grammar](grammar.md) — EBNF grammar
- [Patterns](patterns.md) — Common patterns and permanent exclusions
- [IR Reference](ir.md) — Compiler intermediate representation (54 ops, 4 tiers)
- [Target Reference](targets.md) — Tier compatibility per VM
