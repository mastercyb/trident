# Polygon Miden

[← Target Reference](../../reference/targets.md) | VM: [MIDEN](../../vm/miden/README.md)

---

## Runtime Parameters

| Parameter | Value |
|---|---|
| VM | MIDEN |
| Runtime binding | `miden.ext.*` |
| Account model | Account |
| Storage model | Merkle-authenticated |
| Cost model | Proof-based (table rows) |
| Cross-chain | Ethereum L2 |

## Runtime Binding (`miden.ext.*`)

- Account management — create and manage Miden accounts
- Note operations — send and receive notes between accounts
- Storage access — read/write account storage slots
- Cross-contract calls — invoke other accounts' interfaces

## Notes

Polygon Miden is a ZK rollup on Ethereum. Client-side proving — users prove their own transactions.

For VM details, see [miden.md](../../vm/miden/README.md).
