# macOS — Operating System

[← OS Reference](../../reference/targets.md) | VM: [ARM64](../../vm/arm64/README.md), [x86-64](../../vm/x86-64/README.md)

---

## Runtime Parameters

| Parameter | Value |
|---|---|
| VM | ARM64 (default), x86-64 |
| Runtime binding | `macos.ext.*` |
| Process model | Multi-process, multi-threaded |
| Storage model | Filesystem (POSIX + APFS) |
| Cost model | Wall-clock time |
| Interop | POSIX syscalls, Mach ports, frameworks |

## Runtime Binding (`macos.ext.*`)

- Filesystem — open, read, write, close, stat (planned)
- Network — socket, connect, bind, listen (planned)
- Process — fork, exec, signal handling (planned)
- Memory — mmap, mprotect (planned)

## Notes

macOS targets Apple Silicon (ARM64) and Intel (x86-64) Macs. The compiler
produces Mach-O binaries. Runtime bindings expose macOS syscalls through
the `macos.ext.*` module.

Shares the POSIX-compatible API surface with Linux — most `linux.ext.*`
programs port to `macos.ext.*` with minimal changes.

For VM details, see [arm64.md](../../vm/arm64/README.md) or
[x86-64.md](../../vm/x86-64/README.md).
