# Multi-Target Architecture

Trident compiles to any stack-based zero-knowledge virtual machine. The compiler
currently targets **Triton VM** as its production backend, with the architecture
designed for extensibility to other zkVMs. This document describes the
multi-target compilation system as implemented.

---

## Overview

Every zkVM computes over a finite field. Trident treats `Field` as the universal
primitive of provable computation -- the specific prime is a property of the
target, not of the program. A Trident program that multiplies two field elements
and asserts the result means the same thing on every backend.

The compiler is organized in three layers:

```
+-----------------------------------------+
|         Trident Universal Core          |
|  (types, control flow, field arithmetic,|
|   modules, cost transparency)           |
+-----------------------------------------+
|         Abstraction Layer               |
|  (I/O, hash, memory, Merkle, sponge,   |
|   cost model, events)                   |
+----------+----------+----------+-------+
|  Triton  |  Miden   |  OpenVM  |  SP1  | ...
|  Backend |  Backend  | Backend | Backend|
|  + os/   |  + os/   |  + os/   | + os/ |
+----------+----------+----------+-------+
```

Each backend implements the abstraction layer for its target VM and may publish
**backend extensions** -- additional types, intrinsics, and library modules that
expose target-specific capabilities.

---

## TargetConfig

Targets are defined as TOML files in the `vm/` directory. The compiler loads
a target by name via `--target <name>`, which resolves to `vm/<name>.toml`.
Triton VM also has a hardcoded fallback in `TargetConfig::triton()` so the
compiler works without any TOML files on disk.

### TOML Schema

Each target file declares the following sections:

```toml
[target]
name = "triton"                       # Short identifier (CLI, file paths)
display_name = "Triton VM"            # Human-readable name
architecture = "stack"                # "stack" or "register"
output_extension = ".tasm"            # File extension for compiled output

[field]
prime = "2^64 - 2^32 + 1"            # Field prime (informational)
limbs = 2                             # U32 limbs when splitting a field element

[stack]
depth = 16                            # Operand stack depth before spilling
spill_ram_base = 1073741824           # Base RAM address for spilled variables

[hash]
function = "Tip5"                     # Hash function name (informational)
digest_width = 5                      # Width of a hash digest in field elements
rate = 10                             # Hash absorption rate in field elements

[extension_field]
degree = 3                            # Extension field degree (0 if none)

[cost]
tables = ["processor", "hash", "u32", "op_stack", "ram", "jump_stack"]
```

### TargetConfig Struct

The `TargetConfig` struct in `src/target.rs` holds all parameters:

```rust
pub struct TargetConfig {
    pub name: String,
    pub display_name: String,
    pub architecture: Arch,           // Arch::Stack or Arch::Register
    pub field_prime: String,
    pub field_limbs: u32,
    pub stack_depth: u32,
    pub spill_ram_base: u64,
    pub digest_width: u32,
    pub xfield_width: u32,
    pub hash_rate: u32,
    pub output_extension: String,
    pub cost_tables: Vec<String>,
}
```

Target resolution (`TargetConfig::resolve`) searches for the TOML file relative
to the compiler binary and the working directory. Path traversal in target names
is rejected.

### Shipped Target Configurations

| File             | Name   | Arch     | Field           | Digest | Hash Rate |
|------------------|--------|----------|-----------------|:------:|:---------:|
| `triton.toml`    | triton | stack    | Goldilocks      | 5      | 10        |
| `miden.toml`     | miden  | stack    | Goldilocks      | 4      | 8         |
| `openvm.toml`    | openvm | register | Goldilocks      | 8      | 8         |
| `sp1.toml`       | sp1    | register | Mersenne-31     | 8      | 8         |
| `cairo.toml`     | cairo  | register | Stark-252       | 1      | 2         |

---

## Backend Traits

### StackLowering

The `StackLowering` trait in `src/tir/lower/mod.rs` abstracts instruction
emission for stack-machine targets. It converts TIR operations to target
assembly text, sharing all stack management and control-flow logic.

```rust
pub(crate) trait StackLowering {
    fn target_name(&self) -> &str;
    fn output_extension(&self) -> &str;

    // Stack operations
    fn inst_push(&self, value: u64) -> String;
    fn inst_pop(&self, count: u32) -> String;
    fn inst_dup(&self, depth: u32) -> String;
    fn inst_swap(&self, depth: u32) -> String;

    // Arithmetic
    fn inst_add(&self) -> &'static str;
    fn inst_mul(&self) -> &'static str;
    fn inst_eq(&self) -> &'static str;
    fn inst_invert(&self) -> &'static str;
    // ... (split, lt, and, xor, div_mod, log2, pow, pop_count, xb_mul, x_invert)

    // I/O
    fn inst_read_io(&self, count: u32) -> String;
    fn inst_write_io(&self, count: u32) -> String;
    fn inst_divine(&self, count: u32) -> String;

    // Memory
    fn inst_read_mem(&self, count: u32) -> String;
    fn inst_write_mem(&self, count: u32) -> String;

    // Hash and Merkle
    fn inst_hash(&self) -> &'static str;
    fn inst_sponge_init(&self) -> &'static str;
    fn inst_sponge_absorb(&self) -> &'static str;
    fn inst_sponge_squeeze(&self) -> &'static str;
    fn inst_merkle_step(&self) -> &'static str;
    // ...

    // Control flow
    fn inst_assert(&self) -> &'static str;
    fn inst_skiz(&self) -> &'static str;
    fn inst_call(&self, label: &str) -> String;
    fn inst_return(&self) -> &'static str;
    fn inst_halt(&self) -> &'static str;

    // Inline assembly passthrough
    fn inst_push_neg_one(&self) -> &'static str;
}
```

