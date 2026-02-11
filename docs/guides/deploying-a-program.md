# Deploying a Program

This is the fourth stage of the Trident program lifecycle:
**Writing** -> **Compiling** -> **Running** -> **Deploying** -> **Generating Proofs** -> **Verifying Proofs**.

You have a compiled `.tasm` artifact. This guide covers what "deployment" means
for zero-knowledge programs and how to get your compiled Trident program into a
real system.

---

## What "Deployment" Means for ZK Programs

If you are coming from smart contract development, "deploy" means sending
bytecode to a chain and receiving an address. ZK programs work differently.

A Trident program compiles to a `.tasm` file -- a sequence of Triton VM
instructions. That file **is** the deployable artifact. There is no on-chain
deployment transaction, no contract address, no ABI registry. Instead:

1. The program is identified by its **Tip5 hash** (the `program_digest` in the
   Claim structure). Anyone who has the same source compiles to the same hash.
2. **Provers** execute the program locally with their inputs and produce a STARK
   proof that the computation was performed correctly.
3. **Verifiers** check the proof against the program hash and public I/O. They
   never execute the program themselves.

Deployment, then, means making the compiled program available where it needs to
run: as transaction validation logic on a blockchain, as a verifiable
computation in an off-chain system, or as a library that other programs
reference by hash.

For background on the execution model, see [Programming Model](programming-model.md).

---

## Deployment to Neptune Cash

