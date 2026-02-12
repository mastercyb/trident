# Integration Status

Single source of truth for Trident target integration. Every VM and OS
has a machine-readable `[status]` section in its TOML config; this
document aggregates those into a human-readable dashboard.

Last updated: 2025-05 (compiler v0.5).

---

## Part I — Integration Levels

### VM Levels (L0 -- L5)

| Level | Name | Artifact | Example |
|-------|------|----------|---------|
| L0 | Declared | `vm/<vm>.toml` exists | All 20 VMs |
| L1 | Documented | `docs/reference/vm/<vm>.md` exists | All 20 VMs |
| L2 | Scaffold | Legacy `StackBackend` in `src/legacy/backend/` | SP1, OPENVM, CAIRO |
| L3 | Lowering | New-pipeline lowering trait in `src/tir/lower/`, `src/tree/lower/`, or `src/lir/lower/` | Triton, Miden, Nock, x86-64 |
| L4 | Costed | `CostModel` in `src/cost/model/` | TRITON, MIDEN, SP1, OPENVM, CAIRO |
| L5 | Tested | End-to-end compilation tests pass | Triton, Miden |

L2 and L3 are not cumulative. Some VMs skip L2 and go straight to L3
(e.g., Nock has TreeLowering but no legacy StackBackend). Levels
describe what artifacts exist.

### OS Levels (L0 -- L3)

| Level | Name | Artifact | Example |
|-------|------|----------|---------|
| L0 | Declared | `os/<os>.toml` exists, `vm` field references a VM | All 25 OSes |
| L1 | Documented | `docs/reference/os/<os>.md` exists | All 25 OSes |
| L2 | Bound | `ext/<os>/*.tri` runtime bindings exist | Neptune |
| L3 | Tested | End-to-end OS-targeted compilation tests pass | None yet |

---

## Part II — VM Integration Matrix

20 VMs. Checkmarks indicate the level is complete.

| VM | L0 | L1 | L2 | L3 | L4 | L5 | Path | Notes |
|----|:--:|:--:|:--:|:--:|:--:|:--:|------|-------|
| triton | Y | Y | Y | Y | Y | Y | tir (StackLowering) | Primary target. 6-table cost model. 30+ lowering tests. |
| miden | Y | Y | Y | Y | Y | Y | tir (StackLowering) | 4-table cost model. 8+ Miden-specific tests. |
| nock | Y | Y | -- | Y | -- | -- | tree (TreeLowering) | Jets stubbed. Noun-based lowering. |
| sp1 | Y | Y | Y | -- | Y | -- | legacy | RISC-V scaffold. CycleCostModel. |
| openvm | Y | Y | Y | -- | Y | -- | legacy | RISC-V scaffold. CycleCostModel. |
| cairo | Y | Y | Y | -- | Y | -- | legacy | Sierra scaffold. CairoCostModel. |
| x86-64 | Y | Y | -- | Y | -- | -- | lir (RegisterLowering) | Native target. todo!() stubs in lowering. |
| arm64 | Y | Y | -- | Y | -- | -- | lir (RegisterLowering) | Native target. todo!() stubs in lowering. |
| riscv | Y | Y | -- | Y | -- | -- | lir (RegisterLowering) | Native target. todo!() stubs in lowering. |
| evm | Y | Y | -- | -- | -- | -- | none | Planned: specialized EvmLowering. |
| wasm | Y | Y | -- | -- | -- | -- | none | Planned: specialized WasmLowering. |
| tvm | Y | Y | -- | -- | -- | -- | none | TON VM. Planned: StackLowering. |
| sbpf | Y | Y | -- | -- | -- | -- | none | Solana SBPF. Planned: SbpfLowering. |
| movevm | Y | Y | -- | -- | -- | -- | none | Planned: MoveLowering. |
| avm | Y | Y | -- | -- | -- | -- | none | Aleo Virtual Machine. |
| aztec | Y | Y | -- | -- | -- | -- | none | AZTEC/ACIR. Planned: AcirLowering. |
| risczero | Y | Y | -- | -- | -- | -- | none | RISC-V zkVM. |
| jolt | Y | Y | -- | -- | -- | -- | none | Lookup-based zkVM. |
| ckb | Y | Y | -- | -- | -- | -- | none | CKB (RISC-V). |
| polkavm | Y | Y | -- | -- | -- | -- | none | Polkadot RISC-V. |

### Lowering Path Summary

| Path | Pipeline | VMs | Status |
|------|----------|-----|--------|
| **tir** (StackLowering) | TIR -> stack instructions | triton, miden | Production |
| **tree** (TreeLowering) | TIR -> Noun combinators | nock | Partial (jets stubbed) |
| **lir** (RegisterLowering) | TIR -> LIR -> register instructions | x86-64, arm64, riscv | Scaffold (todo!() bodies) |
| **legacy** (StackBackend) | Legacy emitter pipeline | sp1, openvm, cairo | Functional but deprecated |
| **none** | Not started | 11 VMs | -- |

