# 📦 Deploying a Program

This is the fourth stage of the Trident program lifecycle:
Writing -> Compiling -> Running -> Deploying -> Generating Proofs -> Verifying Proofs.

You have a compiled `.tasm` artifact. This guide covers what "deployment" means
for zero-knowledge programs and how to get your compiled Trident program into a
real system.

---

## 🛠️ CLI Commands

Trident provides two commands for the deployment pipeline:

- `trident package` — compile, hash, and produce a self-contained artifact
- `trident deploy` — package + publish to a registry server (or blockchain node)

### `trident deploy` — Deploy to a Server

The `deploy` command is the last mile. It compiles your program, packages
the artifact, and deploys it to a registry server:

```nu
# Compile + package + deploy to default registry (localhost:8090)
trident deploy lock.tri --target neptune

# Deploy to a specific registry
trident deploy my_project/ --registry http://prod-registry:8090

# Verify before deploying (runs symbolic verification)
trident deploy lock.tri --verify

# Deploy a pre-packaged artifact directly
trident deploy lock.deploy/

# Dry run — see what would happen without deploying
trident deploy lock.tri --dry-run
```

### `trident package` — Build an Artifact

The `package` command produces a `.deploy/` directory without deploying it
anywhere. Use this when you want to inspect the artifact, archive it, or
deploy it later:

```nu
# Compile + hash + produce artifact
trident package lock.tri --target neptune

# Package with verification
trident package lock.tri --verify

# Output to a custom directory
trident package lock.tri -o /artifacts/
```

### Artifact Format

Both commands produce the same `.deploy/` directory:

```
my_program.deploy/
  program.tasm        # Compiled TASM artifact
  manifest.json       # Metadata (see below)
```

The `manifest.json` contains everything needed for integration:

```json
{
  "name": "my_program",
  "version": "0.1.0",
  "program_digest": "a1b2c3...64hex",
  "source_hash": "d4e5f6...64hex",
  "target": {
    "vm": "triton",
    "os": "neptune",
    "architecture": "stack"
  },
  "cost": {
    "processor": 512,
    "hash": 128,
    "u32": 64,
    "padded_height": 1024
  },
  "functions": [
    { "name": "main", "hash": "abcdef...64hex", "signature": "fn main()" }
  ],
  "entry_point": "main",
  "built_at": "2026-02-11T12:00:00Z",
  "compiler_version": "0.1.0"
}
```

Key fields:
- `program_digest` — Poseidon2 hash of the compiled TASM. This is what
  verifiers check proofs against. Same source always produces the same digest.
- `source_hash` — BLAKE3 content hash of the source AST.
- `cost` — table heights for proving cost estimation.
- `functions` — per-function content hashes and signatures.

Both commands default to `--profile release` (unlike `build` which defaults to
`debug`), because deployment artifacts should be release-optimized.

---

## On-Chain Atlas Publishing

Atlas extends deployment beyond HTTP servers. Each OS
maintains a TSP-2 Card collection as its Atlas instance. Publishing a
program to Atlas mints a Card — the package name becomes the
`asset_id`, and the compiled artifact's content hash becomes the
`metadata_hash`.

### Publishing Workflow

```nu
# Deploy to Neptune's Atlas
trident deploy my_skill.tri --target neptune

# This:
# 1. Compiles my_skill.tri → .tasm artifact
# 2. Hashes the artifact (content-addressed)
# 3. Mints a Card in Neptune's Atlas collection
#    asset_id     = hash("my_skill")
#    metadata_hash = content_hash(artifact)
#    owner_id     = deployer's neuron identity
```

### Version Updates

Publishing a new version updates the Card's metadata:

```nu
# Update existing package with new version
trident deploy my_skill.tri --target neptune --update
# Executes TSP-2 Update operation (Op 2) on the existing Card
```

### Referencing Deployed Programs

Other programs reference deployed skills in two ways:

```trident
// By registry name (resolved at compile time via on-chain query)
use os.neptune.registry.my_skill

// By content hash (in PLUMB hook config — resolved at verification time)
// pay_hook = 0xabcd...1234
```

See the [Atlas Reference](../../reference/atlas.md)
for the full Atlas architecture.

---

## 📦 What "Deployment" Means for ZK Programs

A Trident program compiles to a `.tasm` file -- a sequence of Triton VM
instructions. That file is the deployable artifact. There is no on-chain
deployment transaction, no contract address, no ABI registry. Instead:

1. The program is identified by its Tip5 hash (the `program_digest` in the
   Claim structure). Anyone who has the same source compiles to the same hash.
2. Provers execute the program locally with their inputs and produce a STARK
   proof that the computation was performed correctly.
3. Verifiers check the proof against the program hash and public I/O. They
   never execute the program themselves.

Deployment, then, means making the compiled program available where it needs to
run: as transaction validation logic on a blockchain, as a verifiable
computation in an off-chain system, or as a library that other programs
reference by hash.

For background on the execution model, see [Programming Model](../explanation/programming-model.md).

---

## 🚀 Deployment to Neptune Cash

