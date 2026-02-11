# Sui — Operating System

[← Target Reference](../targets.md) | VM: [MoveVM](../targets/movevm.md)

---

## Runtime Parameters

| Parameter | Value |
|---|---|
| VM | MoveVM |
| Runtime binding | `ext.sui.*` |
| Account model | Object-centric (ownership graph) |
| Storage model | Object store |
| Cost model | Gas |
| Cross-chain | -- |

## Runtime Binding (`ext.sui.*`)

- **Object operations** — create, read, update, delete objects
- **Dynamic fields** — attach and access dynamic fields on objects
- **Shared objects** — consensus-ordered access to shared mutable objects
- **Transfer operations** — transfer object ownership between addresses

## Notes

Sui uses an object-centric model — assets are objects with ownership,
enabling parallel execution of independent object graphs. Transactions
touching disjoint object sets execute concurrently without contention.

Owned objects can be processed without consensus (simple transactions),
while shared objects require consensus ordering. This hybrid approach
achieves high throughput for common operations like token transfers.

The Move type system enforces resource safety at the bytecode level —
objects cannot be duplicated or implicitly destroyed, preventing
double-spend and asset-loss bugs by construction.

For MoveVM details (instruction set, lowering path, bytecode format),
see [movevm.md](../targets/movevm.md).
