# Cairo

[← Target Reference](../targets.md)

---

## Parameters

| Parameter | Value |
|---|---|
| Architecture | Register |
| Field | STARK-252 (p = 2^251 + 17 * 2^192 + 1) |
| Field bits | 251 |
| Hash function | Pedersen |
| Digest width | 1 field element |
| Hash rate | 2 field elements |
| Extension field | None |
| Stack depth | 0 (no operand stack — memory-addressed) |
| Output format | `.sierra` |
| Cost model | Steps + builtins |
| OS | Starknet |

The 251-bit field means a single field element can hold values that would
require multiple elements on smaller-field targets. Pedersen hash has a narrow
rate (2 elements) and produces a single-element digest. Stack depth 0 means
all data lives in memory — the compiler manages allocation automatically.

---

## Cost Model (Steps + Builtins)

Cairo measures cost in two dimensions: execution steps and builtin
invocations. Steps are the primary metric; builtins add specialized
coprocessor usage.

| Metric | What grows it | Notes |
|---|---|---|
| Steps | Every instruction | 1 per instruction (some cost more) |
| Builtins | `hash`, `sponge_*` | 1 per hash/sponge invocation |

### Per-Instruction Costs

| Trident construct | Steps | Builtins |
|---|---:|---:|
| `a + b`, `a * b`, `a == b` | 1 | 0 |
| `a < b`, `a & b`, `a ^ b`, `a /% b` | 1 | 0 |
| `hash(...)` | 3 | **1** |
| `sponge_init()` | 5 | **1** |
| `sponge_absorb(...)` | 5 | **1** |
| `sponge_squeeze()` | 5 | **1** |
| All other builtins | 1 | 0 |
| fn call+return | 2 | 0 |
| if/else overhead | 2 | 0 |
| for-loop overhead | 4 | 0 |
