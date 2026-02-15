# 🏔️ MIDEN

[← Target Reference](../../reference/targets.md)

---

## Parameters

| Parameter | Value |
|---|---|
| Architecture | Stack |
| Field | Goldilocks (p = 2^64 - 2^32 + 1) |
| Field bits | 64 |
| Hash function | Rescue-Prime |
| Digest width | 4 field elements |
| Hash rate | 8 field elements |
| Extension field | None |
| Stack depth | 16 |
| Output format | `.masm` |
| Cost model | 4 tables: processor, hash, chiplets, stack |
| OS | Polygon Miden |

Same field as Triton, different hash function and cost model. 4-table model
with a chiplets table that combines hashing, bitwise, and memory operations.
No extension field support — programs using `XField` or `os.neptune.*` cannot
target Miden.

---

## Cost Model (4 tables)

Each instruction contributes rows to multiple tables simultaneously.
Proving cost is determined by the tallest table, not the sum.

| Table | What grows it | Notes |
|---|---|---|
| Processor | Every instruction | 1 per instruction |
| Hash | `hash`, `hperm` | 8 rows per permutation |
| Chiplets | Hashing, bitwise, memory | Combined table |
| Stack | Stack depth changes | 1 per stack op |

### Per-Instruction Costs

| Trident construct | Processor | Hash | Stack |
|---|---:|---:|---:|
| `a + b`, `a * b`, `a == b` | 1 | 0 | 2 |
| `a < b`, `a & b`, `a ^ b`, `a /% b` | 1 | 0 | 2 |
| `hash(...)` | 1 | 8 | 0 |
| `split(a)` | 1 | 0 | 0 |
| All other builtins | 1 | 0 | 0 |
| fn call+return | 2 | 0 | 0 |
| if/else overhead | 2 | 0 | 1 |
| for-loop overhead | 3 | 0 | 1 |

U32 operations use 16 chiplet rows (not shown — chiplets table rarely dominates).
