# Neptune

[← Target Reference](../targets.md) | VM: [Triton VM](../targets/triton.md)

---

## Runtime Parameters

| Parameter | Value |
|---|---|
| VM | Triton VM |
| Runtime binding | `ext.neptune.*` |
| Account model | UTXO |
| Storage model | Merkle-authenticated |
| Cost model | Proof-based (table rows) |
| Cross-chain | -- |

## Runtime Binding (`ext.neptune.*`)

- **UTXO management** — lock scripts and type scripts for transaction outputs
- **Kernel operations** — interaction with the Neptune kernel
- **Proof generation/verification** — STARK proof lifecycle
- **Registry** — on-chain program registry

## Notes

Neptune is the provable blockchain powered by Triton VM. Programs produce STARK proofs of correct execution.

For VM details, see [triton.md](../targets/triton.md).
