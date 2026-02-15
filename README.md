# Trident

## One Field. Three Revolutions.

<p align="center">
  <img src="media/tri.gif" width="100%" alt="Trident" />
</p>

Trident is a programming language for provable, intelligent, quantum-native
computation. Every variable, every operation, every function compiles to
arithmetic over the Goldilocks prime field (p = 2^64 - 2^32 + 1). Programs
produce STARK proofs — hash-based, post-quantum secure, no trusted setup.

Three computational revolutions — quantum computing, privacy, and
artificial intelligence — share a common algebraic foundation in prime
field arithmetic. Trident sits at their intersection because its native
`Field` type simultaneously satisfies the requirements of all three.
The convergence is a
[mathematical inevitability](docs/explanation/quantum.md).

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

## The Thesis

Three revolutions are advancing in isolation. They share a common root.

**Quantum** — requires unitary operations on state spaces with no
decoherence channels. When the dimension is prime, the Hilbert space has no
invariant subspaces. Every gate touches the full state space. A single
prime-dimensional qudit replaces 64 entangled qubits — four orders of
magnitude gate count reduction.

**Privacy** — requires reversible computation with complete arithmetic.
Zero-knowledge proofs (STARKs), fully homomorphic encryption (TFHE), and
multi-party computation (Shamir sharing) all demand a field where every
nonzero element has a multiplicative inverse and no information is destroyed.
All three operate natively over the same prime field.

**AI** — requires nonlinear functions over fixed-width arithmetic for
provable inference. Neural networks expressed in field arithmetic produce
STARK proofs alongside their outputs. Weights, activations, and gradients
are field elements from the start — no float-to-field quantization.

The cyclic group Z/pZ is the shared algebraic skeleton. Classically, it
defines the additive group of the field. Quantum mechanically, it defines
the computational basis of a prime-dimensional qudit. In neural networks,
it defines the native arithmetic of provable inference. In cryptography,
it defines the domain of ZK proofs, FHE ciphertexts, and MPC secret shares.
Same structure. Three readings.

> Reversible computation with complete arithmetic lives in prime fields.
> Both classical provability and quantum mechanics require reversible
> computation with complete arithmetic. Therefore both require prime fields.
> Trident makes prime field elements its fundamental primitive. The
> convergence is structural.

See [Quantum Computing](docs/explanation/quantum.md) for the full proof.

---

## The Rosetta Stone

The deepest structural insight: a single lookup table over the Goldilocks
field simultaneously functions as four different mechanisms.

| Reading | Role | What it provides |
|---------|------|------------------|
| Cryptographic S-box | Hash nonlinearity | Security |
| Neural activation | Network expressiveness | Intelligence |
| FHE bootstrap | Encrypted evaluation | Privacy |
| STARK lookup | Proof authentication | Verifiability |

One table. One field. Four purposes. A mathematical identity. When all systems operate over the same prime field,
four separate mechanisms collapse into one data structure read four ways.

A program that performs neural network inference on FHE-encrypted data with
a STARK correctness proof uses the same ReLU table for the activation
function, the FHE bootstrapping, and the STARK authentication. Three roles
served by a single array of field elements.

---

## Why a New Language

Provable VMs need a language designed for how they work. Four structural
facts drive every design decision:

**Arithmetic circuits are not programs.** The machine word is a field
element, not a byte. A language that treats `Field`, `Digest`, and
extension fields as first-class types generates native circuits. One that
wraps byte-oriented code in ZK proofs fights the machine at every step.

**Proofs compose, calls don't.** There is no `msg.sender` calling a
contract. Programs produce independent proofs that a verifier checks
together. Composition is recursive — a proof can verify another proof
inside it, so any chain of proofs collapses into a single proof. Trident
is designed for recursive proof composition — not invocation.

**Bounded execution is a feature.** Circuits must terminate. Loops must
be bounded. The compiler computes exact proving cost from source,
before execution. The same bound that makes programs provable makes them
quantum-native: bounded loops map directly to fixed-depth quantum circuits.

**The field is the type system.** Goldilocks prime, cubic extension fields,
5-element digests — these are the native machine words. The same algebraic
structure required for STARK proofs is optimal for
[quantum computation](docs/explanation/quantum.md),
[private computation](docs/explanation/vision.md), and neural network
inference. One design choice, three futures.

### What follows from these facts

What you see is what you prove. Source compiles through a 54-operation
[intermediate representation](docs/reference/ir.md) that maps nearly 1:1 to
target instructions.

