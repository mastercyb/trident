# Code Generation

The codegen module translates a type-checked AST into target assembly for stack-machine VMs. A single AST walker ([emitter.rs](emitter.rs)) drives the entire process, delegating instruction selection to a pluggable [backend/](backend/) trait. This design means all control flow, variable management, and optimization logic is written once and shared across all five compilation targets.

```
         AST (from typecheck/)
              |
              v
     ┌────────────────┐
     │    Emitter      │  emitter.rs — walks the AST
     │                 │
     │  emit_file()    │  top-level: items, structs, constants
     │  emit_fn()      │  function prologue, body, epilogue
     │  emit_block()   │  sequential statements
     │  emit_stmt()    │  let, assign, if, while, for, return, asm {}
     │  emit_expr()    │  arithmetic, calls, field access, indexing
     │  emit_call()    │  function calls + intrinsic dispatch
     └───────┬────────┘
             │ calls b_push(), b_add(), b_call(), ...
             v
     ┌────────────────┐
     │  StackBackend   │  backend/mod.rs — trait
     │                 │
     │  TritonBackend  │  → .tasm  (Triton VM)
     │  MidenBackend   │  → .masm  (Miden VM)
     │  OpenVMBackend  │  → .oasm  (OpenVM)
     │  SP1Backend     │  → .s1asm (SP1)
     │  CairoBackend   │  → .sierra (StarkNet)
     └────────────────┘
             │
             v
     ┌────────────────┐
     │    Linker       │  linker.rs — multi-module assembly
     └────────────────┘
             │
             v
        output file
```

## Files

| File | LOC | Role |
|------|-----|------|
| [emitter.rs](emitter.rs) | 2,775 | AST walker — the core of code generation |
| [stack.rs](stack.rs) | 474 | LRU-based stack manager with automatic RAM spill/reload |
| [linker.rs](linker.rs) | 134 | Multi-module linker with label mangling |
| [backend/](backend/) | 703 | StackBackend trait + 5 target implementations |

## Emitter

The `Emitter` struct is the central code generator. It walks the AST top-down through a fixed method chain:

```
emit_file → emit_fn → emit_block → emit_stmt → emit_expr → emit_call
```

Rather than emitting target instructions directly, the emitter calls wrapper methods (`b_push()`, `b_add()`, `b_call()`, etc.) that delegate to the active `StackBackend`. This indirection means the emitter never contains target-specific strings — all instruction selection lives in the backend.

Key responsibilities:
- **Variable tracking** — struct field layouts, widths, stack positions
- **Generic monomorphization** — emits specialized copies of generic functions with concrete size parameters
- **Deferred blocks** — if/else branches and loop bodies are emitted as labeled subroutines after the current function
- **Inline assembly** — `asm { ... }` blocks spill all managed variables to RAM, emit raw instructions, then resume tracking
- **Conditional compilation** — `#[cfg(...)]` attributes on items and functions
- **Events** — `emit` statements generate tagged I/O sequences

## Stack Manager

[stack.rs](stack.rs) implements an LRU-based operand stack model. Stack-machine VMs have a fixed operand stack depth (typically 16 elements). When the program has more live variables than slots, the manager automatically:

1. Identifies the **least-recently-used** named variable
2. **Spills** it to RAM using target-specific instructions (via `SpillFormatter`)
3. **Reloads** it on next access

The manager tracks named variables and anonymous temporaries separately — temporaries are never spilled. Configuration (max depth, RAM base address) comes from `TargetConfig`, defaulting to Triton VM's parameters (depth=16, spill base=2^30).

## Linker

[linker.rs](linker.rs) combines multiple module outputs into a single program. Each module's labels are mangled with a module prefix (`crypto.sponge` → `crypto_sponge__`) to avoid collisions. The linker:

1. Emits the entry point (`call modname__main` + `halt`)
2. Concatenates each module's assembly with mangled labels
3. Rewrites `call` targets to use mangled names

## Data Flow

```
Trident source
     │
     ├─ Frontend parses → AST
     ├─ TypeChecker validates → MonoInstances (generic resolutions)
     │
     v
  Emitter::emit_file(ast)
     │
     ├─ Registers struct layouts, constants, event tags
     ├─ For each function: emit_fn() walks the body
     │    ├─ StackManager tracks variable positions
     │    ├─ b_*() methods → StackBackend → target instructions
     │    └─ Deferred blocks emitted after function body
     │
     v
  Vec<String> (assembly lines)
     │
     ├─ Single module: joined directly
     ├─ Multi-module: linker::link() mangles and concatenates
     │
     v
  Final assembly output
```
