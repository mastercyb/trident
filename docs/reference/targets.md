# Trident Target Reference

Write once. Run anywhere.

---

## The OS Model

An OS is a runtime that loads programs, manages I/O, enforces billing, and
provides storage. A blockchain is one kind of OS. Linux is another.

The **VM is the CPU** — the instruction set architecture. The **OS is the
runtime** — storage, accounts, syscalls, billing. One VM can power multiple
OSes, just as one CPU architecture runs multiple operating systems.

| Concept | Traditional | Provable | Blockchain |
|---------|-------------|----------|------------|
| CPU / ISA | x86-64, ARM64, RISC-V | Triton VM, Miden VM, Cairo VM, RISC-V zkVMs, Jolt, Nock, AVM | EVM, WASM, eBPF, MoveVM, TVM, CKB-VM, PolkaVM |
| OS / Runtime | Linux, macOS, Windows | Neptune, Polygon Miden, Starknet, Boundless, Aleo, Aztec | Ethereum, Solana, Near, Cosmos, Sui, Aptos, Ton, Nervos, Polkadot, Arbitrum, Icp |
| Word size | 32-bit, 64-bit | Field (31-bit, 64-bit, 251-bit, 254-bit) | 64-bit, 256-bit (EVM), 257-bit (TVM) |
| ISA extensions | SSE, AVX, NEON | Hash coprocessor, Merkle, sponge | Precompiles, host functions |
| Registers | 16 GP registers | Stack depth (16, 32, 0) | Varies (10 eBPF, 32 RISC-V, stack, Move locals) |
| RAM | Byte-addressed | Word-addressed (field elements) | Byte-addressed, cell-based (TVM) |
| System calls | read, write, mmap | pub_read, pub_write, hint | Storage, cross-contract calls, IBC, XCM |
| Process model | Multi-threaded | Sequential, deterministic | Sequential, deterministic (parallel: Sui, Aptos) |
| Billing | None (or quotas) | Cost tables (rows, cycles, steps, gates) | Gas, compute units, weight, cycles |

The compiler does two jobs, just like gcc:

1. **Instruction selection** (VM/CPU) — translate IR ops to the target VM's
   native instructions. This is the same job gcc does for x86-64 vs ARM64.

2. **Runtime binding** (OS) — link against OS-specific modules
   (`ext.<os>.*`) that provide transaction models, account structures,
   storage layouts, and syscall conventions. This is the same job libc
   does — it differs between Linux and macOS even on the same CPU.

### Target Resolution

A **target** is either a VM or an OS. The compiler resolves `--target <name>`
by checking OS configs first, then VM configs:

1. Is `<name>` an OS? → load `os/<name>.toml`, derive VM from `vm` field
2. Is `<name>` a VM? → load `targets/<name>.toml`, no OS (bare compilation)
3. Neither → error: unknown target

```
trident build --target neptune     # OS → derives vm="triton" → full compilation
trident build --target ethereum    # OS → derives vm="evm" → EVM + Ethereum runtime
trident build --target near        # OS → derives vm="wasm" → WASM + Near runtime
trident build --target solana      # OS → derives vm="svm" → eBPF + Solana runtime
trident build --target triton      # VM → bare Triton VM, no OS (Tier 0-1 only)
trident build --target evm         # VM → bare EVM bytecode, no OS
trident build --target wasm        # VM → generic WASM, no OS
```

When targeting an OS, `ext.<os>.*` modules are automatically available.
When targeting a bare VM, using `ext.*` modules is a compile error — there
is no OS to bind against.

One VM can power multiple OSes. The OS config (`os/<name>.toml`)
declares which VM it runs on via the `vm` field.

---

## Part I — Virtual Machines (CPUs)

A VM defines the instruction set. The compiler's job is instruction
selection: translate TIR ops to the VM's native instructions. Everything
in this section is about the CPU — field size, word width, hash function,
register layout, cost model. OS-specific concerns (storage layout,
transaction format, account model) belong in Part II.

### Architecture Families

#### Stack Machines

