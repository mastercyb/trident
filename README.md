# 🔱 Trident

## Write once. Prove anywhere

<p align="center">
  <img src="assets/trident-girl.gif" width="100%" alt="Trident" />
</p>

Trident is a programming language for provable virtual machines.

Provable VMs are a new kind of machine. They execute over finite fields,
not bytes. Every instruction is a polynomial constraint. Programs produce
cryptographic proofs, not return values. This is fundamentally different
from conventional computing — and it needs a language built for how the
machine actually works.

Today Trident compiles to [Triton VM](https://triton-vm.org/), powering
[Neptune Cash](https://neptune.cash/) — the only programmable, private,
mineable, quantum-safe blockchain that exists. The architecture supports
[multi-target compilation](docs/explanation/multi-target.md) — the same
source will compile to other proving backends as they ship.

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

## 🧬 Why a New Language

Provable VMs need a language designed for how they work. Four structural
facts drive every design decision in Trident:

**Arithmetic circuits are not programs.** The machine word is a field
element, not a byte. A language that treats `Field`, `Digest`, and
extension fields as first-class types generates native circuits. One that
wraps byte-oriented code in ZK proofs fights the machine at every step.

**Proofs compose, calls don't.** There is no `msg.sender` calling a
contract. Programs produce independent proofs that a verifier checks
together. Trident is designed for proof composition — not invocation.

**Bounded execution is a feature.** Circuits must terminate. Loops must
be bounded. This isn't a limitation — it's what makes proofs finite and
costs predictable. The compiler computes exact proving cost from source,
before execution.

**The field is the type system.** Goldilocks prime (2^64 - 2^32 + 1),
cubic extension fields, 5-element digests — these are the native machine
words. The same algebraic structure required for STARK proofs is also
optimal for [quantum computation](docs/explanation/quantum.md). One
design choice, two futures.

### What follows from these facts

What you see is what you prove. Source compiles through a 54-operation
[intermediate representation](docs/reference/ir.md) that maps nearly 1:1 to
target instructions. No optimization engine reorders your operations.

Hash performance dominance. Triton VM executes
[Tip5](https://eprint.iacr.org/2023/107) in 1 clock cycle. SP1 needs ~3,000
cycles for SHA-256. RISC Zero needs ~1,000. For hash-heavy applications —
Merkle trees, content addressing, token transfers — this is decisive.
See the [Comparative Analysis](docs/explanation/provable-computing.md).

Formal verification built in. Annotate with `#[requires]` and
`#[ensures]`, run `trident verify`, get a proof of correctness for all
inputs — or a concrete counterexample.
See [Formal Verification](docs/explanation/formal-verification.md).

Content-addressed code. Every function has a unique cryptographic
identity derived from its normalized AST. Audit certificates travel with
the code. See [Content-Addressed Code](docs/explanation/content-addressing.md).

---

## 🏗️ What You Can Build Today

[Coins](examples/coin/coin.tri) — A 535-line ZK-native fungible token
(TSP-1) with 5 [PLUMB](docs/explanation/gold-standard.md) operations (Pay,
Lock, Update, Mint, Burn), time-locks, nullifiers, configurable authorities,
composable hooks, and 12 formally specified security properties.

[Uniqs](examples/uniq/uniq.tri) — Non-fungible tokens (TSP-2) with
per-asset metadata, royalties, creator immutability, and 5-bit flag-gated
operations. Same PLUMB framework, same proof architecture.

Neptune transaction validation — UTXO verification, supply conservation,
nullifier checking — expressible in Trident instead of hand-written TASM.

Recursive STARK verification — Verify a proof inside a proof. Neptune
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
Full index: [docs/README.md](docs/README.md)

| Category | Start Here |
|----------|-----------|
| Tutorials | [The Builder's Journey](docs/tutorials/README.md) — six chapters, from hello-proof to a DAO |
| Guides | [Compiling a Program](docs/guides/compiling-a-program.md) — build, test, deploy, prove, verify |
| Reference | [Language Reference](docs/reference/language.md) — types, operators, builtins, grammar |
| Explanation | [Vision](docs/explanation/vision.md) — why Trident exists |

---

## 🧭 Design Principles

1. Field elements all the way down. The core numeric type is a finite field element.
2. Bounded execution. All loops require explicit bounds. No recursion. No halting problem.
3. Compile-time everything. All type widths, array sizes, and costs known statically.
4. Constraints are features. No heap, no dynamic dispatch, no callbacks — safety guarantees.
5. Provable-first. Designed for ZK. These constraints make great conventional programs too.
6. Minimal dependencies. 5 runtime crates: clap, ariadne, blake3, tower-lsp, tokio.

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