[Neptune Cash](https://neptune.cash/) is the reference blockchain built on
Triton VM. It is the primary deployment target for Trident programs today.

### The UTXO Model

Neptune uses a UTXO (Unspent Transaction Output) model rather than an
account model. Each UTXO represents a discrete piece of value, and it carries
two kinds of scripts:

- **Lock script** -- a Trident program (compiled to TASM) that guards the UTXO.
  To spend the UTXO, the spender must produce a valid STARK proof that the lock
  script executed successfully. This is ownership logic: hash-preimage locks,
  signature verification, multisig schemes, timelocks, or any custom condition.

- **Type script** -- a Trident program that validates the *type* of value stored
  in the UTXO (native currency, custom tokens, NFTs). Type scripts can
  authenticate both kernel fields and the actual coin data, enforcing supply
  invariants and transfer rules.

A UTXO stores only the **hash** of its lock script, not the script itself.
When spending, the prover supplies the full program as part of the witness
and proves it matches the stored hash.

### Transaction Structure

Every Neptune transaction has a TransactionKernel with 8 fields organized as a
Merkle tree. The kernel MAST hash is the public input for both lock scripts and
type scripts. Programs use the divine-and-authenticate pattern to access kernel
fields: divine the value from secret input, then authenticate it against the
MAST hash using Merkle proofs.

```
program simple_lock

use std.io.io
use std.crypto.hash
use std.crypto.auth

fn main() {
    // Read kernel MAST hash from public input
    let kernel_hash: Digest = io.read5()

    // Divine the secret preimage
    let preimage: Digest = io.divine5()

    // Verify: hash(preimage) matches expected lock hash
    std.crypto.auth.verify_preimage(preimage, kernel_hash)
}
```

### Deployment Flow for Neptune

1. Write your lock script or type script as a Trident program.
2. Compile it: `trident build lock.tri -o lock.tasm`
3. The compiled TASM is hashed (Tip5) to produce the `lock_script_hash`.
4. When creating a UTXO, embed the `lock_script_hash` in the UTXO data.
5. When spending, the prover executes the TASM program with the appropriate
   inputs and produces a STARK proof.
6. The network verifies the proof against the program hash and kernel hash.

The relevant standard library modules for Neptune deployment:

| Module | Purpose |
|--------|---------|
| `ext.triton.kernel` | Authenticate transaction kernel fields |
| `ext.triton.utxo` | UTXO verification primitives |
| `ext.triton.storage` | Persistent storage read/write |
| `std.crypto.auth` | Hash-lock authentication patterns |
| `std.crypto.merkle` | Merkle proof verification |

For the full Neptune programming model, see [Programming Model](programming-model.md).
For the blockchain developer mental model, see
[For Blockchain Devs](for-blockchain-devs.md).

---

## Multi-Target Deployment

Trident's three-layer architecture -- universal core, abstraction layer, backend
extensions -- is designed so the same source can compile to different zkVMs.

**Today**: Triton VM is the only supported backend. The `--target triton` flag
is the default and currently the only option that produces output.

**Planned**: Miden VM, Cairo VM (StarkWare), and SP1/RISC Zero backends are on
the roadmap. The architecture is in place but the backends are not yet
implemented.

### How Portability Works

Programs that use only `std.*` modules are fully portable. They contain no
target-specific instructions and will compile to any backend once it exists.

Programs that import `ext.*` modules (e.g., `ext.triton.kernel`) are bound to
that specific backend. They will only compile for the named target.

The `asm` block syntax enables target-specific code paths within otherwise
portable programs:

```
fn efficient_hash(a: Field, b: Field) -> Field {
    asm(triton, -1) {
        // Triton VM-specific: uses native hash instruction
        hash
        swap 5 pop 1
        swap 4 pop 1
        swap 3 pop 1
        swap 2 pop 1
        swap 1 pop 1
    }
}
```

The `asm(triton, -1)` block compiles only when targeting Triton VM. A future
Miden backend would skip this block entirely. To write a function that works
across targets, you would provide multiple `asm` blocks or use only `std.*`
functions that the abstraction layer maps to each backend's native instructions.

### The `--target` Flag

```bash
# Current (only supported target)
trident build program.tri --target triton -o program.tasm

# Future targets (architecture supports it, not yet implemented)
trident build program.tri --target miden -o program.masm
```

For the full multi-target design, see [Vision](vision.md) and
[Universal Design](universal-design.md).

---

## Project Configuration

### `trident.toml`

The project manifest declares the entry point and target configuration:

```toml
[project]
name = "my_lock_script"
version = "0.1.0"
entry = "main.tri"

[targets.triton]
backend = "triton"
```

### Directory Structure

A typical project ready for deployment:

```
my_project/
  trident.toml          # Project configuration
  main.tri              # Entry point (program declaration)
  lock.tri              # Lock script logic
  types.tri             # Type script logic
  helpers.tri           # Shared library module
  std/                  # Universal standard library (auto-discovered)
    core/
    io/
    crypto/
  ext/                  # Backend extensions
    triton/             #   Triton VM-specific modules
```

Build artifacts (`.tasm` files) are written to the location specified by the
`-o` flag, or default to the same directory as the source file with a `.tasm`
extension.

---

## Deployment Checklist

Before deploying a Trident program to production (whether as a Neptune lock
script, a standalone verifiable computation, or any other use case), follow
these steps in order:

### 1. Type-Check

```bash
trident check main.tri
```

Catches type errors, width mismatches, unresolved imports, and bounded-loop
violations without emitting any TASM. Fix all diagnostics before proceeding.

### 2. Test

```bash
trident test main.tri
```

Run all `#[test]` functions. Every code path that matters should be covered.
In ZK programs, a missed edge case does not just produce a wrong answer -- it
produces no proof at all (the VM crashes on assertion failure).

### 3. Analyze Proving Cost

```bash
trident build main.tri --costs
```

The `--costs` flag prints a breakdown across all six Triton VM tables
(Processor, Hash, U32, Op Stack, RAM, Jump Stack). The padded height -- the
next power of two of the tallest table -- determines actual STARK proving time
and memory. Understand this number before deploying.

```bash
trident build main.tri --hotspots    # Top cost contributors
trident build main.tri --annotate    # Per-line cost annotations
```

### 4. Optimize

```bash
trident build main.tri --hints
```

The compiler suggests concrete optimizations: redundant stack operations,
expensive patterns that have cheaper alternatives, opportunities to reduce
table heights. Apply what makes sense, then re-check costs.

```bash
trident build main.tri --costs --compare previous.json
```

Use `--save-costs` and `--compare` to track cost changes across iterations.
See the [Optimization Guide](optimization.md) for strategies.

### 5. Build the Final Artifact

```bash
trident build main.tri -o main.tasm
```

This is the artifact you deploy. The Tip5 hash of this TASM program is its
identity -- the `program_digest` that verifiers will check proofs against.

### 6. Integrate

- **Neptune Cash**: Embed the `lock_script_hash` (Tip5 hash of the compiled
  TASM) in the UTXO you create. Provide the full TASM as witness data when
  spending.
- **Standalone verification**: Feed the `.tasm` file to the Triton VM prover
  along with public and secret inputs. The prover produces a STARK proof.
  Distribute the Claim (program hash + public I/O) and Proof to verifiers.
- **Proof composition**: A Trident program can verify another program's proof
  internally, enabling recursive proof structures. The inner program's hash
  becomes part of the outer program's public input.

---

## Deployment Tooling

### Content-Addressed Code

Every Trident function is identified by its BLAKE3 content hash. This means
deployments are reproducible: the same source always compiles to the same
artifact with the same hash. The `trident hash` command computes a program's
content hash, and the `trident ucm` codebase manager tracks definitions by
hash. See [Content-Addressed Code](content-addressed.md) for details.

### Token Standards

Trident includes standard token implementations ready for deployment:

- **TSP-1**: Fungible token standard with conservation laws, mint authority,
  and burn support. See `examples/neptune/type_custom_token.tri`.
- **TSP-2**: Non-fungible token standard with unique IDs and metadata.
  See `examples/nft/nft.tri`.
- **Native currency**: Neptune's built-in currency type script.
  See `examples/neptune/type_native_currency.tri`.

### On-Chain Registry

The `ext.triton.registry` module provides an on-chain Merkle-tree registry
for content-addressed definitions. Programs can be registered, verified,
looked up, and updated via provable operations.

### Proof Composition

A Trident program can verify another program's proof internally using the
`ext.triton.proof` library. This enables recursive proof structures where
the inner program's hash becomes part of the outer program's public input.
See `examples/neptune/proof_aggregator.tri` for a transaction batching
example and `examples/neptune/transaction_validation.tri` for the full
Neptune transaction validation orchestrator.

### Not Yet Available

- **Web playground**: An in-browser Trident compiler (via WASM) for
  experimenting without installing the toolchain.
- **Browser extension integration**: A library for web applications to
  construct transactions and trigger proof generation.

---

## Next Step

With your program deployed, the next stage is proof generation -- executing the
program and producing the STARK proof that verifiers will check.

Continue to [Generating Proofs](generating-proofs.md).
