# OpenVM Network — Operating System

[← Target Reference](../targets.md) | VM: [OpenVM](../targets/openvm.md)

---

## Runtime Parameters

| Parameter | Value |
|---|---|
| VM | OpenVM |
| Runtime binding | `ext.openvm.*` |
| Account model | Journal I/O |
| Storage model | No persistent storage |
| Cost model | Cycles |
| Cross-chain | -- |

## Runtime Binding (`ext.openvm.*`)

- **Journal I/O** — read inputs from and write outputs to the execution journal
- **Guest-host communication** — syscall interface between guest program and host prover

## Notes

OpenVM is a modular zkVM framework with configurable extensions. Programs
execute in a zero-knowledge proving environment — there is no persistent
on-chain storage in the traditional sense. Instead, programs read inputs
and produce outputs via a journal mechanism.

The guest program runs inside the VM and communicates with the host prover
through a syscall interface. The host provides inputs (private or public),
and the guest writes committed outputs to the journal, which becomes part
of the verifiable proof.

Cost is measured in cycles — the number of VM execution steps, which
directly determines proof generation time and size.

For OpenVM details (instruction set, lowering path, bytecode format),
see [openvm.md](../targets/openvm.md).
