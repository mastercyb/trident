# Ton — Operating System

[← Target Reference](../targets.md) | VM: [TVM](../vm/tvm.md)

---

## Runtime Parameters

| Parameter | Value |
|---|---|
| VM | TVM |
| Runtime binding | `ext.ton.*` |
| Account model | Account (cell-based state) |
| Storage model | Cell-based |
| Cost model | Gas (per-opcode + cell creation/storage) |
| Cross-chain | -- |

## Runtime Binding (`ext.ton.*`)

- **Cell operations** — construct, parse, and manipulate cells (the fundamental data unit)
- **Message sending** — internal and external message dispatch between contracts
- **Contract storage** — persistent state access via cell trees
- **Ton DNS/Storage** — access to Ton DNS resolution and decentralized storage

## Notes

Ton uses a sharding architecture targeting 100K+ TPS across workchains.
Each account's state is stored as a tree of cells, and the TVM operates
directly on cell-based data structures (stacks of cells and continuations).

Telegram integration provides access to 500M+ monthly active users,
making Ton one of the most widely distributed operating systems.
650+ dApps deployed on mainnet.

Gas is metered per TVM opcode with additional charges for cell creation
and persistent storage. The cell-based model means all data — code,
state, messages — is represented as directed acyclic graphs of cells.

For TVM details (instruction set, lowering path, bytecode format),
see [tvm.md](../vm/tvm.md).
