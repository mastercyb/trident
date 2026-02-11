# Aptos — Operating System

[← Target Reference](../targets.md) | VM: [MoveVM](../targets/movevm.md)

---

## Runtime Parameters

| Parameter | Value |
|---|---|
| VM | MoveVM |
| Runtime binding | `ext.aptos.*` |
| Account model | Account-centric (resources) |
| Storage model | Resource storage |
| Cost model | Gas |
| Cross-chain | -- |

## Runtime Binding (`ext.aptos.*`)

- **Account resources** — read and modify resources stored under accounts
- **Tables** — scalable key-value storage for large collections
- **Coin operations** — mint, burn, transfer via the coin module
- **Multi-agent transactions** — transactions signed by multiple accounts

## Notes

Aptos uses an account-centric resource model — each account stores typed
resources that are governed by Move's linear type system. Resources cannot
be copied or implicitly dropped, ensuring asset safety at the type level.

Block-STM provides optimistic concurrency for parallel execution.
Transactions are speculatively executed in parallel, and conflicts are
detected and resolved via re-execution, achieving high throughput without
requiring explicit dependency declarations.

The Move type system on Aptos enforces module-level encapsulation —
resources can only be created, moved, or destroyed by the module that
defines them.

For MoveVM details (instruction set, lowering path, bytecode format),
see [movevm.md](../targets/movevm.md).
