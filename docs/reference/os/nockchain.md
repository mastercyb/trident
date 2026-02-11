# Nockchain

[← Target Reference](../targets.md) | VM: [Nock](../vm/nock.md)

---

## Runtime Parameters

| Parameter | Value |
|---|---|
| VM | Nock |
| Runtime binding | `ext.nockchain.*` |
| Account model | UTXO (Notes) |
| Storage model | Merkle-authenticated |
| Cost model | Proof-based (nock reductions) |
| Cross-chain | -- |

## Runtime Binding (`ext.nockchain.*`)

- **Note management** — create and consume UTXO notes
- **Kernel operations** — interaction with the Nockchain kernel
- **Proof composition** — compose and verify STARK proofs

## Notes

Nockchain is a provable blockchain using Nock combinator VM with STARK proofs.

For VM details, see [nock.md](../vm/nock.md).
