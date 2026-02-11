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
