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
bindings: storage, accounts, syscalls, transaction models.

| Module | Description |
|--------|-------------|
| `ext.neptune.xfield` | XField ops, `xx_dot_step`, `xb_dot_step` |
| `ext.neptune.kernel` | Neptune kernel interface |
| `ext.neptune.proof` | Recursive proof composition |

Each OS provides its own `ext.<os>.*` modules (e.g., `ext.neptune.*`,
`ext.ethereum.*`, `ext.linux.*`). Importing any `ext.<os>.*` module binds
the program to that OS — the compiler rejects cross-OS imports.

See [targets.md Part II](targets.md) for the full OS registry and available
`ext.*` bindings per OS.

---

## See Also

- [Language Reference](language.md) — Core language (types, operators, statements)
- [Provable Computation](provable.md) — Hash, Merkle, extension field, proof composition
- [CLI Reference](cli.md) — Compiler commands and flags
- [Grammar](grammar.md) — EBNF grammar
- [Patterns](patterns.md) — Common patterns and permanent exclusions
- [Target Reference](targets.md) — OS registry, `ext.*` bindings
