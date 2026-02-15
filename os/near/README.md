# Near Protocol — Operating System

[← Target Reference](../../reference/targets.md) | VM: [WASM](../../vm/wasm/README.md)

---

## Runtime Parameters

| Parameter | Value |
|---|---|
| VM | WASM |
| Runtime binding | `near.ext.*` |
| Account model | 1 contract per account |
| Storage model | Key-value (per-byte cost) |
| Cost model | Gas (per-WASM-instruction + host function calls) |
| Cross-chain | -- |

## Runtime Binding (`near.ext.*`)

- Storage — key-value store with per-byte read/write costs
- Promises — async cross-contract calls (receipt-based)
- Account/balance — account management, token transfers
- Crypto — SHA-256, ed25519 verification via host functions

## Notes

Near uses a singletonish account model where each account can hold one
contract. Supports both synchronous execution within a contract and
asynchronous cross-contract calls via the promise system.

Gas is metered per WASM instruction with additional charges for host
function calls (storage reads/writes, cross-contract calls, cryptographic
operations).

For WASM VM details (instruction set, lowering path, bytecode format),
see [wasm.md](../../vm/wasm/README.md).
