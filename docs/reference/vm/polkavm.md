# PolkaVM

[← Target Reference](../targets.md)

---

## VM Parameters

| Parameter | Value |
|---|---|
| Architecture | Register (RISC-V) |
| Word size | 64-bit |
| Hash function | Blake2b |
| Digest width | 32 bytes |
| Stack depth | 32 GP registers |
| Output format | PVM blob |
| Cost model | Weight (ref_time + proof_size) |

RISC-V register machine designed for the JAM (Join-Accumulate Machine)
protocol. Programs compile through RISC-V ELF to PVM blob format. Supports
both interpretation and JIT compilation, achieving near-native performance.

Cost model is multi-dimensional: **ref_time** measures computation time,
**proof_size** measures state proof overhead for validators. Both dimensions
are metered independently.

System precompiles: Blake2b hashing, sr25519 signature verification,
contract lifecycle management. Shares `RiscVLowering` with SP1, OpenVM,
RISC Zero, Jolt, CKB-VM, and RISC-V native.

See [os/polkadot.md](../os/polkadot.md) for the Polkadot OS runtime.
