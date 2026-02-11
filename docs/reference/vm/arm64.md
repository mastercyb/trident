# ARM64

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
| Stack depth | 31 GP registers |
| Output format | Machine code (ELF / Mach-O) |
| Cost model | Wall-clock time (no proof cost) |
| OS | macOS, Linux, Android |

Same as x86-64 but for ARM-based machines (Apple Silicon, AWS Graviton).
