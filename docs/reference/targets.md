# Trident Target Reference

[← Language Reference](language.md) | [IR Reference](ir.md)

Write once. Run anywhere.

---

## The OS Model

An OS is a runtime that loads programs, manages I/O, enforces billing, and
provides storage. A blockchain is one kind of OS. Linux is another.

The **VM is the CPU** — the instruction set architecture. The **OS is the
runtime** — storage, accounts, syscalls, billing. One VM can power multiple
OSes, just as one CPU architecture runs multiple operating systems.

| Concept | Range |
|---------|-------|
| CPU / ISA | x86-64, ARM64, RISC-V, Triton VM, Miden VM, Cairo VM, EVM, WASM, eBPF, MoveVM, TVM, CKB-VM, PolkaVM, Nock, SP1, OpenVM, RISC Zero, Jolt, AVM, Aztec |
| OS / Runtime | Linux, macOS, Android, WASI, Browser, Neptune, Polygon Miden, Starknet, Ethereum, Solana, Near, Cosmos, Sui, Aptos, Ton, Nervos, Polkadot, Aleo, Aztec, Boundless |
| Word size | 32-bit, 64-bit, 256-bit (EVM), 257-bit (TVM), field elements (31-bit to 254-bit) |
| System calls | POSIX (read, write, mmap), WASI (fd_read, fd_write), browser (fetch, DOM), provable (pub_read, pub_write, hint), blockchain (storage, cross-contract, IBC, XCM) |
| Process model | Multi-threaded, sequential deterministic, parallel (Sui, Aptos), event loop (Browser) |
| Billing | Wall-clock, cost tables (rows, cycles, steps, gates), gas, compute units, weight |

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
2. Is `<name>` a VM? → load `vm/<name>.toml`, no OS (bare compilation)
3. Neither → error: unknown target

```
trident build --target neptune     # OS → derives vm="triton" → full compilation
trident build --target ethereum    # OS → derives vm="evm" → EVM + Ethereum runtime
trident build --target linux       # OS → derives vm="x86-64" → native + Linux runtime
trident build --target wasi        # OS → derives vm="wasm" → WASM + WASI runtime
trident build --target triton      # bare VM → Triton VM, no OS
trident build --target evm         # bare VM → EVM bytecode, no OS
trident build --target wasm        # bare VM → generic WASM, no OS
```

When targeting an OS, `ext.<os>.*` modules are automatically available.
When targeting a bare VM, using `ext.*` modules is a compile error — there
is no OS to bind against.

---

## Part I — Virtual Machines (CPUs)

A VM defines the instruction set. The compiler's job is instruction
selection: translate TIR ops to the VM's native instructions. Everything
in this section is about the CPU — field size, word width, hash function,
register layout, cost model. OS-specific concerns (storage layout,
transaction format, account model) belong in Part II.

### Lowering Paths

Each VM family uses a specific lowering path from TIR to native output.

#### Stack Machines

Push, pop, dup, swap. TIR maps nearly 1:1 to native instructions.

```
TIR → StackLowering → assembly text → Linker → output
```

#### Register Machines

Registers or memory-addressed slots. TIR is first converted to LIR
(register-addressed IR), then lowered to native instructions.

```
TIR → LIR → RegisterLowering → machine code → Linker → output
```

The same `RegisterLowering` path serves both provable and native register
targets. SP1 and native RISC-V share the same `RiscVLowering` — one
produces code for the zkVM, the other for bare metal.

#### Tree Machines

Combinator expressions on binary trees (nouns). TIR lowers directly to
tree expressions.

```
TIR → TreeLowering → Noun → serialized output (.jam)
```

#### Circuit Machines

Programs compile to arithmetic circuits (gates/constraints) proved
client-side. No sequential instruction execution.

```
TIR → AcirLowering → ACIR circuit → prover → proof
```

#### Specialized Lowering

