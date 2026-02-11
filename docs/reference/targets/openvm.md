# OpenVM

[← Target Reference](../targets.md)

---

## Parameters

| Parameter | Value |
|---|---|
| Architecture | Register (RISC-V) |
| Field | Goldilocks (p = 2^64 - 2^32 + 1) |
| Field bits | 64 |
| Hash function | Poseidon2 |
| Digest width | 8 field elements |
| Hash rate | 8 field elements |
| Extension field | None |
| Stack depth | 32 (register file) |
| Output format | `.S` (RISC-V assembly) |
| Cost model | Cycles |
| Blockchain | OpenVM network |

Same field as Triton/Miden, different hash and architecture. RISC-V backend
with cycle-based cost model.
