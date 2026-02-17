# 🎯 Trident Target Reference

[← Language Reference](language.md) | [VM Reference](vm.md) | [OS Reference](os.md)

Write once. Run anywhere.

---

## The OS Model

An OS is a runtime that loads programs, manages I/O, enforces billing, and
provides storage. A blockchain is one kind of OS. Linux is another.

The VM is the CPU — the instruction set architecture. The OS is the
runtime — storage, accounts, syscalls, billing. One VM can power multiple
OSes, just as one CPU architecture runs multiple operating systems.

| Concept | Range |
|---------|-------|
| CPU / ISA | X86-64, ARM64, RISCV, TRITON, MIDEN, CAIRO, EVM, WASM, SBPF, MOVEVM, TVM, CKB, POLKAVM, NOCK, SP1, OPENVM, RISCZERO, JOLT, AVM, AZTEC |
| OS / Runtime | Linux, macOS, Android, WASI, Browser, Neptune, Polygon Miden, Starknet, Ethereum, Solana, Near, Cosmos, Sui, Aptos, Ton, Nervos, Polkadot, Aleo, Aztec, Boundless |
| Word size | 32-bit, 64-bit, 256-bit (EVM), 257-bit (TVM), field elements (31-bit to 254-bit) |
| System calls | POSIX (read, write, mmap), WASI (fd_read, fd_write), browser (fetch, DOM), provable (pub_read, pub_write, hint), blockchain (storage, cross-contract, IBC, XCM) |
| Process model | Multi-threaded, sequential deterministic, parallel (Sui, Aptos), event loop (Browser) |
| Billing | Wall-clock, cost tables (rows, cycles, steps, gates), gas, compute units, weight |

The compiler does two jobs, just like gcc:

1. Instruction selection (VM/CPU) — translate IR ops to the target VM's
   native instructions. This is the same job gcc does for x86-64 vs ARM64.

2. Runtime binding (OS) — link against OS-specific modules
   (`os.<os>.*`) that provide transaction models, account structures,
   storage layouts, and syscall conventions. This is the same job libc
   does — it differs between Linux and macOS even on the same CPU.

### Target Resolution

A target is either a VM or an OS. The compiler resolves `--target <name>`
by checking OS configs first, then VM configs:

1. Is `<name>` an OS? → load `os/<name>.toml`, derive VM from `vm` field
2. Is `<name>` a VM? → load `vm/<name>.toml`, no OS (bare compilation)
3. Neither → error: unknown target

```trident
trident build --target neptune     # OS → derives vm="triton" → full compilation
trident build --target ethereum    # OS → derives vm="evm" → EVM + Ethereum runtime
trident build --target linux       # OS → derives vm="x86-64" → native + Linux runtime
trident build --target wasi        # OS → derives vm="wasm" → WASM + WASI runtime
trident build --target triton      # bare VM → TRITON, no OS
trident build --target evm         # bare VM → EVM bytecode, no OS
trident build --target wasm        # bare VM → generic WASM, no OS
```

When targeting an OS, `os.<os>.*` modules are automatically available.
When targeting a bare VM, using `os.<os>.*` modules is a compile error —
there is no OS to bind against.

---

## Integration Levels

### VM Levels (L0 -- L5)

| Level | Name | Artifact | Example |
|-------|------|----------|---------|
| L0 | Declared | `vm/<vm>.toml` exists | All 20 VMs |
| L1 | Documented | `reference/vm/<vm>.md` exists | All 20 VMs |
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
| L1 | Documented | `reference/os/<os>.md` exists | All 25 OSes |
| L2 | Bound | `os/<os>/*.tri` runtime bindings exist | Neptune |
| L3 | Tested | End-to-end OS-targeted compilation tests pass | None yet |

---

## VM Integration Matrix

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
| tir (StackLowering) | TIR -> stack instructions | triton, miden | Production |
| tree (TreeLowering) | TIR -> Noun combinators | nock | Partial (jets stubbed) |
| lir (RegisterLowering) | TIR -> LIR -> register instructions | x86-64, arm64, riscv | Scaffold (todo!() bodies) |
| legacy (StackBackend) | Legacy emitter pipeline | sp1, openvm, cairo | Functional but deprecated |
| none | Not started | 11 VMs | -- |