| Lowering | VM(s) | Notes |
|----------|-------|-------|
| `EvmLowering` | EVM | 256-bit stack, unique opcode set |
| `WasmLowering` | WASM | Standard WASM bytecode |
| `BpfLowering` | eBPF (SVM) | 10-register eBPF bytecode |
| `MoveLowering` | MoveVM | Resource-oriented bytecode |
| `KernelLowering` | CUDA, Metal, Vulkan | GPU data-parallel (planned) |

See [ir.md](ir.md) for the full IR architecture and lowering paths.

---

### VM Registry

Each VM is defined by a `.toml` configuration file in `vm/` specifying
CPU parameters. `TargetConfig` is the compiler's hardware abstraction layer.

20 VMs across three categories:

| VM | Arch | Word | Hash | Tier | Output | Details |
|----|------|------|------|------|--------|---------|
| **Provable** | | | | | | |
| Triton VM | Stack | Goldilocks 64-bit | Tip5 | 0-3 | `.tasm` | [triton.md](vm/triton.md) |
| Miden VM | Stack | Goldilocks 64-bit | Rescue-Prime | 0-2 | `.masm` | [miden.md](vm/miden.md) |
| Nock | Tree | Goldilocks 64-bit | Tip5 | 0-3 | `.jam` | [nock.md](vm/nock.md) |
| SP1 | Register (RISC-V) | Mersenne31 31-bit | Poseidon2 | 0-1 | `.S` | [sp1.md](vm/sp1.md) |
| OpenVM | Register (RISC-V) | Goldilocks 64-bit | Poseidon2 | 0-1 | `.S` | [openvm.md](vm/openvm.md) |
| RISC Zero | Register (RISC-V) | BabyBear 31-bit | SHA-256 | 0-1 | ELF | [risczero.md](vm/risczero.md) |
| Jolt | Register (RISC-V) | BN254 254-bit | Poseidon2 | 0-1 | ELF | [jolt.md](vm/jolt.md) |
| Cairo VM | Register | STARK-252 251-bit | Pedersen | 0-1 | `.sierra` | [cairo.md](vm/cairo.md) |
| AVM (Leo) | Register | Aleo 251-bit | Poseidon | 0-1 | `.aleo` | [leo.md](vm/leo.md) |
| Aztec (Noir) | Circuit (ACIR) | BN254 254-bit | Poseidon2 | 0-1 | `.acir` | [aztec.md](vm/aztec.md) |
| **Non-provable** | | | | | | |
| EVM | Stack | u256 | Keccak-256 | 0-1 | `.evm` | [evm.md](vm/evm.md) |
| WASM | Stack | u64 | -- (runtime-dependent) | 0-1 | `.wasm` | [wasm.md](vm/wasm.md) |
| eBPF (SVM) | Register | u64 | SHA-256 | 0-1 | `.so` | [svm.md](vm/svm.md) |
| MoveVM | Register/hybrid | u64 | SHA3-256 | 0-1 | `.mv` | [movevm.md](vm/movevm.md) |
| TVM | Stack | u257 | SHA-256 | 0-1 | `.boc` | [tvm.md](vm/tvm.md) |
| CKB-VM | Register (RISC-V) | u64 | Blake2b | 0-1 | ELF | [ckb.md](vm/ckb.md) |
| PolkaVM | Register (RISC-V) | u64 | Blake2b | 0-1 | PVM | [polkavm.md](vm/polkavm.md) |
| **Native** | | | | | | |
| x86-64 | Register | u64 | Software | 0-1 | ELF | [x86-64.md](vm/x86-64.md) |
| ARM64 | Register | u64 | Software | 0-1 | ELF | [arm64.md](vm/arm64.md) |
| RISC-V | Register | u64 | Software | 0-1 | ELF | [riscv.md](vm/riscv.md) |

**Planned**: CUDA, Metal, Vulkan (GPU — `KernelLowering`).

---

### Tier Compatibility

