# 🔭 Trident: Provable Programs for Everyone

*Write once. Prove anywhere.*

---

## 🔭 The Problem

Today, if you want to build a provable program on the only quantum-safe,
private, programmable, mineable OS that exists --
[Neptune Cash](https://neptune.cash/) running on
[Triton VM](https://triton-vm.org/) -- you write raw assembly. TASM. ~45
instructions. No types. No modules. No error messages beyond "assertion
failed." You manage a 16-element operand stack by hand, track six algebraic
tables in your head, and debug by staring at execution traces.

Neptune Cash did this. Their team wrote a working recursive STARK verifier,
a full transaction validation engine, and a Proof-of-Work blockchain -- all in
raw TASM. It took years. It produced an inflation bug. It works, but it
doesn't scale to a world where more than three people need to write provable
programs.

The alternative ecosystems each sacrifice at least one essential property:

| OS | VM | Quantum-Safe | Private | Programmable | Mineable |
|----|-----|:---:|:---:|:---:|:---:|
| StarkNet | Cairo/Stwo | Yes | Yes | Yes | **No** (PoS L2) |
| Boundless | SP1 | **No** (Groth16 wrap) | No | Yes | **No** |
| RISC Zero | RISC Zero | **No** (Groth16 wrap) | Yes | Yes | **No** |
| Aleo | AVM | **No** (Pasta curves) | Yes | Yes | **Partial** (stake-gated) |
| Mina | Kimchi | **No** (Pasta curves) | Partial | **Partial** | **No** |
| Nockchain | Nock | **No** (no STARK hash) | **No** | **Partial** (Jock alpha) | Yes |
| **Neptune** | **Triton** | **Yes** | **Yes** | **Yes** | **Yes** |

Neptune is the only OS that passes all four tests today. Trident is
the language that makes it accessible.

---

## 🧬 What Trident Is

Trident is a minimal, security-first language for provable computation.
Source code compiles through a [54-operation IR](../reference/ir.md) that
lowers to any target VM through one of four backend paths. The default
target is Triton VM, with the architecture ready for Miden, Cairo, RISC-V,
EVM, Nock, and native backends.

The design constraints are deliberate:

- **Bounded loops.** Every loop has a compile-time bound. No infinite execution.
- **No heap, no recursion, no dynamic dispatch.** All data has known size.
- **Fixed-width types.** `Field`, `U32`, `Bool`, `Digest`, fixed arrays, structs.
- **Cost transparency.** Proving cost computable from source before execution.

These constraints make every program a fixed, bounded computation -- exactly
what a STARK prover requires. See [For Offchain Devs](for-offchain-devs.md) for
the full language model.

The compiler is ~43K lines of Rust with 5 runtime dependencies. 25 standard
library modules. 743 tests. 20 VM targets. 25 OS targets. 53 `.tri` files.

For architecture details, see [Multi-Target Compilation](multi-target.md).
For hash performance and quantum safety comparisons, see
[Comparative Analysis](provable-computing.md).

---

## 🔮 The Vision

**Sovereign, private, provable computation should be accessible to every
developer -- and it should not be locked to a single virtual machine.**

The bet is fourfold:

**Quantum computers will break elliptic curves within our professional
lifetimes.** Every SNARK system in production has an expiration date.
Infrastructure built on hash-only STARKs does not need to migrate.

**Privacy is a requirement, not a feature.** When you build tokens, identity,
governance, or any system touching human agency, the default must be private.

**Developer experience determines adoption.** Triton VM is the right
foundation. Raw TASM is the wrong interface. Cairo proved this for StarkWare.
Trident proves this for the only OS that gets all four properties right.

**No program should be stranded on one VM.** The universal core compiles to
any target. Backend extensions add power without limiting portability.
Choosing Trident is not choosing a single ecosystem -- it is choosing all
of them.

### The Strategic Position

```text
             Expressiveness
                  |
     Rust/C++  *  |
                  |     Cairo *
                  |
                  |          Trident *
                  |
        Circom *  |     Noir *
                  |
                  +---------------------->  Provability
```

Not the most expressive language. Not the most minimal circuit DSL. The
sweet spot for provable programs that need to be portable, auditable, and
cost-transparent. Every trend -- more zkVMs, ZK expanding beyond crypto,
regulatory pressure for auditable code, cross-chain interoperability --
makes this position stronger.

---

## 🔗 See Also

- [For Offchain Devs](for-offchain-devs.md) -- Zero-knowledge programming from scratch
- [For Onchain Devs](for-onchain-devs.md) -- From Solidity/Cairo/Anchor to Trident
- [Multi-Target Compilation](multi-target.md) -- One source, every chain
- [Comparative Analysis](provable-computing.md) -- Triton VM vs every other ZK system
- [How STARK Proofs Work](stark-proofs.md) -- From execution traces to quantum-safe proofs
- [Gold Standard](gold-standard.md) -- Token standards (TSP-1, TSP-2) and capability library
- [Language Reference](../reference/language.md) -- Types, operators, builtins, grammar
- [IR Reference](../reference/ir.md) -- 54 TIR operations, 4 lowering paths
