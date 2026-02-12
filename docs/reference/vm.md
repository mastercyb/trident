# Virtual Machine Reference

[← Target Reference](targets.md) | [IR Reference](ir.md)

The VM is the CPU — the instruction set architecture. The compiler's job
is instruction selection: translate TIR ops to the VM's native instructions.
Everything in this document is about the CPU — field size, word width, hash
function, register layout, cost model. OS-specific concerns (storage layout,
transaction format, account model) belong in [os.md](os.md).

---

## Lowering Paths

Each VM family uses a specific lowering path from TIR to native output.

### Stack Machines

Push, pop, dup, swap. TIR maps nearly 1:1 to native instructions.

```
TIR → StackLowering → assembly text → Linker → output
```

### Register Machines

Registers or memory-addressed slots. TIR is first converted to LIR
(register-addressed IR), then lowered to native instructions.

```
TIR → LIR → RegisterLowering → machine code → Linker → output
```

The same `RegisterLowering` path serves both provable and native register
targets. SP1 and native RISC-V share the same `RiscVLowering` — one
produces code for the zkVM, the other for bare metal.

### Tree Machines

Combinator expressions on binary trees (nouns). TIR lowers directly to
tree expressions.

```
TIR → TreeLowering → Noun → serialized output (.jam)
```

### Circuit Machines

Programs compile to arithmetic circuits (gates/constraints) proved
client-side. No sequential instruction execution.

```
TIR → AcirLowering → ACIR circuit → prover → proof
```

### Specialized Lowering

| Lowering | VM(s) | Notes |
|----------|-------|-------|
| `EvmLowering` | EVM | 256-bit stack, unique opcode set |
| `WasmLowering` | WASM | Standard WASM bytecode |
| `SbpfLowering` | SBPF | 10-register SBPF bytecode |
| `MoveLowering` | MOVEVM | Resource-oriented bytecode |
| `KernelLowering` | CUDA, Metal, Vulkan | GPU data-parallel (planned) |

See [ir.md](ir.md) for the full IR architecture and lowering paths.

---

## VM Registry

Each VM is defined by a `.toml` configuration file in `vm/` specifying
CPU parameters. `TargetConfig` is the compiler's hardware abstraction layer.

20 VMs across three categories:

| VM | Arch | Word | Hash | Tier | Output | Details |
|----|------|------|------|------|--------|---------|
| **Provable** | | | | | | |
| TRITON | Stack | Goldilocks 64-bit | Tip5 | 0-3 | `.tasm` | [triton.md](vm/triton.md) |
| MIDEN | Stack | Goldilocks 64-bit | Rescue-Prime | 0-2 | `.masm` | [miden.md](vm/miden.md) |
| NOCK | Tree | Goldilocks 64-bit | Tip5 | 0-3 | `.jam` | [nock.md](vm/nock.md) |
| SP1 | Register (RISC-V) | Mersenne31 31-bit | Poseidon2 | 0-1 | `.S` | [sp1.md](vm/sp1.md) |
| OPENVM | Register (RISC-V) | Goldilocks 64-bit | Poseidon2 | 0-1 | `.S` | [openvm.md](vm/openvm.md) |
| RISCZERO | Register (RISC-V) | BabyBear 31-bit | SHA-256 | 0-1 | ELF | [risczero.md](vm/risczero.md) |
| JOLT | Register (RISC-V) | BN254 254-bit | Poseidon2 | 0-1 | ELF | [jolt.md](vm/jolt.md) |
| CAIRO | Register | STARK-252 251-bit | Pedersen | 0-1 | `.sierra` | [cairo.md](vm/cairo.md) |
| AVM | Register | Aleo 251-bit | Poseidon | 0-1 | `.aleo` | [avm.md](vm/avm.md) |
| AZTEC | Circuit (ACIR) | BN254 254-bit | Poseidon2 | 0-1 | `.acir` | [aztec.md](vm/aztec.md) |
| **Non-provable** | | | | | | |
| EVM | Stack | u256 | Keccak-256 | 0-1 | `.evm` | [evm.md](vm/evm.md) |
| WASM | Stack | u64 | -- (runtime-dependent) | 0-1 | `.wasm` | [wasm.md](vm/wasm.md) |
| SBPF | Register | u64 | SHA-256 | 0-1 | `.so` | [sbpf.md](vm/sbpf.md) |
| MOVEVM | Register/hybrid | u64 | SHA3-256 | 0-1 | `.mv` | [movevm.md](vm/movevm.md) |
| TVM | Stack | u257 | SHA-256 | 0-1 | `.boc` | [tvm.md](vm/tvm.md) |
| CKB | Register (RISC-V) | u64 | Blake2b | 0-1 | ELF | [ckb.md](vm/ckb.md) |
| POLKAVM | Register (RISC-V) | u64 | Blake2b | 0-1 | PVM | [polkavm.md](vm/polkavm.md) |
| **Native** | | | | | | |
| X86-64 | Register | u64 | Software | 0-1 | ELF | [x86-64.md](vm/x86-64.md) |
| ARM64 | Register | u64 | Software | 0-1 | ELF | [arm64.md](vm/arm64.md) |
| RISCV | Register | u64 | Software | 0-1 | ELF | [riscv.md](vm/riscv.md) |

