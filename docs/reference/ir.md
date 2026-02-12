# Trident IR: Architecture & Design

[← Language Reference](language.md) | [Target Reference](targets.md)

54 operations. 4 tiers. One source language compiles everywhere.

```
Source (.tri)
  │
  ▼
Lexer → Parser → AST
  │
  ▼
TypeChecker
  │
  ▼
TIRBuilder → Vec<TIROp>              ← 54 ops, target-independent
  │
  ├─→ StackLow       → Vec<String>   ← stack targets (Triton, Miden)
  │
  ├─→ LIR → RegLow   → Vec<u8>      ← register targets (x86-64, ARM64, RISC-V)
  │
  ├─→ TreeLow        → Noun → bytes  ← tree targets (Nock)
  │
  └─→ KIR → KernelLow → String       ← GPU targets (CUDA, Metal, Vulkan)
        │
        ▼
      Linker                          ← multi-module resolution (all targets)
```

---

## The OS Model

The IR is the compiler's universal instruction set. Each target is an operating
system with its own CPU (VM), word size (field), and instruction set extensions
(hash function, Merkle ops). Lowering translates the IR to each OS's native
instruction set. See [targets.md](targets.md) for the full OS model and target
profiles.

---

## Part I: The 54 Operations

### Tier 0 — Structure (11)

Every program, every target. Just computation.

| Group | Variants | Notes |
|-------|----------|-------|
| **Control flow — flat** (3) | `Call(String)` `Return` `Halt` | |
| **Control flow — structural** (3) | `IfElse { then, else }` `IfOnly { then }` `Loop { label, body }` | Nested bodies, not flat jumps |
| **Program structure** (3) | `FnStart(String)` `FnEnd` `Entry(String)` | `Entry` = program entry point (main function label) |
| **Passthrough** (2) | `Comment(String)` `Asm { lines, effect }` | `Asm` = inline assembly escape hatch |

The IR expresses intent, not formatting. Lowering handles labels, entry
boilerplate, and blank lines.

### Tier 1 — Universal (31)

Every target — provable or non-provable. All values are field elements.
Arithmetic groups are named by **interpretation**: how the value is treated.

| Group | Variants | Interpretation |
|-------|----------|----------------|
| **Stack** (4) | `Push(u64)` `Pop(u32)` `Dup(u32)` `Swap(u32)` | Stack manipulation |
| **Modular** (5) | `Add` `Sub` `Mul` `Neg` `Invert` | Modular field arithmetic (wraps at prime) |
| **Comparison** (2) | `Eq` `Lt` | Boolean (0 or 1) result |
| **Bitwise** (5) | `And` `Or` `Xor` `PopCount` `Split` | Bit pattern. `Split` → 2 u32 limbs |
| **Unsigned** (5) | `DivMod` `Shl` `Shr` `Log2` `Pow` | Unsigned integer. `DivMod` → 2 values |
| **I/O** (2) | `ReadIo(u32)` `WriteIo(u32)` | Public input/output channels |
| **Memory** (2) | `ReadMem(u32)` `WriteMem(u32)` | Address on stack, popped after access |
| **Assertions** (1) | `Assert(u32)` | Assert N elements are nonzero |
| **Hash** (1) | `Hash { width: u32 }` | Hash N elements into a digest |
| **Events** (2) | `Reveal { name, tag, field_count }` `Seal { name, tag, field_count }` | `Reveal` = fields in the clear; `Seal` = hashed, only digest visible. `Seal` is a Tier 1 op but emits Tier 2 sponge ops internally — programs using `seal` require a Tier 2 target |
| **Storage** (2) | `ReadStorage { width }` `WriteStorage { width }` | Persistent state access |

### Tier 2 — Provable (7)

Proof-capable targets only. No meaningful equivalent on non-provable targets.