The VM executes on a stack of field elements. Push, pop, dup, swap.
The compiler's IR (TIR) maps nearly 1:1 to native instructions via
`StackLowering`.

```
TIR → StackLowering → assembly text → Linker → output
```

#### Register Machines

The VM (or CPU) uses registers or memory-addressed slots. TIR is first
converted to LIR (register-addressed IR), then lowered to native instructions
via `RegisterLowering`.

```
TIR → LIR → RegisterLowering → machine code → Linker → output
```

The same `RegisterLowering` path serves both provable and conventional
register targets. SP1 and native RISC-V share the same `RiscVLowering` —
one produces code for the zkVM, the other for bare metal.

#### Tree Machines

The VM evaluates combinator expressions on binary trees (nouns).
TIR is lowered directly to tree expressions via `TreeLowering`.

```
TIR → TreeLowering → Noun → serialized output (.jam)
```

#### Circuit Machines

The "VM" is a constraint system. Programs compile to arithmetic circuits
(gates/constraints) proved client-side. No sequential instruction execution.

```
TIR → AcirLowering → ACIR circuit → prover → proof
```

#### Additional Lowering Paths

| Lowering | VM(s) | Notes |
|----------|-------|-------|
| `EvmLowering` | EVM | 256-bit stack, unique opcode set |
| `WasmLowering` | WASM | Standard WASM bytecode, multiple OS runtimes |
| `BpfLowering` | eBPF (SVM) | 10-register eBPF, Solana-specific |
| `MoveLowering` | MoveVM | Resource-oriented bytecode |
| `KernelLowering` | CUDA, Metal, Vulkan | GPU data-parallel (planned) |

See [ir.md](ir.md) for the full IR architecture and lowering paths.

---

### VM Registry

Each VM is defined by a `.toml` configuration file specifying the CPU
parameters. `TargetConfig` is the compiler's hardware abstraction layer.

#### Provable VMs

VMs designed for zero-knowledge proof generation. Programs produce
cryptographic proofs of correct execution.

| VM | Arch | Field | Hash | Tier | Output | Details |
|----|------|-------|------|------|--------|---------|
| Triton VM | Stack | Goldilocks 64-bit | Tip5 | 0-3 | `.tasm` | [triton.md](targets/triton.md) |
| Miden VM | Stack | Goldilocks 64-bit | Rescue-Prime | 0-2 | `.masm` | [miden.md](targets/miden.md) |
| Nock | Tree | Goldilocks 64-bit | Tip5 | 0-3 | `.jam` | [nock.md](targets/nock.md) |
| SP1 | Register (RISC-V) | Mersenne31 31-bit | Poseidon2 | 0-1 | `.S` | [sp1.md](targets/sp1.md) |
| OpenVM | Register (RISC-V) | Goldilocks 64-bit | Poseidon2 | 0-1 | `.S` | [openvm.md](targets/openvm.md) |
| RISC Zero | Register (RISC-V) | BabyBear 31-bit | SHA-256 | 0-1 | ELF | [risczero.md](targets/risczero.md) |
| Jolt | Register (RISC-V) | BN254 254-bit | Poseidon2 | 0-1 | ELF | [jolt.md](targets/jolt.md) |
| Cairo VM | Register | STARK-252 251-bit | Pedersen | 0-1 | `.sierra` | [cairo.md](targets/cairo.md) |
| AVM (Leo) | Register | Aleo 251-bit | Poseidon | 0-1 | `.aleo` | [leo.md](targets/leo.md) |
| Aztec (Noir) | Circuit (ACIR) | BN254 254-bit | Poseidon2 | 0-1 | `.acir` | [aztec.md](targets/aztec.md) |

#### Blockchain VMs

VMs that execute smart contracts on-chain. No proof generation — programs
run directly in the VM.

