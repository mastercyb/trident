# Miden VM

[← Target Reference](../targets.md)

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
No extension field support — programs using `XField` or `ext.neptune.*` cannot
target Miden.
