# WASI — Operating System

[← OS Reference](../../reference/targets.md) | VM: [WASM](../../vm/wasm/README.md)

---

## Runtime Parameters

| Parameter | Value |
|---|---|
| VM | WASM |
| Runtime binding | `wasi.ext.*` |
| Process model | Single-process, capability-based |
| Storage model | Filesystem (capability-scoped) |
| Cost model | Wall-clock time |
| Interop | WASI preview 2, component model |

## Runtime Binding (`wasi.ext.*`)

- Filesystem — open, read, write, close (capability-gated) (planned)
- Clock — monotonic clock, wall clock (planned)
- Random — cryptographic random bytes (planned)
- Stdio — stdin, stdout, stderr (planned)

## Notes

WASI (WebAssembly System Interface) provides a POSIX-like API for WASM
modules running outside the browser. Runtimes include wasmtime, wasmer,
and WasmEdge.

The same `.wasm` binary produced for blockchain OSes (Near, Cosmos, ICP)
can run under WASI — only the host function imports differ. WASI provides
filesystem, clock, and random capabilities instead of blockchain storage
and accounts.

For WASM VM details (instruction set, lowering path, bytecode format),
see [wasm.md](../../vm/wasm/README.md).