[Neptune Cash](https://neptune.cash/) is the reference blockchain built on
Triton VM. It is the primary deployment target for Trident programs today.

### The UTXO Model

Neptune uses a UTXO (Unspent Transaction Output) model rather than an
account model. Each UTXO represents a discrete piece of value, and it carries
two kinds of scripts:

- Lock script -- a Trident program (compiled to TASM) that guards the UTXO.
  To spend the UTXO, the spender must produce a valid STARK proof that the lock
  script executed successfully. This is ownership logic: hash-preimage locks,
  signature verification, multisig schemes, timelocks, or any custom condition.

- Type script -- a Trident program that validates the *type* of value stored
  in the UTXO (native currency, custom tokens, uniqs). Type scripts can
  authenticate both kernel fields and the actual coin data, enforcing supply
  invariants and transfer rules.

A UTXO stores only the hash of its lock script, not the script itself.
When spending, the prover supplies the full program as part of the witness
and proves it matches the stored hash.

### Transaction Structure

Every Neptune transaction has a TransactionKernel with 8 fields organized as a
Merkle tree. The kernel MAST hash is the public input for both lock scripts and
type scripts. Programs use the divine-and-authenticate pattern to access kernel
fields: divine the value from secret input, then authenticate it against the
MAST hash using Merkle proofs.

```trident
program simple_lock

use vm.io.io
use vm.crypto.hash
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
2. Deploy it: `trident deploy lock.tri --target neptune --registry <url>`
3. The `program_digest` from `manifest.json` is the `lock_script_hash`.
4. When creating a UTXO, embed the `lock_script_hash` in the UTXO data.
5. When spending, the prover executes the TASM program with the appropriate
   inputs and produces a STARK proof.
6. The network verifies the proof against the program hash and kernel hash.

Or step by step:
```nu
trident package lock.tri --target neptune   # produce artifact
trident deploy lock.deploy/                 # deploy pre-packaged artifact
```

The relevant standard library modules for Neptune deployment:

| Module | Purpose |
|--------|---------|
| `os.neptune.kernel` | Authenticate transaction kernel fields |
| `os.neptune.utxo` | UTXO verification primitives |
| `os.neptune.storage` | Persistent storage read/write |
| `std.crypto.auth` | Hash-lock authentication patterns |
| `std.crypto.merkle` | Merkle proof verification |

For the full Neptune programming model, see [Programming Model](../explanation/programming-model.md).
For the blockchain developer mental model, see
[For Onchain Devs](../explanation/for-onchain-devs.md).

---

## 🎯 Multi-Target Deployment

Trident's three-layer architecture -- universal core, abstraction layer, backend
extensions -- is designed so the same source can compile to different zkVMs.

Today: Triton VM is the only supported backend. The `--target triton` flag
is the default and currently the only option that produces output.

### How Portability Works

Programs that use only `std.*` modules are fully portable. They contain no
target-specific instructions and will compile to any backend once it exists.

Programs that import `os.<os>.*` modules (e.g., `os.neptune.kernel`) are bound to
that specific backend. They will only compile for the named target.

The `asm` block syntax enables target-specific code paths within otherwise
portable programs:

```trident
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

### The `--target` Flag

```nu
# Current (only supported target)
trident build program.tri --target triton -o program.tasm
```

For the full multi-target design, see [Vision](../explanation/vision.md) and
[Multi-Target Compilation](../explanation/multi-target.md).

---

## ⚙️ Project Configuration

See [Compiling a Program](compiling-a-program.md) for project configuration (`trident.toml`), directory structure, and build profiles.

---

## ✅ Deployment Checklist

Before deploying a Trident program to production (whether as a Neptune lock
script, a standalone verifiable computation, or any other use case), follow
these steps in order:

### 1. Compile, Test, and Optimize

Complete the build pipeline: `trident check`, `trident test`, `trident build --costs`. See [Compiling a Program](compiling-a-program.md) and [Optimization Guide](optimization.md).

### 2. Package the Artifact

```nu
trident package main.tri --target neptune --verify
```

This compiles, verifies, and produces the `.deploy/` directory. The
`program_digest` in `manifest.json` is the program's identity -- the hash
that verifiers check proofs against.

### 3. Deploy

```nu
# Deploy from source (packages automatically)
trident deploy main.tri --target neptune --registry http://prod:8090

# Or deploy the pre-packaged artifact
trident deploy main.deploy/ --registry http://prod:8090
```

### 4. Integrate

- Neptune Cash: Embed the `lock_script_hash` (Tip5 hash of the compiled
  TASM) in the UTXO you create. Provide the full TASM as witness data when
  spending.
- Standalone verification: Feed the `.tasm` file to the Triton VM prover
  along with public and secret inputs. The prover produces a STARK proof.
  Distribute the Claim (program hash + public I/O) and Proof to verifiers.
- Proof composition: A Trident program can verify another program's proof
  internally, enabling recursive proof structures. The inner program's hash
  becomes part of the outer program's public input.

---

## 🔧 Deployment Tooling

### Content-Addressed Code

Every Trident function is identified by its content hash. This means
deployments are reproducible: the same source always compiles to the same
artifact with the same hash. The `trident hash` command computes a program's
content hash, and the `trident store` codebase manager tracks definitions by
hash. See [Content-Addressed Code](../explanation/content-addressing.md) for details.

### Token Standards

Trident includes standard token implementations ready for deployment:

- TSP-1: Coin standard with conservation laws, mint authority,
  and burn support. See `os/neptune/types/custom_token.tri`.
- TSP-2: Uniq standard with unique IDs and metadata.
  See `os/neptune/standards/card.tri`.
- Native currency: Neptune's built-in currency type script.
  See `os/neptune/types/native_currency.tri`.

### Proof Composition

A Trident program can verify another program's proof internally using the
`os.neptune.proof` library. This enables recursive proof structures where
the inner program's hash becomes part of the outer program's public input.
See `os/neptune/programs/proof_aggregator.tri` for a transaction batching
example and `os/neptune/programs/transaction_validation.tri` for the full
Neptune transaction validation orchestrator.

---

## 🚀 Next Step

With your program deployed, the next stage is proof generation -- executing the
program and producing the STARK proof that verifiers will check.

Continue to [Generating Proofs](generating-proofs.md).
