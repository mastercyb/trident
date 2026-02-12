# Polygon Miden

[← Target Reference](../targets.md) | VM: [MIDEN](../vm/miden.md)

---

## Runtime Parameters

| Parameter | Value |
|---|---|
| VM | MIDEN |
| Runtime binding | `ext.miden.*` |
| Account model | Account |
| Storage model | Merkle-authenticated |
| Cost model | Proof-based (table rows) |
| Cross-chain | Ethereum L2 |

## Runtime Binding (`ext.miden.*`)

- **Account management** — create and manage Miden accounts
- **Note operations** — send and receive notes between accounts
- **Storage access** — read/write account storage slots
- **Cross-contract calls** — invoke other accounts' interfaces

## Notes

Polygon Miden is a ZK rollup on Ethereum. Client-side proving — users prove their own transactions.

For VM details, see [miden.md](../vm/miden.md).
