# Trident TIR: Architecture & Design

The Trident compiler uses a target-independent intermediate representation (TIR)
between the type-checked AST and backend code generation. TIR is a sequence
of stack operations with structural control flow, lowered to assembly text by
target-specific backends.

```
Source (.tri)
  │
  ▼
Lexer → Parser → AST
  │
  ▼
TypeChecker  →  Exports { mono_instances, call_resolutions }
  │
  ▼
TIRBuilder    →  Vec<TIROp>          ← target-independent
  │
  ├─→ Lowering          → Vec<String>  ← stack targets (Triton, Miden, EVM)
  │     │
  │     ▼
  │   Linker → final .tasm/.masm
  │
  ├─→ tir_to_lir()      → Vec<LIROp>  ← register targets (x86-64, ARM64, RISC-V)
  │     │
  │     ▼
  │   RegisterLowering → Vec<u8>       ← native machine code
  │
  └─→ KernelLowering    → String       ← GPU targets (CUDA, Metal, Vulkan)
```

---

## Why a TIR?

Proof VMs have fundamentally different architectures:

| Target | Control flow | Functions | Events |
|--------|-------------|-----------|--------|
| Triton VM | Deferred subroutines + `skiz` | `__label:` | `write_io` |
| Miden VM | Inline `if.true/else/end` | `proc/end` | comments |
| RISC-V (OpenVM/SP1) | Branch instructions | call/ret | syscalls |
| Cairo | `branch_align` + enum dispatch | functions | hints |

Without a TIR, every target's conventions would be embedded in the AST walker.
The TIR separates **what to compute** (stack operations with structural control
flow) from **how to emit it** (target-specific instruction selection).

Adding a new backend means implementing one trait method — not reimplementing
the entire compiler.

---

## File Layout

```
src/tir/                            ← canonical location
├── mod.rs                         ← TIROp enum + Display
├── builder/                       ← AST → Vec<TIROp>
│   ├── mod.rs                     ← TIRBuilder struct, build_file, build_fn
│   ├── stmt.rs                    ← statement emission (let, if, for, match, emit, seal)
│   ├── expr.rs                    ← expression emission (literals, vars, binops, structs)
│   ├── call.rs                    ← intrinsic dispatch (~40 builtins) + user calls
│   ├── helpers.rs                 ← spill parser, cfg checks, label gen, stack helpers
│   ├── layout.rs                  ← type width resolution, struct field layouts
│   └── tests.rs                   ← builder unit tests
└── lower/                         ← Vec<TIROp> → assembly text
    ├── mod.rs                     ← Lowering trait + create_lowering factory
    ├── triton.rs                  ← Triton VM backend (TASM)
    ├── miden.rs                   ← Miden VM backend (MASM)
    └── tests.rs                   ← lowering tests + Emitter comparison suite

src/codegen/ir/                    ← backward-compatible re-exports only
├── mod.rs                         ← pub use crate::tir::{TIROp, Lowering, ...}
└── builder.rs                     ← pub use crate::tir::builder::TIRBuilder
```

---

## TIROp: The Operation Set

`TIROp` is an enum with **52 variants** in four tiers. Higher tier = narrower
target set. No target instructions (`skiz`, `recurse`, `if.true`, `proc`)
appear in the TIR. All names follow **verb-first** convention — ops are
imperative commands: Read, Write, Open, Assert.

### Tier 0 — Structure (10 variants)

The scaffolding. Present in every program, on every target. Not
blockchain-specific — just computation. The IR expresses intent, not
formatting — lowering handles labels, entry boilerplate, and blank lines.

| Group | Variants | Notes |
|-------|----------|-------|
| **Functions** (4) | `FnStart(String)` `FnEnd` `Call(String)` `Return` | |
| **Control flow** (3) | `IfElse { then_body, else_body }` `IfOnly { then_body }` `Loop { label, body }` | Bodies are nested `Vec<TIROp>`, not flat jumps |
| **Termination** (1) | `Halt` | |
| **Passthrough** (2) | `Comment(String)` `Asm { lines, effect }` | `Asm` passes inline assembly verbatim |

Each backend lowers structural ops differently:

- **Triton**: extracts bodies into deferred subroutines, emits `skiz` + `call`
- **Miden**: emits inline `if.true / else / end`
- **RISC-V**: could emit conditional branches to labels

### Tier 1 — Universal (31 variants)

Compiles to every target — blockchain or conventional. Stack primitives,
arithmetic, I/O, memory, hashing, events, storage.