All VMs support **Tier 0** (program structure) and **Tier 1** (universal
computation). Higher tiers require specific VM capabilities:

| Tier | What it adds | VMs |
|------|-------------|-----|
| 0 — Structure | Entry, Call, Return, Const, Let | All 20 VMs |
| 1 — Universal | Arithmetic, control flow, memory, I/O | All 20 VMs |
| 2 — Provable | Witness, Sponge, MerkleStep | Triton VM, Miden VM, Nock + partial: RISC Zero (SHA-256), AVM (Poseidon), Aztec (Poseidon2) |
| 3 — Recursion | ProofBlock, FriVerify, recursive composition | Triton VM, Nock |

Programs using only Tier 0-1 compile to any VM. Programs using Tier 2+
require a VM with native coprocessor support for the relevant operations.

---

### Type and Builtin Availability

Types, operators, and builtins are tier-gated. Programs using higher-tier
features cannot target lower-tier VMs. The tables below show only VMs where
behavior differs. Unlisted VMs (all Tier 0-1 only) behave identically:
`yes` for Tier 0-1 features, `--` for Tier 2+.

#### Types

`Bool` and `U32` are available on every VM (Tier 0). The table below shows
only the types that differ across VMs.

| VM | `Field` | `Digest` | `XField` |
|----|---------|----------|----------|
| Triton VM | 64-bit | [Field; 5] | [Field; 3] |
| Miden VM | 64-bit | [Field; 4] | -- |
| Nock | 64-bit | [Field; 5] | [Field; 3] |
| Cairo VM | 251-bit | [Field; 1] | -- |
| AVM (Leo) | 251-bit | [Field; 1] | -- |
| Aztec (Noir) | 254-bit | [Field; 1] | -- |
| EVM | u256 | 32 bytes | -- |
| TVM | u257 | 32 bytes | -- |
| All others | u64 | 32 bytes | -- |

`XField` is Tier 2 — only Triton VM and Nock. "All others" = SP1, OpenVM,
RISC Zero, Jolt, WASM, eBPF, MoveVM, CKB-VM, PolkaVM, x86-64, ARM64,
RISC-V.

#### Operators

| Operator | Tier | Notes |
|----------|------|-------|
| `+` `*` `==` | 1 | All VMs. Nock: jets. |
| `<` `&` `^` `/%` | 1 | All VMs. Nock: jets. |
| `*.` (extension field multiply) | 2 | Triton VM, Nock only. |

#### Builtins — Tier 1 (Universal)

All Tier 1 builtins compile to every VM. The Hash column shows each VM's
hash function with rate R and digest width D.

| VM | I/O | Field | U32 | Assert | RAM | Hash |
|----|-----|-------|-----|--------|-----|------|
| Triton VM | yes | yes | yes | yes | yes | Tip5 (R=10, D=5) |
| Miden VM | yes | yes | yes | yes | yes | Rescue (R=8, D=4) |
| Nock | scry | jets | jets | crash | tree edit | Tip5 (R=10, D=5) |
| SP1 | yes | yes | yes | yes | yes | Poseidon2 (R=8, D=8) |
| OpenVM | yes | yes | yes | yes | yes | Poseidon2 (R=8, D=8) |
| RISC Zero | journal | yes | yes | yes | yes | SHA-256 (R=16, D=8) |
| Jolt | yes | yes | yes | yes | yes | Poseidon2 (R=8, D=8) |
| Cairo VM | yes | yes | yes | yes | yes | Pedersen (R=2, D=1) |
| AVM (Leo) | yes | native | yes | yes | yes | Poseidon (R=4, D=1) |
| Aztec (Noir) | yes | native | yes | yes | yes | Poseidon2 (R=4, D=1) |
| EVM | yes | yes | yes | revert | yes | Keccak-256 (R=4, D=8) |
| All others | yes | yes | yes | yes | yes | varies |