Planned specialized lowering traits (not yet implemented):
EvmLowering, WasmLowering, BpfLowering, MoveLowering, AcirLowering,
KernelLowering.

---

## OS Integration Matrix

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

## Standard Library Status

19 modules in `std/`.

| Module | File | Status | Notes |
|--------|------|--------|-------|
| std.target | std/target.tri | Hardcoded | Triton-only constants (DIGEST_WIDTH=5, HASH_RATE=10, etc.). Needs target-aware codegen. |
| vm.core.field | std/core/field.tri | Done | Field arithmetic intrinsics (add, mul, sub, neg, inv). |
| vm.core.convert | std/core/convert.tri | Done | Type conversion intrinsics (as_u32, as_field, split). |
| vm.core.u32 | std/core/u32.tri | Done | U32 operations (log2, pow, popcount). |
| vm.core.assert | std/core/assert.tri | Done | Assertion intrinsics (is_true, eq, digest). |
| vm.io.io | std/io/io.tri | Done | Public I/O (read, write, divine). |
| vm.io.mem | std/io/mem.tri | Done | RAM access (read, write, read_block, write_block). |
| std.io.storage | std/io/storage.tri | Done | Storage wrapper (delegates to mem). |
| vm.crypto.hash | std/crypto/hash.tri | Done | Tip5 hash with sponge API (intrinsics). |
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

## Warriors

Trident is the weapon. **Warriors** wield it on specific battlefields.

A **battlefield** is a target — a VM+OS combination where compiled code
runs, proves, and deploys. Every battlefield has two dimensions:

- **Terrain** — the VM (instruction set architecture). Triton, Miden,
  EVM, WASM, x86-64. The ground the warrior fights on.
- **Region** — the OS (runtime environment). Neptune, Ethereum, Solana,
  Linux. The jurisdiction that defines the rules of engagement.

`--target neptune` selects a battlefield: Neptune region, Triton terrain.
`--target triton` selects bare terrain — no region, just raw ground.

A warrior is an external binary trained for a specific terrain+region
combination. It takes Trident's compiled output and handles execution,
proving, and deployment. Trident stays clean — zero heavy dependencies.
Warriors bring the VM runtime, the prover, the GPU acceleration, and
the chain client.

### Why Warriors

Adding triton-vm, neptune-core, and wgpu directly to Trident makes it a
monolith. One VM's dependencies pollute every build. Twenty VMs would be
unmanageable. Warriors solve this: each is a separate crate with its own
dependency tree. Install only what you need.

### The `[warrior]` Section

VM target configs (`vm/<vm>/target.toml`) can declare a warrior:

```toml
[warrior]
name = "trisha"
crate = "trident-trisha"
runner = true
prover = true
```

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Warrior name (used for `trident-<name>` binary lookup) |
| `crate` | string | Rust crate name (for `cargo install`) |
| `runner` | bool | Warrior can execute programs |
| `prover` | bool | Warrior can generate proofs |

### Discovery

Trident discovers warriors via PATH, following the git subcommand
convention (`trident-<name>`, like `git-<subcommand>`).

Resolution order:

1. `trident-<target>` on PATH (direct match)
2. Target's `[warrior]` config → `trident-<warrior.name>` on PATH
3. If target is an OS → underlying VM's warrior config

If no warrior is found, Trident compiles the program and prints
installation guidance.

### Warrior Commands

Three CLI commands delegate to warriors:

| Command | What the warrior does |
|---------|----------------------|
| `trident run` | Execute compiled program on the target VM |
| `trident prove` | Generate a STARK/SNARK proof of execution |
| `trident verify` | Verify a proof against its claim |

All other commands (`build`, `check`, `audit`, `fmt`, `bench`, etc.)
run locally in Trident with zero warrior involvement.

### Warrior vs Trident Responsibilities

