# Trident

A minimal, security-first language for provable computation on Triton VM.

Trident compiles directly to TASM (Triton Assembly) with no intermediate representation. Every language construct maps predictably to known TASM patterns. The compiler is a thin, auditable translation layer -- not an optimization engine.

## Quick Start

```bash
# Build from source
cargo build --release

# Create a new project
trident init my_project
cd my_project

# Build to TASM
trident build main.tri

# Check without emitting
trident check main.tri

# Format source code
trident fmt main.tri

# Run tests
trident test main.tri

# Generate documentation
trident doc main.tri
```

## Hello World

```
program hello

fn main() {
    let a: Field = pub_read()
    let b: Field = pub_read()
    let sum: Field = a + b
    pub_write(sum)
}
```

Build it:

```bash
trident build hello.tri -o hello.tasm
```

## Language Overview

### Types

| Type | Width | Description |
|------|-------|-------------|
| `Field` | 1 | Base field element (mod p, p = 2^64 - 2^32 + 1) |
| `XField` | 3 | Extension field element |
| `Bool` | 1 | Boolean (0 or 1) |
| `U32` | 1 | Unsigned 32-bit integer (range-checked) |
| `Digest` | 5 | Tip5 hash digest |
| `[T; N]` | N*width(T) | Fixed-size array |
| `(T, U)` | width(T)+width(U) | Tuple |

### Structs

```
struct Point {
    pub x: Field,
    pub y: Field,
}

fn origin() -> Point {
    Point { x: 0, y: 0 }
}
```

### Control Flow

```
// Conditionals
if balance > 0 {
    transfer(balance)
} else {
    abort()
}

// Bounded loops (bound is required)
for i in 0..10 bounded 10 {
    process(i)
}

// Pattern matching
match op_code {
    0 => { pay() }
    1 => { lock() }
    _ => { reject() }
}
```

### Functions and Modules

```
// main.tri
program my_app

use helpers

fn main() {
    let x: Field = pub_read()
    let result: Field = helpers.double(x)
    pub_write(result)
}
```

```
// helpers.tri
module helpers

pub fn double(x: Field) -> Field {
    x + x
}
```

### Size-Generic Functions

```
fn sum<N>(arr: [Field; N]) -> Field {
    let mut total: Field = 0
    for i in 0..N bounded N {
        total = total + arr[i]
    }
    total
}
```

### Events

```
event Transfer {
    from: Digest,
    to: Digest,
    amount: Field,
}

fn pay() {
    // Open event (fields visible to verifier)
    emit Transfer { from: sender, to: receiver, amount: value }

    // Sealed event (fields hashed, only digest visible)
    seal Transfer { from: sender, to: receiver, amount: value }
}
```

### Inline Assembly

```
fn custom_hash(a: Field, b: Field) -> Field {
    asm(-1) {
        hash
        swap 5 pop 1
        swap 4 pop 1
        swap 3 pop 1
        swap 2 pop 1
        swap 1 pop 1
    }
}
```

## Standard Library

13 modules providing Triton VM primitives:

| Module | Functions | Purpose |
|--------|-----------|---------|
| `std.io` | `pub_read`, `pub_write`, `divine` | Public and secret I/O |
| `std.hash` | `tip5`, `sponge_init/absorb/squeeze` | Tip5 hashing |
| `std.field` | `add`, `sub`, `mul`, `neg`, `inv` | Field arithmetic |
| `std.convert` | `as_u32`, `as_field`, `split` | Type conversions |
| `std.u32` | `log2`, `pow`, `popcount` | U32 operations |
| `std.assert` | `is_true`, `eq`, `digest` | Assertions |
| `std.xfield` | `new`, `inv` | Extension field ops |
| `std.mem` | `read`, `write`, `read_block`, `write_block` | RAM access |
| `std.storage` | `read`, `write`, `read_digest`, `write_digest` | Persistent storage |
| `std.merkle` | `verify1`..`verify4`, `authenticate_leaf3` | Merkle proofs |
| `std.auth` | `verify_preimage`, `verify_digest_preimage` | Authorization |
| `std.kernel` | `authenticate_field`, `tree_height` | Neptune kernel |
| `std.utxo` | `authenticate` | UTXO verification |

## CLI Reference

### `trident build`

Compile a `.tri` file to TASM.

```bash
trident build main.tri                    # Output to main.tasm
trident build main.tri -o out.tasm        # Custom output path
trident build main.tri --costs            # Print cost analysis
trident build main.tri --hotspots         # Show top cost contributors
trident build main.tri --hints            # Show optimization hints
trident build main.tri --annotate         # Per-line cost annotations
trident build main.tri --save-costs c.json  # Save costs as JSON
trident build main.tri --compare c.json   # Diff costs with previous build
trident build main.tri --target release   # Release target
```

### `trident check`

Type-check without emitting TASM.

```bash
trident check main.tri
trident check main.tri --costs
```

### `trident fmt`

Format source code.

```bash
trident fmt main.tri          # Format in place
trident fmt src/              # Format directory
trident fmt main.tri --check  # Check only (exit 1 if unformatted)
```

### `trident test`

Run `#[test]` functions.

```bash
trident test main.tri
```

### `trident doc`

Generate documentation with cost annotations.

```bash
trident doc main.tri                # Print to stdout
trident doc main.tri -o docs.md     # Write to file
```

### `trident init`

Create a new project.

```bash
trident init my_project
```

### `trident lsp`

Start the Language Server Protocol server.

## Cost Model

Trident tracks proving cost across all six Triton VM tables:

| Table | What It Measures |
|-------|-----------------|
| Processor | Clock cycles (instructions executed) |
| Hash | Hash coprocessor rows (6 per `hash`) |
| U32 | Range checks and bitwise operations |
| Op Stack | Operand stack underflow rows |
| RAM | Memory read/write operations |
| Jump Stack | Function call/return overhead |

The **padded height** (next power of two of the tallest table) determines actual proving cost. Use `--costs` to see the breakdown and `--hints` for optimization suggestions.

## Editor Support

- **Zed**: Extension in `editor/zed/` with syntax highlighting and bracket matching
- **Helix**: Configuration in `editor/helix/languages.toml`
- **Any LSP client**: Run `trident lsp` for diagnostics, completions, hover, go-to-definition, and signature help

## Project Structure

```
my_project/
  trident.toml    # Project configuration
  main.tri        # Entry point (program)
  helpers.tri     # Library module
  std/            # Standard library (auto-discovered)
```

### trident.toml

```toml
[project]
name = "my_project"
version = "0.1.0"
entry = "main.tri"

[targets.debug]
flags = ["debug"]

[targets.release]
flags = ["release"]
```

## Documentation

- [Language Specification](docs/spec.md) -- Complete language reference
- [Programming Model](docs/programming-model.md) -- Triton VM execution model
- [Tutorial](docs/tutorial.md) -- Step-by-step developer guide
- [Optimization Guide](docs/optimization.md) -- Cost reduction strategies
- [Error Catalog](docs/errors.md) -- All error messages with explanations

## Design Principles

1. **No intermediate representation** -- Source to TASM directly, for auditability
2. **Deliberate limitation** -- One obvious way to do everything (Vyper philosophy)
3. **Cost transparency** -- Every function annotated with proving cost
4. **Bounded execution** -- All loops require explicit bounds, no recursion
5. **Compile-time everything** -- All type widths and array sizes known statically
6. **Minimal dependencies** -- 4 runtime crates (clap, ariadne, tower-lsp, tokio)

## License

See LICENSE file for details.
