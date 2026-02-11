# Succinct

[← Target Reference](../targets.md) | VM: [SP1](../targets/sp1.md)

---

## Runtime Parameters

| Parameter | Value |
|---|---|
| VM | SP1 |
| Runtime binding | `ext.succinct.*` |
| Account model | Journal I/O |
| Storage model | No persistent storage |
| Cost model | Cycles |
| Cross-chain | Ethereum verification |

## Runtime Binding (`ext.succinct.*`)

- **Journal I/O** — public inputs and outputs for proof verification
- **Guest-host communication** — data exchange between guest program and host
- **Proof composition** — compose proofs for recursive verification

## Notes

Succinct is SP1's proving network for verifiable computation.

For VM details, see [sp1.md](../targets/sp1.md).