| Group | Variants | Intent |
|-------|----------|--------|
| **Witness** (1) | `Hint(u32)` | Non-deterministic input from the prover |
| **Sponge** (4) | `SpongeInit` `SpongeAbsorb` `SpongeSqueeze` `SpongeLoad` | Incremental algebraic hashing |
| **Merkle** (2) | `MerkleStep` `MerkleLoad` | Merkle tree authentication |

### Tier 3 — Recursion (5)

Recursive verification only. STARK-in-STARK primitives.

| Group | Variants | Intent |
|-------|----------|--------|
| **Extension** (2) | `ExtMul` `ExtInvert` | Extension field arithmetic |
| **Folding** (2) | `FoldExt` `FoldBase` | FRI folding (extension / base field) |
| **Verification** (1) | `ProofBlock { program_hash, body }` | Recursive proof verification block. Body contains verification circuit |

### Totals

| Tier | Name | Count | What it enables |
|------|------|-------|-----------------|
| 0 | Structure | 11 | Any program on any target |
| 1 | Universal | 31 | Full computation — any target |
| 2 | Provable | 7 | Proof generation and verification |
| 3 | Recursion | 5 | Proofs that verify other proofs |
| | **Total** | **54** | |

A program's tier is its highest-tier op. The compiler rejects programs
that use ops above the target's capability.

---

## Part II: Four Lowering Paths

### Stack targets — TIR → StackLowering → assembly text

For stack-machine VMs: Triton, Miden, EVM, WASM.

```rust
pub trait StackLowering {
    fn lower(&self, ops: &[TIROp]) -> Vec<String>;
}
```

TIR maps nearly 1:1 to stack VM instructions. Each backend handles
structural ops differently (Triton: deferred subroutines; Miden: inline
`if.true/else/end`). See [`src/tir/lower/`](../../src/tir/lower/).

### Register targets — TIR → LIR → machine code

For register machines: x86-64, ARM64, RISC-V.

```rust
pub trait RegisterLowering {
    fn target_name(&self) -> &str;
    fn lower(&self, ops: &[LIROp]) -> Vec<u8>;
    fn lower_text(&self, ops: &[LIROp]) -> Vec<String>;
}
```

LIR converts TIR's stack semantics to three-address form with virtual
registers (`Reg(u32)`) and flat control flow (`Branch`/`Jump`/`LabelDef`).
Same 4-tier structure, register-addressed. See [`src/lir/`](../../src/lir/).

### Tree targets — TIR → TreeLowering → Noun

For combinator/tree-rewriting VMs: Nock.

```rust
pub trait TreeLowering {
    fn target_name(&self) -> &str;
    fn lower(&self, ops: &[TIROp]) -> Noun;
    fn serialize(&self, noun: &Noun) -> Vec<u8>;
}
```

Tree lowering takes TIR directly (like KernelLowering) and produces
Nock formulas — recursive tree structures where the program IS data.
The operand stack becomes a right-nested cons tree (the subject).
Stack operations become tree construction and axis addressing.
Control flow maps to Nock 6 (branch) and Nock 7 (compose).

Performance depends on jet matching — lowered formulas must produce
hashes that match registered jets for all cryptographic operations.
Without jets, naive tree interpretation would be extremely slow.
See [`src/tree/`](../../src/tree/).

### GPU targets — TIR → KIR → kernel source

For data-parallel GPUs: CUDA, Metal, Vulkan.

```rust
pub trait KernelLowering {
    fn target_name(&self) -> &str;
    fn lower(&self, ops: &[TIROp]) -> String;
}
```

KIR is not a separate IR. It wraps scalar TIR programs in GPU compute
kernels — each GPU thread runs one program instance with its own I/O.
Parallelism is across instances, not within one execution.
See [`src/kir/`](../../src/kir/).

---

## Part III: Design Principles

### Field elements all the way down

Every value is a field element. This is not a limitation — it's the
founding decision. Proof systems operate over finite fields. STARKs,
SNARKs, every future proof system will use polynomial commitments over
fields. This is math, not a choice.

