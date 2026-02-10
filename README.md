# Trident

A universal language for provable computation. Currently targets [Triton VM](https://triton-vm.org/), with architecture designed for any zkVM.

Trident has a three-layer architecture: a **universal core** (`std/core/`, `std/io/`, `std/crypto/`) that is target-agnostic, an **abstraction layer** that mediates between language semantics and backend specifics, and **backend extensions** (`ext/triton/`) that emit target-specific instructions. The default backend compiles directly to [TASM](https://triton-vm.org/spec/) (Triton Assembly) with no intermediate representation. Every language construct maps predictably to known instruction patterns. The compiler is a thin, auditable translation layer -- not an optimization engine.

## Why Triton VM (Default Target)

Trident currently targets Triton VM as its default backend. [Triton VM](https://triton-vm.org/) is the only zero-knowledge virtual machine that is simultaneously **quantum-safe**, **private**, **programmable**, and **mineable**. No elliptic curves anywhere in the proof pipeline -- security rests entirely on hash functions ([Tip5](https://eprint.iacr.org/2023/107) + [FRI](https://eccc.weizmann.ac.il/report/2017/134/)), making proofs resistant to quantum attacks with no trusted setup.

The VM is purpose-built for ZK: hash operations cost 1 clock cycle + 6 coprocessor rows (vs. thousands of cycles in RISC-V zkVMs). Native instructions for [Merkle tree](https://en.wikipedia.org/wiki/Merkle_tree) authentication, sponge hashing, and extension field dot products make recursive [STARK](docs/stark-proofs.md) verification practical inside the VM itself. [Neptune Cash](https://neptune.cash/) demonstrates this architecture in production as a Proof-of-Work blockchain where miners generate STARK proofs of arbitrary computation.

Trident exists because writing programs in raw TASM assembly doesn't scale. The language gives developers structured types, modules, bounded loops, match expressions, and a cost model -- while preserving the direct, auditable mapping to the VM that makes formal reasoning about proving cost possible.

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

The effect annotation (`-1`) declares the net stack change. The compiler trusts it to track stack layout across `asm` boundaries. The optional target tag restricts a block to a specific backend:

```
fn custom_hash(a: Field, b: Field) -> Field {
    asm(triton, -1) {
        hash
        swap 5 pop 1
        swap 4 pop 1
        swap 3 pop 1
        swap 2 pop 1
        swap 1 pop 1
    }
}
```

When no target tag is provided, `asm(-1) { ... }` applies to all backends. Use `asm(triton) { ... }` to emit instructions only when compiling for Triton VM.

## Standard Library

The standard library is layered: **universal modules** (`std.core.*`, `std.io.*`, `std.crypto.*`) work across all targets, while **backend extensions** (`ext.triton.*`) expose target-specific primitives.

### Universal Core (`std/core/`, `std/io/`, `std/crypto/`)

| Module | Functions | Purpose |
|--------|-----------|---------|
| `std.io.io` | `pub_read`, `pub_write`, `divine` | Public and secret I/O |
| `std.crypto.hash` | `tip5`, `sponge_init/absorb/squeeze` | [Tip5](https://eprint.iacr.org/2023/107) hashing |
| `std.core.field` | `add`, `sub`, `mul`, `neg`, `inv` | Field arithmetic |
| `std.core.convert` | `as_u32`, `as_field`, `split` | Type conversions |
| `std.core.u32` | `log2`, `pow`, `popcount` | U32 operations |
| `std.core.assert` | `is_true`, `eq`, `digest` | Assertions |
| `std.core.mem` | `read`, `write`, `read_block`, `write_block` | RAM access |
| `std.crypto.merkle` | `verify1`..`verify4`, `authenticate_leaf3` | [Merkle proofs](https://en.wikipedia.org/wiki/Merkle_tree) |
| `std.crypto.auth` | `verify_preimage`, `verify_digest_preimage` | Authorization |

### Backend Extensions (`ext/triton/`)

| Module | Functions | Purpose |
|--------|-----------|---------|
| `ext.triton.xfield` | `new`, `inv` | Extension field ops |
| `ext.triton.storage` | `read`, `write`, `read_digest`, `write_digest` | Persistent storage |
| `ext.triton.kernel` | `authenticate_field`, `tree_height` | [Neptune](https://neptune.cash/) kernel |
| `ext.triton.utxo` | `authenticate` | UTXO verification |

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
trident build main.tri --target triton    # VM target (default: triton)
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

When targeting Triton VM, Trident tracks proving cost across all six [Triton VM](https://triton-vm.org/) tables:

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
  std/            # Universal standard library (auto-discovered)
    core/         #   Field, U32, memory, assertions, conversions
    io/           #   Public/secret I/O
    crypto/       #   Hashing, Merkle proofs, authorization
  ext/            # Backend extensions
    triton/       #   Triton VM-specific: xfield, storage, kernel, utxo
```

### trident.toml

```toml
[project]
name = "my_project"
version = "0.1.0"
entry = "main.tri"

[targets.triton]
backend = "triton"
```

## Documentation

### Program Lifecycle

The complete journey from source code to verified proof:

| Stage | Guide | What happens |
|-------|-------|-------------|
| 1. Write | [Writing a Program](docs/writing-a-program.md) | Types, functions, modules, control flow |
| 2. Compile | [Compiling a Program](docs/compiling-a-program.md) | Build, check, cost analysis, error handling |
| 3. Run | [Running a Program](docs/running-a-program.md) | Execute TASM in Triton VM, I/O model, testing |
| 4. Deploy | [Deploying a Program](docs/deploying-a-program.md) | Neptune UTXO scripts, multi-target deployment |
| 5. Prove | [Generating Proofs](docs/generating-proofs.md) | Execution trace to STARK proof, cost optimization |
| 6. Verify | [Verifying Proofs](docs/verifying-proofs.md) | Proof checking, on-chain verification, quantum safety |

### Learning Paths

| You are... | Start here |
|---|---|
| New to zero-knowledge | [For Developers](docs/for-developers.md) &#8594; [Tutorial](docs/tutorial.md) &#8594; [How STARK Proofs Work](docs/stark-proofs.md) &#8594; [Optimization Guide](docs/optimization.md) |
| Coming from Solidity / Anchor / CosmWasm | [For Blockchain Devs](docs/for-blockchain-devs.md) &#8594; [Tutorial](docs/tutorial.md) &#8594; [Programming Model](docs/programming-model.md) &#8594; [Optimization Guide](docs/optimization.md) |
| Evaluating Trident for your project | [Vision](docs/vision.md) &#8594; [Comparative Analysis](docs/analysis.md) &#8594; [How STARK Proofs Work](docs/stark-proofs.md) &#8594; [Programming Model](docs/programming-model.md) |
| Already writing Trident | [Language Reference](docs/reference.md) &#8729; [Error Catalog](docs/errors.md) &#8729; [Optimization Guide](docs/optimization.md) |

### All Documents

**Lifecycle guides** (start here):
- [Writing a Program](docs/writing-a-program.md) -- Program structure, types, functions, modules, inline asm
- [Compiling a Program](docs/compiling-a-program.md) -- Build pipeline, errors, cost analysis, testing
- [Running a Program](docs/running-a-program.md) -- Execution in Triton VM, I/O model, debugging
- [Deploying a Program](docs/deploying-a-program.md) -- Neptune scripts, multi-target, deployment checklist
- [Generating Proofs](docs/generating-proofs.md) -- Trace to proof, cost optimization, recursive proofs
- [Verifying Proofs](docs/verifying-proofs.md) -- Proof checking, on-chain verification, quantum safety

**Background and reference**:
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

1. No intermediate representation: source to target assembly directly, for auditability
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
