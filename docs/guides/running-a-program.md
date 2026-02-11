# Running a Program

Trident is a compiler, not a runtime. After `trident build` produces a `.tasm` file, execution happens inside [Triton VM](https://triton-vm.org/) -- a STARK-based zero-knowledge virtual machine. Trident's job ends at code generation; the VM takes it from there.

This guide covers how compiled Trident programs execute, how to feed them input, and how to test and debug them using the tools available today.

## From TASM to Execution

The `trident build` command translates a `.tri` source file into TASM (Triton Assembly):

```bash
trident build main.tri              # produces main.tasm
trident build main.tri -o out.tasm  # custom output path
```

The resulting `.tasm` file is a complete, self-contained program in Triton VM's instruction set. To actually run it, you load the TASM into Triton VM. Trident itself has no `run` subcommand and no built-in interpreter -- it is purely a source-to-assembly compiler.

The separation is deliberate. Triton VM execution produces an algebraic execution trace, which is what makes STARK proof generation possible. A Trident-side interpreter would bypass the trace machinery entirely, making it useless for proving.

## The I/O Model

Triton VM programs have exactly two input channels and one output channel. There is no filesystem, no network, no environment variables.

### Public Input (`pub_read`)

Values provided via public input are visible to the verifier. They appear in the `Claim.input` field and are part of the public record.

```
let x: Field = pub_read()
```

This compiles to `read_io 1`, which pops one element from the public input stream. The verifier will see this value and can check it.

Use public input for: data the verifier needs to see (kernel hashes, commitment roots, parameters that define what the program is proving).

### Secret Input (`divine`)

Values provided via secret input are invisible to the verifier. They come from the prover's non-deterministic input and never appear in the Claim.

```
let witness: Field = divine()
```

This compiles to `divine 1`, which pops one element from the secret input stream. The verifier has no access to this value.

Use secret input for: witness data (preimages, Merkle authentication paths, private keys, any data the program needs but the world should not see).

### Public Output (`pub_write`)

Programs produce output via `pub_write`, which pushes values to `Claim.output`. This is the program's public result.

```
pub_write(result)
```

### No Other Channels

There is nothing else. No stdin/stdout in the conventional sense, no logging, no side effects. A Trident program consumes public input and secret input, computes, and produces public output. Everything in between is hidden by the zero-knowledge property. See the [Programming Model](programming-model.md) for the full I/O table and the divine-and-authenticate pattern.

## The Execution Model

Triton VM is a stack machine. Understanding the execution model helps when reading `.tasm` output or diagnosing cost issues.

**Operand stack.** The primary workspace. It holds up to 16 elements before spilling to an overflow area (tracked by the Op Stack table). Most instructions operate on the top few stack elements.

**RAM.** Random-access memory addressed by field elements. Accessed via `read_mem` and `write_mem`. Used for data structures that exceed the stack's capacity.

**Jump stack.** Tracks function call/return addresses. Every `call` pushes a return address; every `return` pops one. This is internal bookkeeping -- you do not interact with it directly from Trident source code.

**No heap allocator.** RAM exists, but there is no `malloc`. The compiler manages RAM layout statically. Arrays and structs are placed at known addresses determined at compile time.

For the full execution model, including the six constraint tables that govern proving cost, see the [Programming Model](programming-model.md).

## Running with Triton VM

To execute a compiled `.tasm` program, you use Triton VM directly. The primary interface is the [`triton-vm`](https://crates.io/crates/triton-vm) Rust crate.

A minimal execution looks like this in Rust:

```rust
use triton_vm::prelude::*;

// Load the compiled program
let source = std::fs::read_to_string("main.tasm").unwrap();
let program = Program::from_code(&source).unwrap();

// Prepare inputs
let public_input = PublicInput::from([1_u64, 2]);
let secret_input = NonDeterminism::default();

// Execute
let (trace, output) = program
    .trace_execution(public_input, secret_input)
    .unwrap();

println!("Output: {:?}", output);
```

Key points:

- **`PublicInput`** corresponds to values consumed by `pub_read()` / `read_io`. Order matters -- the program reads them in FIFO order.
- **`NonDeterminism`** corresponds to values consumed by `divine()` / `divine`. It has three parts: `individual_tokens` (field elements), `digests` (for `merkle_step`), and `ram` (pre-initialized memory).
- **`trace_execution`** runs the program and returns both the output and the algebraic execution trace needed for proof generation.
- If you only need the output and not the trace, `VM::run` is lighter weight.

The [`tasm-lang`](https://crates.io/crates/tasm-lang) ecosystem provides higher-level utilities for constructing programs and managing I/O, and is the foundation that Trident's standard library maps onto.

## Testing Programs

Trident supports `#[test]` annotations on functions. The `trident test` command compiles and verifies these:

```bash
trident test main.tri
```

### Writing Tests

```
#[test]
fn test_addition() {
    let a: Field = 3
    let b: Field = 4
    assert(a + b == 7)
}

#[test]
fn test_digest_equality() {
    let d: Digest = hash(1, 2, 3, 4, 5)
    let e: Digest = hash(1, 2, 3, 4, 5)
    assert_digest(d, e)
}
```

### What `trident test` Does

1. Compiles each `#[test]` function as an independent program
2. Verifies that each program halts without error (no assertion failures, no stack underflow, no out-of-bounds access)
3. Reports pass/fail for each test function

Tests do not have access to `pub_read()` or `divine()` -- they are self-contained computations. If a test function triggers a VM error (failed assertion, empty input stream, etc.), the test fails.

### Limitations

- No mocking or dependency injection
- No test fixtures or setup/teardown
- No property-based testing built in
- Tests run as full compilations, so test suites are not instantaneous

For integration testing that exercises I/O, you need to use Triton VM directly (see the previous section) with prepared input streams.

## Debugging

Trident has no debugger. There is no step-through execution, no breakpoints, no variable inspection at runtime. This is an inherent limitation of the architecture: Trident is a compiler, and execution happens inside a VM that produces cryptographic traces, not interactive debugging sessions.

The primary debugging tools are static analysis and cost analysis.

### Type Checking

```bash
trident check main.tri
```

Catches type errors, undefined variables, arity mismatches, and unreachable code without producing a `.tasm` file. This is the fastest feedback loop.

### Cost Analysis

```bash
trident build main.tri --costs
```

Prints a table showing proving cost across all six Triton VM constraint tables (Processor, Hash, U32, Op Stack, RAM, Jump Stack). The padded height -- the next power of two of the tallest table -- determines actual STARK proving cost.

### Per-Line Annotations

```bash
trident build main.tri --annotate
```

Adds cost annotations to every line of the source, showing how many cycles each expression or statement contributes. Useful for identifying which lines are expensive.

### Hotspot Analysis

```bash
trident build main.tri --hotspots
```

Ranks functions and expressions by their contribution to the dominant cost table. When you need to reduce proving cost, start here.

### Cost Comparison

```bash
trident build main.tri --save-costs baseline.json
# ... make changes ...
trident build main.tri --compare baseline.json
```

Saves a cost snapshot and compares against it later. Useful for verifying that optimizations actually reduced cost.

### Optimization Hints

```bash
trident build main.tri --hints
```

Suggests specific optimizations based on the program's cost profile. See the [Optimization Guide](optimization.md) for the full set of cost reduction strategies.

### Common Issues

| Symptom | Likely Cause |
|---------|-------------|
| Assertion failure at runtime | Logic error in divine-and-authenticate pattern; divined value does not match expected hash |
| Stack underflow | Function consumes more stack elements than available; check arity |
| Program does not halt | Loop bound too large or logic never reaches `halt`; all loops require explicit bounds, so infinite loops are not possible, but very large bounds can be effectively unbounded |
| High Op Stack table cost | Too many values on the stack simultaneously; refactor to use RAM for intermediate storage |
| High Hash table cost | Many hash operations; consider batching or restructuring Merkle proofs |

## Understanding Output

The `.tasm` file produced by `trident build` is human-readable Triton Assembly. Understanding its structure helps when diagnosing issues or verifying that the compiler produced correct code.

### Structure of a .tasm File

A typical compiled output looks like:

```
call main
halt

main:
    read_io 1
    read_io 1
    add
    write_io 1
    return

helpers_double:
    dup 0
    add
    return
```

**Entry point.** The file begins with `call main` followed by `halt`. This is the program's top-level control flow.

**Labels.** Each function becomes a label. The naming convention is `modulename_functionname`, so `helpers.double` becomes `helpers_double`. Nested modules use additional underscores.

**Instructions.** Each line is a single TASM instruction. The instruction set is documented in the [Triton VM specification](https://triton-vm.org/spec/). Common ones:

| Instruction | Effect |
|-------------|--------|
| `push N` | Push constant onto stack |
| `dup N` | Duplicate the element at depth N |
| `swap N` | Swap top with element at depth N |
| `pop N` | Remove N elements from stack top |
| `add`, `mul` | Field arithmetic on top two elements |
| `read_io N` | Read N elements from public input |
| `write_io N` | Write N elements to public output |
| `divine N` | Read N elements from secret input |
| `hash` | Hash top 10 stack elements, produce 5-element digest |
| `call label` | Call function |
| `return` | Return from function |
| `assert` | Pop top element, crash if not 1 |
| `halt` | End execution |

### Reading the Output

The `.tasm` file is the definitive record of what will execute. If the program's behavior surprises you, reading the assembly is the most direct way to understand what is happening. The mapping from Trident source to TASM is intentionally straightforward -- there is no optimization pass that rearranges code in unexpected ways.

## See Also

- [Tutorial](tutorial.md) -- Step-by-step guide: types, functions, modules, testing
- [Compiling a Program](compiling-a-program.md) -- The prior stage: build pipeline and cost analysis
- [Error Catalog](errors.md) -- All error messages explained with fixes
- [How STARK Proofs Work](stark-proofs.md) -- Execution traces and the six constraint tables
- [For Developers](for-developers.md) -- Zero-knowledge concepts for general developers
- [Language Reference](reference.md) -- Types, operators, builtins, grammar

## Next Step

Once you can build and run programs locally, the next stage is packaging them for on-chain use: [Deploying a Program](deploying-a-program.md).
