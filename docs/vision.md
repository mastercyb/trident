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

## What Trident Makes Possible

Trident is a minimal, security-first language that compiles directly to TASM
for [Triton VM](https://triton-vm.org/). No intermediate representation. No
optimization engine. What you write is what gets proved.

```
program hello

fn main() {
    let a: Field = pub_read()
    let b: Field = pub_read()
    let sum: Field = a + b
    pub_write(sum)
}
```

That is a complete provable program. Build it with `trident build hello.tri`,
feed it to the Triton VM prover, and you get a [STARK](stark-proofs.md)
proof that `a + b = sum` for secret values of `a` and `b`. Quantum-safe.
Zero-knowledge. No trusted setup. No elliptic curves. No vendor lock-in.

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

**Events, open and sealed.** `emit` writes event fields to public output.
`seal` hashes them and writes only the digest -- the verifier sees that
*something* happened, but not what. This is how you build private transfers.

**Inline assembly with declared stack effects.** When you need the VM's raw
power, drop into TASM:

```
fn custom_hash(a: Field, b: Field) -> Field {
    asm(-1) {
        hash
        swap 5 pop 1
        swap 4 pop 1
        swap 3 pop 1
        swap 2 pop 1
        swap 1 pop 1
    }
}
```

The `(-1)` tells the compiler the net stack change. You get full control when
you need it, structured code when you don't.

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
lines with 12 formally specified security properties. More on this below.

### Provable Computation

Verify any computation without re-executing it. The core pattern is "compute
expensive, verify cheap":

```
fn prove_sqrt(x: Field) {
    let s: Field = divine()      // prover injects sqrt(x)
    assert(s * s == x)           // verifier checks s^2 = x
}
```

The prover does the hard work. The program constrains the result. The
[STARK](stark-proofs.md) proof guarantees the constraint holds.
This generalizes to any computation where verification is cheaper than
computation: matrix multiplication, database queries, ML inference, search.

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

### Private Voting, Compliance, and Bridges

The building blocks in the token example -- Merkle authentication,
nullifiers, hash-preimage authorization -- compose into larger systems:

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

**Cross-chain bridges.** Bitcoin and Ethereum light clients that verify in
zero-knowledge. For Ethereum, instead of implementing BLS12-381 pairing
verification directly in-circuit (millions of cycles), verify a STARK proof
that *someone else* verified the BLS signature correctly. The recursive
verification costs approximately 300K clock cycles regardless of the original
computation's complexity. This is Triton VM's architectural sweet spot: use
native recursive verification to sidestep expensive non-native cryptography.

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

---

## Cost Transparency: Know What You Pay Before You Prove

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

That padding is critical. [Triton VM](https://triton-vm.org/) pads every
table to the next power of two. If your Hash table has 1,025 rows, it pads
to 2,048. If it has 1,024, it stays at 1,024. A single row can double your
proving time. The compiler warns you when you are near these cliffs.

### The --costs Output

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

Every row in this report is computed statically from the source code. The
"dominant" column tells you which table determines the padded height. The
per-iteration breakdown tells you exactly what each loop iteration costs
across all six tables.

The `--hotspots` flag ranks functions by their contribution to the dominant
table:

```bash
$ trident build main.tri --hotspots

Top 5 cost contributors:
  1. merkle.verify:loop_body     120 hash rows (46% of hash table)
  2. compute_inputs_hash:absorb  768 hash rows (29% of hash table)
  3. main:divine5                  5 cc  (negligible)
```

The `--hints` flag gives actionable optimization suggestions:

```
hint[H0001]: hash table is 3.2x taller than processor table
  = Processor optimizations will not reduce proving cost.
  = Consider: batching data before hashing, reducing Merkle depth,
    or using sponge_absorb_mem instead of repeated sponge_absorb.

hint[H0002]: padded height is 1024, but max table height is only 519
  = You have 505 rows of headroom before the next doubling.
  = This function could be 97% more complex at zero additional cost.
```

The `--annotate` flag inlines costs with your source code:

```
pub fn verify(root: Digest, leaf: Digest, index: U32, depth: U32) {
    let mut idx = index                          // cc: 1  hash: 0  u32: 0
    let mut current = leaf                       // cc: 0  hash: 0  u32: 0
    for _ in 0..depth bounded 64 {               // x 64 iterations (worst case)
        (idx, current) = merkle_step(idx, current)  // cc: 1  hash: 6  u32: ~4
    }                                            // subtotal: cc: 64  hash: 384
    assert_digest(current, root)                 // cc: 1  hash: 0  u32: 0
}
// TOTAL: cc: 66  hash: 384  dominant: hash  padded: 512
```

And `--compare` diffs two cost snapshots so you can see the exact impact of
every change:

```bash
$ trident build main.tri --save-costs before.json
# ... make optimizations ...
$ trident build main.tri --compare before.json
```

This is profiling for proving cost. Not guessing. Not estimating. Measuring,
from source code, at compile time. No other ZK language does this.

Without cost transparency, ZK development is trial-and-error: write code,
run the prover, discover it takes 45 minutes, guess which part is expensive,
change something, run again. With Trident, you know before you ever invoke
the prover that adding one `hash()` call near a 512-row boundary will double
your padded height, or that tightening `bounded 128` to `bounded 16` drops
the padded height from 1,024 to 256. This transforms ZK development from
guesswork into engineering.

---

## Quantum Safety Is Not Optional

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

Here is where every major system stands:

| System | Prover Quantum-Safe | Verifier Quantum-Safe | Migration Path |
|--------|:---:|:---:|---|
| StarkWare/Stwo | Yes (Circle STARKs) | Yes (native STARK) | None needed |
| SP1 | Yes (FRI) | **No** (Groth16/BN254) | Fundamental redesign |
| RISC Zero | Yes (0STARK) | **No** (Groth16/BN254) | Fundamental redesign |
| Aleo | **No** (Pasta curves) | **No** (Pasta curves) | Complete crypto migration |
| Mina | **No** (Pasta curves) | **No** (Pasta curves) | Complete crypto migration |
| **Triton VM** | **Yes** (FRI + Tip5) | **Yes** (native STARK) | **None needed** |

SP1 and RISC Zero wrap in Groth16 because Ethereum's gas costs make raw
STARK verification prohibitively expensive. This is a structural vulnerability
they cannot fix without abandoning cheap Ethereum verification.

[Triton VM](https://triton-vm.org/) uses no elliptic curves anywhere. The
proof system is [FRI](https://eccc.weizmann.ac.il/report/2017/134/) -- a
proximity test over finite fields. The hash function is
[Tip5](https://eprint.iacr.org/2023/107) -- an algebraic hash designed for
STARK arithmetic. Security rests entirely on hash collision resistance and
low-degree testing. No pairings. No groups. No discrete log. No trusted setup.

This is not "quantum-resistant as an option." It is quantum-safe by
construction. There is nothing to migrate. The proofs you generate today
will be secure against quantum computers whenever they arrive. For
infrastructure intended to last decades, this is not a feature. It is a
requirement.

---

## The 530-Line Token: A Guided Tour

The best way to understand what Trident makes possible is to read a real
program. The [fungible token example](../examples/fungible_token/token.tri)
is 530 lines of Trident that implement a complete, privacy-preserving
fungible token. Here is how it works.

The full source is at `examples/fungible_token/token.tri`. The formal
specification is at `examples/fungible_token/SPEC.md`.

### State: A Merkle Tree of Account Leaves

Every account is 5 field elements, hashed to a Digest:

```
fn hash_leaf(
    id: Field,
    bal: Field,
    nonce: Field,
    auth: Field,
    lock: Field
) -> Digest {
    hash(id, bal, nonce, auth, lock, 0, 0, 0, 0, 0)
}
```

`id` is the account identifier. `bal` is the token balance (range-checked
to fit in U32). `nonce` is a monotonic counter that prevents replay. `auth`
is the hash of an authorization secret -- not a public key, not a signature
scheme, just a hash commitment. `lock` is a timestamp until which the tokens
cannot move.

These leaves live in a binary Merkle tree. The root is the entire token's
state commitment. To prove any operation, you prove that specific leaves
exist in the tree, transform them according to the operation's rules, and
produce a new root.

This is 6 lines of Trident. One Tip5 hash call. The account model for a
complete private token.

### Token Configuration: 10 Fields, One Hash

The token's business logic is governed by a configuration commitment -- 5
authorities and 5 hooks, packed into a single Tip5 hash:

```
fn hash_config(
    admin_auth: Field, pay_auth: Field, lock_auth: Field,
    mint_auth: Field, burn_auth: Field,
    pay_hook: Field, lock_hook: Field, update_hook: Field,
    mint_hook: Field, burn_hook: Field
) -> Digest {
    hash(admin_auth, pay_auth, lock_auth, mint_auth, burn_auth,
         pay_hook, lock_hook, update_hook, mint_hook, burn_hook)
}
```

Authorities control *who* can perform each operation. Hooks reference
external programs for custom business logic -- compliance checks, transfer
limits, KYC gates, vesting schedules. Every operation divines the 10 config
fields, hashes them, and asserts the hash matches the public commitment. If
it doesn't match, the VM crashes -- no proof generated. The config hash binds
every proof to a specific token: proofs for Token A cannot be reused against
Token B.

### Authorization: Hash Preimage, Not Signatures

```
fn verify_auth(auth_hash: Field) {
    let secret: Field = divine()
    let computed: Digest = hash(secret, 0, 0, 0, 0, 0, 0, 0, 0, 0)
    let (h0, _, _, _, _) = computed
    assert_eq(auth_hash, h0)
}
```

This is the entire authorization mechanism. Four lines. One divine, one hash,
one destructure, one assert. The secret can be anything -- a private key, a
Shamir share reconstruction, a biometric hash, a hardware security module
attestation. The protocol does not prescribe an authentication scheme. It
prescribes a commitment scheme and lets users choose their own security model.

This is account abstraction at the deepest possible level.

### Five Operations, PLUMB Order

The entry point dispatches on a public opcode:

```
fn main() {
    let op: Field = pub_read()
    if op == 0 { pay() }
    else if op == 1 { lock() }
    else if op == 2 { update() }
    else if op == 3 { mint() }
    else if op == 4 { burn() }
}
```

**Pay** (operation 0) transfers tokens between accounts. Here is the core
of the balance logic:

```
// Sufficient balance
let new_s_bal: Field = sub(s_bal, amount)
assert_non_negative(new_s_bal)

// ...

// Compute new leaves
let new_s_nonce: Field = s_nonce + 1
let new_s_leaf: Digest = hash_leaf(
    s_id, new_s_bal, new_s_nonce, s_auth, s_lock
)
let new_r_bal: Field = r_bal + amount
let new_r_leaf: Digest = hash_leaf(r_id, new_r_bal, r_nonce, r_auth, r_lock)
```

The subtraction `sub(s_bal, amount)` is field arithmetic -- if the result
would be negative, it wraps to a value larger than 2^32.
`assert_non_negative` calls `as_u32()`, which inserts a range check via
Triton VM's `split` instruction. If the value exceeds 2^32, the range check
fails, the VM crashes, and no proof can be generated. Negative balances are
mathematically impossible.

After the transfer, the sender emits a sealed nullifier:

```
seal Nullifier { account_id: s_id, nonce: s_nonce }
```

`seal` hashes the event fields and writes only the digest to public output.
The verifier sees that a nullifier was emitted -- enough to reject replays --
but not which account or which nonce. Privacy preserved.

**Lock** (operation 1) time-locks tokens. Locks are monotonic -- you can
extend but never shorten:

```
let lock_diff: Field = sub(lock_time, a_lock)
assert_non_negative(lock_diff)
```

An attacker who gains temporary access cannot remove a lock placed by the
legitimate owner.

**Update** (operation 2) changes the token configuration. Only the admin
can do this:

```
assert_digest(old_root, new_root)   // state must not change
verify_auth(old_admin)              // admin authorization
```

Setting `admin_auth` to zero in the new config permanently renounces admin
authority. The hash preimage of zero is computationally infeasible under
[Tip5](https://eprint.iacr.org/2023/107), so no future update proof can
be constructed. The config becomes immutable forever. Irreversible by
mathematics, not by policy.

**Mint** (operation 3) creates new tokens. Requires config-level authority.
If `mint_auth` is zero, minting is permanently disabled.

**Burn** (operation 4) destroys tokens. Checks time-lock, range-checks
balance, adjusts supply.

### What the Verifier Sees (and What It Doesn't)

Every operation is a standalone zero-knowledge proof. The verifier sees the
old and new Merkle roots, the supply, the current time, the config
commitment, and nullifier commitments for replay prevention.

The verifier **never** sees: account balances, transfer amounts,
authorization secrets, Merkle tree paths, account identifiers, or any of
the 10 config fields. Verified but hidden.

### 12 Security Properties, 530 Lines

The [specification](../examples/fungible_token/SPEC.md) formally defines 12
security properties enforced by constraints in the circuit: no negative
balances (`as_u32()` range check), replay prevention (nonce + sealed
nullifier), time-lock enforcement, lock monotonicity, supply conservation,
account abstraction, config binding, irreversible renounce, config-state
separation, hook composability, symmetric dual authority, and safe defaults
(zero values disable operations, not enable them).

Each property emerges from a small number of constraints in the Trident
source. No runtime. No state machine. Just math, verified by the STARK
prover.

In Solidity, an ERC-20 with comparable features does not exist -- the EVM is
transparent. In raw TASM, this would be thousands of lines of stack
manipulation. Trident sits in the middle: structured enough to be readable,
low-level enough that every line maps to provable computation. 530 lines.
Five operations. Twelve security properties. One file.

---

## The Architecture

### Compilation: Source to TASM, Nothing in Between

```
Source (.tri) --> Parse --> AST --> Type Check --> TASM Emit --> Link
```

No intermediate representation. The AST is type-checked, then each node
emits TASM directly. When you see `hash(a, b, c, d, e, f, g, h, i, j)` in
source, the `hash` instruction fires in the VM -- 1 clock cycle, 6 hash
table rows, 5 elements out. No optimization pass reorders your operations.
No IR introduces instructions you did not write. **What you see is what you
prove.**

The compiler is 16,700 lines of Rust with 4 runtime dependencies. Small
enough for one person to read in a day. Small enough for a security auditor
to verify the source-to-TASM translation in a week. A compiler that
generates proofs of computation must itself be trustworthy, and
trustworthiness scales inversely with complexity.

### Standard Library: 13 Modules

13 modules wrap every Triton VM primitive: I/O (`std.io`), Tip5 hashing
(`std.hash`), Goldilocks field arithmetic (`std.field`), type conversions
(`std.convert`), U32 operations (`std.u32`), assertions (`std.assert`),
extension field ops (`std.xfield`), RAM access (`std.mem`), persistent
storage (`std.storage`), Merkle tree proofs (`std.merkle`), authorization
patterns (`std.auth`), Neptune kernel access (`std.kernel`), and UTXO
verification (`std.utxo`). See the [Language Specification](spec.md) for
the complete API.

Every function in the standard library has a known, fixed cost in table rows.
When you call `std.merkle.verify1(root, leaf, index, depth)`, the compiler
knows exactly how many Hash rows, Processor rows, and RAM rows that
verification adds to your trace.

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

## The Only System That Passes All Four Tests

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
| NockVM | Yes | Yes | Partial | Yes | Partial |

Triton VM is the only system that satisfies all four simultaneously, today,
in production. [Neptune Cash](https://neptune.cash/) proves it works.
Trident makes that system programmable by humans, not just assembly experts.

---

## The Vision

**Sovereign, private, provable computation should be accessible to every
developer, not just the three people who can write TASM by hand.**

[Neptune Cash](https://neptune.cash/) proves the architecture works. Miners
generate STARK proofs. Transactions are private. The chain is secured by
hash functions. Recursive verification runs in production. What was missing
was the language.

Trident fills that gap. Deliberately minimal -- no metaprogramming, no
dynamic dispatch, no heap, no recursion -- because every feature that makes
cost prediction harder makes the system less trustworthy. The
[Vyper](https://docs.vyperlang.org/) philosophy: deliberate limitation as a
feature. One obvious way to do everything. What you see is what you prove.

The bet is threefold:

**Quantum computers will break elliptic curves within our professional
lifetimes.** Every SNARK system in production has an expiration date.
Infrastructure built on hash-only STARKs does not need to migrate.

**Privacy is a requirement, not a feature.** When you build tokens, identity,
governance, or any system touching human agency, the default must be private.

**Developer experience determines adoption.** Triton VM is the right
foundation. Raw TASM is the wrong interface. Cairo proved this for StarkWare.
Leo proved this for Aleo. Trident proves this for the only VM that gets all
four properties right.

The token example is 530 lines. The compiler is 16,700 lines of Rust. The
test suite has 388 tests. The standard library has 13 modules. The cost
model tracks 6 tables.

The numbers are small. The foundation is solid. The rest is building.

---

## Further Reading

### Trident Documentation

- [Tutorial](tutorial.md) -- Step-by-step guide from hello world to Merkle proofs
- [Language Specification](spec.md) -- Complete reference for every construct
- [Programming Model](programming-model.md) -- How Triton VM execution works
- [Optimization Guide](optimization.md) -- Cost reduction strategies and table management
- [Error Catalog](errors.md) -- Every error message explained with fixes
- [Comparative Analysis](analysis.md) -- Triton VM vs. every other ZK system
- [Developer Guide](for-developers.md) -- Getting started with Trident
- [Blockchain Developer Guide](for-blockchain-devs.md) -- Trident for Solidity/Cairo developers
- [Fungible Token Example](../examples/fungible_token/) -- Complete private token implementation

### External Resources

- [Triton VM](https://triton-vm.org/) -- The virtual machine and its specification
- [Triton VM ISA Specification](https://triton-vm.org/spec/) -- The TASM instruction set
- [Neptune Cash](https://neptune.cash/) -- Proof-of-Work blockchain running Triton VM in production
- [tasm-lib](https://github.com/TritonVM/tasm-lib) -- Reusable TASM snippets and patterns
- [Tip5 Hash Function](https://eprint.iacr.org/2023/107) -- The algebraic hash (ePrint 2023/107)
- [How STARK Proofs Work](stark-proofs.md) -- From execution traces to quantum-safe proofs
- [FRI Protocol](https://eccc.weizmann.ac.il/report/2017/134/) -- The Fast Reed-Solomon IOP (ECCC 2017/134)
- [Goldilocks Field](https://xn--2-umb.com/22/goldilocks/) -- The base field (p = 2^64 - 2^32 + 1)
- [NIST Post-Quantum Cryptography](https://csrc.nist.gov/Projects/post-quantum-cryptography) -- The standardization effort
- [Shor's Algorithm](https://en.wikipedia.org/wiki/Shor%27s_algorithm) -- Why elliptic curves have an expiration date

---

*Trident is open source. The specification, compiler, examples, and this
document are available at
[github.com/nicktriton/trident](https://github.com/nicktriton/trident).
Contributions welcome.*
