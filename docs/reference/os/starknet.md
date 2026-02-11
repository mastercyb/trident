# Starknet

[← Target Reference](../targets.md) | VM: [Cairo VM](../targets/cairo.md)

---

## Runtime Parameters

| Parameter | Value |
|---|---|
| VM | Cairo VM |
| Runtime binding | `ext.starknet.*` |
| Account model | Account |
| Storage model | Key-value |
| Cost model | Steps + builtins |
| Cross-chain | Ethereum L2 |

## Runtime Binding (`ext.starknet.*`)

- **Storage** — contract state read/write operations
- **Account abstraction** — native account abstraction support
- **Cross-contract calls** — invoke other deployed contracts
- **L1/L2 messaging** — send and receive messages between Ethereum L1 and Starknet L2

## Notes

Starknet is a ZK rollup on Ethereum using STARK proofs from Cairo VM.

For VM details, see [cairo.md](../targets/cairo.md).