| VM | Arch | Word | Hash | Tier | Output | Details |
|----|------|------|------|------|--------|---------|
| EVM | Stack | u256 | Keccak-256 | 0-1 | `.evm` | [evm.md](targets/evm.md) |
| WASM | Stack | u64 | -- (runtime-dependent) | 0-1 | `.wasm` | [wasm.md](targets/wasm.md) |
| eBPF (SVM) | Register | u64 | SHA-256 | 0-1 | `.so` | [svm.md](targets/svm.md) |
| MoveVM | Register/hybrid | u64 | SHA3-256 | 0-1 | `.mv` | [movevm.md](targets/movevm.md) |
| TVM | Stack | u257 | SHA-256 | 0-1 | `.boc` | [tvm.md](targets/tvm.md) |
| CKB-VM | Register (RISC-V) | u64 | Blake2b | 0-1 | ELF | [ckb.md](targets/ckb.md) |
| PolkaVM | Register (RISC-V) | u64 | Blake2b | 0-1 | PVM | [polkavm.md](targets/polkavm.md) |

#### Conventional Targets

No VM — native machine code. For testing and local execution.

| Target | Arch | Field | Hash | Tier | Output | Details |
|--------|------|-------|------|------|--------|---------|
| x86-64 | Register | Goldilocks 64-bit | Software | 0-1 | ELF | [x86-64.md](targets/x86-64.md) |
| ARM64 | Register | Goldilocks 64-bit | Software | 0-1 | ELF | [arm64.md](targets/arm64.md) |
| RISC-V native | Register | Goldilocks 64-bit | Software | 0-1 | ELF | [riscv.md](targets/riscv.md) |

#### GPU Targets (planned)

| Target | Arch | Notes |
|--------|------|-------|
| CUDA | Data-parallel | `KernelLowering` |
| Metal | Data-parallel | `KernelLowering` |
| Vulkan | Data-parallel | `KernelLowering` |

---

### Tier Compatibility

Which VMs support which [IR tiers](ir.md):

| VM | Tier 0 (Structure) | Tier 1 (Universal) | Tier 2 (Provable) | Tier 3 (Recursion) |
|----|---|---|---|---|
| Triton VM | yes | yes | yes | yes |
| Miden VM | yes | yes | yes | no |
| Nock | yes | yes | yes | yes |
| SP1 | yes | yes | no | no |
| OpenVM | yes | yes | no | no |
| RISC Zero | yes | yes | no | no |
| Jolt | yes | yes | no | no |
| Cairo VM | yes | yes | no | no |
| AVM (Leo) | yes | yes | no | no |
| Aztec (Noir) | yes | yes | no | no |
| EVM | yes | yes | no | no |
| WASM | yes | yes | no | no |
| eBPF (SVM) | yes | yes | no | no |
| MoveVM | yes | yes | no | no |
| TVM | yes | yes | no | no |
| CKB-VM | yes | yes | no | no |
| PolkaVM | yes | yes | no | no |
| x86-64 | yes | yes | no | no |
| ARM64 | yes | yes | no | no |
| RISC-V native | yes | yes | no | no |

**Tier 0** — Program structure (Entry, Call, Return, etc.). All VMs.

**Tier 1** — Universal computation (arithmetic, control flow, memory, I/O).
All VMs — provable, blockchain, conventional, and GPU.

**Tier 2** — Provable computation (Hash, MerkleStep, Sponge, Reveal, Seal).
Provable VMs with native coprocessors.

**Tier 3** — Recursive proof composition (ProofBlock, FriVerify, etc.).
Triton VM and Nock — requires native STARK verification support.

---

### Type and Builtin Availability

Types, operators, and builtins are tier-gated. Programs using higher-tier
features cannot target lower-tier VMs.

#### Types per VM

