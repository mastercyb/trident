# Compiling a Program

This guide covers everything about the Trident compilation process: how source code becomes Triton Assembly, how to invoke the compiler, how to read errors, and how to analyze proving cost before you ever run a program. It is the second stage of the Trident lifecycle (Writing -> **Compiling** -> Running -> Deploying -> Generating Proofs -> Verifying Proofs).

## The Compilation Pipeline

Trident compiles `.tri` source files directly to [TASM](https://triton-vm.org/spec/) (Triton Assembly) with no intermediate representation. The pipeline has six stages:

```
source (.tri)
  |
  v
Lexer        -- tokenize source into a stream of lexemes
  |
  v
Parser       -- build an AST from the token stream
  |
  v
Type Checker -- validate types, resolve names, detect recursion
  |
  v
Emitter      -- walk the AST and produce TASM for each module
  |
  v
Linker       -- mangle labels, stitch modules, emit entry point
  |
  v
output (.tasm)
```

There is no optimization pass and no IR. Every language construct maps to a known instruction pattern. The compiler is a thin, auditable translation layer -- you can read the generated TASM and trace it back to the source line that produced it.

This design is deliberate. In provable computation, predictability matters more than cleverness. If you can read the source, you can reason about the proof.

## Building with `trident build`

### Basic Usage

Compile a single file:

```bash
trident build main.tri
```

Output:

```
Compiled -> main.tasm
```

The default output file replaces the `.tri` extension with `.tasm`. To specify a different path:

```bash
trident build main.tri -o output/program.tasm
```

### Project Builds

If your project has a `trident.toml`, you can point `trident build` at the project directory instead of a specific file:

```bash
trident build .
```

The compiler reads `trident.toml`, finds the entry point, resolves all module dependencies, and produces a single linked `.tasm` file named after the project:

```
Compiled -> my_project.tasm
```

You can also pass any `.tri` file inside a project directory. If the compiler finds a `trident.toml` in the file's directory or any ancestor, it builds the full project:

```bash
trident build src/main.tri    # finds trident.toml, builds whole project
```

### Output File Contents

The generated `.tasm` file is a complete Triton Assembly program. For multi-module projects, the linker produces a single file with:

1. An entry point that calls the program's `main` function and halts
2. Each module's functions with mangled labels (e.g., `helpers__double:`)
3. Comments marking module boundaries

```tasm
    call my_app__main
    halt

// === module: helpers ===
helpers__double:
    dup 0
    add
    return

// === module: my_app ===
my_app__main:
    read_io 1
    call helpers__double
    write_io 1
    return
```

## Type Checking with `trident check`

To validate a program without producing any output file, use `trident check`:

```bash
trident check main.tri
```

Output on success:

```
OK: main.tri
```

On failure, the compiler prints diagnostics and exits with a non-zero status code. This makes `check` useful in CI pipelines and editor integrations:

```bash
# CI: fail the build if any type errors exist
trident check .
```

The `check` command resolves modules the same way `build` does -- it type-checks all dependencies in topological order. You can also request a cost report without emitting TASM:

```bash
trident check main.tri --costs
```

## Understanding Errors

Trident uses [ariadne](https://crates.io/crates/ariadne) to render diagnostics with source spans, color-coded severity, and contextual help. A typical error looks like:

```
error: binary operator '+' requires matching types, got Field and Bool
  --> main.tri:5:21
   |
 5 |     let z: Field = x + y
   |                     ^^^^^
   |
  help: ensure both operands have the same type
```

### Error Categories

**Lexer errors** catch invalid characters and missing syntax before parsing begins. For example, using `-` instead of `sub(a, b)` or `/` instead of `/%`:

```
error: unexpected '-'; Trident has no subtraction operator
  help: use the `sub(a, b)` function instead of `a - b`
```

**Parser errors** report structural problems: missing declarations, unmatched braces, exceeded nesting depth.

**Type errors** are the most common. They include type mismatches in operations, assignments, and return types; undefined variables and functions; arity mismatches; and immutability violations.

**Control flow errors** catch missing `bounded` annotations on for loops, non-exhaustive `match` statements, and unreachable code after `return`.

**Module errors** report missing module files, circular dependencies, and duplicate definitions.

**Recursion detection** is a dedicated pass. Trident prohibits all recursion (direct and indirect) because Triton VM requires deterministic trace lengths:

```
error: recursive function call detected: main -> foo -> main
  help: Trident does not allow recursion; use `for` loops instead
```

For the complete list of every error message with explanations and fixes, see the [Error Catalog](errors.md).

## Cost Analysis at Compile Time

Trident can estimate proving cost statically -- without executing the program. This is possible because all loop bounds are known at compile time and there is no recursion.

### The Six Triton VM Tables

When targeting Triton VM, proving cost is determined by [six execution tables](stark-proofs.md). The STARK prover must pad the tallest table to the next power of two, so proving cost is dominated by whichever table is tallest:

| Table | Grows With |
|-------|-----------|
| **Processor** | Every instruction executed (loop iterations, function calls) |
| **Hash** | Every `hash` / `tip5` call (6 rows per hash permutation) |
| **U32** | Range checks and bitwise operations (`as_u32`, `split`, `&`, `^`) |
| **Op Stack** | Stack underflow handling (deep variable access, large structs) |
| **RAM** | Memory reads and writes (array indexing, struct fields, spilling) |
| **Jump Stack** | Function calls and returns, if/else branches |

### Cost Flags

All cost flags work with both `trident build` and `trident check`.

**`--costs`** prints a summary report showing each function's cost across all six tables and the program's padded height:

```bash
trident build main.tri --costs
```

**`--hotspots`** shows the top cost contributors (functions sorted by their impact on the dominant table):

```bash
trident build main.tri --hotspots
```

**`--hints`** prints actionable optimization suggestions (hint codes H0001-H0004). For example, it might suggest batching multiple single-value `tip5` calls into one:

```bash
trident build main.tri --hints
```

```
Optimization hints:
  H0001: function 'process' has 3 separate tip5 calls that could be batched
    note: each tip5 call adds 6 Hash Table rows regardless of input count
    help: pack up to 10 field elements into a single tip5 call
```

**`--annotate`** prints every source line with its cost breakdown in brackets:

```bash
trident build main.tri --annotate
```

```
 1 | program test
 2 |
 3 | fn main() {
 4 |     let a: Field = pub_read()                  [cc=1]
 5 |     let b: Field = pub_read()                  [cc=1]
 6 |     let sum: Field = a + b                     [cc=3, os=2]
 7 |     pub_write(sum)                             [cc=1]
 8 | }
```

### Tracking Costs Over Time

Save a baseline and compare after changes:

```bash
trident build main.tri --save-costs baseline.json
# ... make changes ...
trident build main.tri --compare baseline.json
```

The comparison shows which functions got cheaper or more expensive, making cost regressions visible in code review.

For strategies on reducing proving cost, see the [Optimization Guide](optimization.md).

## Multi-Module Compilation

### Module Resolution

When the compiler encounters a `use` statement, it resolves the module name to a file path using these search paths in order:

| Module prefix | Search path | Example |
|---|---|---|
| `std.*` | Standard library directory (`std/`) | `use std.crypto.hash` resolves to `std/crypto/hash.tri` |
| `ext.*` | Extension library directory (`ext/`) | `use ext.triton.xfield` resolves to `ext/triton/xfield.tri` |
| (no prefix) | Project root directory | `use helpers` resolves to `helpers.tri` |
| (dotted) | Project root, nested | `use crypto.sponge` resolves to `crypto/sponge.tri` |

The standard library directory is found by searching (in order):

1. The `TRIDENT_STDLIB` environment variable
2. `std/` relative to the compiler binary
3. `std/` in the current working directory

The extension directory follows the same pattern using `TRIDENT_EXTLIB` and `ext/`.

### Dependency Order

The compiler discovers all reachable modules by scanning `use` statements, then type-checks them in topological order (dependencies before dependents). If a circular dependency is detected, compilation fails:

```
error: circular dependency detected involving module 'a'
  help: break the cycle by extracting shared definitions into a separate module
```

### Label Mangling

The linker mangles all function labels with the module name to prevent collisions. A function `verify` in module `crypto.sponge` becomes `crypto_sponge__verify` in the linked output. Cross-module calls are rewritten to use the mangled names.

### Project Configuration

A `trident.toml` at the project root configures the build:

```toml
[project]
name = "my_project"
version = "0.1.0"
entry = "main.tri"

[targets.debug]
flags = ["debug", "verbose"]

[targets.release]
flags = ["release"]
```

| Field | Purpose |
|---|---|
| `name` | Project name (used for output file naming) |
| `version` | Project version |
| `entry` | Entry point file (default: `main.tri`) |
| `target` | Default VM target (optional, overrides `--target` default) |

Profile-specific flags enable conditional compilation with `cfg` attributes. Use `--profile` to select which flag set is active:

```bash
trident build . --profile release
```

## Targeting VMs

Trident's compiler is parameterized by a `TargetConfig` that defines every target-specific constant: stack depth, digest width, hash rate, field prime, cost tables, and output extension. The default target is Triton VM.

```bash
trident build main.tri --target triton    # explicit (same as default)
```

The `--target` flag selects a `TargetConfig` by name. The built-in `triton` config sets:

| Parameter | Value |
|---|---|
| Architecture | Stack machine |
| Field prime | 2^64 - 2^32 + 1 (Goldilocks) |
| Stack depth | 16 |
| Digest width | 5 field elements |
| Extension field degree | 3 |
| Hash rate | 10 field elements |
| Output extension | `.tasm` |
| Cost tables | processor, hash, u32, op_stack, ram, jump_stack |

Custom targets can be defined as TOML files in a `targets/` directory. The compiler searches for `targets/{name}.toml` relative to the compiler binary and the working directory. A custom target file specifies the same parameters:

```toml
[target]
name = "custom_vm"
display_name = "Custom VM"
architecture = "stack"
output_extension = ".casm"

[field]
prime = "2^31 - 1"
limbs = 1

[stack]
depth = 32
spill_ram_base = 0

[hash]
digest_width = 4
rate = 8

[extension_field]
degree = 0

[cost]
tables = ["cycles", "memory"]
```

The architecture field (`stack` or `register`) determines how the emitter generates code. Stack architectures (like Triton VM) use direct emission; register architectures would require a lightweight IR. Currently only stack-based targets are supported.

## Formatting

`trident fmt` reformats source files to the canonical Trident style. It parses the file, preserves comments, and re-emits the AST with consistent indentation and spacing:

```bash
trident fmt main.tri          # format in place
trident fmt src/              # format all .tri files recursively
```

Output:

```
Formatted: main.tri
Already formatted: helpers.tri
```

Use `--check` mode in CI to verify formatting without modifying files. It exits with status 1 if any file would change:

```bash
trident fmt --check .
```

```
OK: main.tri
would reformat: helpers.tri
```

Hidden directories and `target/` are automatically skipped during recursive formatting.

## Testing

Annotate test functions with `#[test]`. Test functions take no arguments and return no value:

```
program my_app

fn add(a: Field, b: Field) -> Field {
    a + b
}

#[test]
fn test_add() {
    assert(add(2, 3) == 5)
}
```

Run tests with:

```bash
trident test main.tri
```

Output:

```
running 1 test
  test test_add ... ok (cc=8, hash=0, u32=0)

test result: ok. 1 passed; 0 failed
```

The test runner compiles each test function and reports pass/fail along with cost metrics. For project builds, it discovers `#[test]` functions across all modules:

```bash
trident test .
```

## See Also

- [Tutorial](tutorial.md) -- Step-by-step guide: types, functions, modules, inline asm
- [Error Catalog](errors.md) -- Every error message explained with fixes
- [Optimization Guide](optimization.md) -- Cost reduction strategies for all six tables
- [Language Reference](reference.md) -- Types, operators, builtins, grammar
- [Programming Model](programming-model.md) -- How programs execute inside Triton VM
- [Universal Design](universal-design.md) -- Multi-target architecture and the `--target` flag
- [How STARK Proofs Work](stark-proofs.md) -- The six tables that determine proving cost
- [Formal Verification](formal-verification.md) -- Verify program properties at compile time

## Next Step

[Running a Program](running-a-program.md) -- execute your compiled TASM in Triton VM.