`hash()` is Tier 1 — every VM has a hash function. R = hash rate (fields
per absorption), D = digest width (fields per digest). The hash function
and its parameters are VM-specific (see VM Registry above).

Tier 1 builtins map to different primitives depending on the VM: I/O
becomes host function calls on virtual machines, stdio on native targets.
Assertions become revert on EVM, crash on Nock, abort on native. Field
arithmetic uses software modular reduction on non-provable targets.

#### Builtins — Tier 2 (Provable)

Tier 2 builtins require a proof-capable VM. `--` = not available.

| VM | Witness | Sponge | Merkle | XField |
|----|---------|--------|--------|--------|
| Triton VM | yes | native | native | yes |
| Miden VM | yes | native | emulated | -- |
| Nock | Nock 11 | jets | jets | yes |
| RISC Zero | yes | -- | -- | quartic |
| AVM (Leo) | yes | -- | -- | -- |
| Aztec (Noir) | yes | -- | -- | -- |
| All others | -- | -- | -- | -- |

Sponge = incremental hashing via `sponge_init`/`sponge_absorb`/`sponge_squeeze`.
Not to be confused with `hash()` which is Tier 1 (see above).

---

### Cost Model

Each VM has its own cost model. The compiler reports costs in the VM's
native units. The Trident cost infrastructure — static analysis,
per-function annotations, `--costs` flag — works identically across all VMs.

| VM | Cost unit | What determines cost |
|----|-----------|---------------------|
| [Triton VM](vm/triton.md) | Table rows | Tallest of 6 tables, padded to next power of 2 |
| [Miden VM](vm/miden.md) | Table rows | Tallest of 4 tables |
| [Nock](vm/nock.md) | Nock reductions | Formula evaluation steps (jet calls count as 1) |
| [SP1](vm/sp1.md) | Cycles | Total cycle count |
| [OpenVM](vm/openvm.md) | Cycles | Total cycle count |
| [RISC Zero](vm/risczero.md) | Cycles (segments) | Cycle count, split into segments for parallel proving |
| [Jolt](vm/jolt.md) | Cycles | Total cycle count (sumcheck-based) |
| [Cairo VM](vm/cairo.md) | Steps + builtins | Step count plus builtin usage |
| [AVM (Leo)](vm/leo.md) | Constraints | Constraint count (off-chain); microcredits (on-chain finalize) |
| [Aztec (Noir)](vm/aztec.md) | Gates / Gas | Private: gate count (client-side); Public: gas (sequencer) |
| [EVM](vm/evm.md) | Gas | Per-opcode cost (arithmetic 3-8, storage 5K-20K) |
| [WASM](vm/wasm.md) | Gas / Cycles | Per-instruction cost (varies by OS runtime) |
| [eBPF (SVM)](vm/svm.md) | Compute units | Per-instruction cost (budget 200K default, 1.4M max) |
| [MoveVM](vm/movevm.md) | Gas | Per-bytecode-instruction + storage operations |
| [TVM](vm/tvm.md) | Gas | Per-opcode + cell creation/storage charges |
| [CKB-VM](vm/ckb.md) | Cycles | Flat per-instruction (1 cycle), higher for branches/mul |
| [PolkaVM](vm/polkavm.md) | Weight | ref_time (computation) + proof_size (state proof overhead) |
| [x86-64](vm/x86-64.md) / [ARM64](vm/arm64.md) / [RISC-V](vm/riscv.md) | Wall-clock | No proof cost — direct execution |

The cost model is a property of the VM, not the OS. Provable VMs report
proving cost. Non-provable VMs report execution metering. Native targets
report wall-clock time. Each VM doc has per-instruction cost tables.

---

## Part II — Operating Systems

An OS defines the runtime environment: storage, accounts, syscalls, and
I/O conventions. The compiler's job is runtime binding — link against
OS-specific modules (`ext.<os>.*`). Everything in this section is about
the OS, not the instruction set.

### OS Registry

