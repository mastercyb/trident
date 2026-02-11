# Solana

[← Target Reference](../targets.md) | VM: [eBPF/SVM](../targets/svm.md)

---

## Runtime Parameters

| Parameter | Value |
|---|---|
| VM | eBPF/SVM |
| Runtime binding | `ext.solana.*` |
| Account model | Stateless programs (state in accounts passed as tx inputs) |
| Storage model | Account-based |
| Cost model | Compute units |
| Cross-chain | -- |

## Runtime Binding (`ext.solana.*`)

- **Account access** — read/write account data passed as transaction inputs
- **Cross-program invocation (CPI)** — call other Solana programs
- **Program-derived addresses (PDAs)** — deterministic address generation for program-owned accounts
- **System program interactions** — account creation, lamport transfers, and system-level operations

## Notes

Solana uses stateless programs — all state lives in accounts passed as transaction inputs.

For VM details, see [svm.md](../targets/svm.md).
