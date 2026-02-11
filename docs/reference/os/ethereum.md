# Ethereum

[← Target Reference](../targets.md) | VM: [EVM](../vm/evm.md)

---

## Runtime Parameters

| Parameter | Value |
|---|---|
| VM | EVM |
| Runtime binding | `ext.ethereum.*` |
| Account model | Account |
| Storage model | Key-value (SLOAD/SSTORE) |
| Cost model | Gas |
| Cross-chain | -- (canonical L1) |

## Runtime Binding (`ext.ethereum.*`)

- **Storage** — SLOAD/SSTORE for contract state
- **Account management** — balance queries and account interaction
- **ETH transfers** — send ETH between accounts
- **Event logging** — LOG opcodes for indexed event emission
- **Precompile access** — call EVM precompiled contracts

## Notes

Ethereum is the canonical EVM chain — L1 settlement layer. Same .evm bytecode runs on all EVM-compatible chains with different ext.* bindings.

For VM details, see [evm.md](../vm/evm.md).