Hash performance dominance. Triton VM executes
[Tip5](https://eprint.iacr.org/2023/107) in 1 clock cycle. SP1 needs ~3,000
cycles for SHA-256. RISC Zero needs ~1,000. For hash-heavy applications —
Merkle trees, content addressing, token transfers — this is decisive.
See [Comparative Analysis](docs/explanation/provable-computing.md).

Formal verification built in. Annotate with `#[requires]` and
`#[ensures]`, run `trident verify`, get a proof of correctness for all
inputs — or a concrete counterexample.
See [Formal Verification](docs/explanation/formal-verification.md).

Content-addressed code. Every function has a unique cryptographic
identity derived from its normalized AST. Audit certificates travel with
the code. See [Content-Addressed Code](docs/explanation/content-addressing.md).

---

## Apps

Production programs that compile to TASM with `trident build` today.

[Coin](os/neptune/standards/coin.tri) — Fungible token (TSP-1).
Five operations (Pay, Lock, Update, Mint, Burn), time-locks, nullifiers,
configurable authorities, composable hooks.

[Card](os/neptune/standards/card.tri) — Non-fungible token (TSP-2).
Per-asset metadata, royalties, creator immutability, flag-gated
operations. Same PLUMB framework as Coin.

[Lock scripts](os/neptune/locks/) — Generation, symmetric, timelock,
multisig spending authorization.

[Type scripts](os/neptune/types/) — Native currency and custom token
conservation laws.

[Programs](os/neptune/programs/) — Transaction validation, recursive
verification, proof aggregation and relay.

See the [Gold Standard](docs/explanation/gold-standard.md) for the full
PLUMB specification and the [Skill Library](docs/explanation/skill-library.md)
for designed token capabilities.

---

## Quick Start

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

## Source Tree

```text
src/          Compiler in Rust            ~36K lines, 5 runtime dependencies
vm/           VM intrinsics in Trident    Compiler primitives (hash, I/O, field ops)
std/          Standard library in Trident Crypto algorithms (sha256, bigint, ecdsa)
os/           OS bindings in Trident      Per-OS config, programs, and extensions
```

The four-tier namespace:

```
vm.*              Compiler intrinsics       TIR ops (hash, sponge, pub_read, assert)
std.*             Real libraries            Implemented in Trident (sha256, bigint, ecdsa)
os.*              Portable runtime          os.signal, os.neuron, os.state, os.time
os.<os>.*         OS-specific APIs          os.neptune.xfield, os.solana.pda
```

---

## The Endgame

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

When quantum hardware matures, the same programs that produce classical
STARK proofs will have their proof generation quantum-accelerated — with
zero source code changes. The field structure is preserved end-to-end from
source code to quantum execution.

---

## Standard Library Vision

The `std.*` architecture reflects the three-pillar thesis:

**Foundation** — `std.field` (Goldilocks arithmetic, NTT, extensions),
`std.crypto` (Poseidon2, Tip5, signatures, FRI), `std.math` (exact field
arithmetic, linear algebra), `std.data` (Merkle trees, tensors,
authenticated structures), `std.io` (witness injection, storage).

**Three Pillars** — `std.quantum` (state management, gates, Grover's, QFT,
VQE, error correction), `std.private` (ZK + FHE + MPC: credentials,
auctions, voting, compliance-compatible privacy), `std.nn` (field-native
neural networks: matrix multiply, attention, convolutions, lookup-table
activations).

**Intersections** — `std.nn_quantum` (hybrid quantum-classical networks,
variational circuits), `std.nn_private` (private inference on encrypted
data), `std.quantum_priv` (quantum-secure MPC, threshold schemes).

**Applications** — `std.agent` (autonomous agents with proofs), `std.defi`
(financial instruments), `std.science` (verifiable computation for research).

---

## Documentation

Organized following the [Diataxis](https://diataxis.fr/) framework.
Full index: [docs/README.md](docs/README.md)

| Category | Start Here |
|----------|-----------|
| Tutorials | [The Builder's Journey](docs/tutorials/README.md) — six chapters, from hello-proof to a DAO |
| Guides | [Compiling a Program](docs/guides/compiling-a-program.md) — build, test, deploy, prove, verify |
| Reference | [Language Reference](docs/reference/language.md) — types, operators, builtins, grammar |
| Explanation | [Vision](docs/explanation/vision.md) — why Trident exists and what it's building toward |

---

## Design Principles

1. Field elements all the way down. The core numeric type is a finite field element.
2. Bounded execution. All loops require explicit bounds. No recursion. No halting problem.
3. Compile-time everything. All type widths, array sizes, and costs known statically.
4. Constraints are features. No heap, no dynamic dispatch, no callbacks — safety guarantees.
5. Provable-first. Designed for ZK. These constraints make great conventional programs too.
6. Field-native intelligence. Neural networks in field arithmetic, not floats.
7. Quantum-native by construction. The same field structure optimizes for quantum execution.
8. Minimal dependencies. 5 runtime crates: clap, ariadne, blake3, tower-lsp, tokio.

---

## Editor Support

| Editor | Setup |
|--------|-------|
| [Zed](https://zed.dev/) | Extension in `editor/zed/` |
| [Helix](https://helix-editor.com/) | Config in `editor/helix/languages.toml` |
| Any LSP client | `trident lsp` — diagnostics, completions, hover, go-to-definition |

---

## License

[Cyber License](docs/explanation/cyber-license.md): Don't trust. Don't fear. Don't beg.
