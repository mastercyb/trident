# macOS — Operating System

[← OS Reference](../targets.md) | VM: [ARM64](../vm/arm64.md), [x86-64](../vm/x86-64.md)

---

## Runtime Parameters

| Parameter | Value |
|---|---|
| VM | ARM64 (default), x86-64 |
| Runtime binding | `ext.macos.*` |
| Process model | Multi-process, multi-threaded |
| Storage model | Filesystem (POSIX + APFS) |
| Cost model | Wall-clock time |
| Interop | POSIX syscalls, Mach ports, frameworks |

## Runtime Binding (`ext.macos.*`)

- **Filesystem** — open, read, write, close, stat (planned)
- **Network** — socket, connect, bind, listen (planned)
- **Process** — fork, exec, signal handling (planned)
- **Memory** — mmap, mprotect (planned)

## Notes

macOS targets Apple Silicon (ARM64) and Intel (x86-64) Macs. The compiler
produces Mach-O binaries. Runtime bindings expose macOS syscalls through
the `ext.macos.*` module.

Shares the POSIX-compatible API surface with Linux — most `ext.linux.*`
programs port to `ext.macos.*` with minimal changes.

For VM details, see [arm64.md](../vm/arm64.md) or
[x86-64.md](../vm/x86-64.md).
