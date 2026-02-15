# Linux — Operating System

[← OS Reference](../../reference/targets.md) | VM: [x86-64](../../vm/x86-64/README.md), [ARM64](../../vm/arm64/README.md), [RISC-V](../../vm/riscv/README.md)

---

## Runtime Parameters

| Parameter | Value |
|---|---|
| VM | x86-64 (default), ARM64, RISC-V |
| Runtime binding | `linux.ext.*` |
| Process model | Multi-process, multi-threaded |
| Storage model | Filesystem (POSIX) |
| Cost model | Wall-clock time |
| Interop | POSIX syscalls, shared libraries |

## Runtime Binding (`linux.ext.*`)

- Filesystem — open, read, write, close, stat (planned)
- Network — socket, connect, bind, listen (planned)
- Process — fork, exec, signal handling (planned)
- Memory — mmap, mprotect (planned)

## Notes

Linux is the standard POSIX OS target. The compiler produces native ELF
binaries for x86-64, ARM64, or RISC-V. Runtime bindings expose Linux
syscalls through the `linux.ext.*` module.

Multiple VMs share this OS — the same `linux.ext.*` API works regardless
of whether the underlying CPU is x86-64, ARM64, or RISC-V.

For VM details, see [x86-64.md](../../vm/x86-64/README.md),
[arm64.md](../../vm/arm64/README.md), or [riscv.md](../../vm/riscv/README.md).
