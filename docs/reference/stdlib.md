# 📚 Standard Library Reference

[← Language Reference](language.md) | [OS Reference](os.md)

---

## Module Map

Available on all targets. These modules provide the core language runtime.

| Module | Key functions |
|--------|---------------|
| `vm.core.field` | `add`, `sub`, `mul`, `neg`, `inv` |
| `vm.core.convert` | `as_u32`, `as_field`, `split` |
| `vm.core.u32` | U32 arithmetic helpers |
| `vm.core.assert` | `is_true`, `eq`, `digest` |
| `vm.io.io` | `pub_read`, `pub_write`, `divine` |
| `vm.io.mem` | `read`, `write`, `read_block`, `write_block` |
| `std.io.storage` | Persistent storage helpers |
| `vm.crypto.hash` | `hash`, `sponge_init`, `sponge_absorb`, `sponge_squeeze` |
| `std.crypto.merkle` | `verify1`..`verify4`, `authenticate_leaf3` |
| `std.crypto.auth` | `verify_preimage`, `verify_digest_preimage` |

---

## `vm.core`

### `core.field` — Field arithmetic

Intrinsics that map directly to the target VM's field operations.
`add`, `sub`, `mul`, `neg`, `inv`. On non-provable targets, these use
software modular reduction.

### `core.convert` — Type conversions

`as_u32`, `as_field`, `split`. Convert between `Field`, `U32`, and
component types. `split` decomposes a field element into its constituent
limbs.

### `core.u32` — Unsigned 32-bit operations

`log2`, `pow`, `popcount`. Higher-level U32 operations built on the
primitive `U32` type.

### `core.assert` — Assertions

`is_true`, `eq`, `digest`. Runtime assertions — on provable VMs, a
failed assertion means no valid proof can be generated. On EVM, assertions
revert. On NOCK, they crash.

---

## `vm.io`

### `io.io` — Public I/O

`pub_read`, `pub_write`, `divine`. The public input/output interface.
`pub_read` reads from the public input stream. `pub_write` writes to the
public output stream. `divine` reads non-deterministic advice (prover
hint).

### `io.mem` — Memory operations

`read`, `write`, `read_block`, `write_block`. Direct RAM access. On stack
machines, these map to RAM table operations. On register machines, these
map to load/store instructions.

---

## `std.io`

### `io.storage` — Persistent storage

Storage wrapper that delegates to mem operations. For OS-level persistent
state (blockchain storage, filesystem), see [os.state](os.md).

---

## `vm.crypto`

### `crypto.hash` — Hash functions

`hash`, `sponge_init`, `sponge_absorb`, `sponge_squeeze`. The hash
function is VM-specific (Tip5 on TRITON/NOCK, Rescue on MIDEN, etc.) but
the API is identical. `hash()` is Tier 1 (all VMs). Sponge operations are
Tier 2 (provable VMs only).

---

## `std.crypto`

### `crypto.merkle` — Merkle authentication

`verify1`..`verify4`, `authenticate_leaf3`. Merkle tree verification
primitives. Tier 2 — require a provable VM with native or emulated Merkle
coprocessor support.

### `crypto.auth` — Authentication

`verify_preimage`, `verify_digest_preimage`. Hash preimage verification
patterns used by Neptune lock scripts and UTXO authorization.

### `crypto.sha256` — SHA-256

Full SHA-256 implementation. Available on all targets (software on
non-SHA-256 VMs, native on RISCZERO).

### `crypto.keccak256` — Keccak-256

Keccak-f[1600] permutation, 24 rounds. Available on all targets (native
on EVM).

### `crypto.poseidon2` — Poseidon2

Full Poseidon2 (t=8, rate=4, x^7 S-box). Available on all targets
(native on SP1, OPENVM, JOLT, AZTEC).

### `crypto.bigint` — Big integer arithmetic

256-bit unsigned integer arithmetic. Used for cross-field operations and
non-native field emulation.

### `crypto.ecdsa` — ECDSA signatures

Signature structure, input reading, range validation. Foundation for
secp256k1 and ed25519 verification.

### `crypto.secp256k1` — secp256k1 (stub)

`point_add`/`scalar_mul` return identity. `verify_ecdsa()` unimplemented.

### `crypto.ed25519` — Ed25519 (stub)

`point_add`/`scalar_mul` return identity. `verify()` incomplete.

### `crypto.poseidon` — Poseidon (placeholder)

Dummy round constants, simplified S-box/MDS. NOT cryptographically secure.
Placeholder for future proper implementation.

---

## Common Patterns

### Read-Compute-Write (Universal)

```trident
fn main() {
    let a: Field = pub_read()
    let b: Field = pub_read()
    pub_write(a + b)
}
```

### Accumulator (Universal)

```trident
fn sum<N>(arr: [Field; N]) -> Field {
    let mut total: Field = 0
    for i in 0..N { total = total + arr[i] }
    total
}
```

### Non-Deterministic Verification (Universal)

```trident
fn prove_sqrt(x: Field) {
    let s: Field = divine()      // prover injects sqrt(x)
    assert(s * s == x)           // verifier checks s^2 = x
}
```

### Merkle Proof Verification (Tier 2)

```trident
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

```trident
event Transfer { from: Digest, to: Digest, amount: Field }

fn process(sender: Digest, receiver: Digest, value: Field) {
    // ... validation ...
    reveal Transfer { from: sender, to: receiver, amount: value }
}
```

---

## 🔗 See Also

- [OS Reference](os.md) — `os.*` portable layer, neuron/signal/token model, extensions
- [Language Reference](language.md) — Types, operators, builtins, grammar, sponge, Merkle, extension field, proof composition
- [VM Reference](vm.md) — VM registry, tier/type/builtin tables
- [CLI Reference](cli.md) — Compiler commands and flags
- [Grammar](grammar.md) — EBNF grammar

---

*Trident v0.5 — Write once. Run anywhere.*
