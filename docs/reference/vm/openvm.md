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
| OS | OpenVM network |

Same field as Triton/Miden, different hash and architecture. RISC-V backend
with cycle-based cost model.

---

## Cost Model (Cycles)

Single metric: CPU cycles. Same cost model as SP1 — both use
`CycleCostModel` with identical weights.

### Per-Instruction Costs

| Trident construct | Cycles |
|---|---:|
| `a + b`, `a * b`, `a == b` | 1 |
| `a < b`, `a & b`, `a ^ b`, `a /% b` | 1 |
| `hash(...)` | **400** |
| `sponge_init()` | **200** |
| `sponge_absorb(...)` | **200** |
| `sponge_squeeze()` | **200** |
| `merkle_step(...)` | **500** |
| `split(a)` | 2 |
| All other builtins | 1 |
| fn call+return | 4 |
| if/else overhead | 3 |
| for-loop overhead | 5 |
