# SP1

[← Target Reference](../../reference/targets.md)

---

## Parameters

| Parameter | Value |
|---|---|
| Architecture | Register (RISC-V) |
| Field | Mersenne31 (p = 2^31 - 1) |
| Field bits | 31 |
| Hash function | Poseidon2 |
| Digest width | 8 field elements |
| Hash rate | 8 field elements |
| Extension field | None |
| Stack depth | 32 (register file) |
| Output format | `.S` (RISC-V assembly) |
| Cost model | Cycles |
| OS | Succinct |

RISC-V zkVM. Single cost metric: cycle count. The 31-bit field means
field elements hold less data than on Goldilocks targets — programs may need
more elements to represent the same values. Requires `RegisterLowering`.

---

## Cost Model (Cycles)

Single metric: CPU cycles. No multi-table complexity — total cycle count
determines proving cost directly.

### Per-Instruction Costs

| Trident construct | Cycles |
|---|---:|
| `a + b`, `a * b`, `a == b` | 1 |
| `a < b`, `a & b`, `a ^ b`, `a /% b` | 1 |
| `hash(...)` | 400 |
| `sponge_init()` | 200 |
| `sponge_absorb(...)` | 200 |
| `sponge_squeeze()` | 200 |
| `merkle_step(...)` | 500 |
| `split(a)` | 2 |
| All other builtins | 1 |
| fn call+return | 4 |
| if/else overhead | 3 |
| for-loop overhead | 5 |

Cryptographic operations dominate — a single `hash()` costs 400x a basic
arithmetic op. Minimize hash calls and Merkle depth for best performance.
