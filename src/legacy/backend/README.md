# Backends

This directory implements the `StackBackend` trait and its five target-specific implementations. The trait is the single abstraction boundary between Trident's target-independent code generation logic (the [Emitter](../emitter/)) and the instruction sets of individual stack-machine VMs.

## StackBackend Trait

Defined in [mod.rs](mod.rs), the trait has ~40 methods organized by category:

| Category | Methods | Purpose |
|----------|---------|---------|
| Metadata | `target_name()`, `output_extension()` | Target identification and file naming |
| Stack | `inst_push`, `inst_pop`, `inst_dup`, `inst_swap` | Operand stack manipulation |
| Arithmetic | `inst_add`, `inst_mul`, `inst_eq`, `inst_lt`, ... | Field and u32 arithmetic |
| I/O | `inst_read_io`, `inst_write_io`, `inst_divine` | Program input/output and non-deterministic advice |
| Memory | `inst_read_mem`, `inst_write_mem` | RAM access |
| Hash | `inst_hash`, `inst_sponge_*` | Cryptographic hash operations |
| Merkle | `inst_merkle_step`, `inst_merkle_step_mem` | Merkle tree authentication |
| Control | `inst_assert`, `inst_skiz`, `inst_call`, `inst_return`, `inst_halt` | Branching and function calls |

Each method returns a `String` or `&'static str` containing the target assembly for that operation. The emitter calls these methods through `b_*()` wrappers that also update the stack model.

## Target Implementations

| File | Target | Output | Architecture | Field |
|------|--------|--------|-------------|-------|
| [triton.rs](triton.rs) | [Triton VM](https://triton-vm.org/) | `.tasm` | Stack-based | 64-bit Goldilocks (p = 2^64 - 2^32 + 1) |
| [miden.rs](miden.rs) | [Miden VM](https://polygon.technology/polygon-miden) | `.masm` | Stack-based | 64-bit Goldilocks |
| [openvm.rs](openvm.rs) | [OpenVM](https://github.com/openvm-org/openvm) | `.oasm` | Stack-based | Configurable |
| [sp1.rs](sp1.rs) | [SP1](https://github.com/succinctlabs/sp1) | `.s1asm` | Stack-based | Configurable |
| [cairo.rs](cairo.rs) | [Cairo/Sierra](https://www.cairo-lang.org/) | `.sierra` | Register-based* | 251-bit prime |

*Cairo is register-based, so its backend translates stack operations into Sierra's SSA-style register syntax (e.g., `push 42` becomes `felt252_const<42>() -> ([0])`).

## Backend Factory

The `create_backend()` function in [mod.rs](mod.rs) maps target name strings to backend instances:

```rust
create_backend("triton") → Box<TritonBackend>
create_backend("miden")  → Box<MidenBackend>
create_backend("openvm") → Box<OpenVMBackend>
create_backend("sp1")    → Box<SP1Backend>
create_backend("cairo")  → Box<CairoBackend>
```

Unknown target names fall back to Triton.

## Instruction Differences

The same Trident operation produces different assembly depending on the target:

| Operation | Triton | Miden | Cairo |
|-----------|--------|-------|-------|
| Push 42 | `push 42` | `push.42` | `felt252_const<42>() -> ([0])` |
| Add | `add` | `add` | `felt252_add([0], [1]) -> ([2])` |
| Call foo | `call foo` | `exec.foo` | `function_call<foo>([0]) -> ([1])` |
| Return | `return` | `end` | `return([0])` |
| Pop 1 | `pop 1` | `drop` | `drop([0])` |

## Adding a New Backend

1. Create `src/legacy/backend/yourvm.rs`
2. Define a unit struct implementing `StackBackend` — fill in all ~40 methods
3. Add `pub mod yourvm;` to [mod.rs](mod.rs) and a `pub(crate) use` re-export
4. Add a match arm in `create_backend()`
5. Add a `TargetConfig` variant in [target.rs](../../tools/target.rs) for VM-specific parameters (stack depth, field size, RAM layout)

No changes to the emitter, stack manager, or linker are needed — the new backend is automatically used when the user compiles with `--target yourvm`.