| Group | Variants | Notes |
|-------|----------|-------|
| **Stack** (4) | `Push(u64)` `Pop(u32)` `Dup(u32)` `Swap(u32)` | Indices from top (0 = TOS). Depth ≤ [`stack_depth`](../src/tools/target.rs:20) |
| **Modular** (5) | `Add` `Sub` `Mul` `Neg` `Invert` | Modular field arithmetic (wraps at field prime) |
| **Comparison** (2) | `Eq` `Lt` | Produce boolean (0 or 1) result |
| **Bitwise** (5) | `And` `Or` `Xor` `PopCount` `Split` | Treat values as bit patterns. `Split` → 2 u32 limbs |
| **Unsigned** (5) | `DivMod` `Shl` `Shr` `Log2` `Pow` | Treat values as unsigned integers. `DivMod` → 2 values; `Shl`/`Shr` shift by N bits |
| **I/O** (2) | `ReadIo(u32)` `WriteIo(u32)` | Public input/output channels |
| **Memory** (2) | `ReadMem(u32)` `WriteMem(u32)` | Address on stack, popped after access |
| **Assertions** (1) | `Assert(u32)` | Assert N stack elements are nonzero |
| **Hash** (1) | `Hash { width: u32 }` | Hash N elements into a digest |
| **Events** (2) | `Open { name, tag, field_count }` `Seal { name, tag, field_count }` | `Open` = fields in the clear; `Seal` = fields hashed, only digest visible |
| **Storage** (2) | `ReadStorage { width }` `WriteStorage { width }` | Persistent state access |

### Tier 2 — Provable (7 variants)

Requires a proof-capable target. Non-deterministic witness input, sponge
construction, and Merkle authentication have no meaningful equivalent on
conventional VMs.

| Group | Variants | Intent |
|-------|----------|--------|
| **Witness** (1) | `Hint(u32)` | Non-deterministic input from the prover (advice/witness) |
| **Sponge** (4) | `SpongeInit` `SpongeAbsorb` `SpongeSqueeze` `SpongeLoad` | Incremental algebraic hashing for proof systems |
| **Merkle** (2) | `MerkleStep` `MerkleLoad` | Merkle tree authentication |

Programs using these ops require `--target` with proof capability.
The compiler can reject them when targeting conventional architectures.

### Tier 3 — Recursion (4 variants)

STARK-in-STARK verification primitives. Extension field arithmetic and FRI
folding steps required for recursive proof verification.

```
ExtMul        — extension field multiply
ExtInvert     — extension field inverse
FoldExt       — fold extension field elements
FoldBase      — fold base field elements
```

These are the primitives that make recursion practical. Every STARK-based
system that supports recursive verification needs equivalent functionality:
Triton has native instructions, Miden has `fri_ext2fold`, SP1/OpenVM would
use precompile syscalls. The math is universal — only the acceleration
mechanism differs per backend.

Currently only Triton provides native support. As backends gain recursion
capabilities, these ops will generalize into abstract operations where
each backend supplies its own implementation.

---

## TIRBuilder: AST to TIR

[`TIRBuilder`](../src/tir/builder/mod.rs:37) walks the type-checked AST and
produces `Vec<TIROp>`. It manages a
[`StackManager`](../src/codegen/stack.rs:58) that models the runtime stack
with automatic LRU spill/reload to RAM.

### Configuration

```rust
TIRBuilder::new(target_config)
    .with_cfg_flags(flags)               // conditional compilation
    .with_intrinsics(intrinsic_map)      // fn name → native instruction
    .with_module_aliases(aliases)        // short → full module name
    .with_constants(constants)           // resolved constant values
    .with_mono_instances(instances)      // generic instantiations
    .with_call_resolutions(resolutions)  // per-call-site generic resolutions
    .build_file(&file)
```

### build_file: Pre-scan then emit

[`build_file`](../src/tir/builder/mod.rs:144) runs five pre-scan passes before
emitting any instructions:

1. **Return widths** — resolve return type width for every function (needed by
   callers to adjust the stack model)
2. **Generic detection** — save generic function ASTs for later monomorphization
3. **Intrinsic mapping** — parse `#[intrinsic(...)]` annotations
4. **Struct/constant registration** — collect struct definitions and constant
   values for field layout and constant folding
5. **Event tags** — assign sequential integer tags (0, 1, 2...) to events

Then emission:

```
1. sec_ram declarations → Comment ops
2. If program (not library): Preamble("main")
3. Non-generic, non-test functions → build_fn each
4. Monomorphized generic instances → build_mono_fn each
```

