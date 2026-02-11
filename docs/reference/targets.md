# Trident Target Reference

Write once. Prove anywhere.

---

## The OS Model

A blockchain is an operating system. Not metaphorically — structurally.

The VM is the CPU — the instruction set architecture. The blockchain is the
OS — the runtime that loads programs, manages I/O, enforces billing, and
provides storage. One VM can power multiple blockchains, just as one CPU
architecture runs multiple operating systems.

| Concept | Traditional | Trident |
|---------|-------------|---------|
| CPU / ISA | x86-64, ARM64, RISC-V | Triton VM, Miden VM, Cairo VM, RISC-V |
| OS / Runtime | Linux, macOS, Windows | Neptune, Polygon Miden, Starknet |
| Word size | 32-bit, 64-bit | Field (31-bit, 64-bit, 251-bit) |
| Instruction set extensions | SSE, AVX, NEON | Hash coprocessor, Merkle, sponge |
| Register file | 16 GP registers | Stack depth (16, 32, 0) |
| RAM | Byte-addressed | Word-addressed (field elements) |
| System calls | read, write, mmap | pub_read, pub_write, divine |
| Process model | Multi-threaded | Sequential, deterministic |
| Billing | None (or quotas) | Cost tables (rows, cycles, steps) |

The compiler doesn't "target multiple chains." It targets multiple CPUs.
Each CPU may run under one or more operating systems, but the compiler's job
is instruction selection and code generation — the same job gcc does for
x86-64 regardless of whether the binary runs on Linux or macOS.

### VMs and Their Blockchains

| VM (CPU) | Blockchain (OS) | Notes |
|----------|-----------------|-------|
| Triton VM | Neptune | Native STARK recursion |
| Miden VM | Polygon Miden | Account-based execution |
| Cairo VM | Starknet | Sierra intermediate format |
| RISC-V (SP1) | Succinct | General-purpose zkVM |
| RISC-V (OpenVM) | OpenVM network | Goldilocks field RISC-V |
| x86-64 | (conventional) | Testing, debugging, local execution |
| ARM64 | (conventional) | Testing, debugging, local execution |
| RISC-V native | (conventional) | Testing, debugging, local execution |
| CUDA / Metal / Vulkan | (GPU compute) | Batch parallel execution |

Conventional targets have no blockchain, no proofs, no billing. They run the
same Trident programs with the same field arithmetic — for testing, debugging,
benchmarking, and local execution. If you want to verify your program logic
before generating a proof, compile to x86-64 and run it.

Each target is defined by a `.toml` configuration file that specifies the
CPU parameters. The compiler reads this configuration and adapts code generation
accordingly. `TargetConfig` is the compiler's hardware abstraction layer.

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

## Target Profiles

### Triton VM

| Parameter | Value |
|---|---|
| Architecture | Stack |
| Field | Goldilocks (p = 2^64 - 2^32 + 1) |
| Field bits | 64 |
| Hash function | Tip5 |
| Digest width | 5 field elements |
| Hash rate | 10 field elements |
| Extension field | Cubic (degree 3) |
| Stack depth | 16 |
| Output format | `.tasm` |
| Cost model | 6 tables: processor, hash, u32, op_stack, ram, jump_stack |
| Blockchain | Neptune |

The primary target. 6-table cost model — proving cost is determined by the
tallest table, padded to the next power of 2. Tip5 is the native hash: 5 rounds
per permutation, 6 hash table rows per hash operation.

Has native Merkle step instructions (`merkle_step`, `merkle_step_mem`), native
extension field arithmetic (`xx_dot_step`, `xb_dot_step`), and native U32
coprocessor (`lt`, `and`, `xor`, `div_mod`, `split`, `pow`, `log_2_floor`,
`pop_count`).

### Miden VM

| Parameter | Value |
|---|---|
| Architecture | Stack |
| Field | Goldilocks (p = 2^64 - 2^32 + 1) |
| Field bits | 64 |
| Hash function | Rescue-Prime |
| Digest width | 4 field elements |
| Hash rate | 8 field elements |
| Extension field | None |
| Stack depth | 16 |
| Output format | `.masm` |
| Cost model | 4 tables: processor, hash, chiplets, stack |
| Blockchain | Polygon Miden |

Same field as Triton, different hash function and cost model. 4-table model
with a chiplets table that combines hashing, bitwise, and memory operations.
No extension field support — programs using `XField` or `ext.triton.*` cannot
target Miden.

### SP1