| Trident provides | Warriors provide |
|-------------------|------------------|
| Compilation (.tri → assembly) | VM execution (run the assembly) |
| PrimeField trait + field impls | VM-specific runtime (triton-vm, etc.) |
| Generic Poseidon2 sponge | GPU-accelerated proving (wgpu, CUDA) |
| Proof cost estimation | Actual proof generation |
| ProgramBundle artifact format | Chain deployment (neptune-core, etc.) |
| `audit` (formal verification) | `verify` (proof verification) |

### Current Warriors

| Warrior | Crate | VM | OS | Status |
|---------|-------|----|----|--------|
| Trisha | `trident-trisha` | Triton | Neptune | Planned |

### How to Build a Warrior

A warrior is a Rust crate that depends on `trident-lang`:

```toml
[dependencies]
trident-lang = "0.1"    # compiler + field math + Poseidon2 + traits
triton-vm = "0.42"      # VM-specific heavy dependency
```

Implement the runtime traits (`Runner`, `Prover`, `Verifier`, `Deployer`)
from `trident::runtime` and provide a `trident-<name>` binary that accepts
`run`, `prove`, and `verify` subcommands.

The warrior uses Trident's universal primitives — field arithmetic,
Poseidon2 hashing, proof estimation — instead of reimplementing them.

---

## How to Add a New VM

Step-by-step checklist with exact file paths.

### L0 — Declare

- [ ] Create `vm/<vm>.toml` with all sections:
  `[target]`, `[field]`, `[stack]`, `[hash]`, `[extension_field]`, `[cost]`, `[status]`
- [ ] Set `[status] level = 0`
- [ ] Verify `--target <vm>` resolves (the compiler reads `vm/` at startup)

### L1 — Document

- [ ] Create `reference/vm/<vm>.md` — include architecture, word size,
  instruction set summary, cost model parameters, and hash function
- [ ] Add the VM to the VM Registry table in [vm.md](vm.md)
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

---

## How to Add a New OS

### L0 — Declare

- [ ] Create `os/<os>.toml` with sections:
  `[os]`, `[runtime]`, `[cross_chain]`, `[status]`
- [ ] The `vm` field in `[os]` must reference an existing VM in `vm/`
- [ ] Set `[status] level = 0`

### L1 — Document

- [ ] Create `reference/os/<os>.md` — include programming model,
  state model, `os.<os>.*` API surface, and deployment patterns
- [ ] Add the OS to the OS Registry table in [os.md](os.md)
- [ ] Update the OS Integration Matrix in this file
- [ ] Set `[status] level = 1`

### L2 — Bind

- [ ] Create `os/<os>/` directory
- [ ] Write `.tri` binding modules (one per concern: storage, account,
  transfer, events, etc.)
- [ ] Each file declares `module os.<os>.<name>`
- [ ] Set `[status] level = 2`, `ext_modules = <count>`,
  `notes = "<comma-separated module names>"`

### L3 — Test

- [ ] Add end-to-end compilation tests targeting this OS
- [ ] Verify `os.<os>.*` module resolution works
- [ ] Set `[status] level = 3`, `tests = true`

### Finalize

- [ ] Update `[status]` in `os/<os>.toml`
- [ ] Update the OS Integration Matrix in this file

---

## How to Add a std/ Module

1. Create `std/<category>/<name>.tri` with `module std.<category>.<name>`
2. Implement functions. Use `#[intrinsic]` for VM-native operations.
3. Determine status: Done, Stub, Placeholder, or Hardcoded.
4. Update the Standard Library Status table in this file.
5. If the module is target-specific, document which targets support it
   in [stdlib.md](stdlib.md).

---

## 🔗 See Also

- [VM Reference](vm.md) — VM registry, lowering paths, tier/type/builtin tables, cost models
- [OS Reference](os.md) — OS concepts, `os.*` gold standard, extensions
- [Standard Library](stdlib.md) — `std.*` modules
- [Language Reference](language.md) — Types, operators, builtins, grammar, sponge, Merkle, extension field, proof composition
- [IR Reference](ir.md) — 54 operations, 4 tiers, lowering paths
- [CLI Reference](cli.md) — Compiler commands and flags
- [Error Catalog](errors.md) — All compiler error messages explained

---

*Trident v0.5 — Write once. Run anywhere.*