Planned specialized lowering traits (not yet implemented):
EvmLowering, WasmLowering, BpfLowering, MoveLowering, AcirLowering,
KernelLowering.

---

## Part III — OS Integration Matrix

25 OSes. Each OS references exactly one VM.

| OS | L0 | L1 | L2 | L3 | VM | ext/ modules | Notes |
|----|:--:|:--:|:--:|:--:|-----|:------------:|-------|
| neptune | Y | Y | Y | -- | triton | 6 | kernel, proof, recursive, registry, utxo, xfield |
| ethereum | Y | Y | -- | -- | evm | 0 | Account model. Deep doc. |
| solana | Y | Y | -- | -- | sbpf | 0 | Account model. Deep doc. |
| starknet | Y | Y | -- | -- | cairo | 0 | Account model. Deep doc. |
| sui | Y | Y | -- | -- | movevm | 0 | Object model. Deep doc. |
| miden | Y | Y | -- | -- | miden | 0 | Account + note model. |
| aleo | Y | Y | -- | -- | avm | 0 | Record/UTXO model. |
| aptos | Y | Y | -- | -- | movevm | 0 | Account model (Move). |
| arbitrum | Y | Y | -- | -- | wasm | 0 | EVM L2 (Stylus WASM). |
| aztec | Y | Y | -- | -- | aztec | 0 | Private L2 (Noir). |
| boundless | Y | Y | -- | -- | risczero | 0 | Verifiable compute (journal). |
| cosmwasm | Y | Y | -- | -- | wasm | 0 | Cosmos WASM contracts. |
| icp | Y | Y | -- | -- | wasm | 0 | Internet Computer canisters. |
| near | Y | Y | -- | -- | wasm | 0 | NEAR WASM contracts. |
| nervos | Y | Y | -- | -- | ckb | 0 | CKB cell model. |
| nockchain | Y | Y | -- | -- | nock | 0 | Nock combinator chain. |
| openvm-network | Y | Y | -- | -- | openvm | 0 | Verifiable compute (journal). |
| polkadot | Y | Y | -- | -- | polkavm | 0 | Polkadot parachains. |
| succinct | Y | Y | -- | -- | sp1 | 0 | SP1 verifiable compute (journal). |
| ton | Y | Y | -- | -- | tvm | 0 | TON cell-based contracts. |
| android | Y | Y | -- | -- | arm64 | 0 | Mobile native (ARM64). |
| browser | Y | Y | -- | -- | wasm | 0 | Browser WASM runtime. |
| linux | Y | Y | -- | -- | x86-64 | 0 | POSIX native. |
| macos | Y | Y | -- | -- | arm64 | 0 | Apple native (ARM64). |
| wasi | Y | Y | -- | -- | wasm | 0 | WASM System Interface. |

---

## Part IV — Standard Library Status

19 modules in `std/`.

| Module | File | Status | Notes |
|--------|------|--------|-------|
| std.target | std/target.tri | Hardcoded | Triton-only constants (DIGEST_WIDTH=5, HASH_RATE=10, etc.). Needs target-aware codegen. |
| std.core.field | std/core/field.tri | Done | Field arithmetic intrinsics (add, mul, sub, neg, inv). |
| std.core.convert | std/core/convert.tri | Done | Type conversion intrinsics (as_u32, as_field, split). |
| std.core.u32 | std/core/u32.tri | Done | U32 operations (log2, pow, popcount). |
| std.core.assert | std/core/assert.tri | Done | Assertion intrinsics (is_true, eq, digest). |
| std.io.io | std/io/io.tri | Done | Public I/O (read, write, divine). |
| std.io.mem | std/io/mem.tri | Done | RAM access (read, write, read_block, write_block). |
| std.io.storage | std/io/storage.tri | Done | Storage wrapper (delegates to mem). |
| std.crypto.hash | std/crypto/hash.tri | Done | Tip5 hash with sponge API (intrinsics). |
| std.crypto.merkle | std/crypto/merkle.tri | Done | Merkle tree verification (verify1--4, leaf auth). |
| std.crypto.auth | std/crypto/auth.tri | Done | Preimage verification, Neptune lock script pattern. |
| std.crypto.bigint | std/crypto/bigint.tri | Done | 256-bit unsigned integer arithmetic. |
| std.crypto.sha256 | std/crypto/sha256.tri | Done | SHA-256 implementation. |
| std.crypto.keccak256 | std/crypto/keccak256.tri | Done | Keccak-f[1600] permutation, 24 rounds. |
| std.crypto.poseidon2 | std/crypto/poseidon2.tri | Done | Full Poseidon2 (t=8, rate=4, x^7 S-box). |
| std.crypto.ecdsa | std/crypto/ecdsa.tri | Done | Signature structure, input reading, range validation. |
| std.crypto.poseidon | std/crypto/poseidon.tri | Placeholder | Dummy round constants, simplified S-box/MDS. NOT cryptographically secure. |
| std.crypto.ed25519 | std/crypto/ed25519.tri | Stub | point_add/scalar_mul return identity. verify() incomplete. |
| std.crypto.secp256k1 | std/crypto/secp256k1.tri | Stub | point_add/scalar_mul return identity. verify_ecdsa() unimplemented. |