The following targets implement this trait:

- **`TritonLowering`** -- Triton Assembly (TASM). Production target.
- **`MidenLowering`** -- Miden Assembly (MASM). Uses `dup.N` / `movup.N` syntax,
  `adv_push.1` for divine, `hperm` for hashing.

Register targets use a separate `RegisterLowering` path via LIR. Tree targets
(Nock) use `TreeLowering`. See [IR Reference](../reference/ir.md) for the
full lowering architecture.

The `create_backend(target_name)` factory function returns the appropriate
implementation.

### CostModel

The `CostModel` trait in `src/cost.rs` provides target-specific proving cost
analysis. The cost analyzer walks the AST once; all target-specific knowledge
flows through this trait.

```rust
pub(crate) trait CostModel {
    fn table_names(&self) -> &[&str];
    fn table_short_names(&self) -> &[&str];
    fn builtin_cost(&self, name: &str) -> TableCost;
    fn binop_cost(&self, op: &BinOp) -> TableCost;
    fn call_overhead(&self) -> TableCost;
    fn stack_op(&self) -> TableCost;
    fn if_overhead(&self) -> TableCost;
    fn loop_overhead(&self) -> TableCost;
    fn hash_rows_per_permutation(&self) -> u64;
    fn target_name(&self) -> &str;
}
```

Implemented cost models:

| Struct            | Target      | Tables                                              |
|-------------------|-------------|------------------------------------------------------|
| `TritonCostModel` | Triton VM   | processor, hash, u32, op_stack, ram, jump_stack      |
| `MidenCostModel`  | Miden VM    | processor, hash, chiplets, stack                     |
| `CycleCostModel`  | OpenVM, SP1 | cycles (single-dimension)                            |
| `CairoCostModel`  | Cairo       | steps, builtins                                      |

The `create_cost_model(target_name)` factory returns the appropriate model. The
`CostAnalyzer` struct is parameterized by a `&dyn CostModel` reference, so the
same analysis code produces target-appropriate reports, hotspot rankings, and
optimization hints (H0001 hash dominance, H0002 headroom, H0004 loop bound
waste).

---

## Standard Library Layers

The standard library is organized into three layers that enable code portability
across targets.

### Layer 1: `std.core` -- Universal

Pure Trident code with no VM dependencies. Compiles identically on every target.

```
std/core/
  field.tri       Field arithmetic helpers
  convert.tri     as_u32, as_field (with range checks)
  assert.tri      Assertion helpers
  u32.tri         U32 arithmetic helpers
```

### Layer 2: `std.io` / `std.crypto` -- Abstraction

Same user-facing API on every target. The compiler dispatches to the appropriate
backend instructions via intrinsic annotations.

```
std/io/
  io.tri          pub_read, pub_write, divine
  mem.tri         ram_read, ram_write, ram_read_block, ram_write_block
  storage.tri     Persistent storage abstraction

std/crypto/
  hash.tri        hash(), sponge_init/absorb/squeeze
  merkle.tri      Merkle tree verification
  auth.tri        Preimage verification
  poseidon.tri    Poseidon hash (native on some targets, software on others)
  poseidon2.tri   Poseidon2 hash
  sha256.tri      SHA-256 (precompile on RISC-V targets)
  keccak256.tri   Keccak-256 (precompile on RISC-V targets)
  ecdsa.tri       ECDSA signature verification
  secp256k1.tri   secp256k1 curve operations
  ed25519.tri     Ed25519 curve operations
  bigint.tri      Big integer arithmetic
```

### Layer 3: `<target>.ext` -- Target-Specific

Backend extensions that expose target-unique capabilities. Programs that import
from `os.<target>.*` are explicitly bound to that target.

```
os/neptune/
  xfield.tri      XField type (cubic extension), xx_add, xx_mul, x_invert
  kernel.tri      Neptune kernel interface (authenticate_field, tree_height)
  utxo.tri        UTXO verification
  proof.tri       Recursive STARK verifier components
  recursive.tri   Recursive proof composition
  registry.tri    Registry operations
```

### Target Detection

`std/target.tri` exposes compile-time constants derived from the active
`TargetConfig`:

```
pub const DIGEST_WIDTH    // 5 for Triton (Tip5), 4 for Miden (RPO), etc.
pub const FIELD_LIMBS     // 2 for Goldilocks, 4 for Stark-252, etc.
pub const HASH_RATE       // 10 for Tip5, 8 for RPO, etc.
```

