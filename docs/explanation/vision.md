# Trident: Provable Programs for Everyone

*The language that makes zero-knowledge programming practical.*

---

## The Problem

Zero-knowledge proofs are the most important cryptographic primitive of this
century. They let you prove that a computation happened correctly without
revealing the inputs. Private transactions. Verifiable computation. Trustless
bridges. Sovereign chains. The applications are extraordinary.

Writing them is hell.

Today, if you want to build a provable program on the only quantum-safe,
private, programmable, mineable OS that exists --
[Neptune Cash](https://neptune.cash/) running on
[Triton VM](https://triton-vm.org/) -- you write raw assembly. TASM. ~45
instructions. No types. No modules. No error messages beyond "assertion
failed." You manage a 16-element operand stack by hand, track six algebraic
tables in your head, and debug by staring at execution traces until the
numbers stop swimming.

Neptune Cash did this. Their team wrote a working recursive STARK verifier,
a full transaction validation engine, and a Proof-of-Work blockchain -- all in
raw TASM. It took years. It produced an inflation bug. It works, but it
doesn't scale to a world where more than three people need to write provable
programs.

The alternative ecosystems are not alternatives at all:

- **Cairo/StarkWare** is production-grade but a walled garden. One company
  controls the language, compiler, prover, and the L2 it runs on. No
  Proof-of-Work path -- StarkNet is PoS only.

- **SP1 and RISC Zero** let you write Rust, then wrap the final proof in
  **Groth16** -- BN254 elliptic curve pairings for cheap Ethereum
  verification. The prover is STARK, but the verifier is not. One quantum
  computer breaks every proof ever generated.

- **Aleo** is privacy-native but SNARK-based on Pasta curves, requires a
  trusted setup ceremony, and its PoSW hybrid stake-gates validators at
  1M+ ALEO.

- **Mina** proved you can compress a blockchain to 22 KB using recursive
  proofs. But Kimchi uses Pasta curves (not quantum-safe), zkApps get 8
  fields of on-chain state, and the general-purpose VM is still roadmapped.

- **Nockchain** has the right instincts: UTXO model, Proof-of-Work, no
  foundation gatekeeping. But the Nock VM is not quantum-resistant (no
  STARK-native hash, no algebraic field arithmetic) and has no privacy
  primitives (planned H2 2026). [Jock](https://docs.jock.org/) (alpha,
  June 2025) compiles to Nock with Swift-inspired syntax but lacks a type
  system and many standard features. Smart contract programmability
  (optimistic execution, fraud proofs) is roadmapped for Q2 2026.

Every one of these systems makes a compromise. Either you give up quantum
safety, or privacy, or programmability, or permissionless participation.

Four properties. Every existing system sacrifices at least one.

These are OS-level properties -- the VM provides the cryptographic
foundation, but privacy, programmability, and mineability come from the
OS (the chain, consensus, and economics built on top):

| OS | VM | Quantum-Safe | Private | Programmable | Mineable |
|----|-----|:---:|:---:|:---:|:---:|
| StarkNet | Cairo/Stwo | Yes | Yes | Yes | **No** (PoS L2) |
| Boundless | SP1 | **No** (Groth16 wrap) | No | Yes | **No** |
| Succinct | SP1 | **No** (Groth16 wrap) | No | Yes | **No** |
| RISC Zero | RISC Zero | **No** (Groth16 wrap) | Yes | Yes | **No** |
| Aleo | AVM | **No** (Pasta curves) | Yes | Yes | **Partial** (stake-gated) |
| Mina | Kimchi | **No** (Pasta curves) | Partial | **Partial** | **No** |
| Nockchain | Nock | **No** (no STARK hash) | **No** | **Partial** (Jock alpha) | Yes |
| **Neptune** | **Triton** | **Yes** | **Yes** | **Yes** | **Yes** |

Neptune is the only OS that passes all four tests today. Trident is
the language that makes it accessible.

Developers should not need a PhD in algebraic geometry to write provable
programs. They should not need to pick which security property to sacrifice.
They should not need to write assembly.

That is why Trident exists.

---

## The Solution

Trident is a minimal, security-first language with a universal compilation
architecture. Source code compiles through a 54-operation intermediate
representation ([TIR](../reference/ir.md)) that lowers to any target VM
through one of four backend paths. The default target is
[Triton VM](https://triton-vm.org/) (`--target triton`), with the
architecture ready for Miden, Cairo, RISC-V, EVM, Nock, and native backends.

```trident
program hello

fn main() {
    let a: Field = pub_read()
    let b: Field = pub_read()
    let sum: Field = a + b
    pub_write(sum)
}
```

That is a complete provable program. Build it with
`trident build hello.tri --target triton`, feed it to the Triton VM prover,
and you get a [STARK](stark-proofs.md) proof that `a + b = sum` for secret
values of `a` and `b`. Quantum-safe. Zero-knowledge. No trusted setup. No
elliptic curves. No vendor lock-in.

Here is what the language gives you:

**Types that match the VM.** `Field` (64-bit
[Goldilocks](https://xn--2-umb.com/22/goldilocks/) elements), `XField`
(extension field triples), `Bool`, `U32` (range-checked), `Digest`
(5-element [Tip5](https://eprint.iacr.org/2023/107) hashes), fixed-size
arrays, tuples, and structs. Every type has a known width in stack elements.
No heap. No dynamic allocation.

**Bounded loops only.** Every `for` loop requires an explicit bound. No
unbounded iteration. No recursion. If the compiler accepts your program,
the proving cost is computable before you ever run the prover.

**Modules and imports.** Split your program across files. 26 modules across
`vm/`, `std/`, and `os/` wrap every target primitive -- hashing, Merkle
proofs, sponge operations, field arithmetic, memory, I/O, and cryptographic
algorithms (SHA-256, Keccak, Poseidon2, ECDSA, Ed25519, secp256k1, bigint)
-- so you never write raw assembly unless you choose to.

**Events, open and sealed.** `reveal` writes event fields to public output.
`seal` hashes them and writes only the digest -- the verifier sees that
*something* happened, but not what. This is how you build private transfers.

**Proof composition.** Language-level support for verifying a proof inside a
proof. Recursive verification -- Triton VM's architectural sweet spot -- is
a first-class operation.

**Inline assembly with target tags.** Drop into target-specific assembly
when you need the VM's raw power:

```trident
fn custom_hash(a: Field, b: Field) -> Field {
    asm(triton) -1 {
        hash
        swap 5 pop 1
        swap 4 pop 1
        swap 3 pop 1
        swap 2 pop 1
        swap 1 pop 1
    }
}
```

The target tag `(triton)` binds the block to a specific backend. The `-1`
declares the net stack change. You get full control when you need it,
structured code when you don't.

**Const generic expressions.** Array sizes computed from generic parameters:
`fn concat<M, N>() -> [Field; M + N]` enables compile-time array operations
with zero runtime cost.

**Formal verification (designed).** The `#[pure]` annotation marks functions
for automated verification. Because Trident programs are bounded,
first-order, and heap-free, the compiler can symbolically verify correctness
for all possible inputs. See [Formal Verification](formal-verification.md).

**Content-addressed code (designed).** Every function is designed to compile
to a deterministic constraint system with a unique cryptographic identity. Audit
certificates and verification results are portable across projects and time.
See [Content-Addressed Code](content-addressing.md).

### The Four-Tier Namespace

Programs use four tiers of imports, each adding capability and narrowing
portability:

| Tier | Prefix | Scope | Example |
|------|--------|-------|---------|
| VM-native | `vm.*` | Field types, conversions, assertions | `vm.core.field`, `vm.core.u32` |
| Standard | `std.*` | I/O, memory, storage, crypto | `std.crypto.merkle`, `std.io.storage` |
| OS portable | `os.*` | Identity, transfer, tokens, state, time | `os.neuron`, `os.token` |
| OS extensions | `os.<os>.*` | OS-specific: kernel, UTXO, XField | `os.neptune.xfield`, `os.neptune.kernel` |

Programs using only `vm.*` + `std.*` compile to **any** target. Adding
`os.*` gives portable OS interaction -- the compiler maps each call to the
target OS's native mechanism. Adding `os.<os>.*` binds to one OS but
unlocks its full power.

### The Portable OS API

The `os.*` tier provides five modules that abstract over every supported OS:

| Module | Purpose | Key Functions |
|--------|---------|---------------|
| `os.neuron` | Identity and authorization | `id()`, `auth()`, `verify()` |
| `os.signal` | Value transfer | `send()`, `receive()` |
| `os.token` | Token operations (PLUMB) | `pay()`, `lock()`, `update()`, `mint()`, `burn()` |
| `os.state` | Persistent storage | `read()`, `write()`, `root()` |
| `os.time` | Clock and timestamps | `now()`, `step()` |

The compiler lowers each `os.*` call to the target OS's native mechanism.
`os.neuron.id()` becomes a UTXO lookup on Neptune, an `msg.sender` on
Ethereum, a `tx.origin` on Solana. Same source, different chain. See the
[OS Reference](../reference/os.md) for full API and per-OS lowering tables.

### Scale

20 VM targets. 25 OS targets. 54 TIR operations across 4 tiers. 4 lowering
paths. 26 standard library modules. 53 `.tri` files. 743 tests.

---

## Architecture

### Universal Compilation: One Source, Every Target

Trident's compiler is built on a 3-layer universal design:

```text
+-------------------------------------------+
|         Trident Universal Core            |
|   (types, control flow, modules, field    |
|    arithmetic, I/O, cost transparency)    |
+-------------------------------------------+
|         Abstraction Layer                 |
|   (hash, memory, stack/register mgmt,    |
|    Merkle ops, cost model, events)        |
+------+------+------+------+--------------+
|Triton| Miden| Cairo| SP1  | Nock | Native|
|      |      |      | /RZ  |      | /GPU  |
| +ext | +ext | +ext | +ext | +ext | +ext  |
+------+------+------+------+------+-------+
```

The **universal core** (~56% of the language surface) compiles identically to
every target: types, field arithmetic, bounded loops, modules, functions. The
**abstraction layer** (~22%) provides the same syntax with per-target
dispatch: I/O, hashing, Merkle operations, memory, events. **Backend
extensions** (~22%) expose target-specific capabilities through `os/<target>/`.

### Compilation Pipeline

```text
Source (.tri)
  │
  ▼
Lexer → Parser → AST
  │
  ▼
TypeChecker
  │
  ▼
TIRBuilder → Vec<TIROp>              ← 54 ops, target-independent
  │
  ├─→ StackLow       → assembly text  ← stack targets (Triton, Miden)
  │
  ├─→ LIR → RegLow   → machine code   ← register targets (x86-64, ARM64, RISC-V)
  │
  ├─→ TreeLow        → Noun → bytes   ← tree targets (Nock)
  │
  └─→ KIR → KernelLow → kernel source  ← GPU targets (CUDA, Metal, Vulkan)
        │
        ▼
      Linker                           ← multi-module resolution
```

The TIR is a thin, transparent layer -- 54 operations organized in 4 tiers
that map nearly 1:1 to target instructions on stack machines. No optimization
pass reorders your operations. For stack-machine targets (Triton, Miden),
`StackLow` emits assembly directly from TIR. Register-machine targets use
LIR as a second IR. Tree targets (Nock) use TreeLow to emit combinator
nouns. GPU targets use KIR for kernel generation.

The `--target` flag selects the backend (default: `triton`):

```bash
trident build main.tri                     # default: --target triton
trident build main.tri --target triton
trident build main.tri --target miden      # when backend ships
```

**What you see is what you prove.** The TIR preserves program structure.
When you see `hash(a, b, c, d, e, f, g, h, i, j)` in source, the `hash`
instruction fires in the VM -- 1 clock cycle, 6 hash table rows, 5 elements
out. No optimization engine introduces instructions you did not write.

See [IR Reference](../reference/ir.md) for the full 54-operation catalog.

### Standard Library: Layered for Portability

```trident
vm/
├── core/               VM-native — zero target dependencies
│   ├── field.tri         Goldilocks field arithmetic
│   ├── u32.tri           U32 operations
│   ├── convert.tri       Type conversions (as_u32, as_field)
│   └── assert.tri        Assertions (is_true, eq, digest)
├── crypto/             VM-native — hash primitives
│   ├── hash.tri          hash(), sponge_init/absorb/squeeze
│   └── merkle.tri        Merkle tree verification
└── io/                 VM-native — channels and memory
    ├── io.tri            pub_read, pub_write, divine
    └── mem.tri           ram_read, ram_write, block operations

std/
├── crypto/             Abstraction layer — per-target dispatch
│   ├── auth.tri          Hash-preimage authorization
│   ├── merkle.tri        Merkle tree operations
│   ├── bigint.tri        Arbitrary-precision arithmetic
│   ├── ecdsa.tri         ECDSA signatures
│   ├── ed25519.tri       Ed25519 signatures
│   ├── keccak256.tri     Keccak-256 hashing
│   ├── poseidon.tri      Poseidon hashing
│   ├── poseidon2.tri     Poseidon2 hashing
│   ├── secp256k1.tri     secp256k1 curve operations
│   └── sha256.tri        SHA-256 hashing
├── io/
│   └── storage.tri       Persistent storage
└── target.tri            Target-specific constants

os/
└── neptune/            Backend extensions — Neptune / Triton VM specific
    ├── xfield.tri        XField type, xx_add, xx_mul, x_invert
    ├── kernel.tri        Neptune kernel interface
    ├── utxo.tri          UTXO verification
    ├── proof.tri         Recursive STARK verifier components
    ├── recursive.tri     Recursive proof composition
    └── registry.tri      Registry operations
```

Every function in the standard library has a known, fixed cost in table rows.
When you call `std.crypto.merkle.verify(root, leaf, index, depth)`, the
compiler knows exactly how many Hash rows, Processor rows, and RAM rows that
verification adds to your trace.

### Cost Transparency

In most ZK systems, proving cost is a runtime discovery. Trident takes a
different approach: **every program's proving cost is computable from the
source code alone, before execution.**

This is possible because all loops are bounded, all types have fixed widths,
and every instruction's contribution to all six Triton VM tables is known.
The compiler sums these across all paths and reports the maximum table height
padded to the next power of two -- which determines proving time.

```bash
$ trident build main.tri --costs
```

The `--costs` flag reports per-function cost breakdown across all six tables.
The `--hotspots` flag ranks functions by cost contribution. The `--hints`
flag gives actionable optimization suggestions. See the
[Optimization Guide](../guides/optimization.md) for details.

### Hash Performance: The Categorical Advantage

[Triton VM](https://triton-vm.org/) was purpose-built for zero-knowledge
computation. Its hash coprocessor executes
[Tip5](https://eprint.iacr.org/2023/107) in 1 clock cycle + 6 coprocessor
rows. This is not an incremental improvement over general-purpose VMs. It is
a categorical difference:

| System | Hash function | Cost per hash | Relative cost |
|--------|--------------|---------------|:---:|
| **Triton VM** | Tip5 (native) | 1 cc + 6 hash rows | **1x** |
| StarkWare | Poseidon (native) | ~5-10 cc | ~5-10x |
| SP1 | SHA-256 (software) | ~3,000+ cc | ~3,000x |
| RISC Zero | SHA-256 (accelerated) | ~1,000 cc | ~1,000x |

For a Merkle tree of 2^20 nodes (about 1 million): Triton VM finishes in
roughly 1 million cycles. SP1 needs 3 billion. RISC Zero needs 1 billion.
For hash-heavy applications -- content addressing, Merkle authentication,
sponge accumulation -- this dominance is decisive.

The compiler is ~43K lines of Rust with 5 runtime dependencies. Small
enough for one person to audit in depth. A compiler that generates proofs
of computation must itself be trustworthy, and trustworthiness scales
inversely with complexity.

---

## What You Can Build

### Coins (TSP-1, PLUMB)

A coin where balances, transfers, and participants are hidden from
everyone except the parties involved. State is a Merkle tree. Authorization
is hash-preimage based. Transfers emit sealed events: the verifier sees a
commitment, not the transfer itself.

Trident ships with a [complete 535-line implementation](../../examples/coin/coin.tri)
covering 5 PLUMB operations (Pay, Lock, Update, Mint, Burn), time-locks,
nullifiers, configurable authorities, composable hooks, and 12 formally
specified security properties. See [Gold Standard](gold-standard.md) for the
full TSP-1 specification.

The authorization check:

```trident
fn verify_auth(auth_hash: Field) {
    let secret: Field = divine()
    let computed: Digest = hash(secret, 0, 0, 0, 0, 0, 0, 0, 0, 0)
    let (h0, _, _, _, _) = computed
    assert_eq(auth_hash, h0)
}
```

`divine()` reads a secret witness from the prover -- invisible to the
verifier. `hash()` is a single Tip5 invocation (1 clock cycle). `assert_eq`
ensures the hash matches the on-chain commitment. The verifier learns nothing
about the secret. The proof is unforgeable.

The balance check:

```trident
let new_s_bal: Field = sub(s_bal, amount)
assert_non_negative(new_s_bal)
```

Negative balances are impossible because field underflow produces values
larger than 2^32, which fail the U32 range check. No SafeMath library. The
field arithmetic and the range check make the constraint automatic.

### Uniqs (TSP-2, PLUMB)

Unique assets with per-asset metadata, royalties, creator immutability, and
5-bit flag-gated operations (transferable, burnable, updatable, lockable,
mintable). The [uniq implementation](../../examples/uniq/uniq.tri) uses the same
PLUMB framework as TSP-1: 10-field leaves, 10-field config (5 authorities +
5 hooks), and all 5 operations with dual authorization, time-locks, and
collection binding. See [Gold Standard](gold-standard.md) §6 for the full
TSP-2 specification.

### Neptune Transaction Validation

Trident compiles directly to the programs that Neptune Cash miners prove.
The transaction validation engine -- UTXO verification, supply conservation,
nullifier checking -- is expressible in Trident instead of hand-written TASM.
Neptune's consensus rules become readable source code that any developer can
audit.

### Recursive STARK Verification

Verify a proof inside a proof. [Triton VM](https://triton-vm.org/) was
designed for this: `xx_dot_step` and `xb_dot_step` instructions compute
extension-field dot products for [FRI](https://eccc.weizmann.ac.il/report/2017/134/)
verification. [Neptune Cash](https://neptune.cash/) has a working recursive
verifier running in production today.

A structural sketch in Trident:

```trident
pub fn verify(claim: Claim) {
    let commitment: Digest = divine5()

    for i in 0..NUM_ROUNDS {
        let idx: U32 = as_u32(divine())
        let leaf: Digest = divine5()
        merkle.verify(commitment, leaf, idx, FRI_DEPTH)
    }

    stark.fri.verify_all_layers(FRI_DEPTH)
    verify_transitions(claim)
}
```

Recursive verification cost is approximately 300K clock cycles regardless of
the original computation's complexity.

### And More

- **DeFi protocols** -- AMMs, lending, stablecoins on a private, quantum-safe
  foundation. TIDE (unified liquidity) and COMPASS (oracle) are designed.
  See [Gold Standard](gold-standard.md) §§7-8.
- **Cross-chain bridges** -- Recursive STARK verification sidesteps expensive
  non-native cryptography. Bridge logic written once, compiled to both sides.
- **Identity and credentials** -- The `verify_auth` pattern generalizes to
  anonymous credentials, selective disclosure, and delegated authorization.
- **Sovereign mining** -- Neptune Cash runs Proof-of-Work STARK mining in
  production today. Trident makes it practical to write the programs miners
  prove.

---

## Why Trident

### Quantum Safety Is Not Optional

Quantum safety is a VM-level property -- it depends on the cryptographic
primitives the proof system uses:

| VM | Prover Quantum-Safe | Verifier Quantum-Safe | Migration Path |
|----|:---:|:---:|---|
| Cairo/Stwo | Yes (Circle STARKs) | Yes (native STARK) | None needed |
| SP1 | Yes (FRI) | **No** (Groth16/BN254) | Fundamental redesign |
| RISC Zero | Yes (0STARK) | **No** (Groth16/BN254) | Fundamental redesign |
| AVM (Aleo) | **No** (Pasta curves) | **No** (Pasta curves) | Complete crypto migration |
| Kimchi (Mina) | **No** (Pasta curves) | **No** (Pasta curves) | Complete crypto migration |
| Nock | **No** (no algebraic hash) | **No** | VM-level redesign |
| **Triton** | **Yes** (FRI + Tip5) | **Yes** (native STARK) | **None needed** |

[Triton VM](https://triton-vm.org/) uses no elliptic curves anywhere. The
proof system is [FRI](https://eccc.weizmann.ac.il/report/2017/134/) over
finite fields. The hash function is
[Tip5](https://eprint.iacr.org/2023/107) -- algebraic, STARK-native.
Security rests entirely on hash collision resistance and low-degree testing.
No pairings. No groups. No discrete log. No trusted setup.

When quantum computers break elliptic curves -- and
[NIST has already standardized post-quantum algorithms](https://csrc.nist.gov/Projects/post-quantum-cryptography)
-- every Groth16 proof ever generated becomes forgeable. The break is total
and retroactive. Infrastructure built on hash-only STARKs does not need to
migrate.

### Write Once, Prove Anywhere -- Including the Incomplete Platforms

Neptune/Triton VM is the only platform that satisfies all four requirements
today. But Trident is designed to also compile to the incomplete platforms --
Cairo, SP1, Miden, EVM, Nock -- when their tradeoffs are acceptable for
your use case.

You deploy to Neptune for quantum safety, privacy, programmability, and
permissionless mining. You deploy the same source to Ethereum for liquidity
and network effects. You deploy to Solana for throughput. The code is one.
The tradeoffs are per-deployment.

A program written today for Triton VM is architecturally ready to compile
to Miden, Cairo, RISC-V zkVMs, EVM, WASM, and Nock as those backends ship.

### Formal Verification (Designed)

Trident programs are bounded, first-order, heap-free computations over finite
fields. The verification problem is decidable: the compiler architecture
supports automated correctness proofs for all possible inputs. This makes
Trident a natural environment for LLM-generated formally verified code.
See [Formal Verification](formal-verification.md).

### Content-Addressed Code (Designed)

Every Trident function has a unique cryptographic identity derived from its
normalized AST. Names are metadata; the hash is the identity. Audit
certificates, verification results, and cost analyses are portable across
projects, teams, and time. See [Content-Addressed Code](content-addressing.md).

### The Strategic Position

Trident sits at a unique intersection:

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

It is not the most expressive language, and it is not the most minimal
circuit DSL. It is the sweet spot for provable programs that need to be
portable, auditable, and cost-transparent. Every trend -- more zkVMs
launching, ZK expanding beyond crypto into data and AI, regulatory pressure
for auditable code, cross-chain interoperability becoming critical -- makes
this position stronger.

---

## The Vision

**Sovereign, private, provable computation should be accessible to every
developer, not just the three people who can write TASM by hand -- and it
should not be locked to a single virtual machine.**

[Neptune Cash](https://neptune.cash/) proves the architecture works. Miners
generate STARK proofs. Transactions are private. The chain is secured by
hash functions. Recursive verification runs in production. What was missing
was the language.

Trident fills that gap. Deliberately minimal -- no metaprogramming, no
dynamic dispatch, no heap, no recursion -- because every feature that makes
cost prediction harder makes the system less trustworthy. The
[Vyper](https://docs.vyperlang.org/) philosophy: deliberate limitation as a
feature. One obvious way to do everything. What you see is what you prove.

The bet is fourfold:

**Quantum computers will break elliptic curves within our professional
lifetimes.** Every SNARK system in production has an expiration date.
Infrastructure built on hash-only STARKs does not need to migrate.

**Privacy is a requirement, not a feature.** When you build tokens, identity,
governance, or any system touching human agency, the default must be private.

**Developer experience determines adoption.** Triton VM is the right
foundation. Raw TASM is the wrong interface. Cairo proved this for StarkWare.
Leo proved this for Aleo. Trident proves this for the only OS that gets all
four properties right.

**No program should be stranded on one VM.** The universal core is designed
to compile to any target. Backend extensions add power without limiting
portability. The
architecture ensures that choosing Trident is not choosing a single
ecosystem -- it is choosing all of them.

The token example is 535 lines. The uniq example follows the same PLUMB
framework with 5 operations, 10-field leaves, and 5-bit flags. The compiler
is ~43K lines of Rust. The test suite has 743 tests. 53 `.tri` files span
`vm/`, `std/`, `os/`, and `examples/`. 54 TIR operations lower through 4
paths designed for 20 VMs and 25 OSes. The cost model tracks 6 tables. Two token
standards (TSP-1, TSP-2) share the PLUMB framework.

The numbers are growing. The foundation is solid. The rest is building.

---

## Further Reading

### Trident Documentation

- [Tutorial](../tutorials/tutorial.md) -- Step-by-step guide from hello world to Merkle proofs
- [Language Reference](../reference/language.md) -- Types, operators, builtins, grammar, CLI flags
- [OS Reference](../reference/os.md) -- The os.* portable API and per-OS lowering tables
- [IR Reference](../reference/ir.md) -- 54 TIR operations, 4 lowering paths, pipeline architecture
- [Programming Model](programming-model.md) -- How Triton VM execution works
- [Multi-Target Compilation](multi-target.md) -- Write once, deploy to every blockchain
- [OS Abstraction](os-abstraction.md) -- How Trident abstracts over 25 operating systems
- [Gold Standard](gold-standard.md) -- Neptune financial primitives: PLUMB, TSP-1, TSP-2, TIDE
- [Optimization Guide](../guides/optimization.md) -- Cost reduction strategies and table management
- [Comparative Analysis](provable-computing.md) -- Triton VM vs. every other ZK system
- [Developer Guide](for-developers.md) -- Getting started with Trident
- [Blockchain Developer Guide](for-blockchain-devs.md) -- Trident for Solidity/Cairo developers
- [How STARK Proofs Work](stark-proofs.md) -- From execution traces to quantum-safe proofs
- [Formal Verification](formal-verification.md) -- Automated correctness proofs for Trident programs
- [Content-Addressed Code](content-addressing.md) -- Code identity by computation, not by name

### External Resources

- [Triton VM](https://triton-vm.org/) -- The virtual machine and its specification
- [Triton VM ISA Specification](https://triton-vm.org/spec/) -- The TASM instruction set
- [Neptune Cash](https://neptune.cash/) -- Proof-of-Work blockchain running Triton VM in production
- [tasm-lib](https://github.com/TritonVM/tasm-lib) -- Reusable TASM snippets and patterns
- [Tip5 Hash Function](https://eprint.iacr.org/2023/107) -- The algebraic hash (ePrint 2023/107)
- [FRI Protocol](https://eccc.weizmann.ac.il/report/2017/134/) -- The Fast Reed-Solomon IOP (ECCC 2017/134)
- [Goldilocks Field](https://xn--2-umb.com/22/goldilocks/) -- The base field (p = 2^64 - 2^32 + 1)
- [NIST Post-Quantum Cryptography](https://csrc.nist.gov/Projects/post-quantum-cryptography) -- The standardization effort
- [Shor's Algorithm](https://en.wikipedia.org/wiki/Shor%27s_algorithm) -- Why elliptic curves have an expiration date

---

*Trident is open source. The specification, compiler, examples, and this
document are available at
[github.com/nicktriton/trident](https://github.com/nicktriton/trident).
Contributions welcome.*
