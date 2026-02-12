# AVM — Aleo Virtual Machine

[← Target Reference](../targets.md)

---

## VM Parameters

| Parameter | Value |
|---|---|
| Architecture | Register (AVM bytecode) |
| Field | Aleo 251-bit (p = 2^251 + ...) |
| Field bits | 251 |
| Hash function | Poseidon (BHP for commitments) |
| Digest width | 1 field element |
| Extension field | None |
| Stack depth | Register-addressed |
| Output format | `.aleo` (AVM bytecode) |
| Cost model | Constraints (off-chain) / microcredits (on-chain finalize) |

Register-based VM with native field arithmetic and ZK-friendly operations.
The 251-bit field is similar to Cairo's STARK-252.

Dual execution model: `transition` functions run off-chain with full privacy
(ZK proof generated), `finalize` functions run on-chain publicly. This split
is explicit in the program structure.

Supports Keccak-256 and ECDSA verification for Ethereum interoperability.

See [os/aleo.md](../os/aleo.md) for the Aleo OS runtime.

---

## Cost Model (Constraints / Microcredits)

Dual cost model matching the dual execution model:

| Context | Cost unit | What determines cost |
|---|---|---|
| `transition` (off-chain) | Constraints | Number of R1CS constraints in the circuit |
| `finalize` (on-chain) | Microcredits | Per-instruction (1 microcredit ≈ 1 Aleo opcode) |

| Operation class | Constraints (off-chain) | Notes |
|---|---:|---|
| Arithmetic | 1-3 | Field add = 1, mul = 1, div = 3 |
| Comparison | 252 | Bit decomposition required |
| Hash (Poseidon) | ~300 | Per permutation |
| Hash (BHP) | ~1,500 | Commitment hash |
| Signature verify | ~10,000 | ed25519 |

Detailed cost model planned.