| Parameter | Value |
|---|---|
| Architecture | Register (RISC-V) |
| Field | Mersenne31 (p = 2^31 - 1) |
| Field bits | 31 |
| Hash function | Poseidon2 |
| Digest width | 8 field elements |
| Hash rate | 8 field elements |
| Extension field | None |
| Stack depth | 32 (register file) |
| Output format | `.S` (RISC-V assembly) |
| Cost model | Cycles |
| Blockchain | Succinct |

RISC-V zkVM. Single cost metric: cycle count. The 31-bit field means
field elements hold less data than on Goldilocks targets — programs may need
more elements to represent the same values. Requires `RegisterLowering`.

### OpenVM

| Parameter | Value |
|---|---|
| Architecture | Register (RISC-V) |
| Field | Goldilocks (p = 2^64 - 2^32 + 1) |
| Field bits | 64 |
| Hash function | Poseidon2 |
| Digest width | 8 field elements |
| Hash rate | 8 field elements |
| Extension field | None |
| Stack depth | 32 (register file) |
| Output format | `.S` (RISC-V assembly) |
| Cost model | Cycles |
| Blockchain | OpenVM network |

Same field as Triton/Miden, different hash and architecture. RISC-V backend
with cycle-based cost model.

### Cairo

| Parameter | Value |
|---|---|
| Architecture | Register |
| Field | STARK-252 (p = 2^251 + 17 * 2^192 + 1) |
| Field bits | 251 |
| Hash function | Pedersen |
| Digest width | 1 field element |
| Hash rate | 2 field elements |
| Extension field | None |
| Stack depth | 0 (no operand stack — memory-addressed) |
| Output format | `.sierra` |
| Cost model | Steps + builtins |
| Blockchain | Starknet |

The 251-bit field means a single field element can hold values that would
require multiple elements on smaller-field targets. Pedersen hash has a narrow
rate (2 elements) and produces a single-element digest. Stack depth 0 means
all data lives in memory — the compiler manages allocation automatically.

### x86-64 (conventional)

| Parameter | Value |
|---|---|
| Architecture | Register |
| Field | Goldilocks (p = 2^64 - 2^32 + 1) |
| Field bits | 64 |
| Hash function | Software (Tip5 or Poseidon2) |
| Digest width | Configurable |
| Extension field | None |
| Stack depth | 16 GP registers |
| Output format | Machine code (ELF) |
| Cost model | Wall-clock time (no proof cost) |
| Blockchain | None |

For testing and local execution. Field arithmetic is modular reduction on
native 64-bit integers. No proof generation — the program runs directly.
Useful for debugging program logic before deploying to a provable target.

### ARM64 (conventional)

| Parameter | Value |
|---|---|
| Architecture | Register |
| Field | Goldilocks (p = 2^64 - 2^32 + 1) |
| Field bits | 64 |
| Hash function | Software (Tip5 or Poseidon2) |
| Digest width | Configurable |
| Extension field | None |
| Stack depth | 31 GP registers |
| Output format | Machine code (ELF / Mach-O) |
| Cost model | Wall-clock time (no proof cost) |
| Blockchain | None |

Same as x86-64 but for ARM-based machines (Apple Silicon, AWS Graviton).

### RISC-V native (conventional)

| Parameter | Value |
|---|---|
| Architecture | Register |
| Field | Goldilocks (p = 2^64 - 2^32 + 1) |
| Field bits | 64 |
| Hash function | Software (Tip5 or Poseidon2) |
| Digest width | Configurable |
| Extension field | None |
| Stack depth | 32 GP registers |
| Output format | Machine code (ELF) |
| Cost model | Wall-clock time (no proof cost) |
| Blockchain | None |

Same `RiscVLowering` as SP1/OpenVM but targeting bare-metal RISC-V, not a
zkVM. Useful for embedded execution or cross-compilation testing.

---

## Cost Model

### How Cost Works

Each target has its own cost model. The compiler reports costs in the target's
native units. The Trident cost infrastructure — static analysis, per-function
annotations, `--costs` flag — works identically across all targets.

| Target | Cost unit | What determines proving time |
|---|---|---|
| Triton VM | Table rows | Tallest of 6 tables, padded to next power of 2 |
| Miden VM | Table rows | Tallest of 4 tables |
| SP1 | Cycles | Total cycle count |
| OpenVM | Cycles | Total cycle count |
| Cairo | Steps + builtins | Step count plus builtin usage |
| x86-64 / ARM64 / RISC-V | Wall-clock | No proof cost — direct execution |

Conventional targets have no proving overhead. Their "cost" is wall-clock
execution time. The `--costs` flag still works — it reports estimated
instruction counts for comparison with provable targets.

