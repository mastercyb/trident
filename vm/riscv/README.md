# RISCV

[← Target Reference](../../reference/targets.md)

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
| OS | Linux |

Same `RiscVLowering` as SP1/OPENVM but targeting bare-metal RISCV, not a
zkVM. Useful for embedded execution, Linux servers, or cross-compilation
testing.

---

## Cost Model (Wall-clock)

No proof cost — direct native execution. Same as x86-64/ARM64: software
modular reduction, no metering or gas.
