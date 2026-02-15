# POLKAVM

[← Target Reference](../../reference/targets.md)

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

Cost model is multi-dimensional: ref_time measures computation time,
proof_size measures state proof overhead for validators. Both dimensions
are metered independently.

System precompiles: Blake2b hashing, sr25519 signature verification,
contract lifecycle management. Shares `RiscVLowering` with SP1, OPENVM,
RISCZERO, JOLT, CKB, and RISCV.

See [os/polkadot.md](../../os/polkadot/README.md) for the Polkadot OS runtime.

---

## Cost Model (Weight)

Two-dimensional metering: ref_time (computation) and proof_size
(state proof overhead).

| Operation class | ref_time | proof_size | Notes |
|---|---:|---:|---|
| Arithmetic / logic | 1 | 0 | Basic RISC-V ops |
| Memory access | 1 | 0 | Load/store |
| Host call | 200-5,000 | varies | Depends on host function |
| Storage read | 5,000 | 80 bytes | Per key |
| Storage write | 10,000 | 80 bytes | Per key |

Both dimensions must stay within block limits. Detailed cost model planned.
