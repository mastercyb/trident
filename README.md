# Trident

A minimal, security-first language for provable computation on [Triton VM](https://triton-vm.org/).

Trident compiles directly to [TASM](https://triton-vm.org/spec/) (Triton Assembly) with no intermediate representation. Every language construct maps predictably to known TASM patterns. The compiler is a thin, auditable translation layer -- not an optimization engine.

## Why Triton VM

[Triton VM](https://triton-vm.org/) is the only zero-knowledge virtual machine that is simultaneously **quantum-safe**, **private**, **programmable**, and **mineable**. No elliptic curves anywhere in the proof pipeline -- security rests entirely on hash functions ([Tip5](https://eprint.iacr.org/2023/107) + [FRI](https://eccc.weizmann.ac.il/report/2017/134/)), making proofs resistant to quantum attacks with no trusted setup.

The VM is purpose-built for ZK: hash operations cost 1 clock cycle + 6 coprocessor rows (vs. thousands of cycles in RISC-V zkVMs). Native instructions for [Merkle tree](https://en.wikipedia.org/wiki/Merkle_tree) authentication, sponge hashing, and extension field dot products make recursive [STARK](docs/stark-proofs.md) verification practical inside the VM itself. [Neptune Cash](https://neptune.cash/) demonstrates this architecture in production as a Proof-of-Work blockchain where miners generate STARK proofs of arbitrary computation.

Trident exists because writing programs in raw TASM assembly doesn't scale. The language gives developers structured types, modules, bounded loops, and a cost model -- while preserving the direct, auditable mapping to the VM that makes formal reasoning about proving cost possible.

For a detailed comparison of Triton VM against StarkWare, SP1, RISC Zero, Aleo, Mina, and NockVM, see the [comparative analysis](docs/analysis.md).

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
trident build hello.tri -o hello.tasm    # compile to TASM
trident build hello.tri                   # defaults to hello.tasm
```

## Language Overview

### Types

| Type | Width | Description |
|------|-------|-------------|
| `Field` | 1 | Base field element (mod p, [Goldilocks prime](https://xn--2-umb.com/22/goldilocks/) p = 2^64 - 2^32 + 1) |
| `XField` | 3 | Extension field element |
| `Bool` | 1 | Boolean (0 or 1) |
| `U32` | 1 | Unsigned 32-bit integer (range-checked) |
| `Digest` | 5 | [Tip5](https://eprint.iacr.org/2023/107) hash digest |
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

The effect annotation (`-1`) declares the net stack change. The compiler trusts it to track stack layout across `asm` boundaries:

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
| `std.hash` | `tip5`, `sponge_init/absorb/squeeze` | [Tip5](https://eprint.iacr.org/2023/107) hashing |
| `std.field` | `add`, `sub`, `mul`, `neg`, `inv` | Field arithmetic |
| `std.convert` | `as_u32`, `as_field`, `split` | Type conversions |
| `std.u32` | `log2`, `pow`, `popcount` | U32 operations |
| `std.assert` | `is_true`, `eq`, `digest` | Assertions |
| `std.xfield` | `new`, `inv` | Extension field ops |
| `std.mem` | `read`, `write`, `read_block`, `write_block` | RAM access |
| `std.storage` | `read`, `write`, `read_digest`, `write_digest` | Persistent storage |
| `std.merkle` | `verify1`..`verify4`, `authenticate_leaf3` | [Merkle proofs](https://en.wikipedia.org/wiki/Merkle_tree) |
| `std.auth` | `verify_preimage`, `verify_digest_preimage` | Authorization |
| `std.kernel` | `authenticate_field`, `tree_height` | [Neptune](https://neptune.cash/) kernel |
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

Trident tracks proving cost across all six [Triton VM](https://triton-vm.org/) tables:

| Table | What It Measures |
|-------|-----------------|
| Processor | Clock cycles (instructions executed) |
| Hash | Hash coprocessor rows (6 per `hash`) |
| U32 | Range checks and bitwise operations |
| Op Stack | Operand stack underflow rows |
| RAM | Memory read/write operations |
| Jump Stack | Function call/return overhead |

The **padded height** (next power of two of the tallest table) determines actual [STARK](docs/stark-proofs.md) proving cost. Use `--costs` to see the breakdown and `--hints` for optimization suggestions.

## Editor Support

- **[Zed](https://zed.dev/)**: Extension in `editor/zed/` with syntax highlighting and bracket matching
- **[Helix](https://helix-editor.com/)**: Configuration in `editor/helix/languages.toml`
- **Any [LSP](https://microsoft.github.io/language-server-protocol/) client**: Run `trident lsp` for diagnostics, completions, hover, go-to-definition, and signature help

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

### Learning Paths

| You are... | Start here |
|---|---|
| New to zero-knowledge | [For Developers](docs/for-developers.md) &#8594; [Tutorial](docs/tutorial.md) &#8594; [How STARK Proofs Work](docs/stark-proofs.md) &#8594; [Optimization Guide](docs/optimization.md) |
| Coming from Solidity / Anchor / CosmWasm | [For Blockchain Devs](docs/for-blockchain-devs.md) &#8594; [Tutorial](docs/tutorial.md) &#8594; [Programming Model](docs/programming-model.md) &#8594; [Optimization Guide](docs/optimization.md) |
| Evaluating Triton VM for your project | [Vision](docs/vision.md) &#8594; [Comparative Analysis](docs/analysis.md) &#8594; [How STARK Proofs Work](docs/stark-proofs.md) &#8594; [Programming Model](docs/programming-model.md) |
| Already writing Trident | [Language Reference](docs/reference.md) &#8729; [Error Catalog](docs/errors.md) &#8729; [Optimization Guide](docs/optimization.md) |

### All Documents

- [Vision](docs/vision.md) -- Why Trident exists and what you can build
- [Tutorial](docs/tutorial.md) -- Step-by-step developer guide
- [For Developers](docs/for-developers.md) -- Zero-knowledge from scratch (for Rust/Python/Go devs)
- [For Blockchain Devs](docs/for-blockchain-devs.md) -- Mental model migration (for Solidity/Anchor/CosmWasm devs)
- [How STARK Proofs Work](docs/stark-proofs.md) -- From execution traces to quantum-safe proofs
- [Language Reference](docs/reference.md) -- Quick lookup: types, operators, builtins, grammar
- [Language Specification](docs/spec.md) -- Complete design specification
- [Programming Model](docs/programming-model.md) -- Triton VM execution model
- [Optimization Guide](docs/optimization.md) -- Cost reduction strategies
- [Error Catalog](docs/errors.md) -- All error messages with explanations
- [Comparative Analysis](docs/analysis.md) -- Triton VM vs. every other ZK system

## Design Principles

1. No intermediate representation: source to TASM directly, for auditability
2. Deliberate limitation: one obvious way to do everything ([Vyper](https://docs.vyperlang.org/) philosophy)
3. Cost transparency: every function annotated with proving cost
4. Bounded execution: all loops require explicit bounds, no recursion
5. Compile-time everything: all type widths and array sizes known statically
6. Minimal dependencies: 4 runtime crates ([clap](https://crates.io/crates/clap), [ariadne](https://crates.io/crates/ariadne), [tower-lsp](https://crates.io/crates/tower-lsp), [tokio](https://crates.io/crates/tokio))

## Getting Help

- [GitHub Issues](https://github.com/nicktriton/trident/issues) -- Bug reports and feature requests
- [Language Specification](docs/spec.md) -- Complete reference for all language constructs
- [Error Catalog](docs/errors.md) -- Every error message explained with fixes

## License

See the LICENSE file for details.