| Type | Tier | Triton VM | Miden VM | Nock | SP1 | OpenVM | Cairo VM | RISC Zero | Jolt | AVM | Aztec | Blockchain VMs | Conventional |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `Field` | 0 | 64-bit | 64-bit | 64-bit (Belt) | 31-bit | 64-bit | 251-bit | 31-bit | 254-bit | 251-bit | 254-bit | native int | 64-bit |
| `Bool` | 0 | yes | yes | yes | yes | yes | yes | yes | yes | yes | yes | yes | yes |
| `U32` | 0 | yes | yes | yes | yes | yes | yes | yes | yes | yes | yes | yes | yes |
| `Digest` | 0 | [Field; 5] | [Field; 4] | [Field; 5] | [Field; 8] | [Field; 8] | [Field; 1] | 32 bytes | [Field; 1] | [Field; 1] | [Field; 1] | 32 bytes | configurable |
| `XField` | 2 | [Field; 3] | -- | [Field; 3] (Felt) | -- | -- | -- | quartic | -- | -- | -- | -- | -- |

#### Operators per VM

| Operator | Tier | Triton VM | Miden VM | Nock | SP1 | OpenVM | Cairo VM | RISC Zero | Jolt | AVM | Aztec | Blockchain VMs | Conventional |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `+` `*` `==` | 1 | yes | yes | yes (jets) | yes | yes | yes | yes | yes | yes | yes | yes | yes |
| `<` `&` `^` `/%` | 1 | yes | yes | yes (jets) | yes | yes | yes | yes | yes | yes | yes | yes | yes |
| `*.` | 2 | yes | -- | yes (jets) | -- | -- | -- | -- | -- | -- | -- | -- | -- |

#### Builtins per VM

| Builtin group | Tier | Triton VM | Miden VM | Nock | SP1 | OpenVM | Cairo VM | RISC Zero | Jolt | AVM | Aztec | Blockchain VMs | Conventional |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| I/O (`pub_read`, `pub_write`) | 1 | yes | yes | yes (scry) | yes | yes | yes | yes (journal) | yes | yes | yes | yes (host calls) | yes (stdio) |
| Field (`sub`, `neg`, `inv`) | 1 | yes | yes | yes (jets) | yes | yes | yes | yes | yes | yes (native) | yes (native) | yes (software) | yes |
| U32 (`split`, `log2`, `pow`, etc.) | 1 | yes | yes | yes (jets) | yes | yes | yes | yes | yes | yes | yes | yes | yes |
| Assert (`assert`, `assert_eq`) | 1 | yes | yes | yes (crash) | yes | yes | yes | yes | yes | yes | yes | yes (revert) | yes (abort) |
| RAM (`ram_read`, `ram_write`) | 1 | yes | yes | yes (tree edit) | yes | yes | yes | yes | yes | yes | yes | yes (memory) | yes (memory) |
| Witness (`hint`) | 2 | yes | yes | yes (Nock 11) | yes | yes | yes | yes | yes | yes | yes | -- | -- |
| Hash (`hash`, `sponge_*`) | 2 | R=10, D=5 | R=8, D=4 | R=10, D=5 (Tip5) | -- | -- | -- | SHA-256 accel | -- | Poseidon | Poseidon2 | -- | -- |
| Merkle (`merkle_step`) | 2 | native | emulated | jets (ZTD) | -- | -- | -- | -- | -- | -- | -- | -- | -- |
| XField (`xfield`, `xinvert`, dot) | 2 | yes | -- | yes (Felt jets) | -- | -- | -- | quartic | -- | -- | -- | -- | -- |

R = hash rate (fields per absorption). D = digest width (fields per digest).

On blockchain VMs, Tier 1 builtins map to VM-native operations: I/O becomes
host function calls, assertions become revert/abort, RAM becomes VM memory.

On conventional targets, Tier 1 builtins map to standard operations: I/O
becomes stdio, assertions become abort, RAM becomes heap memory.

Field arithmetic uses software modular reduction on non-provable targets.

---

### Cost Model

Each VM has its own cost model. The compiler reports costs in the VM's
native units. The Trident cost infrastructure — static analysis, per-function
annotations, `--costs` flag — works identically across all VMs.