### Statement and expression dispatch

- [`build_stmt`](../src/tir/builder/stmt.rs:24) — `let`, `assign`, `if/else`,
  `for`, `match`, `emit`, `seal`, `asm`, `return`
- [`build_expr`](../src/tir/builder/expr.rs:11) — literals, variables, binary
  ops, function calls, tuples, arrays, field access, indexing, struct init
- [`build_call`](../src/tir/builder/call.rs:12) — resolves ~40 intrinsics
  (pub_read/write, hint, assert, hash, sponge, merkle, ram, xfield ops) or
  delegates to [`build_user_call`](../src/tir/builder/call.rs:225) for
  user-defined functions

### Monomorphization

Generic functions flow through the IR as separate monomorphized copies:

```
TypeChecker discovers: array_sum called with <8> and <16>
  → exports.mono_instances = [
      MonoInstance { name: "array_sum", size_args: [8] },
      MonoInstance { name: "array_sum", size_args: [16] },
    ]

TIRBuilder emits two functions:
  FnStart("array_sum__8")   ... FnEnd
  FnStart("array_sum__16")  ... FnEnd

Call sites emit:
  Call("array_sum__8")   or   Call("array_sum__16")
```

Type parameters substitute into width calculations:
`fn process<N>(arr: [Field; N])` with N=8 → parameter width = 8.

---

## Stack Management

[`StackManager`](../src/codegen/stack.rs:58) tracks every value on the operand
stack by name, width, and LRU timestamp. When the stack exceeds
`max_stack_depth` (16 for Triton), it spills the least-recently-used named
variable to RAM.

### Spill/reload round-trip

The `StackManager` generates spill instructions as strings (via
[`SpillFormatter`](../src/codegen/stack.rs:16)), which `TIRBuilder` converts
back to `TIROp` values through
[`parse_spill_effect`](../src/tir/builder/helpers.rs:16):

```
StackManager detects overflow
  → generates:  ["swap 15", "push 1073741824", "swap 1", "write_mem 1", "pop 1"]
  → TIRBuilder calls parse_spill_effect on each
  → produces:   [Swap(15), Push(1073741824), Swap(1), WriteMem(1), Pop(1)]
```

On reload (when a spilled variable is accessed):

```
  → generates:  ["push 1073741824", "read_mem 1", "pop 1"]
  → produces:   [Push(1073741824), ReadMem(1), Pop(1)]
```

The string round-trip exists because `StackManager` predates the IR and was
designed for the old `Emitter` which worked with strings directly. A future
cleanup could make `StackManager` emit `TIROp` directly.

### Key invariants

- Stack depth never exceeds `max_stack_depth` after `ensure_space()` calls
- Every spilled variable is automatically reloaded when accessed
- `flush_stack_effects()` must be called after any `StackManager` operation
  that may produce side effects

---

## Lowering: TIR to Assembly

The [`Lowering`](../src/tir/lower/mod.rs:17) trait has one method:

```rust
pub trait Lowering {
    fn lower(&self, ops: &[TIROp]) -> Vec<String>;
}
```

[`create_lowering`](../src/tir/lower/mod.rs:23) returns the right backend
by target name.

### Triton VM lowering

[`TritonLowering`](../src/tir/lower/triton.rs:19) produces TASM using a
**deferred subroutine pattern**:

**IfElse** — the condition is on stack. Push a marker, then use two `skiz`
instructions to dispatch:

```
push 1           ← marker
swap 1           ← move condition above marker
skiz             ← if condition: call then (which clears marker)
call __then__1
skiz             ← if marker still set: call else
call __else__2
```

The `then` block pops the marker on entry and pushes 0 on exit (clearing it).
The `else` block runs only if the marker survived.

Deferred blocks are collected during function lowering and flushed after
[`FnEnd`](../src/tir/lower/triton.rs:255). Nested control flow creates nested
deferred blocks, flushed iteratively until empty.

**Loop** — emitted as a labeled subroutine with counter check:

```
__loop__1:
    dup 0          ← copy counter
    push 0
    eq
    skiz           ← if zero: exit
    return
    push -1
    add            ← decrement
    {body}
    recurse        ← loop back
```

### Miden VM lowering

[`MidenLowering`](../src/tir/lower/miden.rs:10) produces MASM with **inline
structured control flow**:

**IfElse**:
```
if.true
    {then_body}
else
    {else_body}
end
```

