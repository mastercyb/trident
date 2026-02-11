# Linux — Operating System

[← OS Reference](../targets.md) | VM: [x86-64](../vm/x86-64.md), [ARM64](../vm/arm64.md), [RISC-V](../vm/riscv.md)

---

## Runtime Parameters

| Parameter | Value |
|---|---|
| VM | x86-64 (default), ARM64, RISC-V |
| Runtime binding | `ext.linux.*` |
| Process model | Multi-process, multi-threaded |
| Storage model | Filesystem (POSIX) |
| Cost model | Wall-clock time |
| Interop | POSIX syscalls, shared libraries |

## Runtime Binding (`ext.linux.*`)

- **Filesystem** — open, read, write, close, stat (planned)
- **Network** — socket, connect, bind, listen (planned)
- **Process** — fork, exec, signal handling (planned)
- **Memory** — mmap, mprotect (planned)

## Notes

Linux is the standard POSIX OS target. The compiler produces native ELF
binaries for x86-64, ARM64, or RISC-V. Runtime bindings expose Linux
syscalls through the `ext.linux.*` module.

Multiple VMs share this OS — the same `ext.linux.*` API works regardless
of whether the underlying CPU is x86-64, ARM64, or RISC-V.

For VM details, see [x86-64.md](../vm/x86-64.md),
[arm64.md](../vm/arm64.md), or [riscv.md](../vm/riscv.md).