**Planned**: CUDA, Metal, Vulkan (GPU — `KernelLowering`).

---

## Tier Compatibility

All VMs support **Tier 0** (program structure) and **Tier 1** (universal
computation). Higher tiers require specific VM capabilities:

| Tier | What it adds | VMs |
|------|-------------|-----|
| 0 — Structure | Entry, Call, Return, Const, Let | All 20 VMs |
| 1 — Universal | Arithmetic, control flow, memory, I/O | All 20 VMs |
| 2 — Provable | Witness, Sponge, MerkleStep | TRITON, MIDEN, NOCK + partial: RISCZERO (SHA-256), AVM (Poseidon), AZTEC (Poseidon2) |
| 3 — Recursion | ProofBlock, FriVerify, recursive composition | TRITON, NOCK |

Programs using only Tier 0-1 compile to any VM. Programs using Tier 2+
require a VM with native coprocessor support for the relevant operations.

---

## Type Availability

Types, operators, and builtins are tier-gated. Programs using higher-tier
features cannot target lower-tier VMs. The tables below show only VMs where
behavior differs. Unlisted VMs (all Tier 0-1 only) behave identically:
`yes` for Tier 0-1 features, `--` for Tier 2+.

### Types

`Bool` and `U32` are available on every VM (Tier 0). The table below shows
only the types that differ across VMs.

| VM | `Field` | `Digest` | `XField` |
|----|---------|----------|----------|
| TRITON | 64-bit | [Field; 5] | [Field; 3] |
| MIDEN | 64-bit | [Field; 4] | -- |
| NOCK | 64-bit | [Field; 5] | [Field; 3] |
| CAIRO | 251-bit | [Field; 1] | -- |
| AVM | 251-bit | [Field; 1] | -- |
| AZTEC | 254-bit | [Field; 1] | -- |
| EVM | u256 | 32 bytes | -- |
| TVM | u257 | 32 bytes | -- |
| All others | u64 | 32 bytes | -- |

`XField` is Tier 2 — only TRITON and NOCK. "All others" = SP1, OPENVM,
RISCZERO, JOLT, WASM, SBPF, MOVEVM, CKB, POLKAVM, X86-64, ARM64,
RISCV.

### Operators

| Operator | Tier | Notes |
|----------|------|-------|
| `+` `*` `==` | 1 | All VMs. NOCK: jets. |
| `<` `&` `^` `/%` | 1 | All VMs. NOCK: jets. |
| `*.` (extension field multiply) | 2 | TRITON, NOCK only. |

---

## Builtin Availability

### Tier 1 (Universal)

All Tier 1 builtins compile to every VM. The Hash column shows each VM's
hash function with rate R and digest width D.

| VM | I/O | Field | U32 | Assert | RAM | Hash |
|----|-----|-------|-----|--------|-----|------|
| TRITON | yes | yes | yes | yes | yes | Tip5 (R=10, D=5) |
| MIDEN | yes | yes | yes | yes | yes | Rescue (R=8, D=4) |
| NOCK | scry | jets | jets | crash | tree edit | Tip5 (R=10, D=5) |
| SP1 | yes | yes | yes | yes | yes | Poseidon2 (R=8, D=8) |
| OPENVM | yes | yes | yes | yes | yes | Poseidon2 (R=8, D=8) |
| RISCZERO | journal | yes | yes | yes | yes | SHA-256 (R=16, D=8) |
| JOLT | yes | yes | yes | yes | yes | Poseidon2 (R=8, D=8) |
| CAIRO | yes | yes | yes | yes | yes | Pedersen (R=2, D=1) |
| AVM | yes | native | yes | yes | yes | Poseidon (R=4, D=1) |
| AZTEC | yes | native | yes | yes | yes | Poseidon2 (R=4, D=1) |
| EVM | yes | yes | yes | revert | yes | Keccak-256 (R=4, D=8) |
| All others | yes | yes | yes | yes | yes | varies |

