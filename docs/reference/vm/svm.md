# eBPF / SVM (Solana Virtual Machine)

[← Target Reference](../targets.md)

---

## VM Parameters

| Parameter | Value |
|---|---|
| Architecture | Register (eBPF) |
| Word size | 64-bit native |
| Hash function | SHA-256 |
| Digest width | 32 bytes |
| Stack depth | 10 registers (r0-r9) |
| Output format | `.so` (eBPF ELF shared object) |
| Cost model | Compute units (per-instruction, budget 200K default, 1.4M max) |

eBPF-based register machine. Programs compile to BPF bytecode and deploy as
shared objects. The 64-bit word size maps naturally to Trident's Goldilocks
field, though SVM operates on raw integers, not field elements.

Instruction costs are fixed per opcode class. Memory access is byte-addressed
within a 32 KB stack and heap region. Requires dedicated `BpfLowering` for
eBPF register conventions (10 registers, different calling convention from
standard RISC-V).

See [os/solana.md](../os/solana.md) for the Solana OS runtime.

---

## Cost Model (Compute Units)

Per-instruction compute unit cost. Budget: 200K default, 1.4M max per
transaction.

| Operation class | CU | Notes |
|---|---:|---|
| Arithmetic / logic | 1 | Most ALU instructions |
| Memory access | 1 | Load/store within 32 KB |
| SHA-256 | 85 per 64 bytes | Syscall (accelerated) |
| Keccak-256 | 36 per 64 bytes | Syscall |
| Logging | 100 per call | sol_log_ syscalls |
| CPI (cross-program) | 1,000 base | Per invocation |
| Syscall overhead | 100 | Per syscall |

Detailed per-instruction cost model planned.
