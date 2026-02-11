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
