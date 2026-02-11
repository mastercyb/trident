# Nervos CKB — Operating System

[← Target Reference](../targets.md) | VM: [CKB-VM](../targets/ckb.md)

---

## Runtime Parameters

| Parameter | Value |
|---|---|
| VM | CKB-VM |
| Runtime binding | `ext.nervos.*` |
| Account model | Cell (UTXO-like: lock scripts + type scripts) |
| Storage model | Cell-based |
| Cost model | Cycles |
| Cross-chain | -- |

## Runtime Binding (`ext.nervos.*`)

- **Cell access** — read and manipulate cells (the fundamental state unit)
- **Syscalls** — reading transaction data (inputs, outputs, witnesses, headers)
- **Cryptographic operations** — secp256k1, Blake2b, BLS12-381 via built-in syscalls

## Notes

CKB uses a UTXO-like cell model: all state lives in cells with lock scripts
(authorization) and type scripts (validation). Lock scripts determine who can
consume a cell; type scripts enforce invariants on cell data transformations.

This model provides strong isolation between contracts and enables off-chain
computation patterns — scripts only need to verify state transitions, not
compute them. CKB-VM executes RISC-V instructions directly, so any language
that compiles to RISC-V can target Nervos.

Cost is measured in cycles, corresponding to the number of RISC-V instructions
executed plus syscall overhead.

For CKB-VM details (instruction set, lowering path, bytecode format),
see [ckb.md](../targets/ckb.md).