25 OSes across provable, blockchain, and traditional runtimes:

| OS | VM | Runtime binding | Account / process model | Interop | Details |
|----|-----|----------------|------------------------|---------|---------|
| **Provable** | | | | | |
| Neptune | [Triton VM](vm/triton.md) | `ext.neptune.*` | UTXO | -- | [neptune.md](os/neptune.md) |
| Polygon Miden | [Miden VM](vm/miden.md) | `ext.miden.*` | Account | -- | [miden.md](os/miden.md) |
| Nockchain | [Nock](vm/nock.md) | `ext.nockchain.*` | UTXO (Notes) | -- | [nockchain.md](os/nockchain.md) |
| Starknet | [Cairo VM](vm/cairo.md) | `ext.starknet.*` | Account | Ethereum L2 | [starknet.md](os/starknet.md) |
| Boundless | [RISC Zero](vm/risczero.md) | `ext.boundless.*` | -- | Ethereum verification | [boundless.md](os/boundless.md) |
| Succinct | [SP1](vm/sp1.md) | `ext.succinct.*` | -- | Ethereum verification | [succinct.md](os/succinct.md) |
| OpenVM network | [OpenVM](vm/openvm.md) | `ext.openvm.*` | -- | -- | [openvm-network.md](os/openvm-network.md) |
| Aleo | [AVM (Leo)](vm/leo.md) | `ext.aleo.*` | Record (UTXO) | -- | [aleo.md](os/aleo.md) |
| Aztec | [Aztec (Noir)](vm/aztec.md) | `ext.aztec.*` | Note (UTXO) + public | Ethereum L2 | [aztec.md](os/aztec.md) |
| **Blockchain** | | | | | |
| Ethereum | [EVM](vm/evm.md) | `ext.ethereum.*` | Account | -- | [ethereum.md](os/ethereum.md) |
| Solana | [eBPF (SVM)](vm/svm.md) | `ext.solana.*` | Account (stateless programs) | -- | [solana.md](os/solana.md) |
| Near Protocol | [WASM](vm/wasm.md) | `ext.near.*` | Account (1 contract each) | -- | [near.md](os/near.md) |
| Cosmos (100+ chains) | [WASM](vm/wasm.md) | `ext.cosmwasm.*` | Account | IBC | [cosmwasm.md](os/cosmwasm.md) |
| Arbitrum | [WASM](vm/wasm.md) + [EVM](vm/evm.md) | `ext.arbitrum.*` | Account (EVM-compatible) | Ethereum L2 | [arbitrum.md](os/arbitrum.md) |
| Internet Computer | [WASM](vm/wasm.md) | `ext.icp.*` | Canister | -- | [icp.md](os/icp.md) |
| Sui | [MoveVM](vm/movevm.md) | `ext.sui.*` | Object-centric | -- | [sui.md](os/sui.md) |
| Aptos | [MoveVM](vm/movevm.md) | `ext.aptos.*` | Account (resources) | -- | [aptos.md](os/aptos.md) |
| Ton | [TVM](vm/tvm.md) | `ext.ton.*` | Account (cells) | -- | [ton.md](os/ton.md) |
| Nervos CKB | [CKB-VM](vm/ckb.md) | `ext.nervos.*` | Cell (UTXO-like) | -- | [nervos.md](os/nervos.md) |
| Polkadot | [PolkaVM](vm/polkavm.md) | `ext.polkadot.*` | Account | XCM | [polkadot.md](os/polkadot.md) |
| **Traditional** | | | | | |
| Linux | [x86-64](vm/x86-64.md) / [ARM64](vm/arm64.md) / [RISC-V](vm/riscv.md) | `ext.linux.*` | Process | POSIX syscalls | [linux.md](os/linux.md) |
| macOS | [ARM64](vm/arm64.md) / [x86-64](vm/x86-64.md) | `ext.macos.*` | Process | POSIX + Mach | [macos.md](os/macos.md) |
| Android | [ARM64](vm/arm64.md) / [x86-64](vm/x86-64.md) | `ext.android.*` | Process (sandboxed) | NDK, JNI | [android.md](os/android.md) |
| WASI | [WASM](vm/wasm.md) | `ext.wasi.*` | Process (capability) | WASI preview 2 | [wasi.md](os/wasi.md) |
| Browser | [WASM](vm/wasm.md) | `ext.browser.*` | Event loop | JavaScript, Web APIs | [browser.md](os/browser.md) |

