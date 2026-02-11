# Standard Library

[← Language Reference](language.md) | [Target Reference](targets.md)

---

## Universal Modules (`std.*`)

Available on all targets. These modules provide the core language runtime.

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

---

## Portable OS Layer (`std.os.*`)

Available on all blockchain and traditional OSes. These modules provide
target-independent access to OS-level concerns. The compiler lowers each
function to the OS-native mechanism based on `--target`.

Programs using only `std.*` + `std.os.*` are portable across all OSes that
support the required operations. If an OS doesn't support a concept (e.g.,
`caller.id()` on UTXO chains, `transfer.send()` on journal targets), the
compiler emits a clear error.

### `std.os.state` — Portable key-value state

| Function | Signature | Description |
|----------|-----------|-------------|
| `read(key)` | `(key: Field) -> Field` | Read one field element at key |
| `write(key, value)` | `(key: Field, value: Field) -> ()` | Write one field element at key |
| `read_n(key, width)` | `(key: Field, width: U32) -> [Field; N]` | Read N elements starting at key |
| `write_n(key, values)` | `(key: Field, values: [Field; N]) -> ()` | Write N elements starting at key |
| `exists(key)` | `(key: Field) -> Bool` | Check if key has been written |

**Supported:** Account, Stateless, Object, UTXO, Process.
**Compile error:** Journal (no persistent state).

On UTXO chains, the compiler auto-generates the divine-and-authenticate
pattern: divine the value, hash it, Merkle-prove against the state root.
The developer writes `state.read(key)` — the proof machinery is invisible.

### `std.os.caller` — Portable identity

| Function | Signature | Description |
|----------|-----------|-------------|
| `id()` | `() -> Digest` | Identity of the current caller |
| `verify(expected)` | `(expected: Digest) -> Bool` | Check caller matches expected |

**Supported:** Account, Stateless, Object, Process.
**Compile error:** UTXO (no caller concept — use `std.os.auth`), Journal (no identity).

Returns `Digest` — the universal identity container. A 20-byte EVM address,
a 32-byte Solana pubkey, and a 251-bit Starknet felt all fit in a Digest.

### `std.os.auth` — Portable authorization

| Function | Signature | Description |
|----------|-----------|-------------|
| `verify(credential)` | `(credential: Digest) -> ()` | Assert operation is authorized; crash if not |

**Supported:** Account, Stateless, Object, UTXO, Process.
**Compile error:** Journal (no identity).

`auth.verify` is an assertion — it succeeds silently or crashes the VM.
On account chains, it checks the caller address. On UTXO chains, it checks
a hash preimage (divine the secret, hash it, assert the digest matches).
Same source code, different mechanism. This is the only auth mechanism that
works on every OS with identity.

### `std.os.transfer` — Portable value movement

| Function | Signature | Description |
|----------|-----------|-------------|
| `send(to, amount)` | `(to: Digest, amount: Field) -> ()` | Transfer native value |
| `balance(account)` | `(account: Digest) -> Field` | Query account balance |

**Supported:** Account, Stateless, Object, UTXO.
**Compile error:** Journal (no value), Process (no native value).

### `std.os.time` — Portable clock

| Function | Signature | Description |
|----------|-----------|-------------|
| `now()` | `() -> Field` | Current timestamp |
| `block_height()` | `() -> Field` | Current block/slot number |

**Supported:** All OS families.

On blockchain OSes, `now()` returns block/slot timestamp. On traditional
OSes, it returns wall-clock time. On journal targets, it returns the
timestamp provided as public input.

### `std.os.event` — Events (already universal)

`reveal` and `seal` are the event mechanism. They compile to the TIR ops
`Reveal` and `Seal`, which each backend lowers to its native event
mechanism (LOG on EVM, sol_log on Solana, announcements on Neptune).
No additional `std.os.event` module needed — events use language-level
`reveal`/`seal` statements directly.

### The three-tier model

```
std.*          S0 — Proof primitives      All 20 VMs, all 25 OSes
std.os.*       S1 — Portable OS           All blockchain + traditional OSes
ext.<os>.*     S2 — OS-native             One specific OS
```

Programs can mix all three tiers. `std.*` for math and crypto. `std.os.*`
for portable state, auth, and events. `ext.<os>.*` when OS-native features
are needed (PDAs, object ownership, L1/L2 messaging, CPI, etc.).

For per-OS lowering details (what each `std.os.*` function compiles to on
each specific OS), see [targets.md — `std.os.*` Lowering](targets.md).

---

## OS Extensions (`ext.<os>.*`)

Each OS provides its own `ext.<os>.*` modules with runtime-specific
bindings: storage, accounts, syscalls, transaction models. Importing any
`ext.<os>.*` module binds the program to that OS — the compiler rejects
cross-OS imports.

### Implemented

| Module | Description | OS doc |
|--------|-------------|--------|
| `ext.neptune.kernel` | Transaction kernel MAST authentication | [neptune.md](os/neptune.md) |
| `ext.neptune.utxo` | UTXO structure authentication | [neptune.md](os/neptune.md) |
| `ext.neptune.xfield` | Extension field arithmetic intrinsics | [neptune.md](os/neptune.md) |
| `ext.neptune.proof` | Recursive STARK verification | [neptune.md](os/neptune.md) |
| `ext.neptune.recursive` | Low-level recursive proof primitives | [neptune.md](os/neptune.md) |
| `ext.neptune.registry` | On-chain definition registry (5 ops) | [neptune.md](os/neptune.md) |

### Designed (not yet implemented)

| OS | Modules | OS doc |
|----|---------|--------|
| Ethereum | `ext.ethereum.` storage, account, transfer, call, event, block, tx, precompile | [ethereum.md](os/ethereum.md) |
| Solana | `ext.solana.` account, pda, cpi, transfer, system, log, clock, rent | [solana.md](os/solana.md) |
| Starknet | `ext.starknet.` storage, account, call, event, messaging, crypto | [starknet.md](os/starknet.md) |
| Sui | `ext.sui.` object, transfer, dynamic_field, tx, coin, event | [sui.md](os/sui.md) |

See each OS doc for the full API reference. See [targets.md Part II](targets.md)
for the complete OS registry (25 OSes).

---

## See Also

- [Language Reference](language.md) — Core language (types, operators, statements)
- [Provable Computation](provable.md) — Hash, Merkle, extension field, proof composition
- [CLI Reference](cli.md) — Compiler commands and flags
- [Grammar](grammar.md) — EBNF grammar
- [Patterns](patterns.md) — Common patterns and permanent exclusions
- [Target Reference](targets.md) — OS registry, `ext.*` bindings
