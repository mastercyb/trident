# Arbitrum (Stylus) — Operating System

[← Target Reference](../targets.md) | VM: [WASM](../vm/wasm.md) + [EVM](../vm/evm.md)

---

## Runtime Parameters

| Parameter | Value |
|---|---|
| VM | WASM (Stylus) + EVM (coexisting) |
| Runtime binding | `ext.arbitrum.*` |
| Account model | Account (EVM-compatible) |
| Storage model | EVM storage (SLOAD/SSTORE) |
| Cost model | Gas (EVM-compatible units, lower cost per WASM op) |
| Cross-chain | Ethereum L2 (rollup) |

## Runtime Binding (`ext.arbitrum.*`)

- **EVM storage** — SLOAD/SSTORE (shared with EVM contracts)
- **Contract calls** — WASM ↔ EVM cross-calls, Solidity ABI compatible
- **msg context** — msg.sender, msg.value, block context
- **Events** — LOG opcodes (EVM-compatible event logging)

## Notes

Arbitrum is the largest Ethereum L2 ($8B+ TVL). Stylus adds WASM execution
alongside EVM — both VMs coexist with full interoperability. WASM contracts
can call and be called by Solidity contracts seamlessly.

10-100x faster than EVM for compute-heavy workloads. 26-50% gas savings on
oracle operations. Gas model is EVM-compatible — same units, same block
limits, but WASM execution costs less per operation.

For WASM VM details (instruction set, lowering path, bytecode format),
see [wasm.md](../vm/wasm.md).