Key observations:

- **One VM, many OSes.** WASM powers 6+ OSes (Near, Cosmos, ICP, Arbitrum,
  WASI, Browser). x86-64 and ARM64 power Linux, macOS, Android. MoveVM
  powers Sui and Aptos. Same bytecode output, different `ext.*` bindings.
- **RISC-V lowering is shared** across SP1, OpenVM, RISC Zero, Jolt, CKB-VM,
  PolkaVM, and native RISC-V — 7 targets from one `RiscVLowering`.
- **Arbitrum** supports both WASM (Stylus) and EVM.

---

### `std.os.*` Lowering by OS Family

The portable OS layer (`std.os.*`) maps intent to OS-native mechanism.
The compiler reads the `[runtime]` section of the target's OS TOML
(`account_model`, `storage_model`, `transaction_model`) to select the
correct lowering strategy. See [stdlib.md](stdlib.md) for full API specs.

#### `std.os.state` — Persistent State

| OS family | OSes | `state.read(key)` lowers to |
|-----------|------|-----------------------------|
| Account | Ethereum, Starknet, Near, Cosmos, Ton, Polkadot, Miden | `SLOAD(key)` / storage read syscall |
| Stateless | Solana | `account.data(derived_index, offset)` |
| Object | Sui, Aptos | `dynamic_field.borrow(context_object, key)` |
| UTXO | Neptune, Nockchain, Nervos, Aleo, Aztec | `divine()` + `merkle_authenticate(key, root)` |
| Process | Linux, macOS, WASI, Browser, Android | File / environment read |
| Journal | Boundless, Succinct, OpenVM network | **Compile error** — no persistent state |

#### `std.os.caller` — Identity

| OS family | OSes | `caller.id()` lowers to |
|-----------|------|-------------------------|
| Account (EVM) | Ethereum | `msg.sender` (padded to Digest) |
| Account (Cairo) | Starknet | `get_caller_address` |
| Account (WASM) | Near, Cosmos | `predecessor_account_id` / `info.sender` |
| Account (other) | Ton, Polkadot, Miden | OS-native caller address |
| Stateless | Solana | `account.key(0)` (first signer) |
| Object | Sui, Aptos | `tx_context::sender` |
| Process | Linux, macOS, WASI, Android | `getuid()` (padded to Digest) |
| UTXO | Neptune, Nockchain, Nervos, Aleo, Aztec | **Compile error** — no caller; use `std.os.auth` |
| Journal | Boundless, Succinct, OpenVM network | **Compile error** — no identity |

#### `std.os.auth` — Authorization

`auth.verify(cred)` is an assertion — succeeds silently or crashes the VM.

| OS family | OSes | `auth.verify(cred)` lowers to |
|-----------|------|-------------------------------|
| Account (EVM) | Ethereum | `assert(msg.sender == cred)` |
| Account (Cairo) | Starknet | `assert(get_caller_address() == cred)` |
| Account (WASM) | Near, Cosmos | `assert(predecessor == cred)` / `assert(sender == cred)` |
| Account (other) | Ton, Polkadot, Miden | `assert(caller == cred)` |
| Stateless | Solana | `assert(is_signer(find_account(cred)))` |
| Object | Sui, Aptos | `assert(tx.sender() == cred)` |
| UTXO | Neptune, Nockchain, Nervos, Aleo, Aztec | `divine()` + `hash()` + `assert_eq(hash, cred)` |
| Process | Linux, macOS, WASI, Android | `assert(getuid() == cred)` |
| Journal | Boundless, Succinct, OpenVM network | **Compile error** — no identity |