| VM | Cost unit | What determines cost |
|----|-----------|---------------------|
| Triton VM | Table rows | Tallest of 6 tables, padded to next power of 2 |
| Miden VM | Table rows | Tallest of 4 tables |
| Nock | Nock reductions | Formula evaluation steps (jet calls count as 1) |
| SP1 | Cycles | Total cycle count |
| OpenVM | Cycles | Total cycle count |
| RISC Zero | Cycles (segments) | Cycle count, split into segments for parallel proving |
| Jolt | Cycles | Total cycle count (sumcheck-based) |
| Cairo VM | Steps + builtins | Step count plus builtin usage |
| AVM (Leo) | Constraints | Constraint count (off-chain); microcredits (on-chain finalize) |
| Aztec (Noir) | Gates / Gas | Private: gate count (client-side); Public: gas (sequencer) |
| EVM | Gas | Per-opcode cost (arithmetic 3-8, storage 5K-20K) |
| WASM | Gas / Cycles | Per-instruction cost (varies by OS runtime) |
| eBPF (SVM) | Compute units | Per-instruction cost (budget 200K default, 1.4M max) |
| MoveVM | Gas | Per-bytecode-instruction + storage operations |
| TVM | Gas | Per-opcode + cell creation/storage charges |
| CKB-VM | Cycles | Flat per-instruction (1 cycle), higher for branches/mul |
| PolkaVM | Weight | ref_time (computation) + proof_size (state proof overhead) |
| x86-64 / ARM64 / RISC-V | Wall-clock | No proof cost — direct execution |

Provable VMs report proving cost. Blockchain VMs report on-chain metering
cost. Conventional targets report wall-clock time. The cost model is a
property of the VM, not the OS.

See [targets/triton.md](targets/triton.md) for the full per-instruction
cost matrix and optimization hints.

---

## Part II — Operating Systems

An OS defines the runtime environment. The compiler's job is runtime
binding: link against OS-specific modules (`ext.<os>.*`) that provide
storage, accounts, syscalls, and I/O conventions. Everything in this
section is about the OS — not the instruction set.

Multiple OSes can share the same VM, just as Linux and macOS share x86-64.
The same compiled WASM bytecode deploys to Near, Cosmos, WASI, and a
browser — only the runtime bindings differ.

### OS Registry

| OS | VM | Runtime binding | Account/process model | Interop | Details |
|----|-----|----------------|----------------------|---------|---------|
| Neptune | Triton VM | `ext.neptune.*` | UTXO | -- | [neptune.md](os/neptune.md) |
| Polygon Miden | Miden VM | `ext.miden.*` | Account | -- | [miden.md](os/miden.md) |
| Nockchain | Nock | `ext.nockchain.*` | UTXO (Notes) | -- | [nockchain.md](os/nockchain.md) |
| Succinct | SP1 | `ext.succinct.*` | -- | Ethereum verification | [succinct.md](os/succinct.md) |
| OpenVM network | OpenVM | `ext.openvm.*` | -- | -- | [openvm-network.md](os/openvm-network.md) |
| Starknet | Cairo VM | `ext.starknet.*` | Account | Ethereum L2 | [starknet.md](os/starknet.md) |
| Boundless | RISC Zero | `ext.boundless.*` | -- | Ethereum verification | [boundless.md](os/boundless.md) |
| Aleo | AVM (Leo) | `ext.aleo.*` | Record (UTXO) | -- | [aleo.md](os/aleo.md) |
| Aztec | Aztec (Noir) | `ext.aztec.*` | Note (UTXO) + public | Ethereum L2 | [aztec.md](os/aztec.md) |
| Ethereum | EVM | `ext.ethereum.*` | Account | -- | [ethereum.md](os/ethereum.md) |
| Solana | eBPF (SVM) | `ext.solana.*` | Account (stateless programs) | -- | [solana.md](os/solana.md) |
| Near Protocol | WASM | `ext.near.*` | Account (1 contract each) | -- | [near.md](os/near.md) |
| Cosmos (100+ chains) | WASM | `ext.cosmwasm.*` | Account | IBC | [cosmwasm.md](os/cosmwasm.md) |
| Arbitrum | WASM + EVM | `ext.arbitrum.*` | Account (EVM-compatible) | Ethereum L2 | [arbitrum.md](os/arbitrum.md) |
| Internet Computer | WASM | `ext.icp.*` | Canister | -- | [icp.md](os/icp.md) |
| Sui | MoveVM | `ext.sui.*` | Object-centric | -- | [sui.md](os/sui.md) |
| Aptos | MoveVM | `ext.aptos.*` | Account (resources) | -- | [aptos.md](os/aptos.md) |
| Ton | TVM | `ext.ton.*` | Account (cells) | -- | [ton.md](os/ton.md) |
| Nervos CKB | CKB-VM | `ext.nervos.*` | Cell (UTXO-like) | -- | [nervos.md](os/nervos.md) |
| Polkadot | PolkaVM | `ext.polkadot.*` | Account | XCM | [polkadot.md](os/polkadot.md) |
| Linux | x86-64 / ARM64 / RISC-V | `ext.linux.*` | Process | POSIX syscalls | [linux.md](os/linux.md) |
| macOS | ARM64 / x86-64 | `ext.macos.*` | Process | POSIX + Mach | [macos.md](os/macos.md) |
| Android | ARM64 / x86-64 | `ext.android.*` | Process (sandboxed) | NDK, JNI | [android.md](os/android.md) |
| WASI | WASM | `ext.wasi.*` | Process (capability) | WASI preview 2 | [wasi.md](os/wasi.md) |
| Browser | WASM | `ext.browser.*` | Event loop | JavaScript, Web APIs | [browser.md](os/browser.md) |

