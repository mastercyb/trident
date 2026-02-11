# Aleo

[← Target Reference](../targets.md) | VM: [AVM/Leo](../targets/leo.md)

---

## Runtime Parameters

| Parameter | Value |
|---|---|
| VM | AVM/Leo |
| Runtime binding | `ext.aleo.*` |
| Account model | Record (UTXO-like private state) |
| Storage model | On-chain mapping (public) |
| Cost model | Constraints (off-chain) / microcredits (on-chain) |
| Cross-chain | -- |

## Runtime Binding (`ext.aleo.*`)

- **Record management** — private UTXO state creation and consumption
- **On-chain mapping storage** — public key-value storage via mappings
- **Async/await** — cross-program calls via async execution model

## Notes

Privacy-first L1 — programs execute off-chain and produce proofs verified on-chain.

For VM details, see [leo.md](../targets/leo.md).
