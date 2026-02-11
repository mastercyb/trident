# Icp (Internet Computer) — Operating System

[← Target Reference](../targets.md) | VM: [WASM](../targets/wasm.md)

---

## Runtime Parameters

| Parameter | Value |
|---|---|
| VM | WASM |
| Runtime binding | `ext.icp.*` |
| Account model | Canister (code + persistent memory) |
| Storage model | Stable memory (up to 2 TiB per subnet) |
| Cost model | Cycles (per-instruction + storage + inter-canister calls) |
| Cross-chain | -- |

## Runtime Binding (`ext.icp.*`)

- **Stable memory** — persistent across upgrades, byte-addressable
- **Inter-canister calls** — async/await semantics
- **HTTP outcalls** — direct HTTPS requests from canisters
- **Timers** — periodic and one-shot timers
- **Threshold crypto** — ECDSA and Schnorr signing (chain-key)
- **Entry points** — `#[update]` (state-changing), `#[query]` (read-only)

## Notes

Programs deploy as canisters — bundles of WASM code + persistent memory
running on subnet replicas. 979K+ canisters deployed (163% annual growth).

Unique among WASM runtimes: supports unbounded computation via automatic
deterministic time slicing (20B+ instructions per call). Canisters maintain
stable memory across upgrades. Concurrent inter-canister calls with
async/await semantics.

Cycle cost model is distinct from gas — cycles are purchased with Icp
tokens and consumed per instruction plus storage.

For WASM VM details (instruction set, lowering path, bytecode format),
see [wasm.md](../targets/wasm.md).
