# Browser — Operating System

[← OS Reference](../targets.md) | VM: [WASM](../vm/wasm.md)

---

## Runtime Parameters

| Parameter | Value |
|---|---|
| VM | WASM |
| Runtime binding | `ext.browser.*` |
| Process model | Single-threaded event loop (+ Web Workers) |
| Storage model | IndexedDB, localStorage |
| Cost model | Wall-clock time, frame budget |
| Interop | JavaScript, Web APIs |

## Runtime Binding (`ext.browser.*`)

- **DOM** — element creation, query, mutation (planned)
- **Fetch** — HTTP requests (planned)
- **Storage** — IndexedDB, localStorage (planned)
- **Canvas/WebGL** — 2D/3D rendering (planned)

## Notes

Browser targets web applications via WASM. The compiler produces `.wasm`
modules that load in any modern browser. Runtime bindings expose Web APIs
through the `ext.browser.*` module.

The same `.wasm` bytecode runs in browsers and WASI runtimes — only the
host function imports differ. Browser provides DOM, fetch, and Web APIs
instead of filesystem and clock capabilities.

For WASM VM details (instruction set, lowering path, bytecode format),
see [wasm.md](../vm/wasm.md).