### Triton VM Cost Model (6 tables)

The most detailed model. Each instruction contributes rows to multiple tables
simultaneously. Proving cost is determined by the **tallest** table, not the sum.

| Table | What grows it | Rows per trigger |
|---|---|---|
| Processor | Every instruction | 1 per instruction |
| Hash | `hash`, `sponge_*`, `merkle_step*` + program attestation | 6 per hash op |
| U32 | `split`, `lt`, `and`, `xor`, `div_mod`, `pow`, `log_2_floor`, `pop_count` | Variable (worst-case 33) |
| Op Stack | Stack depth changes | 1 per stack op |
| RAM | `read_mem`, `write_mem`, `sponge_absorb_mem`, `xx_dot_step`, `xb_dot_step` | 1 per word |
| Jump Stack | `call`, `return` | 1 per jump |

The padded height is `2^ceil(log2(max_table_height))`. Crossing a power-of-2
boundary doubles proving time.

### Per-Instruction Costs (Triton VM)

| Trident construct | Processor | Hash | U32 | OpStack | RAM |
|---|---:|---:|---:|---:|---:|
| `a + b` | 1 | 0 | 0 | 1 | 0 |
| `a * b` | 1 | 0 | 0 | 1 | 0 |
| `inv(a)` | 1 | 0 | 0 | 0 | 0 |
| `a == b` | 1 | 0 | 0 | 1 | 0 |
| `a < b` | 1 | 0 | 33 | 1 | 0 |
| `a & b` | 1 | 0 | 33 | 1 | 0 |
| `a ^ b` | 1 | 0 | 33 | 1 | 0 |
| `split(a)` | 1 | 0 | 33 | 1 | 0 |
| `a /% b` | 1 | 0 | 33 | 0 | 0 |
| `pow(b, e)` | 1 | 0 | 33 | 1 | 0 |
| `log2(a)` | 1 | 0 | 33 | 0 | 0 |
| `popcount(a)` | 1 | 0 | 33 | 0 | 0 |
| `hash(...)` | 1 | **6** | 0 | 1 | 0 |
| `sponge_init()` | 1 | **6** | 0 | 0 | 0 |
| `sponge_absorb(...)` | 1 | **6** | 0 | 1 | 0 |
| `sponge_squeeze()` | 1 | **6** | 0 | 1 | 0 |
| `sponge_absorb_mem(p)` | 1 | **6** | 0 | 1 | 10 |
| `merkle_step(i, d)` | 1 | **6** | 33 | 0 | 0 |
| `merkle_step_mem(...)` | 1 | **6** | 33 | 0 | 5 |
| `divine()` | 1 | 0 | 0 | 1 | 0 |
| `pub_read()` | 1 | 0 | 0 | 1 | 0 |
| `pub_write(v)` | 1 | 0 | 0 | 1 | 0 |
| `ram_read(addr)` | 2 | 0 | 0 | 2 | 1 |
| `ram_write(addr, v)` | 2 | 0 | 0 | 2 | 1 |
| `xx_dot_step(...)` | 1 | 0 | 0 | 0 | 6 |
| `xb_dot_step(...)` | 1 | 0 | 0 | 0 | 4 |
| `assert(x)` | 1 | 0 | 0 | 1 | 0 |
| `assert_digest(a, b)` | 2 | 0 | 0 | 2 | 0 |
| fn call+return | 2 | 0 | 0 | 0 | 2 |
| if/else overhead | 3 | 0 | 0 | 2 | 0 |
| for-loop overhead | 8 | 0 | 0 | 4 | 0 |

U32 table rows depend on operand bit-width; 33 is the worst-case (32-bit).

### Optimization Hints

The compiler provides actionable suggestions:

| Hint | Pattern | Suggestion |
|---|---|---|
| H0001 | Hash table dominates | Batch data before hashing, reduce Merkle depth |
| H0002 | Near power-of-2 boundary | Room for more complexity at zero cost |
| H0003 | Redundant range check | Remove duplicate `as_u32()` |
| H0004 | Loop bound waste | Tighten `bounded N` declaration |

### Cost CLI

```bash
trident build --costs                   # Cost report
trident build --hotspots                # Top cost contributors
trident build --hints                   # Optimization hints
trident build --annotate                # Per-line cost annotations
trident build --save-costs costs.json   # Save for comparison
trident build --compare costs.json      # Diff with previous build
```

---

## Adding a Target

1. Write a `targets/name.toml` with all CPU parameters
2. Implement `StackLowering` (stack machines) or `RegisterLowering` (register machines)
3. Implement `CostModel` for the target's billing model
4. Add the target name to CLI dispatch

