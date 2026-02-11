# Aztec (Noir / AVM)

[← Target Reference](../targets.md)

---

## VM Parameters

| Parameter | Value |
|---|---|
| Architecture | Circuit (ACIR) + Register (public AVM) |
| Field | BN254 scalar field 254-bit |
| Field bits | 254 |
| Hash function | Poseidon2 (Pedersen for commitments) |
| Digest width | 1 field element |
| Extension field | None |
| Stack depth | Register-addressed (public VM) |
| Output format | `.acir` (circuit IR) / AVM bytecode |
| Cost model | Gates (private) + Gas (public) |

Dual execution VM: private functions compile to ACIR circuits (proved
client-side), public functions run on the Aztec VM (AVM) on sequencers.

**Noir** is the ZK DSL — Rust-like syntax, proving-system agnostic, compiles
to ACIR (Abstract Circuit Intermediate Representation). 600+ GitHub projects.
Most popular ZK development language. ACIR can target multiple backends:
Barretenberg (default), Plonk variants, others.

Requires dedicated `AcirLowering` for private execution. Public execution
uses a register-based AVM.

See [os/aztec.md](../os/aztec.md) for the Aztec OS runtime.

---

## Cost Model (Gates / Gas)

Dual cost model matching the dual execution model:

| Context | Cost unit | What determines cost |
|---|---|---|
| Private (ACIR) | Gates | Number of arithmetic gates in the circuit |
| Public (AVM) | Gas | Per-instruction gas on sequencer |

| Operation class | Gates (private) | Notes |
|---|---:|---|
| Arithmetic | 1 | Field add/mul |
| Comparison | ~254 | Bit decomposition on BN254 |
| Hash (Poseidon2) | ~400 | Per permutation |
| Hash (Pedersen) | ~1,000 | Commitment hash |
| ECDSA verify | ~15,000 | secp256k1 |

Detailed cost model planned.