Summary: 15 done, 1 placeholder, 2 stubs, 1 hardcoded.

---

## Part V — How to Add a New VM

Step-by-step checklist with exact file paths.

### L0 — Declare

- [ ] Create `vm/<vm>.toml` with all sections:
  `[target]`, `[field]`, `[stack]`, `[hash]`, `[extension_field]`, `[cost]`, `[status]`
- [ ] Set `[status] level = 0`
- [ ] Verify `--target <vm>` resolves (the compiler reads `vm/` at startup)

### L1 — Document

- [ ] Create `docs/reference/vm/<vm>.md` — include architecture, word size,
  instruction set summary, cost model parameters, and hash function
- [ ] Add the VM to the VM Registry table in `targets.md`
- [ ] Update the VM Integration Matrix in this file
- [ ] Set `[status] level = 1`

### L2 — Scaffold (optional, legacy path)

Only if using the legacy emitter pipeline. New VMs should prefer L3.

- [ ] Create `src/legacy/backend/<vm>.rs` implementing `StackBackend`
- [ ] Register in `src/legacy/backend/mod.rs` factory (`create_backend()`)
- [ ] Set `[status] level = 2`, `lowering_path = "legacy"`

### L3 — Lowering (pick one path)

| Path | Trait | Location | Factory |
|------|-------|----------|---------|
| Stack | `StackLowering` | `src/tir/lower/<vm>.rs` | `create_stack_lowering()` in `src/tir/lower/mod.rs` |
| Register | `RegisterLowering` | `src/lir/lower/<vm>.rs` | `create_register_lowering()` in `src/lir/lower/mod.rs` |
| Tree | `TreeLowering` | `src/tree/lower/<vm>.rs` | `create_tree_lowering()` in `src/tree/lower/mod.rs` |
| Specialized | Dedicated trait | Dedicated module | Per-trait factory |

- [ ] Implement the chosen lowering trait
- [ ] Register in the appropriate factory function
- [ ] Set `[status] level = 3`, `lowering = "<TraitName>"`, `lowering_path = "<path>"`

### L4 — Cost

- [ ] Create `src/cost/model/<vm>.rs` implementing `CostModel`
- [ ] Register in `src/cost/model/mod.rs` factory (`create_cost_model()`)
- [ ] Set `[status] level = 4`, `cost_model = true`

### L5 — Test

- [ ] Add lowering tests (e.g., `src/tir/lower/tests.rs` or equivalent for
  tree/register paths)
- [ ] Add end-to-end compilation tests
- [ ] Verify `cargo test` passes with the new VM
- [ ] Set `[status] level = 5`, `tests = true`

### Finalize

- [ ] Update `[status]` in `vm/<vm>.toml` to reflect completed level
- [ ] Update the VM Integration Matrix in this file
- [ ] Update the Lowering Path Summary table if a new path was used

---

## Part VI — How to Add a New OS

### L0 — Declare

- [ ] Create `os/<os>.toml` with sections:
  `[os]`, `[runtime]`, `[cross_chain]`, `[status]`
- [ ] The `vm` field in `[os]` must reference an existing VM in `vm/`
- [ ] Set `[status] level = 0`

### L1 — Document

- [ ] Create `docs/reference/os/<os>.md` — include programming model,
  state model, ext.* API surface, and deployment patterns
- [ ] Add the OS to the OS Registry table in `targets.md`
- [ ] Update the OS Integration Matrix in this file
- [ ] Set `[status] level = 1`

### L2 — Bind

- [ ] Create `ext/<os>/` directory
- [ ] Write `.tri` binding modules (one per concern: storage, account,
  transfer, events, etc.)
- [ ] Each file declares `module ext.<os>.<name>`
- [ ] Set `[status] level = 2`, `ext_modules = <count>`,
  `notes = "<comma-separated module names>"`

### L3 — Test

- [ ] Add end-to-end compilation tests targeting this OS
- [ ] Verify ext.* module resolution works
- [ ] Set `[status] level = 3`, `tests = true`

### Finalize

- [ ] Update `[status]` in `os/<os>.toml`
- [ ] Update the OS Integration Matrix in this file

---

## Part VII — How to Add a std/ Module

1. Create `std/<category>/<name>.tri` with `module std.<category>.<name>`
2. Implement functions. Use `#[intrinsic]` for VM-native operations.
3. Determine status: Done, Stub, Placeholder, or Hardcoded.
4. Update the Standard Library Status table in this file.
5. If the module is target-specific, document which targets support it
   in `docs/reference/stdlib.md`.

---

## See Also

- [Targets Reference](targets.md) — VM Registry, OS Registry, cost models
- [IR Reference](ir.md) — TIROp variants, lowering paths, pipeline
- [Standard Library](stdlib.md) — `std.*` module API documentation
- [Language Reference](language.md) — Types, operators, builtins

---

*Trident v0.5 — Write once. Run anywhere.*
