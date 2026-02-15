# CKB

[← Target Reference](../../reference/targets.md)

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
compressed. Shares `RiscVLowering` with SP1, OPENVM, RISCZERO, JOLT,
POLKAVM, and RISCV.

See [os/nervos.md](../../os/nervos/README.md) for the Nervos CKB OS runtime.

---

## Cost Model (Cycles)

Flat per-instruction cycle cost with higher costs for branches and
multiply.

| Operation class | Cycles | Notes |
|---|---:|---|
| Arithmetic / logic | 1 | ADD, SUB, AND, OR, XOR |
| Multiply / divide | 5 | MUL, DIV, REM |
| Branch | 3 | Conditional and unconditional |
| Load / store | 2 | Memory access (4 MB limit) |
| Syscall | 500+ | Blake2b, secp256k1, etc. |

Detailed per-instruction cost model planned.