**Loop**:
```
dup.0
push.0
eq
if.true
    drop
else
    push.18446744069414584320    ← Goldilocks -1
    add
    {body}
    exec.self                    ← tail recursion
end
```

**Functions**: `proc.name / end` instead of labels.

### Instruction mapping differences

| TIR | Triton | Miden |
|----|--------|-------|
| `Pop(n)` | `pop n` | `drop` (repeated n times) |
| `Swap(d)` | `swap d` | `swap` (d=1) or `movup.d` |
| `Lt` | `lt` | `u32lt` |
| `And` | `and` | `u32and` |
| `DivMod` | `div_mod` | `u32divmod` |
| `Invert` | `invert` | `inv` |
| `Call(f)` | `call __f` | `exec.f` |
| `Return` | `return` | (implicit — `end` closes proc) |
| `Comment(s)` | `// s` | `# s` |
| `Neg` | `push -1; mul` | `push.18446744069414584320; mul` |
| `Sub` | `push -1; mul; add` | `push.18446744069414584320; mul; add` |

---

## Compile pipeline integration

Single-file compilation ([`compile_with_options`](../src/lib.rs)):

```rust
let file = parse_source(source, filename)?;
let exports = TypeChecker::with_target(config).check_file(&file)?;

let ir = TIRBuilder::new(config)
    .with_cfg_flags(flags)
    .with_mono_instances(exports.mono_instances)
    .with_call_resolutions(exports.call_resolutions)
    .build_file(&file);

let lowering = create_lowering(&config.name);
let asm = lowering.lower(&ir).join("\n");
```

Multi-module compilation ([`compile_project_with_options`](../src/lib.rs))
adds intrinsic maps, module aliases, and external constants gathered across
all modules before building IR.

---

## Design decisions

### Structural over flat control flow

Bodies are nested `Vec<TIROp>`, not basic blocks with jumps. This is deliberate:

- The source language has structured control flow — preserving it avoids
  reconstructing nesting from flat CFGs
- Stack-machine backends (Triton, Miden) need nesting to emit their native
  patterns
- Register backends (RISC-V) can trivially flatten nested bodies into labeled
  blocks

### Stack-level, not variable-level

The TIR has explicit `Push/Pop/Dup/Swap`. Variable-to-stack-position resolution
happens in the builder via `StackManager`. This makes Triton lowering nearly
1:1 and avoids inventing register allocation for stack machines.

Register backends would track a virtual stack and map operations to register
moves — more work per backend, but keeps the common path fast.

### No target instructions in the TIR

No `skiz`, `recurse`, `if.true`, `proc`, `movup`. The TIR expresses intent
(conditional branch, loop, function boundary) and each lowering chooses
the mechanism. This is what makes the TIR target-independent.

### Abstract operations over hardcoded patterns

`Open` instead of `push tag; write_io 1; write_io 1; ...`. The abstract
op carries the semantic meaning (event name, tag, field count), letting each
backend implement events in its native way — or ignore them entirely.

---

## Adding a new backend

1. Create `src/tir/lower/new_target.rs`
2. Implement [`Lowering`](../src/tir/lower/mod.rs:17) — one method:
   `fn lower(&self, ops: &[TIROp]) -> Vec<String>`
3. Handle each TIROp variant, paying special attention to:
   - `IfElse`, `IfOnly`, `Loop` — your target's control flow conventions
   - `FnStart`/`FnEnd` — your target's function boundary syntax
   - `Open`/`Seal` — your target's event model
   - `ReadStorage`/`WriteStorage` — your target's persistence model
4. Register in [`create_lowering`](../src/tir/lower/mod.rs:23)
5. Add a [`TargetConfig`](../src/tools/target.rs:20) (hardcoded or TOML)
6. Add tests — the comparison test pattern in
   [`lower/tests.rs`](../src/tir/lower/tests.rs) is a good template

---

## LIR: Register-Based IR for Native Targets

The TIR is a stack-based representation — ideal for stack machines (Triton,
Miden, EVM, WASM). Register machines (x86-64, ARM64, RISC-V) need a different
form: explicit virtual registers, three-address instructions, and flat control
flow.

The **LIR** (Low-level IR) provides this as a parallel lowering path:

```
AST → TIR ─→ Lowering          → Vec<String>  (stack targets)
          ├→ LIR → RegisterLow  → Vec<u8>      (register targets)
          └→ KIR → KernelLow    → String        (GPU kernel source)
```

### LIR design