Key observations:

- **WASM** powers 6+ OSes: Near, Cosmos, Arbitrum (Stylus), Icp, WASI,
  Browser. Same `.wasm` output, different `ext.*` bindings.
- **x86-64** and **ARM64** power traditional OSes: Linux, macOS, Android.
  Same ELF/Mach-O output, different `ext.*` bindings (POSIX vs NDK).
- **MoveVM** powers 2 OSes: Sui (object model) and Aptos (account model).
  Same `.mv` output, different `ext.*` bindings.
- **EVM** bytecode runs on Ethereum and all EVM-compatible chains.
  Arbitrum also supports WASM via Stylus.
- **RISC-V** lowering is shared across SP1, OpenVM, RISC Zero, Jolt, CKB-VM,
  PolkaVM, and native RISC-V — 7 targets from one `RiscVLowering`.

---

## Part III — Adding a Target

### Adding a VM

1. Write `targets/<vm>.toml` with CPU parameters (field, hash, stack, cost).
   This makes `--target <vm>` work for bare (OS-less) compilation.
2. Implement the appropriate lowering trait:
   - `StackLowering` — stack machines (Triton, Miden, TVM)
   - `RegisterLowering` — register machines (SP1, OpenVM, RISC Zero, Jolt, Cairo, AVM)
   - `TreeLowering` — tree/combinator machines (Nock)
   - `EvmLowering` — EVM bytecode
   - `WasmLowering` — WASM bytecode
   - `BpfLowering` — eBPF bytecode (SVM)
   - `MoveLowering` — Move bytecode
   - `AcirLowering` — arithmetic circuits (Aztec/Noir)
   - `KernelLowering` — GPU compute kernels (planned)
3. Implement `CostModel` for the VM's billing model
4. Write `docs/reference/targets/<vm>.md` documentation

### Adding an OS (to an existing VM)

1. Write `os/<os-name>.toml` — must include `vm = "<vm-name>"` referencing
   an existing VM in `targets/`. This makes `--target <os-name>` work.
2. Write `ext/<os-name>/*.tri` runtime binding modules
3. Write `docs/reference/os/<os-name>.md` documentation

No new lowering needed — the VM already compiles. Only the runtime differs.
The `os/<os-name>.toml` file is what registers the OS as a valid `--target`.

The `ext/` directory is keyed by **OS name** (not VM name): `ext/neptune/`,
`ext/solana/`, `ext/near/` — because the bindings are OS-specific.

See [ir.md Part VI](ir.md) for lowering trait interfaces and the backend guide.

---

*Trident v0.5 — Write once. Run anywhere.*
