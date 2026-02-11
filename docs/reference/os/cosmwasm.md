# Cosmos (CosmWasm) — Operating System

[← Target Reference](../targets.md) | VM: [WASM](../targets/wasm.md)

---

## Runtime Parameters

| Parameter | Value |
|---|---|
| VM | WASM |
| Runtime binding | `ext.cosmwasm.*` |
| Account model | Account-based (Cosmos SDK) |
| Storage model | Key-value (Cosmos KV store) |
| Cost model | Gas (per-WASM-instruction + host function calls) |
| Cross-chain | IBC (Inter-Blockchain Communication) |
| Chains | 100+ (Osmosis, Neutron, Injective, Stargaze, ...) |

## Runtime Binding (`ext.cosmwasm.*`)

- **Storage** — key-value store via Cosmos KV
- **IBC** — cross-chain messaging and contract calls
- **Bank** — token transfers via bank module
- **Staking** — delegation and validator queries
- **Entry points** — typed: `instantiate`, `execute`, `query`, `migrate`

## Notes

CosmWasm is the WASM smart contract platform for the Cosmos SDK. Deployed
across 100+ IBC-connected chains. Security-first design prevents common
Solidity attack vectors: no reentrancy by default, explicit message passing,
typed entry points.

The same `.wasm` output deploys to any CosmWasm-enabled chain — runtime
bindings handle chain-specific module differences. IBC enables cross-chain
contract calls natively.

For WASM VM details (instruction set, lowering path, bytecode format),
see [wasm.md](../targets/wasm.md).