Programs use these constants to write target-polymorphic code without `#[cfg]`
guards. For example, `Digest` is defined as `[Field; DIGEST_WIDTH]`, so its
width adjusts automatically per target.

---

## Target-Tagged Assembly

Inline assembly blocks are tagged with the target they belong to:

```
asm(triton) {
    dup 0
    add
    swap 5 pop 1
}
```

The parser recognizes the `asm(<target>) { ... }` syntax. When emitting code,
the compiler compares the tag against the active target name. Assembly blocks
tagged for a different target are silently skipped.

Bare `asm { ... }` blocks (no target tag) are also supported. They use the
declared stack effect annotation and emit for whatever target is active, passing
the body through as raw instructions.

### Multi-Target Programs

A single source file can contain assembly blocks for multiple targets. Only the
blocks matching the active `--target` are emitted:

```
fn fast_double(a: Field) -> Field {
    asm(triton) { dup 0 add }         // Emitted when --target triton
    asm(miden)  { dup.0 add }         // Emitted when --target miden
}
```

The `#[cfg(target)]` conditional compilation attribute works for larger blocks:

```
#[cfg(triton)]
use os.neptune.xfield

fn compute() -> Field {
    #[cfg(triton)]
    {
        // Use native extension field dot products
    }
    #[cfg(not(triton))]
    {
        // Portable fallback
    }
}
```

---

## Adding a New Target

To add support for a new stack-based zkVM:

### 1. Create the target TOML

Add `vm/<name>.toml` with the target's parameters:

```toml
[target]
name = "newvm"
display_name = "New VM"
architecture = "stack"
output_extension = ".nasm"

[field]
prime = "..."
limbs = 2

[stack]
depth = 16
spill_ram_base = 1073741824

[hash]
function = "..."
digest_width = 4
rate = 8

[extension_field]
degree = 0

[cost]
tables = ["cycles"]
```

### 2. Implement the Lowering Trait

For stack targets, add a new struct in `src/tir/lower/` that implements the
`StackLowering` trait. Every method maps a semantic operation (push, add, hash,
etc.) to the target's assembly syntax. For register targets, implement
`RegisterLowering` via the LIR path. See [targets.md](../reference/targets.md)
for which lowering path to use per VM family.

### 3. Implement CostModel

Add a cost model struct in `src/cost.rs` that implements the `CostModel` trait.
Provide per-instruction costs in the target's native cost dimensions. Register
it in `create_cost_model()`.

### 4. Add extension modules

If the target has unique capabilities (special types, native instructions, VM-
specific APIs), add Trident library files under `os/<name>/`.

### 5. Verify

Run the existing test suite with `--target <name>` to validate that universal
core programs compile correctly. Add target-specific tests for extension modules
and instruction encoding.

---

## Current Targets

### Triton VM (Production)

- **Status:** Fully implemented. All compiler features, standard library, cost
  analysis, and tooling work with Triton VM.
- **Architecture:** 16-element operand stack, Goldilocks field, Tip5 hash.
- **Output:** `.tasm` files (Triton Assembly).
- **Extensions:** `os.neptune.xfield`, `os.neptune.kernel`, `os.neptune.utxo`,
  `os.neptune.proof`, `os.neptune.recursive`, `os.neptune.registry`.
- **Cost model:** 6-table model (processor, hash, u32, op_stack, ram,
  jump_stack) with padded-height estimation, boundary warnings, and hotspot
  analysis.

### Other Targets (Architecture Ready)

Backend implementations and target configurations exist for:

- **Miden VM** -- Stack machine, Goldilocks field, Rescue-Prime hash, 4-element
  digests. `StackBackend` and `CostModel` implemented. TOML shipped.
- **OpenVM** -- RISC-V register machine, Goldilocks field, Poseidon2 hash.
  `StackBackend` and cycle-based `CostModel` implemented. TOML shipped.
- **SP1** -- RISC-V register machine, Mersenne-31 field, Poseidon2 hash.
  `StackBackend` and cycle-based `CostModel` implemented. TOML shipped.
- **Cairo** -- Register machine, Stark-252 field, Pedersen hash.
  `StackBackend` and steps-based `CostModel` implemented. TOML shipped.

These backends have structural implementations -- trait methods are filled in
with correct instruction mnemonics and cost tables. They have not been validated
against their respective VM runtimes. Triton VM remains the only target with
end-to-end proving and verification.

---

## Links

- [Tutorial](../tutorials/tutorial.md) -- getting started, including `asm(triton)` blocks
- [Language Reference](../reference/language.md) -- complete syntax and semantics
- [Target Reference](../reference/targets.md) -- OS model, integration tracking, how-to-add checklists
- [Compiling a Program](../guides/compiling-a-program.md) -- `--target` flag and build pipeline
- [Programming Model](programming-model.md) -- bounded execution, cost transparency, auditability
- [Content-Addressed Code](content-addressing.md) -- how target-independent hashing works
- [Comparative Analysis](provable-computing.md) -- proving cost estimation and zkVM comparison
- [For Developers](../tutorials/for-developers.md) -- portability concepts for general developers
- [Vision](vision.md) -- long-term direction for Trident
