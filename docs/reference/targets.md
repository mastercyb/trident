# Trident Target Reference

Write once. Prove anywhere.

---

## The OS Model

A blockchain is an operating system. Not metaphorically — structurally.

The VM is the CPU — the instruction set architecture. The blockchain is the
OS — the runtime that loads programs, manages I/O, enforces billing, and
provides storage. One VM can power multiple blockchains, just as one CPU
architecture runs multiple operating systems.

| Concept | Traditional | Provable |
|---------|-------------|----------|
| CPU / ISA | x86-64, ARM64, RISC-V | Triton VM, Miden VM, Cairo VM, RISC-V |
| OS / Runtime | Linux, macOS, Windows | Neptune, Polygon Miden, Starknet |
| Word size | 32-bit, 64-bit | Field (31-bit, 64-bit, 251-bit) |
| Instruction set extensions | SSE, AVX, NEON | Hash coprocessor, Merkle, sponge |
| Register file | 16 GP registers | Stack depth (16, 32, 0) |
| RAM | Byte-addressed | Word-addressed (field elements) |
| System calls | read, write, mmap | pub_read, pub_write, hint |
| Process model | Multi-threaded | Sequential, deterministic |
| Billing | None (or quotas) | Cost tables (rows, cycles, steps) |

The compiler does two jobs, just like gcc:

1. **Instruction selection** (CPU) — translate IR ops to the target VM's
   native instructions. This is the same job gcc does for x86-64 vs ARM64.

2. **Runtime binding** (OS) — link against OS-specific standard library
   modules (`ext.<target>.*`) that provide transaction models, account
   structures, storage layouts, and syscall conventions. This is the same
   job libc does — it differs between Linux and macOS even on the same CPU.

One VM can power multiple blockchains. The compiler targets the VM for code
generation and the blockchain for runtime binding.

---

## Architecture Families

### Stack Machines

The VM executes on a stack of field elements. Push, pop, dup, swap.
The compiler's IR (TIR) maps nearly 1:1 to native instructions via
`StackLowering`.

**Targets:** Triton VM, Miden VM

```
TIR → StackLowering → assembly text → Linker → output
```

### Register Machines

The VM (or CPU) uses registers or memory-addressed slots. TIR is first
converted to LIR (register-addressed IR), then lowered to native instructions
via `RegisterLowering`.

**Provable:** SP1 (RISC-V), OpenVM (RISC-V), Cairo
**Conventional:** x86-64, ARM64, RISC-V native

```
TIR → LIR → RegisterLowering → machine code → Linker → output
```

The same `RegisterLowering` path serves both provable and conventional
register targets. SP1 and native RISC-V share the same `RiscVLowering` —
one produces code for the zkVM, the other for bare metal.

### Tree Machines

The VM evaluates combinator expressions on binary trees (nouns).
Data is tree-structured, addressed by axes. Computation is
subject-formula evaluation. TIR is lowered directly to tree
expressions via `TreeLowering`.

**Targets:** Nock (Nockchain)

```
TIR → TreeLowering → Noun → serialized output (.jam)
```

The key insight: TIR's structural control flow (nested IfElse/Loop with
`Vec<TIROp>` bodies) maps naturally to tree structure. Stack operations
become tree construction and axis addressing. Performance depends on
jet matching — the lowered formulas must hash-match registered jets
for all cryptographic operations.

### GPU Targets (planned)

Data-parallel execution. Each GPU thread runs one program instance.
TIR is wrapped in a compute kernel via KIR, then lowered with
`KernelLowering`.

**Targets:** CUDA, Metal, Vulkan

```
TIR → KIR → KernelLowering → kernel source → Linker → output
```

See [ir.md](ir.md) for the full IR architecture and lowering paths.

---

## Target Registry

| Target | Arch | Field | Hash | Tier | Blockchain | Details |
|--------|------|-------|------|------|------------|---------|
| Triton VM | Stack | Goldilocks 64-bit | Tip5 | 0-3 | Neptune | [triton.md](targets/triton.md) |
| Miden VM | Stack | Goldilocks 64-bit | Rescue-Prime | 0-2 | Polygon Miden | [miden.md](targets/miden.md) |
| Nock | Tree | Goldilocks 64-bit | Tip5 | 0-3 | Nockchain | [nock.md](targets/nock.md) |
| SP1 | Register (RISC-V) | Mersenne31 31-bit | Poseidon2 | 0-1 | Succinct | [sp1.md](targets/sp1.md) |
| OpenVM | Register (RISC-V) | Goldilocks 64-bit | Poseidon2 | 0-1 | OpenVM network | [openvm.md](targets/openvm.md) |
| Cairo | Register | STARK-252 251-bit | Pedersen | 0-1 | Starknet | [cairo.md](targets/cairo.md) |
| x86-64 | Register | Goldilocks 64-bit | Software | 0-1 | -- | [x86-64.md](targets/x86-64.md) |
| ARM64 | Register | Goldilocks 64-bit | Software | 0-1 | -- | [arm64.md](targets/arm64.md) |
| RISC-V native | Register | Goldilocks 64-bit | Software | 0-1 | -- | [riscv.md](targets/riscv.md) |

Each target is defined by a `.toml` configuration file that specifies the
CPU parameters. The compiler reads this configuration and adapts code generation
accordingly. `TargetConfig` is the compiler's hardware abstraction layer.

---

## Tier Compatibility

Which targets support which [IR tiers](ir.md):

