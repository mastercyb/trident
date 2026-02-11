# WASM (WebAssembly)

[← Target Reference](../targets.md)

---

## Parameters

| Parameter | Value |
|---|---|
| Architecture | Stack |
| Word size | 64-bit (i64), 32-bit (i32) |
| Hash function | -- (runtime-dependent) |
| Digest width | 32 bytes |
| Stack depth | Operand stack (unlimited, bounded by metering) |
| Output format | `.wasm` |
| Cost model | Gas, cycles, or wall-clock (OS-dependent) |

WASM is a VM, not an OS. One `.wasm` binary runs on multiple OSes — only
the runtime binding differs.

## Operating Systems Running WASM

| OS | Runtime | Binding | Notes |
|----|---------|---------|-------|
| Near Protocol | nearcore | `ext.near.*` | 1 contract per account, promise-based async |
| Cosmos (100+ chains) | CosmWasm / wasmd | `ext.cosmwasm.*` | IBC cross-chain, typed entry points |
| Arbitrum | Stylus (ArbOS) | `ext.arbitrum.*` | WASM + EVM coexistence, EVM-compatible gas |
| Icp | Canister runtime | `ext.icp.*` | Deterministic time slicing, persistent memory |
| WASI | wasmtime / wasmer | `ext.wasi.*` | Filesystem, clock, random (planned) |
| Browser | JS runtime | `ext.browser.*` | DOM, fetch, Web APIs (planned) |

## Lowering

All WASM OSes share the same `WasmLowering` path:

```
TIR → WasmLowering → WASM module (.wasm)
```

The WASM bytecode is identical. What differs is:
- **Host functions** — each OS provides different imports (storage,
  messaging, crypto, filesystem)
- **Entry points** — Near uses `#[near]` attributes, CosmWasm uses
  `instantiate`/`execute`/`query`, Stylus uses Solidity ABI, Icp uses
  `#[update]`/`#[query]`, WASI uses `_start`
- **Metering** — Near and Cosmos use gas, Icp uses cycles, Stylus uses
  EVM-compatible gas, WASI uses wall-clock

The runtime binding (`ext.<os>.*`) handles these differences. The compiler
produces one WASM module; the linker injects the correct host function
imports for the target OS.
