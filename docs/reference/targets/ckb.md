# CKB-VM

[← Target Reference](../targets.md)

---

## VM Parameters

| Parameter | Value |
|---|---|
| Architecture | Register (RISC-V rv64imc) |
| Word size | 64-bit |
| Hash function | Blake2b |
| Digest width | 32 bytes |
| Stack depth | 32 GP registers |
| Output format | ELF (RISC-V) |
| Cost model | Cycles (flat per-instruction, higher for branches/mul) |

RISC-V register machine running standard rv64imc (integer, multiply,
compressed). Programs are compiled to Linux-style ELF binaries and loaded
directly by the VM. Any RISC-V toolchain can produce CKB contracts.

Maximum runtime memory is 4 MB in 4 KB pages. Contract size limit: 1 MB
compressed. Shares `RiscVLowering` with SP1, OpenVM, RISC Zero, Jolt,
PolkaVM, and RISC-V native.

See [os/nervos.md](../os/nervos.md) for the Nervos CKB OS runtime.