When compiling to x86, field arithmetic has overhead (modular reduction).
That's the cost of provability. The native targets serve the provable
world — for testing, debugging, local execution. If you want a fast
native calculator without provability, use C.

The type system has `Field`, `U32`, `Bool`, `Digest` — but at the IR
level, they're all field elements with different range constraints.
The type checker validates before IR generation. By the time we emit
TIR, correctness is already guaranteed. The IR doesn't need types
because there's no instruction selection decision that depends on type.

### Provable-first, portable-second

The compiler is not a general-purpose toolchain that happens to support
proofs. It's a provable-first compiler that happens to also run on
non-provable targets.

This resolves every design tension:
- Stack-based TIR as canonical? Yes — proof VMs are stack machines.
- Field elements only? Yes — proofs operate over fields.
- Events in the universal tier? Yes — they're how provable programs
  communicate with the OS. On native targets, they're structured logging.

### Concurrency is orchestration, not computation

54 ops, all sequential. No channels, spawn, or await. This is correct.

A program is a pure sequential computation — read, compute, write.
The runtime decides how many copies run and how they connect:
- **GPU**: runtime dispatches N threads, each runs one program
- **Blockchain**: runtime dispatches transactions, each runs one program
- **Agent**: runtime dispatches particles, each runs one program

Concurrency lives in the runtime. The IR describes one computation.

### Structural over flat control flow

Bodies are nested `Vec<TIROp>`, not basic blocks with jumps:
- Source language is structured — preserving it avoids CFG reconstruction
- Stack backends need nesting for their native patterns
- Register backends trivially flatten nested bodies into labeled blocks

### No target instructions in the TIR

No `skiz`, `recurse`, `if.true`, `proc`. The TIR expresses intent
(branch, loop, function boundary) and each lowering chooses the
mechanism. This is what makes the 54 ops target-independent.

### Abstract operations over hardcoded patterns

`Reveal` instead of `push tag; write_io 1; ...` repeated per field. The
abstract op carries semantic meaning, letting each backend implement
events its own way — or ignore them entirely.

### The IR expresses intent, not formatting

No `Label`, `Preamble`, or `BlankLine` ops. Lowering generates its own
labels, entry boilerplate, and whitespace. If it doesn't affect
semantics, it doesn't belong in the IR.

### Naming conventions

- **Verb-first**: `ReadIo`, `WriteIo`, `ReadStorage`, `WriteStorage`
- **Max two words**: every variant is 1-2 words, no exceptions
- **Arithmetic groups named by interpretation**: Modular (field),
  Bitwise (bits), Unsigned (integer), Comparison (boolean)
- **Symmetric pairs**: `Reveal`/`Seal`, `Read`/`Write`, `FnStart`/`FnEnd`

---

## Part IV: Implementation Details

### TIRBuilder

[`TIRBuilder`](../../src/tir/builder/mod.rs) walks the type-checked AST and
produces `Vec<TIROp>` via [`StackManager`](../../src/stack.rs) with
automatic LRU spill/reload.

```rust
TIRBuilder::new(target_config)
    .with_cfg_flags(flags)
    .with_intrinsics(intrinsic_map)
    .with_module_aliases(aliases)
    .with_constants(constants)
    .with_mono_instances(instances)
    .with_call_resolutions(resolutions)
    .build_file(&file)
```

Five pre-scan passes (return widths, generics, intrinsics, structs/constants,
event tags), then emission: functions, then monomorphized generic instances.

Dispatch: [`build_stmt`](../../src/tir/builder/stmt.rs) →
[`build_expr`](../../src/tir/builder/expr.rs) →
[`build_call`](../../src/tir/builder/call.rs) (~40 intrinsics + user calls).

### Stack Management

[`StackManager`](../../src/stack.rs) tracks values by name, width,
and LRU timestamp. Overflow spills to RAM automatically. The string
round-trip (StackManager → strings → parse back to TIROp) is legacy
from the pre-IR emitter. Future cleanup: emit TIROp directly.