#### `std.os.transfer` — Value Movement

| OS family | OSes | `transfer.send(to, amount)` lowers to |
|-----------|------|---------------------------------------|
| Account (EVM) | Ethereum | `CALL(to, amount, "")` |
| Account (Cairo) | Starknet | `transfer(to, amount)` syscall |
| Account (WASM) | Near, Cosmos | `Promise::transfer` / `BankMsg::Send` |
| Account (other) | Ton, Polkadot | OS-native transfer message |
| Stateless | Solana | `system_program::transfer(signer, to, amount)` |
| Object | Sui, Aptos | `coin::split` + `transfer::public_transfer` |
| UTXO | Neptune, Nockchain, Nervos, Aleo, Aztec | Emit output UTXO/note (amount in coin state) |
| Process | Linux, macOS, WASI, Browser, Android | **Compile error** — no native value |
| Journal | Boundless, Succinct, OpenVM network | **Compile error** — no native value |

#### `std.os.time` — Clock

| OS family | OSes | `time.now()` lowers to |
|-----------|------|------------------------|
| Account (EVM) | Ethereum | `block.timestamp` |
| Account (Cairo) | Starknet | `get_block_timestamp` |
| Account (WASM) | Near, Cosmos | `env.block.time` |
| Account (other) | Ton, Polkadot | OS-native block time |
| Stateless | Solana | `Clock::unix_timestamp` |
| Object | Sui, Aptos | `tx_context::epoch_timestamp_ms` |
| UTXO | Neptune, Nockchain | `kernel.authenticate_timestamp(root)` |
| Process | Linux, macOS, Android | `clock_gettime(CLOCK_REALTIME)` |
| WASI/Browser | WASI, Browser | `wall_clock.now()` / `Date.now()` |
| Journal | Boundless, Succinct, OpenVM network | Timestamp from public input |

#### OS TOML `[runtime]` Fields

The compiler selects lowering strategy from three fields in `os/*.toml`:

| Field | Values | Effect on `std.os.*` |
|-------|--------|---------------------|
| `account_model` | `account`, `stateless`, `object`, `utxo`, `journal`, `process` | Selects caller/auth lowering |
| `storage_model` | `key-value`, `account-data`, `object-store`, `merkle-authenticated`, `filesystem`, `none` | Selects state lowering |
| `transaction_model` | `signed`, `proof-based`, `none` | Selects auth/transfer lowering |

---

## Part III — Adding a Target

For detailed step-by-step checklists with exact file paths, integration
level definitions, and current status matrices, see
**[Integration Status](integration.md)**.

Quick summary:

- **Adding a VM**: Create `vm/<vm>.toml` -> document -> implement lowering
  -> add cost model -> test. (6 levels, L0--L5.)
- **Adding an OS**: Create `os/<os>.toml` -> document -> write `ext/<os>/*.tri`
  bindings -> test. (4 levels, L0--L3.)

No new lowering is needed for an OS — the VM already compiles. Only the
runtime bindings differ. The `ext/` directory is keyed by **OS name**
(not VM name): `ext/neptune/`, `ext/solana/`, `ext/linux/`.

See [ir.md Part VI](ir.md) for lowering trait interfaces.

---

## See Also

- [Language Reference](language.md) — Types, operators, builtins, grammar
- [Provable Computation](provable.md) — Hash, sponge, Merkle, extension field (Tier 2-3)
- [IR Reference](ir.md) — 54 operations, 4 tiers, lowering paths
- [CLI Reference](cli.md) — Compiler commands and flags
- [Error Catalog](errors.md) — All compiler error messages explained
- [Standard Library](stdlib.md) — `std.*` modules and OS extensions

---

*Trident v0.5 — Write once. Run anywhere.*