See [ir.md Part VI](ir.md) for the lowering trait interfaces and backend guide.

---

## Tier Compatibility

Which targets support which [IR tiers](ir.md):

| Target | Tier 0 (Structure) | Tier 1 (Universal) | Tier 2 (Provable) | Tier 3 (Recursion) |
|---|---|---|---|---|
| Triton VM | yes | yes | yes | yes |
| Miden VM | yes | yes | yes | no |
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

| Type | Tier | Triton VM | Miden VM | SP1 | OpenVM | Cairo | x86-64 / ARM64 / RISC-V |
|---|---|---|---|---|---|---|---|
| `Field` | 0 | 64-bit | 64-bit | 31-bit | 64-bit | 251-bit | 64-bit |
| `Bool` | 0 | yes | yes | yes | yes | yes | yes |
| `U32` | 0 | yes | yes | yes | yes | yes | yes |
| `Digest` | 0 | [Field; 5] | [Field; 4] | [Field; 8] | [Field; 8] | [Field; 1] | configurable |
| `XField` | 2 | [Field; 3] | -- | -- | -- | -- | -- |

`Digest` is universal — every target has a hash function and produces digests.
It is a content identifier: the fixed-width fingerprint of arbitrary data.
The width D comes from the target's `digest_width` configuration.

`XField` is Tier 2 only. Programs using `XField` can only compile for targets
where `xfield_width > 0`.

### Operators per Target

| Operator | Tier | Triton VM | Miden VM | SP1 | OpenVM | Cairo | Conventional |
|---|---|---|---|---|---|---|---|
| `+` `*` `==` | 1 | yes | yes | yes | yes | yes | yes |
| `<` `&` `^` `/%` | 1 | yes | yes | yes | yes | yes | yes |
| `*.` | 2 | yes | -- | -- | -- | -- | -- |

### Builtins per Target

| Builtin group | Tier | Triton VM | Miden VM | SP1 | OpenVM | Cairo | Conventional |
|---|---|---|---|---|---|---|---|
| I/O (`pub_read`, `divine`, etc.) | 1 | yes | yes | yes | yes | yes | yes (stdio) |
| Field (`sub`, `neg`, `inv`) | 1 | yes | yes | yes | yes | yes | yes |
| U32 (`split`, `log2`, `pow`, etc.) | 1 | yes | yes | yes | yes | yes | yes |
| Assert (`assert`, `assert_eq`) | 1 | yes | yes | yes | yes | yes | yes (abort) |
| RAM (`ram_read`, `ram_write`) | 1 | yes | yes | yes | yes | yes | yes (memory) |
| Hash (`hash`, `sponge_*`) | 2 | R=10, D=5 | R=8, D=4 | -- | -- | -- | -- |
| Merkle (`merkle_step`) | 2 | native | emulated | -- | -- | -- | -- |
| XField (`xfield`, `xinvert`, dot) | 2 | yes | -- | -- | -- | -- | -- |

R = hash rate (fields per absorption). D = digest width (fields per digest).

On conventional targets, Tier 1 builtins map to standard operations: I/O
becomes stdio, assertions become abort, RAM becomes heap memory. Field
arithmetic uses software modular reduction.

---

## Target Comparison

| | Triton VM | Miden VM | SP1 | OpenVM | Cairo | x86-64 | ARM64 |
|---|---|---|---|---|---|---|---|
| Field bits | 64 | 64 | 31 | 64 | 251 | 64 | 64 |
| Hash | Tip5 | Rescue-Prime | Poseidon2 | Poseidon2 | Pedersen | Software | Software |
| Architecture | Stack | Stack | Register | Register | Register | Register | Register |
| Cost model | 6 tables | 4 tables | Cycles | Cycles | Steps+builtins | Wall-clock | Wall-clock |
| Provable | yes | yes | yes | yes | yes | no | no |
| Extension field | Cubic | None | None | None | None | None | None |
| Native Merkle | Yes | No | No | No | No | No | No |
| Digest width (D) | 5 | 4 | 8 | 8 | 1 | config | config |
| Hash rate (R) | 10 | 8 | 8 | 8 | 2 | config | config |
| XField width (E) | 3 | 0 | 0 | 0 | 0 | 0 | 0 |
| Output | .tasm | .masm | .S | .S | .sierra | ELF | ELF |
| Blockchain | Neptune | Polygon Miden | Succinct | OpenVM | Starknet | -- | -- |

---

*Trident v0.5 — Write once. Prove anywhere.*
