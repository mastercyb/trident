# RISC Zero

[← Target Reference](../targets.md)

---

## VM Parameters

| Parameter | Value |
|---|---|
| Architecture | Register (RISC-V RV32IM) |
| Field | BabyBear 31-bit (p = 2^31 - 2^27 + 1) |
| Field bits | 31 |
| Hash function | SHA-256 (accelerated) |
| Digest width | 32 bytes |
| Extension field | Quartic (degree 4) |
| Stack depth | 32 GP registers |
| Output format | ELF (RISC-V) |
| Cost model | Cycles (segments) |

Dominant zkVM. Standard RISC-V RV32IM instruction set with zk-STARK proofs
and Groth16 wrapping for on-chain verification. Formally verified circuit.
Adopted by 65% of new L2 rollups.

Continuations split long computations into segments proved independently,
enabling unbounded program execution. SHA-256 is hardware-accelerated with
a dedicated coprocessor — 30x faster than SP1 for hash-heavy workloads.

Shares `RiscVLowering` with SP1, OpenVM, Jolt, CKB-VM, PolkaVM, and RISC-V
native. The 31-bit BabyBear field means field elements hold less data than
on Goldilocks targets — similar constraint to SP1's Mersenne31.

See [os/boundless.md](../os/boundless.md) for the Boundless OS runtime.

---

## Cost Model (Cycles / Segments)

Cycle count, split into segments for parallel proving. Each segment is
proved independently and composed.

| Operation class | Cycles | Notes |
|---|---:|---|
| Arithmetic / logic | 1 | Standard RISC-V RV32IM |
| Multiply / divide | 1 | Hardware-accelerated |
| SHA-256 | 68 per block | Accelerated coprocessor (30x vs SP1) |
| Memory access | 1 | Load/store |
| Branch | 1 | Conditional and unconditional |
| Syscall | varies | Host function calls |

Segment size is configurable (default 2^20 cycles). Crossing segment
boundaries adds composition overhead. Detailed cost model planned.