| Property | TIR | LIR |
|----------|-----|-----|
| Operands | Implicit stack | Explicit virtual registers `Reg(u32)` |
| Form | Stack ops (`Push`, `Dup`, `Swap`) | Three-address (`Add(dst, src1, src2)`) |
| Control flow | Nested bodies (`IfElse { then_body, else_body }`) | Flat labels (`Branch`, `Jump`, `LabelDef`) |
| Memory | `ReadMem(n)` / `WriteMem(n)` | `Load { dst, base, offset }` / `Store { src, base, offset }` |
| Output | Assembly text `Vec<String>` | Machine code `Vec<u8>` |

LIR mirrors TIR's four-tier structure with register-addressed equivalents.
Variant counts will track TIR as naming is finalized.

- **Tier 0 — Structure**: `Branch`/`Jump`/`LabelDef` replace nested
  `IfElse`/`IfOnly`/`Loop` bodies. No stack ops.
- **Tier 1 — Universal**: `LoadImm`/`Move` replace stack manipulation.
  Base+offset `Load`/`Store` replace `ReadMem`/`WriteMem`.
- **Tier 2 — Provable**: Same sponge/merkle/hint ops, register-addressed.
- **Tier 3 — Recursion**: Same ops, three-address form.

### File layout

```
src/lir/
├── mod.rs          ← LIROp enum (51 variants) + Reg + Label + Display + tests
├── convert.rs      ← tir_to_lir() stub + ConvertCtx helper
└── lower/
    ├── mod.rs      ← RegisterLowering trait + create_register_lowering() factory
    ├── x86_64.rs   ← X86_64Lowering stub
    ├── arm64.rs    ← Arm64Lowering stub
    └── riscv.rs    ← RiscVLowering stub
```

### RegisterLowering trait

```rust
pub trait RegisterLowering {
    fn target_name(&self) -> &str;
    fn lower(&self, ops: &[LIROp]) -> Vec<u8>;          // machine code
    fn lower_text(&self, ops: &[LIROp]) -> Vec<String>;  // debug text
}
```

### Current status

The LIR module is a **scaffold** — all types, traits, and tests compile
and pass, but `tir_to_lir()` and the three backend `lower()` methods are
`todo!()` stubs. The architecture is established for future implementation
of register allocation and instruction selection.

---

## KIR: GPU Kernel Lowering

GPUs execute thousands of threads in lockstep — the same instruction on
different data. Trident programs are scalar, but can be **batch-executed**:
run N copies of the same program on N different inputs simultaneously.

KIR is not a separate IR. It takes TIR directly and wraps it in a GPU
compute kernel. Each GPU thread runs one program instance:

- `ReadIo` → `buffer[thread_id * input_width + i]`
- `WriteIo` → `buffer[thread_id * output_width + i]`
- All other ops → scalar computation per thread

### KernelLowering trait

```rust
pub trait KernelLowering {
    fn target_name(&self) -> &str;
    fn lower(&self, ops: &[TIROp]) -> String;  // complete kernel source
}
```

### File layout

```
src/kir/
├── mod.rs          ← module docs
└── lower/
    ├── mod.rs      ← KernelLowering trait + create_kernel_lowering() factory
    ├── cuda.rs     ← CudaLowering stub (NVIDIA)
    ├── metal.rs    ← MetalLowering stub (Apple Silicon)
    └── vulkan.rs   ← VulkanLowering stub (cross-platform)
```

### Current status

Scaffold only — all types, traits, and tests compile. Backend `lower()`
methods are `todo!()` stubs.

---

## Cross-references

- [Universal design](universal-design.md) — multi-target architecture overview
- [Universal execution](universal-execution.md) — three-level abstraction model
- [`src/tir/README.md`](../src/tir/README.md) — module-level navigation with line links
- [`src/tir/builder/README.md`](../src/tir/builder/README.md) — builder file map
- [`src/tir/lower/README.md`](../src/tir/lower/README.md) — backend file map + strategy table
- [`src/tools/target.rs`](../src/tools/target.rs) — TargetConfig definition
- [`src/codegen/stack.rs`](../src/codegen/stack.rs) — StackManager implementation
- [`src/lir/mod.rs`](../src/lir/mod.rs) — LIROp enum + register-based IR
- [`src/lir/lower/mod.rs`](../src/lir/lower/mod.rs) — RegisterLowering trait + native backends
- [`src/kir/mod.rs`](../src/kir/mod.rs) — KIR module + GPU kernel lowering
- [`src/kir/lower/mod.rs`](../src/kir/lower/mod.rs) — KernelLowering trait + GPU backends
