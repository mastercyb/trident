# Aztec

[← Target Reference](../targets.md) | VM: [Aztec/Noir](../targets/aztec.md)

---

## Runtime Parameters

| Parameter | Value |
|---|---|
| VM | Aztec/Noir |
| Runtime binding | `ext.aztec.*` |
| Account model | Note (private UTXO) + public storage |
| Storage model | Note tree + public state |
| Cost model | Gates (private) + Gas (public) |
| Cross-chain | Ethereum L2 (rollup, L1/L2 messaging) |

## Runtime Binding (`ext.aztec.*`)

- **Note management** — private UTXO state creation and consumption
- **Public storage** — read/write public contract state
- **Cross-contract calls** — invoke other contracts (private and public)
- **L1/L2 messaging** — send and receive messages between Ethereum L1 and Aztec L2

## Notes

Dual cost model: private in gates (client-side), public in gas (sequencer).

For VM details, see [aztec.md](../targets/aztec.md).