| Target | Tier 0 (Structure) | Tier 1 (Universal) | Tier 2 (Provable) | Tier 3 (Recursion) |
|---|---|---|---|---|
| Triton VM | yes | yes | yes | yes |
| Miden VM | yes | yes | yes | no |
| Nock | yes | yes | yes | yes |
| SP1 | yes | yes | no | no |
| OpenVM | yes | yes | no | no |
| Cairo | yes | yes | no | no |
| x86-64 | yes | yes | no | no |
| ARM64 | yes | yes | no | no |
| RISC-V native | yes | yes | no | no |

**Tier 0** — Program structure (Entry, Call, Return, etc.). All targets.

**Tier 1** — Universal computation (arithmetic, control flow, memory, I/O).
All targets — provable and conventional.

**Tier 2** — Provable computation (Hash, MerkleStep, Sponge, Reveal, Seal).
Stack-machine VMs with native coprocessors.

**Tier 3** — Recursive proof composition (ProofBlock, FriVerify, etc.).
Triton VM only — requires native STARK verification support.

---

## Type and Builtin Availability

Types, operators, and builtins are tier-gated. Programs using higher-tier
features cannot target lower-tier backends.

### Types per Target

| Type | Tier | Triton VM | Miden VM | Nock | SP1 | OpenVM | Cairo | x86-64 / ARM64 / RISC-V |
|---|---|---|---|---|---|---|---|---|
| `Field` | 0 | 64-bit | 64-bit | 64-bit (Belt) | 31-bit | 64-bit | 251-bit | 64-bit |
| `Bool` | 0 | yes | yes | yes | yes | yes | yes | yes |
| `U32` | 0 | yes | yes | yes | yes | yes | yes | yes |
| `Digest` | 0 | [Field; 5] | [Field; 4] | [Field; 5] | [Field; 8] | [Field; 8] | [Field; 1] | configurable |
| `XField` | 2 | [Field; 3] | -- | [Field; 3] (Felt) | -- | -- | -- | -- |

### Operators per Target

| Operator | Tier | Triton VM | Miden VM | Nock | SP1 | OpenVM | Cairo | Conventional |
|---|---|---|---|---|---|---|---|---|
| `+` `*` `==` | 1 | yes | yes | yes (jets) | yes | yes | yes | yes |
| `<` `&` `^` `/%` | 1 | yes | yes | yes (jets) | yes | yes | yes | yes |
| `*.` | 2 | yes | -- | yes (jets) | -- | -- | -- | -- |

### Builtins per Target

| Builtin group | Tier | Triton VM | Miden VM | Nock | SP1 | OpenVM | Cairo | Conventional |
|---|---|---|---|---|---|---|---|---|
| I/O (`pub_read`, `pub_write`) | 1 | yes | yes | yes (scry) | yes | yes | yes | yes (stdio) |
| Field (`sub`, `neg`, `inv`) | 1 | yes | yes | yes (jets) | yes | yes | yes | yes |
| U32 (`split`, `log2`, `pow`, etc.) | 1 | yes | yes | yes (jets) | yes | yes | yes | yes |
| Assert (`assert`, `assert_eq`) | 1 | yes | yes | yes (crash) | yes | yes | yes | yes (abort) |
| RAM (`ram_read`, `ram_write`) | 1 | yes | yes | yes (tree edit) | yes | yes | yes | yes (memory) |
| Witness (`hint`) | 2 | yes | yes | yes (Nock 11) | yes | yes | yes | -- |
| Hash (`hash`, `sponge_*`) | 2 | R=10, D=5 | R=8, D=4 | R=10, D=5 (Tip5) | -- | -- | -- | -- |
| Merkle (`merkle_step`) | 2 | native | emulated | jets (ZTD) | -- | -- | -- | -- |
| XField (`xfield`, `xinvert`, dot) | 2 | yes | -- | yes (Felt jets) | -- | -- | -- | -- |

R = hash rate (fields per absorption). D = digest width (fields per digest).

On conventional targets, Tier 1 builtins map to standard operations: I/O
becomes stdio, assertions become abort, RAM becomes heap memory. Field
arithmetic uses software modular reduction.

---

## Cost Model

Each target has its own cost model. The compiler reports costs in the target's
native units. The Trident cost infrastructure — static analysis, per-function
annotations, `--costs` flag — works identically across all targets.

| Target | Cost unit | What determines proving time |
|---|---|---|
| Triton VM | Table rows | Tallest of 6 tables, padded to next power of 2 |
| Miden VM | Table rows | Tallest of 4 tables |
| Nock | Nock reductions | Formula evaluation steps (jet calls count as 1) |
| SP1 | Cycles | Total cycle count |
| OpenVM | Cycles | Total cycle count |
| Cairo | Steps + builtins | Step count plus builtin usage |
| x86-64 / ARM64 / RISC-V | Wall-clock | No proof cost — direct execution |

Conventional targets have no proving overhead. Their "cost" is wall-clock
execution time. The `--costs` flag still works — it reports estimated
instruction counts for comparison with provable targets.

See [targets/triton.md](targets/triton.md) for the full per-instruction
cost matrix and optimization hints.

---

## Adding a Target

1. Write a `targets/name.toml` with all CPU parameters
2. Implement the appropriate lowering trait:
   - `StackLowering` for stack machines
   - `RegisterLowering` for register machines
   - `TreeLowering` for tree/combinator machines
3. Implement `CostModel` for the target's billing model
4. Add the target name to CLI dispatch

See [ir.md Part VI](ir.md) for the lowering trait interfaces and backend guide.

---

*Trident v0.5 — Write once. Prove anywhere.*
