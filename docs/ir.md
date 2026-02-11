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
  ▼
Lowering     →  Vec<String>        ← target-specific assembly
  │
  ▼
Linker       →  final .tasm/.masm
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

`TIROp` is an enum with **53 variants** organized in three tiers plus a
target-specific section. No target instructions (`skiz`, `recurse`, `if.true`,
`proc`) appear in the IR.

### Tier 1 — Core instructions (24 variants)

1:1 with stack machine primitives. Every backend maps these directly to
native instructions.

| Group | Variants | Notes |
|-------|----------|-------|
| **Stack** (5) | `Push(u64)` `PushNegOne` `Pop(u32)` `Dup(u32)` `Swap(u32)` | Indices from top (0 = TOS). Depth ≤ [`stack_depth`](../src/tools/target.rs:20) |
| **Arithmetic** (12) | `Add` `Mul` `Eq` `Lt` `And` `Xor` `DivMod` `Invert` `Split` `Log2` `Pow` `PopCount` | Native field. `DivMod` → 2 values; `Split` → 2 u32 limbs |
| **I/O** (3) | `ReadIo(u32)` `WriteIo(u32)` `Divine(u32)` | Public I/O and non-deterministic witness |
| **Memory** (2) | `ReadMem(u32)` `WriteMem(u32)` | Address on stack, popped after access |
| **Assertions** (2) | `Assert` `AssertVector` | Single element or word-width check |

### Tier 2 — Abstract operations (12 variants)

Semantic intent that each backend expands to its own native pattern. The IR
says **what**, the lowering decides **how**.

| Group | Variants | Intent |
|-------|----------|--------|
| **Hash** (6) | `Hash` `SpongeInit` `SpongeAbsorb` `SpongeSqueeze` `SpongeAbsorbMem` `HashDigest` | Cryptographic hashing and incremental sponge |
| **Merkle** (2) | `MerkleStep` `MerkleStepMem` | Merkle tree verification |
| **Events** (2) | `EmitEvent { name, tag, field_count }` `SealEvent { name, tag, field_count }` | Observable events and hash-sealed commitments |
| **Storage** (2) | `StorageRead { width }` `StorageWrite { width }` | Persistent state access |

How backends expand abstract ops:

| Op | Triton | Miden | EVM (future) |
|----|--------|-------|-------------|
| `EmitEvent` | `push tag; write_io 1` per field | comment + `drop` | `LOG` + topic hash |
| `SealEvent` | pad + `hash` + `write_io 5` | pad + `hperm` + `drop` | keccak + emit |
| `StorageRead` | `read_mem` + `pop 1` | `mem_load` | `SLOAD` |
| `StorageWrite` | `write_mem` + `pop 1` | `mem_store` | `SSTORE` |
| `HashDigest` | `hash` | `hperm` | `KECCAK256` |

Programs use `emit`, `seal`, `ram_read`, `hash` — the IR keeps them abstract,
and each backend maps them to its native primitives.

### Tier 3 — Structure & control flow (13 variants)

Program organization and control flow. Structural ops carry nested bodies so
each backend chooses its own lowering strategy.

| Group | Variants | Notes |
|-------|----------|-------|
| **Control flow — flat** (3) | `Call(String)` `Return` `Halt` | |
| **Control flow — structural** (3) | `IfElse { then_body, else_body }` `IfOnly { then_body }` `Loop { label, body }` | Bodies are nested `Vec<TIROp>`, not flat jumps |
| **Program structure** (5) | `Label` `FnStart` `FnEnd` `Preamble` `BlankLine` | |
| **Passthrough** (2) | `Comment(String)` `RawAsm { lines, effect }` | `RawAsm` passes inline assembly verbatim |

Each backend lowers structural ops differently:

- **Triton**: extracts bodies into deferred subroutines, emits `skiz` + `call`
- **Miden**: emits inline `if.true / else / end`
- **RISC-V**: could emit conditional branches to labels

The condition/counter is already consumed from the stack when the structural op
executes. Target filtering (`asm(triton) { }`) happens before IR building.

### Recursion extension (4 variants — STARK-in-STARK verification)

Extension field arithmetic and FRI folding steps required for **recursive
proof verification** — verifying a STARK proof inside another STARK program.

```
ExtMul        — extension field multiply
ExtInvert     — extension field inverse
FriFold       — FRI folding step (ext × ext)
FriBaseFold   — FRI folding step (base × ext)
```

These are the primitives that make recursion practical. Every STARK-based
system that supports recursive verification needs equivalent functionality:
Triton has native instructions, Miden has `fri_ext2fold`, SP1/OpenVM would
use precompile syscalls. The math is universal — only the acceleration
mechanism differs per backend.

Currently only Triton provides native support. As backends gain recursion
capabilities, these ops will generalize into abstract operations (like
`EmitEvent`) where each backend supplies its own implementation.

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
  (pub_read/write, divine, assert, hash, sponge, merkle, ram, xfield ops) or
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
| `PushNegOne` | `push -1` | `push.18446744069414584320` |

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

`EmitEvent` instead of `push tag; write_io 1; write_io 1; ...`. The abstract
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
   - `EmitEvent`/`SealEvent` — your target's event model
   - `StorageRead`/`StorageWrite` — your target's persistence model
4. Register in [`create_lowering`](../src/tir/lower/mod.rs:23)
5. Add a [`TargetConfig`](../src/tools/target.rs:20) (hardcoded or TOML)
6. Add tests — the comparison test pattern in
   [`lower/tests.rs`](../src/tir/lower/tests.rs) is a good template

---

## Cross-references

- [Universal design](universal-design.md) — multi-target architecture overview
- [Universal execution](universal-execution.md) — three-level abstraction model
- [`src/tir/README.md`](../src/tir/README.md) — module-level navigation with line links
- [`src/tir/builder/README.md`](../src/tir/builder/README.md) — builder file map
- [`src/tir/lower/README.md`](../src/tir/lower/README.md) — backend file map + strategy table
- [`src/tools/target.rs`](../src/tools/target.rs) — TargetConfig definition
- [`src/codegen/stack.rs`](../src/codegen/stack.rs) — StackManager implementation