`hash()` is Tier 1 — every VM has a hash function. R = hash rate (fields
per absorption), D = digest width (fields per digest). The hash function
and its parameters are VM-specific (see VM Registry above).

Tier 1 builtins map to different primitives depending on the VM: I/O
becomes host function calls on virtual machines, stdio on native targets.
Assertions become revert on EVM, crash on NOCK, abort on native. Field
arithmetic uses software modular reduction on non-provable targets.

### Tier 2 (Provable)

Tier 2 builtins require a proof-capable VM. `--` = not available.

| VM | Witness | Sponge | Merkle | XField |
|----|---------|--------|--------|--------|
| TRITON | yes | native | native | yes |
| MIDEN | yes | native | emulated | -- |
| NOCK | Nock 11 | jets | jets | yes |
| RISCZERO | yes | -- | -- | quartic |
| AVM | yes | -- | -- | -- |
| AZTEC | yes | -- | -- | -- |
| All others | -- | -- | -- | -- |

Sponge = incremental hashing via `sponge_init`/`sponge_absorb`/`sponge_squeeze`.
Not to be confused with `hash()` which is Tier 1 (see above).

---

## Cost Model

Each VM has its own cost model. The compiler reports costs in the VM's
native units. The Trident cost infrastructure — static analysis,
per-function annotations, `--costs` flag — works identically across all VMs.

| VM | Cost unit | What determines cost |
|----|-----------|---------------------|
| [TRITON](vm/triton.md) | Table rows | Tallest of 6 tables, padded to next power of 2 |
| [MIDEN](vm/miden.md) | Table rows | Tallest of 4 tables |
| [NOCK](vm/nock.md) | Nock reductions | Formula evaluation steps (jet calls count as 1) |
| [SP1](vm/sp1.md) | Cycles | Total cycle count |
| [OPENVM](vm/openvm.md) | Cycles | Total cycle count |
| [RISCZERO](vm/risczero.md) | Cycles (segments) | Cycle count, split into segments for parallel proving |
| [JOLT](vm/jolt.md) | Cycles | Total cycle count (sumcheck-based) |
| [CAIRO](vm/cairo.md) | Steps + builtins | Step count plus builtin usage |
| [AVM](vm/avm.md) | Constraints | Constraint count (off-chain); microcredits (on-chain finalize) |
| [AZTEC](vm/aztec.md) | Gates / Gas | Private: gate count (client-side); Public: gas (sequencer) |
| [EVM](vm/evm.md) | Gas | Per-opcode cost (arithmetic 3-8, storage 5K-20K) |
| [WASM](vm/wasm.md) | Gas / Cycles | Per-instruction cost (varies by OS runtime) |
| [SBPF](vm/sbpf.md) | Compute units | Per-instruction cost (budget 200K default, 1.4M max) |
| [MOVEVM](vm/movevm.md) | Gas | Per-bytecode-instruction + storage operations |
| [TVM](vm/tvm.md) | Gas | Per-opcode + cell creation/storage charges |
| [CKB](vm/ckb.md) | Cycles | Flat per-instruction (1 cycle), higher for branches/mul |
| [POLKAVM](vm/polkavm.md) | Weight | ref_time (computation) + proof_size (state proof overhead) |
| [X86-64](vm/x86-64.md) / [ARM64](vm/arm64.md) / [RISCV](vm/riscv.md) | Wall-clock | No proof cost — direct execution |

The cost model is a property of the VM, not the OS. Provable VMs report
proving cost. Non-provable VMs report execution metering. Native targets
report wall-clock time. Each VM doc has per-instruction cost tables.

---

## See Also

- [Target Reference](targets.md) — OS model, integration tracking, how-to-add checklists
- [OS Reference](os.md) — OS concepts, `os.*` gold standard, extensions
- [IR Reference](ir.md) — 54 operations, 4 tiers, lowering paths
- [Language Reference](language.md) — Types, operators, builtins, grammar
- Per-VM docs: `vm/<vm>.md`

---

*Trident v0.5 — Write once. Run anywhere.*
