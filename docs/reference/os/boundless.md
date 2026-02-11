# Boundless

[← Target Reference](../targets.md) | VM: [RISC Zero](../vm/risczero.md)

---

## Runtime Parameters

| Parameter | Value |
|---|---|
| VM | RISC Zero |
| Runtime binding | `ext.boundless.*` |
| Account model | Journal I/O |
| Storage model | No persistent storage |
| Cost model | Cycles (segments) |
| Cross-chain | Ethereum verification via Groth16 |

## Runtime Binding (`ext.boundless.*`)

- **Journal I/O** — public inputs and outputs for proof verification
- **Guest-host communication** — data exchange between guest program and host
- **Assumption/composition** — recursive proof composition and assumption verification

## Notes

Boundless is RISC Zero's proving network — proofs verify on any chain with a Groth16 verifier.

For VM details, see [risczero.md](../vm/risczero.md).
