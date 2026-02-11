# RISC-V (native)

[← Target Reference](../targets.md)

---

## Parameters

| Parameter | Value |
|---|---|
| Architecture | Register |
| Field | Goldilocks (p = 2^64 - 2^32 + 1) |
| Field bits | 64 |
| Hash function | Software (Tip5 or Poseidon2) |
| Digest width | Configurable |
| Extension field | None |
| Stack depth | 32 GP registers |
| Output format | Machine code (ELF) |
| Cost model | Wall-clock time (no proof cost) |
| Blockchain | None |

Same `RiscVLowering` as SP1/OpenVM but targeting bare-metal RISC-V, not a
zkVM. Useful for embedded execution or cross-compilation testing.
