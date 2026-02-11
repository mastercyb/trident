# Jolt

[← Target Reference](../targets.md)

---

## VM Parameters

| Parameter | Value |
|---|---|
| Architecture | Register (RISC-V RV32I) |
| Field | BN254 scalar field 254-bit |
| Field bits | 254 |
| Hash function | Poseidon2 |
| Digest width | 1 field element |
| Extension field | None |
| Stack depth | 32 GP registers |
| Output format | ELF (RISC-V) |
| Cost model | Cycles |

Sumcheck-based SNARK zkVM from a16z. Fundamentally different proof system
from STARK-based VMs (SP1, RISC Zero): uses multivariate polynomials and
lookup tables ("Just One Lookup Table") with the sum-check protocol.

2x faster than RISC Zero/SP1 in some benchmarks. Twist and Shout memory
checking achieves 6x speedup. Highly extensible via "Inlines" — custom
operations added without full precompile overhead.

Standard RISC-V RV32I instruction set. Shares `RiscVLowering` with SP1,
OpenVM, RISC Zero, CKB-VM, PolkaVM, and RISC-V native.

## OS

No dedicated OS. Jolt is a general-purpose proving backend. Proofs
can verify on Ethereum or any chain with a suitable verifier contract.
