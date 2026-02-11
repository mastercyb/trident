# SP1

[← Target Reference](../targets.md)

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