### Monomorphization

Generic functions become separate copies: `array_sum<8>` → `array_sum__8`.
Type parameters substitute into width calculations.

---

## Part V: File Layout

```
src/tir/                           ← stack-based IR (canonical)
├── mod.rs                         ← TIROp enum (54 variants) + Display
├── builder/                       ← AST → Vec<TIROp>
│   ├── mod.rs                     ← TIRBuilder struct
│   ├── stmt.rs                    ← statement emission
│   ├── expr.rs                    ← expression emission
│   ├── call.rs                    ← intrinsic dispatch + user calls
│   ├── helpers.rs                 ← spill parser, cfg, labels
│   ├── layout.rs                  ← type widths, struct layouts
│   └── tests.rs                   ← builder tests
└── lower/                         ← TIR → assembly text
    ├── mod.rs                     ← StackLowering trait + factory
    ├── triton.rs                  ← TRITON (TASM)
    ├── miden.rs                   ← MIDEN (MASM)
    └── tests.rs                   ← lowering tests

src/lir/                           ← register-based IR
├── mod.rs                         ← LIROp enum + Reg + Label + Display
├── convert.rs                     ← tir_to_lir() stub + ConvertCtx
└── lower/                         ← LIR → machine code
    ├── mod.rs                     ← RegisterLowering trait + factory
    ├── x86_64.rs                  ← x86-64 stub
    ├── arm64.rs                   ← ARM64 stub
    └── riscv.rs                   ← RISC-V stub

src/tree/                          ← tree/combinator lowering
├── mod.rs                         ← module docs
└── lower/                         ← TIR → Nock formulas
    ├── mod.rs                     ← TreeLowering trait + Noun type + factory
    └── nock.rs                    ← Nock VM (Nockchain)

src/kir/                           ← GPU kernel lowering
├── mod.rs                         ← module docs
└── lower/                         ← TIR → kernel source
    ├── mod.rs                     ← KernelLowering trait + factory
    ├── cuda.rs                    ← CUDA stub
    ├── metal.rs                   ← Metal stub
    └── vulkan.rs                  ← Vulkan stub

src/legacy/                        ← old emitter (deprecated, comparison tests only)
├── emitter/                       ← AST-to-assembly walker
└── backend/                       ← StackBackend trait + targets
```

---

## Part VI: Adding a Backend

### Stack target

1. Create `src/tir/lower/new_target.rs`
2. Implement `StackLowering` — one method: `fn lower(&self, ops: &[TIROp]) -> Vec<String>`
3. Register in `create_stack_lowering()`
4. Add `TargetConfig`

### Register target

1. Create `src/lir/lower/new_target.rs`
2. Implement `RegisterLowering` — `fn lower(&self, ops: &[LIROp]) -> Vec<u8>`
3. Register in `create_register_lowering()`

### Tree target

1. Create `src/tree/lower/new_target.rs`
2. Implement `TreeLowering` — `fn lower(&self, ops: &[TIROp]) -> Noun` + `fn serialize(&self, noun: &Noun) -> Vec<u8>`
3. Register in `create_tree_lowering()`

### GPU target

1. Create `src/kir/lower/new_target.rs`
2. Implement `KernelLowering` — `fn lower(&self, ops: &[TIROp]) -> String`
3. Register in `create_kernel_lowering()`

---

## See Also

- [Language Reference](language.md) — Types, operators, builtins, grammar
- [Target Reference](targets.md) — OS model, integration tracking, how-to-add checklists
- [Provable Computation](provable.md) — Hash, sponge, Merkle, extension field (Tier 2-3)
- [CLI Reference](cli.md) — Compiler commands and flags
- [Error Catalog](errors.md) — All compiler error messages explained

---

*Trident v0.5 — 54 operations. 4 tiers. One source language compiles everywhere.*
