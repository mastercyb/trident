# Android — Operating System

[← OS Reference](../targets.md) | VM: [ARM64](../vm/arm64.md), [x86-64](../vm/x86-64.md)

---

## Runtime Parameters

| Parameter | Value |
|---|---|
| VM | ARM64 (default), x86-64 |
| Runtime binding | `ext.android.*` |
| Process model | Multi-process, sandboxed |
| Storage model | Filesystem (app-scoped) |
| Cost model | Wall-clock time, battery |
| Interop | Android NDK, JNI |

## Runtime Binding (`ext.android.*`)

- **Filesystem** — app-scoped file I/O (planned)
- **Network** — socket, HTTP (planned)
- **Sensors** — accelerometer, GPS, camera (planned)
- **UI** — native activity, surface (planned)

## Notes

Android targets mobile ARM64 devices (and x86-64 emulators). The compiler
produces shared libraries (.so) loadable via Android NDK. Runtime bindings
expose Android-specific APIs through the `ext.android.*` module.

Uses the Linux kernel underneath but with a different userspace (Bionic
libc, app sandbox, permissions model).

For VM details, see [arm64.md](../vm/arm64.md) or
[x86-64.md](../vm/x86-64.md).
