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
trident package main.tri         # produce .deploy/ artifact
trident deploy main.tri          # package + deploy to registry
```

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
Full index: **[docs/README.md](docs/README.md)**

| Category | Start Here |
|----------|-----------|
| **Tutorials** | [The Builder's Journey](docs/tutorials/README.md) — six chapters, from hello-proof to a DAO |
| **Guides** | [Compiling a Program](docs/guides/compiling-a-program.md) — build, test, deploy, prove, verify |
| **Reference** | [Language Reference](docs/reference/language.md) — types, operators, builtins, grammar |
| **Explanation** | [Vision](docs/explanation/vision.md) — why Trident exists |

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
