# Trident: Provable Programs for Everyone

**The language that makes zero-knowledge programming practical.**

---

## The Problem

Zero-knowledge proofs are the most important cryptographic primitive of this
century. They let you prove that a computation happened correctly without
revealing the inputs. Private transactions. Verifiable computation. Trustless
bridges. Sovereign chains. The applications are extraordinary.

Writing them is hell.

Today, if you want to build a provable program on the only quantum-safe,
private, programmable, mineable VM that exists -- [Triton VM](https://triton-vm.org/) --
you write raw assembly. TASM. Forty-five instructions. No types. No modules.
No error messages beyond "assertion failed." You manage a 16-element operand
stack by hand, track six algebraic tables in your head, and debug by staring
at execution traces until the numbers stop swimming.

Neptune Cash did this. Their team wrote a working recursive STARK verifier,
a full transaction validation engine, and a Proof-of-Work blockchain -- all in
raw TASM. It took years. It produced an inflation bug. It works, but it
doesn't scale to a world where more than three people need to write provable
programs.

The alternative ecosystems are not alternatives at all:

- **Cairo/StarkWare** is production-grade but it's a walled garden. One
  company controls the language, compiler, prover, and the L2 it runs on.
  Your provable programs are guests in someone else's house. And there's no
  Proof-of-Work path -- StarkNet is PoS only.

- **SP1 and RISC Zero** let you write Rust, which is genuinely great for
  developer experience. Then they wrap the final proof in **Groth16** -- a
  SNARK scheme that uses BN254 elliptic curve pairings for cheap Ethereum
  verification. That wrapping step is not quantum-safe. The prover is STARK,
  but the verifier is not. One sufficiently powerful quantum computer breaks
  every proof ever generated.

- **Aleo** is privacy-native with real institutional backing. But it's
  SNARK-based on Pasta curves, requires a trusted setup ceremony, and its
  PoSW hybrid still stake-gates validators at 1M+ ALEO.

- **Mina** proved you can compress a blockchain to 22 KB using recursive
  proofs -- a genuine breakthrough. But Kimchi uses Pasta curves (not
  quantum-safe), zkApps get 8 fields of on-chain state, and the general-
  purpose VM is still on the roadmap.

Every one of these systems makes a compromise. Either you give up quantum
safety, or privacy, or programmability, or permissionless participation.

Developers should not need a PhD in algebraic geometry to write provable
programs. They should not need to pick which security property to sacrifice.
They should not need to write assembly.

That is why Trident exists.

---

## The Solution

Trident is a minimal, security-first language with a universal compilation
architecture. Its 3-layer design -- universal core, abstraction layer, and
backend extensions -- compiles to any zkVM target. The default target is
[Triton VM](https://triton-vm.org/) (`--target triton`), with the
architecture ready for Miden, Cairo, and RISC-V zkVM backends. For stack-
machine targets, there is no intermediate representation. No optimization
engine. What you write is what gets proved.

```
program hello

fn main() {
    let a: Field = pub_read()
    let b: Field = pub_read()
    let sum: Field = a + b
    pub_write(sum)
}
```

That is a complete provable program. Build it with
`trident build hello.tri --target triton` (or omit `--target` for the
default), feed it to the Triton VM prover, and you get a
[STARK](stark-proofs.md) proof that `a + b = sum` for secret values of `a`
and `b`. Because this program uses only the universal core, it can also
compile to other backends as they come online. Quantum-safe. Zero-knowledge.
No trusted setup. No elliptic curves. No vendor lock-in.

Here is what the language gives you:

**Types that match the VM.** `Field` (64-bit [Goldilocks](https://xn--2-umb.com/22/goldilocks/)
elements), `XField` (extension field triples), `Bool`, `U32` (range-checked),
`Digest` (5-element [Tip5](https://eprint.iacr.org/2023/107) hashes),
fixed-size arrays, tuples, and structs. Every type has a known width in stack
elements. No heap. No dynamic allocation. No surprises.

**Modules and imports.** Split your program across files. The 13-module
standard library wraps every Triton VM primitive -- hashing, Merkle proofs,
sponge operations, field arithmetic, memory access, I/O -- so you never
write raw `push`/`pop`/`swap` unless you choose to.

**Bounded loops only.** Every `for` loop requires an explicit bound. No
unbounded iteration. No recursion. This is the property that makes static
cost analysis possible. If the compiler accepts your program, the proving
cost is computable before you ever run the prover.

**Formal verification.** The `#[pure]` annotation marks functions for
automated verification. Because Trident programs are bounded, first-order,
and heap-free, the compiler can symbolically verify that a function satisfies
its specification for all possible inputs -- not by testing, but by
exhaustive analysis. See [Formal Verification](formal-verification.md).

**Content-addressed code.** Every function compiles to a deterministic
constraint system with a unique cryptographic identity. Two functions with
different names but identical computation share the same hash. Audit
certificates, cost analyses, and verification results travel with the code
regardless of naming. See [Content-Addressed Code](content-addressed.md).

**Proof composition.** Language-level support for verifying a proof inside a
proof. The recursive verification pattern -- Triton VM's architectural sweet
spot -- is a first-class operation, not a hand-rolled assembly routine.

**Events, open and sealed.** `emit` writes event fields to public output.
`seal` hashes them and writes only the digest -- the verifier sees that
*something* happened, but not what. This is how you build private transfers.

**Inline assembly with target tags and declared stack effects.** When you
need the VM's raw power, drop into target-specific assembly:

```
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

The target tag `(triton)` binds the block to a specific backend -- the
compiler rejects it when compiling for a different target. The `-1` declares
the net stack change. You get full control when you need it, structured code
when you don't. Programs that omit the target tag default to `(triton)` for
backward compatibility.

**Const generic expressions.** Array sizes can be computed from generic
parameters: `fn concat<M, N>() -> [Field; M + N]` enables compile-time
concat, split, and reshape of fixed-size arrays with zero runtime cost.

**Struct pattern matching.** Destructure structs directly in match arms:
`match point { Point { x: 0, y } => ... }` desugars to field access and
conditional checks with no overhead.

---

## What You Can Build

### Private Tokens

A fungible token where balances, transfers, and participants are hidden from
everyone except the parties involved. State is a Merkle tree. Authorization
is hash-preimage based -- any secret works, from a private key to a Shamir
share set to a ZK proof of identity. Transfers emit sealed events: the
verifier sees a commitment to the transfer, not the transfer itself.

Trident ships with a [complete 530-line implementation](../examples/fungible_token/token.tri)
covering 5 operations (pay, lock, update, mint, burn), time-locks,
nullifiers, configurable authorities, composable hooks, and 12 formally
specified security properties.

Consider what the pay operation looks like in Trident. The entire
authorization check is:

```
fn verify_auth(auth_hash: Field) {
    let secret: Field = divine()
    let computed: Digest = hash(secret, 0, 0, 0, 0, 0, 0, 0, 0, 0)
    let (h0, _, _, _, _) = computed
    assert_eq(auth_hash, h0)
}
```

`divine()` reads a secret witness from the prover -- invisible to the
verifier. `hash()` is a single [Tip5](https://eprint.iacr.org/2023/107)
invocation (1 clock cycle + 6 coprocessor rows). `assert_eq` ensures the
hash matches the on-chain commitment. The verifier learns nothing about the
secret. The proof is unforgeable.

The balance check is equally direct:

```
let new_s_bal: Field = sub(s_bal, amount)
assert_non_negative(new_s_bal)
```

Where `assert_non_negative` is:

```
fn assert_non_negative(val: Field) {
    let checked: U32 = as_u32(val)
}
```

Negative balances are impossible because field underflow produces values
larger than 2^32, which fail the U32 range check inserted by `as_u32()`.
No special overflow handling. No SafeMath library. The field arithmetic
and the range check conspire to make the constraint automatic.

In Solidity, an ERC-20 with comparable privacy features does not exist --
the EVM is a transparent execution environment. In raw TASM, this program
would be thousands of lines of stack manipulation. In Trident, it is 530
lines with 12 formally specified security properties.

### Neptune Transaction Validation

Trident compiles directly to the programs that Neptune Cash miners prove.
The transaction validation engine -- UTXO verification, supply conservation,
nullifier checking -- is expressible in Trident instead of hand-written TASM.
Neptune's consensus rules become readable source code that any developer can
audit, and the same validation logic can be compiled to other backends as
they ship.

### Recursive STARK Verification

Verify a proof inside a proof. [Triton VM](https://triton-vm.org/) was
designed for this: the `xx_dot_step` and `xb_dot_step` instructions compute
extension-field dot products that form the inner loop of
[FRI](https://eccc.weizmann.ac.il/report/2017/134/) verification.
[Neptune Cash](https://neptune.cash/) has a working recursive verifier -- a
STARK proof that verifies another STARK proof -- running in production today.

This is not theoretical. It works. And it enables:

- **Proof aggregation**: batch thousands of individual proofs into one
  constant-size proof
- **Incremental computation**: extend a proof chain without re-proving
  all of history
- **Proof-of-proof bridges**: verify that an external chain's proof is
  valid, inside a Triton VM proof, without implementing the external
  chain's cryptography natively

A structural sketch of the verifier in Trident:

```
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

The verifier reads commitments from the secret input, authenticates them
against Merkle roots using the VM's native `merkle_step` instruction, and
checks FRI proximity using `xx_dot_step`. The recursive verification cost
is approximately 300K clock cycles regardless of the original computation's
complexity.

### DeFi Protocols

The building blocks for decentralized finance on a private, quantum-safe
foundation:

- **Private DEX**: Automated market makers where trade sizes, prices, and
  participants are hidden. The proof guarantees the invariant (e.g.,
  constant product) holds without revealing the state.
- **Lending protocols**: Collateralization ratios verified in zero knowledge.
  Liquidation proofs reveal only that the threshold was crossed, not the
  borrower's position.
- **Prediction markets**: Bet placement and resolution proved without
  revealing individual positions until settlement.

### Identity Without Revelation

The `verify_auth` pattern from the token example generalizes to any identity
system. The `auth_hash` can commit to anything: a password, a biometric hash,
a multi-party computation output, a hardware security module attestation.
The protocol prescribes a commitment scheme and lets users choose their own
security model.

Build on this foundation:

- **Anonymous credentials**: prove membership in a group without revealing
  which member you are (commit identities to a Merkle tree, prove leaf
  inclusion without revealing the leaf)
- **Selective disclosure**: prove you are over 18 without revealing your
  age, prove you are a citizen without revealing your name
- **Delegated authorization**: prove that someone authorized you to act,
  without revealing who or when

### Cross-Chain Bridges

Bitcoin and Ethereum light clients that verify in zero-knowledge. For
Ethereum, instead of implementing BLS12-381 pairing verification directly
in-circuit (millions of cycles), verify a STARK proof that *someone else*
verified the BLS signature correctly. The recursive verification costs
approximately 300K clock cycles regardless of the original computation's
complexity. This is Triton VM's architectural sweet spot: use native
recursive verification to sidestep expensive non-native cryptography.

With Trident's universal compilation, bridge logic is written once and
compiled to both chains' native zkVMs. The verifier on each side provably
computes the same thing. Auditors verify one codebase, not two.

### Private Voting and Compliance

**Private voting.** Voters prove eligibility via Merkle inclusion and cast
votes without revealing which voter cast which vote. The proof guarantees
eligibility, prevents double-voting (nullifier), and ensures well-formedness.
The tally is public. The individual votes are hidden.

**Compliance proofs.** A bank proves reserves exceed liabilities without
revealing individual balances. An exchange proves it has not processed
sanctioned addresses without revealing its address book. Commit sensitive
data to a Merkle tree, run the compliance check inside the STARK proof,
output only the boolean result. The regulator gets cryptographic assurance.
The data stays private.

### Verifiable AI/ML Inference

Prove that a neural network inference was computed correctly. The computation
is inherently bounded (fixed model architecture, fixed input size) and
arithmetic-heavy (matrix multiplications over finite fields). Trident's
properties align naturally:

- Fixed arrays for weight matrices
- Bounded loops for layer computation
- Field arithmetic for quantized operations
- Multi-target compilation to prove inference on whichever VM is fastest
  for matrix operations

### Provable Data Pipelines

Beyond smart contracts, Trident targets provable data processing:

- **Provable ETL**: Transform data with guaranteed correctness. Input hash +
  output hash + Trident program = verifiable data pipeline.
- **Provable aggregation**: Compute statistics (sum, mean, count, Merkle
  root) over datasets with proof of correct computation.
- **Supply chain verification**: Each step runs a Trident program that
  verifies the previous step's proof and extends the chain.

### Sovereign Mining

A Proof-of-Work blockchain where miners generate STARK proofs of arbitrary
computation. Not hypothetical -- [Neptune Cash](https://neptune.cash/) runs
this architecture today. Miners do useful work (verifying transactions)
instead of burning energy on arbitrary hash puzzles. The economic flywheel
is real: hardware investment creates network commitment, which creates
ecosystem growth, which justifies further hardware investment.

Trident makes it practical to write the programs those miners prove. Without
a high-level language, every new program on Triton VM requires an assembly
expert. With Trident, any developer who can read a struct definition and a
for loop can write provable programs for a sovereign, mineable chain.

### Cross-Chain Provable Computation

Because Trident programs are target-agnostic, the same source compiled to
different backends produces independent proofs of the *same computation*.
This unlocks:

- **Cross-chain atomic operations**: A single Trident program describes an
  atomic swap. Compile to each chain's VM. The proofs are independent but
  the source is shared.
- **Proof relay networks**: Chain A proves computation X. Chain B needs to
  verify that proof. If both run Trident-compatible VMs, the recursive
  verifier is the same program compiled to different targets.
- **Prover shopping**: A computation written in Trident can be proved on
  whichever zkVM offers the best price/performance. Triton might be cheapest
  for hash-heavy workloads, SP1 for general computation, Miden for
  state-heavy programs.

---

## Architecture

### Compilation: Source to TASM, Nothing in Between

Trident's compiler is built on a 3-layer universal design:

```
+-------------------------------------------+
|         Trident Universal Core            |
|   (types, control flow, modules, field    |
|    arithmetic, I/O, cost transparency)    |
+-------------------------------------------+
|         Abstraction Layer                 |
|   (hash, memory, stack/register mgmt,    |
|    Merkle ops, cost model, events)        |
+----------+---------+---------+------------+
| Triton   |  Miden  |  Cairo  |  SP1/RZ   |
| Backend  | Backend | Backend |  Backend  |
|  + ext   |  + ext  |  + ext  |  + ext    |
+----------+---------+---------+------------+
```

The **universal core** (~56% of the language surface) compiles identically to
every target: types, field arithmetic, bounded loops, modules, functions. The
**abstraction layer** (~22%) provides the same syntax with per-target dispatch:
I/O, hashing, Merkle operations, memory, events. **Backend extensions** (~22%)
expose target-specific capabilities -- `XField` on Triton, account models on
Miden, precompiles on SP1 -- through a uniform extension mechanism under
`ext/<target>/`.

The `--target` flag selects the backend (default: `triton`):

```bash
trident build main.tri                     # default: --target triton
trident build main.tri --target triton
trident build main.tri --target miden      # when backend ships
```

For the Triton backend, the pipeline is direct:

```
Source (.tri) --> Parse --> AST --> Type Check --> TASM Emit --> Link
```

No intermediate representation. The AST is type-checked, then each node
emits TASM directly. When you see `hash(a, b, c, d, e, f, g, h, i, j)` in
source, the `hash` instruction fires in the VM -- 1 clock cycle, 6 hash
table rows, 5 elements out. No optimization pass reorders your operations.
No IR introduces instructions you did not write. **What you see is what you
prove.**

Register-machine targets (Cairo, SP1/RISC Zero) will use a minimal IR
between type checking and emission. Stack-machine targets (Triton, Miden)
keep the direct-emission model.

The compiler is 36,848 lines of Rust with 4 runtime dependencies. Small
enough for one person to audit in depth. A compiler that generates proofs
of computation must itself be trustworthy, and trustworthiness scales
inversely with complexity.

### Standard Library: Layered for Portability

The standard library mirrors the 3-layer architecture:

```
std/
+-- core/               Universal -- zero VM dependencies
|   +-- field.tri         Goldilocks field arithmetic
|   +-- u32.tri           U32 operations
|   +-- convert.tri       Type conversions (as_u32, as_field)
|   +-- assert.tri        Assertions (is_true, eq, digest)
|
+-- io/                 Abstraction layer -- per-target intrinsic dispatch
|   +-- io.tri            pub_read, pub_write, divine
|   +-- mem.tri           ram_read, ram_write, block operations
|   +-- storage.tri       Persistent storage
|
+-- crypto/             Abstraction layer -- hash-parameterized
    +-- hash.tri          hash(), sponge_init/absorb/squeeze
    +-- merkle.tri        Merkle tree verification
    +-- auth.tri          Hash-preimage authorization

ext/
+-- triton/             Backend extensions -- Triton VM specific
    +-- xfield.tri        XField type, xx_add, xx_mul, x_invert
    +-- kernel.tri        Neptune kernel interface
    +-- utxo.tri          UTXO verification
```

Modules under `std/core/` compile identically to every target. Modules under
`std/io/` and `std/crypto/` use the same syntax everywhere but dispatch to
target-native instructions. Modules under `ext/triton/` are available only
when compiling with `--target triton`. 52 `.tri` files across
`std/`, `ext/`, and `examples/` form the current library and example surface.

Every function in the standard library has a known, fixed cost in table rows.
When you call `std.crypto.merkle.verify(root, leaf, index, depth)`, the
compiler knows exactly how many Hash rows, Processor rows, and RAM rows that
verification adds to your trace.

### Cost Transparency: Know What You Pay Before You Prove

In most ZK systems, proving cost is a runtime discovery. You run the program,
measure the trace, and find out afterward how expensive it was. STARK proving
time is superlinear in trace length -- small changes in program behavior can
produce large changes in cost.

Trident takes a different approach: **every program's proving cost is
computable from the source code alone, before execution.**

This is possible because all loops are bounded, all types have fixed widths,
and every instruction's contribution to all six Triton VM tables is known.
The compiler sums these across all paths and reports the maximum table height
padded to the next power of two -- which is the actual value that determines
proving time.

```bash
$ trident build main.tri --costs
```

```
Cost report:
+---------------------------------------------------------+
| Program: merkle_verifier                                |
+----------------------+------+------+------+------+------+
| Function             |  cc  | hash |  u32 |  ram | dom  |
+----------------------+------+------+------+------+------+
| main                 |   22 |   12 |    4 |    0 | proc |
| merkle.verify        |  142 |  120 |   87 |    0 | proc |
|   per iteration      |    7 |    6 |    4 |    0 | hash |
| TOTAL (worst case)   |  164 |  132 |   91 |    0 | proc |
+----------------------+------+------+------+------+------+
| Padded height: 256                                      |
| Estimated proving time: ~1.6s                           |
+---------------------------------------------------------+
```

The `--hotspots` flag ranks functions by cost contribution. The `--hints`
flag gives actionable optimization suggestions. The `--annotate` flag inlines
costs with your source code. The `--compare` flag diffs two cost snapshots.
This is profiling for proving cost -- not guessing, not estimating, but
measuring from source code at compile time. See the
[Optimization Guide](optimization.md) for details.

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

For a Merkle tree of 2^20 nodes (about 1 million) requiring 2^20 hash
operations: Triton VM finishes in roughly 1 million cycles. SP1 needs
3 billion. RISC Zero needs 1 billion.

For applications built on hash-heavy data structures -- content addressing,
Merkle authentication, sponge accumulation, knowledge graphs, supply chain
verification -- this dominance is decisive.

---

## Why Trident

### Quantum Safety Is Not Optional

Every SNARK system in production today relies on the hardness of the discrete
logarithm problem on elliptic curves. [Shor's algorithm](https://en.wikipedia.org/wiki/Shor%27s_algorithm)
solves discrete logarithm in polynomial time on a quantum computer. The
question is not *whether* sufficiently powerful quantum computers will exist,
but *when*. NIST has already [standardized post-quantum algorithms](https://csrc.nist.gov/Projects/post-quantum-cryptography).
The migration is underway.

When it happens, the break is total and retroactive. Every Groth16 proof
ever generated becomes forgeable. Every KZG polynomial commitment becomes
extractable. Every blockchain using these schemes loses its security
guarantees -- not going forward, but *retroactively*, for all historical
proofs.

| System | Prover Quantum-Safe | Verifier Quantum-Safe | Migration Path |
|--------|:---:|:---:|---|
| StarkWare/Stwo | Yes (Circle STARKs) | Yes (native STARK) | None needed |
| SP1 | Yes (FRI) | **No** (Groth16/BN254) | Fundamental redesign |
| RISC Zero | Yes (0STARK) | **No** (Groth16/BN254) | Fundamental redesign |
| Aleo | **No** (Pasta curves) | **No** (Pasta curves) | Complete crypto migration |
| Mina | **No** (Pasta curves) | **No** (Pasta curves) | Complete crypto migration |
| **Triton VM** | **Yes** (FRI + Tip5) | **Yes** (native STARK) | **None needed** |

[Triton VM](https://triton-vm.org/) uses no elliptic curves anywhere. The
proof system is [FRI](https://eccc.weizmann.ac.il/report/2017/134/) -- a
proximity test over finite fields. The hash function is
[Tip5](https://eprint.iacr.org/2023/107) -- an algebraic hash designed for
STARK arithmetic. Security rests entirely on hash collision resistance and
low-degree testing. No pairings. No groups. No discrete log. No trusted setup.

### The Only System That Passes All Four Tests

The [comparative analysis](analysis.md) evaluates every major ZK system
against four requirements: quantum-safe, private, programmable, and mineable.

| System | Quantum-Safe | Private | Programmable | Mineable | All Four |
|--------|:---:|:---:|:---:|:---:|:---:|
| StarkWare/Stwo | Yes | Yes | Yes | No | **No** |
| SP1 | No (Groth16) | No (default) | Yes | No | **No** |
| RISC Zero | No (Groth16) | Yes | Yes | No | **No** |
| Aleo | No (Pasta) | Yes | Yes | Partial | **No** |
| Mina | No (Pasta) | Partial | Partial | No | **No** |
| **Triton VM** | **Yes** | **Yes** | **Yes** | **Yes** | **Yes** |

Triton VM is the only system that satisfies all four simultaneously, today,
in production. [Neptune Cash](https://neptune.cash/) proves it works.
Trident makes that system programmable by humans, not just assembly experts.

### Formal Verification Is Decidable

Trident programs are bounded, first-order, heap-free computations over finite
fields. The verification problem is decidable: the compiler can automatically
prove that a contract satisfies its specification for all possible inputs.
No human constructs a proof. The compiler is the theorem prover. This makes
Trident the first programming environment where LLMs can reliably produce
formally verified code -- generate, verify in seconds, fix on failure, loop
to convergence. See [Formal Verification](formal-verification.md).

### Content-Addressed Code

Every Trident function has a unique cryptographic identity derived from its
normalized AST. Names are metadata; the hash is the identity. Audit
certificates, verification results, and cost analyses are portable across
projects, teams, and time. Two developers on different continents who write
the same Merkle verifier with different variable names get the same hash --
and share the same audit. See [Content-Addressed Code](content-addressed.md).

### Write Once, Prove Anywhere

The universal core compiles to any target. Backend extensions add power
without limiting portability. Choosing Trident is not choosing a single
ecosystem -- it is choosing all of them. A program written today for Triton
VM is architecturally ready to compile to Miden, Cairo, and RISC-V zkVMs as
those backends ship.

### The Strategic Position

Trident sits at a unique intersection:

```
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

Trident fills that gap. Its universal compilation architecture -- a shared
core, an abstraction layer, and per-target backend extensions -- means that
programs written today for Triton VM are architecturally ready to compile to
Miden, Cairo, and RISC-V zkVMs as those backends ship. The 3-layer design is
implemented: `std/core/` for portable logic, `std/io/` and `std/crypto/` for
abstracted primitives, and `ext/triton/` for Triton-specific power. The
`--target` flag selects the backend; `asm(triton) { ... }` blocks tag
target-specific assembly. Write once, prove anywhere.

Deliberately minimal -- no metaprogramming, no dynamic dispatch, no heap, no
recursion -- because every feature that makes cost prediction harder makes
the system less trustworthy. The [Vyper](https://docs.vyperlang.org/)
philosophy: deliberate limitation as a feature. One obvious way to do
everything. What you see is what you prove.

The bet is fourfold:

**Quantum computers will break elliptic curves within our professional
lifetimes.** Every SNARK system in production has an expiration date.
Infrastructure built on hash-only STARKs does not need to migrate.

**Privacy is a requirement, not a feature.** When you build tokens, identity,
governance, or any system touching human agency, the default must be private.

**Developer experience determines adoption.** Triton VM is the right
foundation. Raw TASM is the wrong interface. Cairo proved this for StarkWare.
Leo proved this for Aleo. Trident proves this for the only VM that gets all
four properties right.

**No program should be stranded on one VM.** The universal core compiles to
any target. Backend extensions add power without limiting portability. The
architecture ensures that choosing Trident is not choosing a single
ecosystem -- it is choosing all of them.

The token example is 530 lines. The compiler is 36,848 lines of Rust. The
test suite has 670 tests. 52 `.tri` files span `std/`, `ext/`, and
`examples/`. The cost model tracks 6 tables. Formal verification is
decidable. Code is content-addressed.

The numbers are growing. The foundation is solid. The rest is building.

---

## Further Reading

### Trident Documentation

- [Tutorial](tutorial.md) -- Step-by-step guide from hello world to Merkle proofs
- [Language Reference](reference.md) -- Quick lookup: types, operators, builtins, grammar, CLI flags
- [Programming Model](programming-model.md) -- How Triton VM execution works
- [Optimization Guide](optimization.md) -- Cost reduction strategies and table management
- [Comparative Analysis](analysis.md) -- Triton VM vs. every other ZK system
- [Developer Guide](for-developers.md) -- Getting started with Trident
- [Blockchain Developer Guide](for-blockchain-devs.md) -- Trident for Solidity/Cairo developers
- [How STARK Proofs Work](stark-proofs.md) -- From execution traces to quantum-safe proofs
- [Formal Verification](formal-verification.md) -- Automated correctness proofs for Trident programs
- [Content-Addressed Code](content-addressed.md) -- Code identity by computation, not by name

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
