# Trident Target Reference

Write once. Prove anywhere.

---

## The OS Model

A blockchain is an operating system. Not metaphorically — structurally.

| OS Concept | Trident Equivalent |
|---|---|
| CPU | VM (Triton, Miden, Cairo, SP1, OpenVM) |
| Word size | Field (64-bit, 31-bit, 251-bit) |
| Instruction set | Hash function, sponge, Merkle |
| Register file | Stack depth (16, 32, 0) |
| RAM | Spill memory, storage |
| System calls | I/O, events, storage |
| Process model | Sequential execution, no threads |
| Billing | Cost tables (rows, cycles, steps) |

The compiler doesn't "target multiple chains." It targets multiple operating
systems with different CPU architectures, word sizes, and instruction sets.

Each target is defined by a `.toml` configuration file that specifies the
OS parameters. The compiler reads this configuration and adapts code generation
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

The VM uses registers or memory-addressed slots. TIR is first converted
to LIR (register-addressed IR), then lowered to native instructions via
`RegisterLowering`.

**Targets:** SP1, OpenVM, Cairo

```
TIR → LIR → RegisterLowering → machine code → Linker → output
```

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
| Cost tables | processor, hash, u32, op_stack, ram, jump_stack |

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
| Cost tables | processor, hash, chiplets, stack |

Same field as Triton, different hash function and cost model. 4-table model
with a chiplets table that combines hashing, bitwise, and memory operations.
No extension field support — programs using `XField` or `ext.triton.*` cannot
target Miden.

### SP1

| Parameter | Value |
|---|---|
| Architecture | Register |
| Field | Mersenne31 (p = 2^31 - 1) |
| Field bits | 31 |
| Hash function | Poseidon2 |
| Digest width | 8 field elements |
| Hash rate | 8 field elements |
| Extension field | None |
| Stack depth | 32 (register file) |
| Output format | `.S` (RISC-V assembly) |
| Cost tables | cycles |

RISC-V zkVM. Single cost metric: cycle count. The 31-bit field means
field elements hold less data than on Goldilocks targets — programs may need
more elements to represent the same values. Requires `RegisterLowering`.

### OpenVM

| Parameter | Value |
|---|---|
| Architecture | Register |
| Field | Goldilocks (p = 2^64 - 2^32 + 1) |
| Field bits | 64 |
| Hash function | Poseidon2 |
| Digest width | 8 field elements |
| Hash rate | 8 field elements |
| Extension field | None |
| Stack depth | 32 (register file) |
| Output format | `.S` (RISC-V assembly) |
| Cost tables | cycles |

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
| Cost tables | steps, builtins |

The 251-bit field means a single field element can hold values that would
require multiple elements on smaller-field targets. Pedersen hash has a narrow
rate (2 elements) and produces a single-element digest. Stack depth 0 means
all data lives in memory — the compiler manages allocation automatically.

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

1. Write a `targets/name.toml` with all OS parameters
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

**Tier 0** — Program structure (Entry, Call, Return, etc.). All targets.

**Tier 1** — Universal computation (arithmetic, control flow, memory, I/O).
All targets.

**Tier 2** — Provable computation (Hash, MerkleStep, Sponge, Reveal, Seal).
Stack machines only — these map to native coprocessor instructions.

**Tier 3** — Recursive proof composition (ProofBlock, FriVerify, etc.).
Triton VM only — requires native STARK verification support.

---

## Target Comparison

| | Triton VM | Miden VM | SP1 | OpenVM | Cairo |
|---|---|---|---|---|---|
| Field bits | 64 | 64 | 31 | 64 | 251 |
| Hash | Tip5 | Rescue-Prime | Poseidon2 | Poseidon2 | Pedersen |
| Architecture | Stack | Stack | Register | Register | Register |
| Cost model | 6 tables | 4 tables | Cycles | Cycles | Steps+builtins |
| Extension field | Cubic | None | None | None | None |
| Native Merkle | Yes | No | No | No | No |
| Digest width | 5 | 4 | 8 | 8 | 1 |
| Output | .tasm | .masm | .S | .S | .sierra |

---

*Trident v0.5 — Write once. Prove anywhere.*
