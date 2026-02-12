# 🔱 Trident

*Write once. Prove anywhere.*

Trident is a minimal, security-first programming language for provable
computation. Today it compiles to [Triton VM](https://triton-vm.org/) —
the only quantum-safe, STARK-native virtual machine in production — powering
[Neptune Cash](https://neptune.cash/), the only blockchain that satisfies
all four requirements: quantum-safe, private, programmable, and mineable.

The compiler is designed for [multi-target compilation](docs/explanation/multi-target.md):
the same source will compile to Miden, Cairo, RISC-V zkVMs, EVM, WASM, and
Nock as those backends ship. Write once, prove anywhere.

```trident
program hello

fn main() {
    let a: Field = pub_read()
    let b: Field = pub_read()
    pub_write(a + b)
}
```

```bash
trident build hello.tri           # compile to TASM (Triton VM)
```

Feed it to the Triton VM prover and you get a
[STARK proof](docs/explanation/stark-proofs.md) that `a + b = sum` for
secret values of `a` and `b`. Quantum-safe. Zero-knowledge. No trusted
setup. No elliptic curves.

Read the [Vision](docs/explanation/vision.md) to understand why Trident
exists and what it's building toward.

---

## 🧬 Why Trident

Every other ZK system trades away at least one of: quantum safety, privacy,
programmability, or permissionless participation.
[Neptune is the only OS that passes all four tests](docs/explanation/vision.md#the-problem).

**What you see is what you prove.** Source compiles through a 54-operation
[intermediate representation](docs/reference/ir.md) that maps nearly 1:1 to
target instructions. No optimization engine reorders your operations.

**Bounded execution, predictable cost.** All loops require explicit bounds.
No recursion. No heap. The compiler computes exact proving cost from source —
before execution. See the [Optimization Guide](docs/guides/optimization.md).

**Hash performance dominance.** Triton VM executes
[Tip5](https://eprint.iacr.org/2023/107) in 1 clock cycle. SP1 needs ~3,000
cycles for SHA-256. RISC Zero needs ~1,000. For hash-heavy applications —
Merkle trees, content addressing, token transfers — this is decisive.
See the [Comparative Analysis](docs/explanation/provable-computing.md).

**Formal verification.** Annotate with `#[requires]` and `#[ensures]`, run
`trident verify`, get a proof of correctness for all inputs — or a concrete
counterexample. See [Formal Verification](docs/explanation/formal-verification.md).

**Content-addressed code.** Every function has a unique cryptographic
identity derived from its normalized AST. Audit certificates travel with
the code. See [Content-Addressed Code](docs/explanation/content-addressing.md).

**Multi-target by design.** The [universal compilation architecture](docs/explanation/multi-target.md)
compiles through TIR to 4 lowering paths: stack machines (Triton, Miden),
register machines (x86-64, ARM64, RISC-V), tree machines (Nock), and GPU
kernels (CUDA, Metal). 20 VM targets, 25 OS targets are
[specified](docs/reference/targets.md). Triton is production-ready today.

---

## 🏗️ What You Can Build Today

**[Coins](examples/coin/coin.tri)** — A 535-line ZK-native fungible token
(TSP-1) with 5 [PLUMB](docs/explanation/gold-standard.md) operations (Pay,
Lock, Update, Mint, Burn), time-locks, nullifiers, configurable authorities,
composable hooks, and 12 formally specified security properties.

**[Uniqs](examples/uniq/uniq.tri)** — Non-fungible tokens (TSP-2) with
per-asset metadata, royalties, creator immutability, and 5-bit flag-gated
operations. Same PLUMB framework, same proof architecture.

**Neptune transaction validation** — UTXO verification, supply conservation,
nullifier checking — expressible in Trident instead of hand-written TASM.

**Recursive STARK verification** — Verify a proof inside a proof. Neptune
Cash runs a recursive verifier in production today. ~300K clock cycles
regardless of the original computation's complexity.

See the [Gold Standard](docs/explanation/gold-standard.md) for the full
specification: PLUMB framework, TSP-1, TSP-2, and the capability library —
23 composable skills from Liquidity (TIDE) to Oracle Pricing (COMPASS).

---

## ⚡ Quick Start

```bash
cargo build --release            # build from source
trident build main.tri           # compile to TASM
trident check main.tri           # type-check without emitting
trident fmt main.tri             # format source
trident test main.tri            # run #[test] functions
trident verify main.tri          # formal verification
```

---

## 🗡️ Language at a Glance

### Types

| Type | Description |
|------|-------------|
| `Field` | Finite field element ([Goldilocks](https://xn--2-umb.com/22/goldilocks/): p = 2^64 - 2^32 + 1) |
| `U32` | Unsigned 32-bit integer (range-checked) |
| `Bool` | Boolean (0 or 1) |
| `Digest` | 5-element hash digest ([Tip5](https://eprint.iacr.org/2023/107)) |
| `XField` | Cubic extension field element |
| `[T; N]` | Fixed-size array |
| structs | Named product types |

Full specification: [Language Reference](docs/reference/language.md)

### Functions and Modules

```trident
// helpers.tri
module helpers

pub fn double(x: Field) -> Field {
    x + x
}
```

```trident
// main.tri
program my_app

use helpers

fn main() {
    let x: Field = pub_read()
    pub_write(helpers.double(x))
}
```

### Control Flow

```trident
if balance > 0 {
    transfer(balance)
} else {
    abort()
}

for i in 0..10 bounded 10 {       // all loops require explicit bounds
    process(i)
}

match op_code {
    0 => { pay() }
    1 => { lock() }
    _ => { reject() }
}
```

### Events

```trident
event Transfer { from: Digest, to: Digest, amount: Field }

fn pay() {
    reveal Transfer { from: sender, to: receiver, amount: value }
    seal Transfer { from: sender, to: receiver, amount: value }
}
```

`reveal` shows event fields to the verifier. `seal` hashes them — only the
digest is visible. Same event type, different privacy models.

### Inline Assembly

```trident
fn custom_op(a: Field, b: Field) -> Field {
    asm(triton, -1) {
        hash
        swap 5 pop 1 swap 4 pop 1
        swap 3 pop 1 swap 2 pop 1
        swap 1 pop 1
    }
}
```

Target-tagged `asm` blocks for direct VM access. The effect annotation
(`-1`) declares the net stack change.

---

## 🏛️ The Four-Tier Namespace

Programs use four tiers of imports, each adding capability and narrowing
portability:

| Tier | Prefix | Scope |
|------|--------|-------|
| VM-native | `vm.*` | Field types, conversions, assertions |
| Standard | `std.*` | I/O, memory, storage, crypto algorithms |
| OS portable | `os.*` | Identity, transfer, tokens, state, time |
| OS extensions | `os.<os>.*` | OS-specific: kernel, UTXO, XField |

Programs using only `vm.*` + `std.*` compile to any target. Adding `os.*`
gives portable OS interaction. Adding `os.<os>.*` binds to one OS.
See the [OS Reference](docs/reference/os.md) for full API and per-OS
lowering tables.

### VM Intrinsics (`vm/`)

| Module | Purpose |
|--------|---------|
| `vm.core.field` | Field arithmetic |
| `vm.core.u32` | U32 operations (log2, pow, popcount) |
| `vm.core.convert` | Type conversions (as_u32, as_field, split) |
| `vm.core.assert` | Assertions (is_true, eq, digest) |
| `vm.io.io` | Public I/O and witness input |
| `vm.io.mem` | RAM access (read, write, block operations) |
| `vm.crypto.hash` | Cryptographic hashing (Tip5, sponge) |
| `vm.crypto.merkle` | Merkle tree authentication |

### Standard Library (`std/`)

| Module | Purpose |
|--------|---------|
| `std.crypto.auth` | Authorization (preimage verification) |
| `std.crypto.sha256` | SHA-256 hash |
| `std.crypto.keccak256` | Keccak-256 hash |
| `std.crypto.ecdsa` | ECDSA signatures |
| `std.crypto.ed25519` | Ed25519 signatures |
| `std.crypto.poseidon` | Poseidon hash |
| `std.crypto.poseidon2` | Poseidon2 hash |
| `std.crypto.secp256k1` | secp256k1 curve operations |
| `std.crypto.bigint` | Big integer arithmetic |
| `std.io.storage` | Persistent storage |

### OS Extensions (`os/`)

| Module | Purpose |
|--------|---------|
| `os.neptune.xfield` | Extension field operations |
| `os.neptune.kernel` | Neptune kernel interface |
| `os.neptune.utxo` | UTXO verification |
| `os.neptune.proof` | Recursive STARK verification |
| `os.neptune.recursive` | Low-level recursive proof primitives |
| `os.neptune.registry` | On-chain definition registry |

---

## 🌳 Source Tree

```text
src/          Compiler in Rust            ~43K lines, 5 runtime dependencies
vm/           VM intrinsics in Trident    Compiler primitives (hash, I/O, field ops)
std/          Standard library in Trident Crypto algorithms (sha256, bigint, ecdsa)
os/           OS bindings in Trident      Per-OS config, docs, and extensions
examples/     Example programs            coin (TSP-1), uniq (TSP-2), and more
```

---

## 🎯 The Endgame

The compiler self-hosts on Triton VM: Trident compiles Trident, producing
a STARK proof that the compilation was correct. Provable compilation.

The source tree reflects this trajectory. `src/` is the Rust bootstrap
compiler — it shrinks as self-hosting progresses. `vm/`, `std/`, and `os/`
are Trident source — they grow. The intrinsic `.tri` files in `vm/` are
already the first pieces written in Trident itself.

Every `trident build` will produce a proof certificate alongside the
assembly — a cryptographic guarantee that the compilation was faithful.
No trusted compiler binary. No trusted build server. You don't trust —
you verify.

---

## 📚 Documentation

Organized following the [Diataxis](https://diataxis.fr/) framework.
Full index: [docs/README.md](docs/README.md)

### Tutorials (learning-oriented)

- [Tutorial](docs/tutorials/tutorial.md) — Build your first program step by step
- [For Developers](docs/explanation/for-developers.md) — Zero-knowledge from scratch
- [For Blockchain Devs](docs/explanation/for-blockchain-devs.md) — From Solidity/Anchor/CosmWasm

### How-to Guides (task-oriented)

- [Compiling a Program](docs/guides/compiling-a-program.md) — Build, check, cost analysis
- [Running a Program](docs/guides/running-a-program.md) — Execute, test, debug
- [Deploying a Program](docs/guides/deploying-a-program.md) — Neptune scripts, multi-target deployment
- [Generating Proofs](docs/guides/generating-proofs.md) — Execution trace to STARK proof
- [Verifying Proofs](docs/guides/verifying-proofs.md) — Proof checking, on-chain verification
- [Optimization Guide](docs/guides/optimization.md) — Cost reduction strategies

### Reference (information-oriented)

- [Language Reference](docs/reference/language.md) — Types, operators, builtins, grammar
- [Grammar (EBNF)](docs/reference/grammar.md) — Complete formal specification
- [Error Catalog](docs/reference/errors.md) — Every error message explained
- [IR Design](docs/reference/ir.md) — 54 TIR operations, 4 tiers, 4 lowering paths
- [Target Reference](docs/reference/targets.md) — OS model, target profiles, cost models
- [OS Reference](docs/reference/os.md) — Portable os.* API and per-OS lowering
- [CLI Reference](docs/reference/cli.md) — Command-line interface

### Explanation (understanding-oriented)

- [Vision](docs/explanation/vision.md) — Why Trident exists and why it matters
- [Multi-Target Compilation](docs/explanation/multi-target.md) — One source, every chain
- [Programming Model](docs/explanation/programming-model.md) — Execution model and stack semantics
- [How STARK Proofs Work](docs/explanation/stark-proofs.md) — From traces to quantum-safe proofs
- [Formal Verification](docs/explanation/formal-verification.md) — Symbolic execution and correctness proofs
- [Content-Addressed Code](docs/explanation/content-addressing.md) — Code identity by computation
- [OS Abstraction](docs/explanation/os-abstraction.md) — How Trident abstracts over 25 operating systems
- [Comparative Analysis](docs/explanation/provable-computing.md) — Trident vs every other ZK system
- [Gold Standard](docs/explanation/gold-standard.md) — Token standards (TSP-1, TSP-2) and capability library (PLUMB framework)

---

## 🧭 Design Principles

1. **Field elements all the way down.** The core numeric type is a finite field element.
2. **Bounded execution.** All loops require explicit bounds. No recursion. No halting problem.
3. **Compile-time everything.** All type widths, array sizes, and costs known statically.
4. **Constraints are features.** No heap, no dynamic dispatch, no callbacks — safety guarantees.
5. **Provable-first.** Designed for ZK. These constraints make great conventional programs too.
6. **Minimal dependencies.** 5 runtime crates: clap, ariadne, blake3, tower-lsp, tokio.

---

## ✏️ Editor Support

| Editor | Setup |
|--------|-------|
| [Zed](https://zed.dev/) | Extension in `editor/zed/` |
| [Helix](https://helix-editor.com/) | Config in `editor/helix/languages.toml` |
| Any LSP client | `trident lsp` — diagnostics, completions, hover, go-to-definition |

---

## ⚖️ License

[Cyber License](docs/explanation/cyber-license.md): Don't trust. Don't fear. Don't beg.
