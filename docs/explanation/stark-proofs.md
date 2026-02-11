# How STARK Proofs Work

**From execution traces to quantum-safe cryptographic proofs**

> **Triton VM target.** This document describes the proof system used by the
> Triton VM backend -- Trident's current default compilation target. Other
> backends (future or third-party) may use different proof systems, different
> arithmetizations, or different hash functions. The concepts here -- execution
> traces, AIR constraints, FRI, Fiat-Shamir -- are broadly applicable to all
> STARK systems, but the specific tables, cost numbers, and instruction
> references are Triton VM specific.

This article explains the proof system underlying Trident programs compiled
to the Triton VM target.
It covers the full pipeline: how an execution trace becomes a polynomial,
how polynomials become commitments, how commitments become a proof, and
why that proof is secure against both classical and quantum adversaries.

For a quick overview, see [What Is a STARK?](for-developers.md#8-what-is-a-stark)
in the developer guide.

---

## 1. The Promise

What if you could prove any computation correct without revealing how you
computed it?

Not a toy example. Real computation:

- **A token transfer.** You prove that sender and receiver balances are
  consistent -- that no tokens were created or destroyed -- without revealing
  the amounts, the accounts, or the authorization secret. The verifier sees
  only "this transfer is valid." The ledger stays private.

- **An identity check.** You prove you are authorized to perform an action
  without showing your secret key, your password hash, or your biometric
  data. The verifier learns one bit: authorized or not. Nothing else leaks.

- **A bridge verification.** You prove that an external blockchain reached a
  particular state -- a specific block header, a specific account balance --
  without replaying every transaction in that chain's history. The verifier
  checks a small proof instead of terabytes of block data.

These are not hypothetical. They are the kinds of programs people write in
Trident and prove on Triton VM today.

The fundamental asymmetry makes this practical: the prover does heavy
computational work once -- seconds to minutes depending on program complexity
-- and then anyone can verify the result in milliseconds. The proof is small
(hundreds of kilobytes), self-contained, and can be checked by anyone with no
special knowledge, no secret keys, and no trust in the prover.

This article walks through every layer of the system that makes this possible.
We start with execution traces (Section 2), show how they become polynomials
(Section 3), make this concrete with Triton VM's six tables (Section 4),
explain how the prover commits to polynomials without revealing them
(Section 5), describe the FRI proximity test at the heart of every STARK
(Section 6), remove interaction via Fiat-Shamir (Section 7), assemble the
full pipeline (Section 8), and then explain why this construction needs no
trusted setup (Section 9), resists quantum computers (Section 10), has
predictable cost (Section 11), and supports recursive proof verification
(Section 12).

---

## 2. Proofs of Computation

When Triton VM runs a program, it does not merely produce an output. It
records everything: every instruction executed, every stack state, every
memory access, every hash permutation. This complete record is the
**execution trace**.

A small example. Consider a program that adds 3 and 7:

```
Cycle | Instruction | Stack[0] | Stack[1] | ...
------+-------------+----------+----------+----
  0   | push 3      |    3     |    0     |
  1   | push 7      |    7     |    3     |
  2   | add         |   10     |    0     |
  3   | write_io 1  |    0     |    0     |
```

Four cycles, four rows. Each row records the complete machine state at that
moment: which instruction is executing, what values sit on the stack, what
the memory looks like. The trace is a table -- rows are time steps, columns
are state components.

If the trace is valid -- if every row follows from the previous row according
to the instruction semantics -- then the computation is correct. The `add` at
cycle 2 must produce a Stack[0] value equal to the sum of Stack[0] and
Stack[1] from cycle 1. The `write_io 1` at cycle 3 must consume the top of
stack and emit it as public output. If any row violates these rules, the
computation is wrong.

The trace is a complete certificate of correct execution. But it is enormous
-- potentially millions of rows for real programs. Sending the entire trace
to a verifier would be impractical and would reveal every secret input.

The STARK proof system solves both problems simultaneously. It compresses the
trace into a small proof (hundreds of kilobytes regardless of trace size) and
hides the trace contents (the verifier learns nothing beyond what the program
explicitly outputs).

The starting point for all of this is the **Claim**: a public statement of
what was proved.

```
Claim {
    program_digest: Digest,   // Tip5 hash of the program
    input:  Vec<Field>,       // public inputs consumed by read_io
    output: Vec<Field>,       // public outputs produced by write_io
}
```

The Claim says: "this specific program, given these public inputs, produced
these public outputs." The proof convinces the verifier that a valid execution
trace exists -- without revealing the trace itself.

Cross-reference: [programming-model.md](programming-model.md) for the full
Triton VM execution model, including public vs. secret input.

---

## 3. Arithmetization: From Traces to Polynomials

This is the central insight of STARKs. Everything that follows depends on it.

### Step 1: Columns become polynomials

Take a single column from the execution trace -- say, Stack[0]. It contains
N values, one per cycle:

```
Cycle 0: 3
Cycle 1: 7
Cycle 2: 10
Cycle 3: 0
```

These four values define a unique polynomial of degree at most 3. This is
Lagrange interpolation: any N points determine a unique polynomial curve of
degree N-1, just as 2 points determine a unique line and 3 points determine
a unique parabola. The actual interpolation formula is not important here --
what matters is that the mapping is exact and reversible. Given the polynomial,
you can recover every value in the column. Given the column, you can recover
the polynomial.

Every column in every table becomes a polynomial. The entire execution trace
-- potentially millions of values across dozens of columns -- becomes a
collection of polynomials.

### Step 2: Constraints become polynomial equations

The rules that define valid execution become polynomial equations over these
column polynomials. Consider the `add` instruction: "the value in Stack[0]
at cycle i+1 must equal Stack[0] at cycle i plus Stack[1] at cycle i." In
polynomial form:

```
S0(x_next) = S0(x) + S1(x)     (when the instruction at cycle x is `add`)
```

This is a **transition constraint** -- it relates consecutive rows. There are
also **boundary constraints** (the initial stack must be empty, the final
output must match the Claim) and **consistency constraints** (cross-table
references must agree).

All of these constraints are polynomial equations. If the execution trace is
valid, the constraint polynomials evaluate to zero at every row of the trace.
If any row is invalid, at least one constraint polynomial is nonzero at that
row.

### Step 3: The Algebraic Intermediate Representation (AIR)

The complete set of polynomial constraints that define "valid Triton VM
execution" is called the AIR -- the Algebraic Intermediate Representation.
Triton VM's AIR specifies what it means for a trace to be correct: every
instruction's semantics, every table's consistency rules, every cross-table
lookup.

The AIR is fixed -- it is part of the VM specification, not the program. Any
valid Triton VM program produces a trace that satisfies the same AIR. The
program-specific information is encoded in the trace values, not in the
constraint structure.

### Step 4: The Schwartz-Zippel insight

Here is where the magic happens. Suppose you have a constraint polynomial
C(x) that should be zero at every point in the trace domain (every cycle).
If the trace has N rows, C(x) should have N roots. You can factor out these
roots:

```
C(x) = Q(x) * Z(x)
```

where Z(x) is the "zerofier" polynomial that is zero at every trace row.
If C(x) truly vanishes on the trace domain, then Q(x) = C(x) / Z(x) is
a polynomial (no remainder). If C(x) does NOT vanish at some row -- if the
trace is invalid -- then Q(x) is not a polynomial. It has a pole. It is
rational, not polynomial.

The **Schwartz-Zippel lemma** gives us a way to check this cheaply: two
different polynomials of degree d can agree on at most d points. So if you
evaluate Q at a random point z and it looks like a polynomial evaluation
(consistent with its claimed degree), the probability that Q is actually
non-polynomial is negligible. One random check replaces checking every row.

This is the core of the STARK: convert "check the entire trace" into "check
at a few random points."

```
Execution Trace  -->  Interpolate columns  -->  Polynomial constraints
    (table)             into polynomials          (AIR equations)
                                                       |
                                                       v
                                                Divide by zerofier
                                                to get quotient Q(x)
                                                       |
                                                       v
                                                Evaluate Q at random
                                                point z -- if Q is a
                                                polynomial, constraints
                                                hold everywhere
```

---

## 4. Triton VM's Six Tables

The theory above works for any execution trace. Triton VM organizes its trace
into six specialized tables, each enforcing a different aspect of correct
execution. The proving cost depends on which tables grow tallest.

These numbers come directly from the Trident compiler's cost model
(`src/cost.rs`) and the language specification:

| Table | Triggered by | Rows per trigger | Purpose |
|-------|-------------|-----------------|---------|
| **Processor** | Every instruction | 1 | Enforces instruction semantics: each row encodes one clock cycle |
| **Hash** | `hash`, `sponge_init`, `sponge_absorb`, `sponge_squeeze`, `merkle_step` | 6 | Enforces the [Tip5](https://eprint.iacr.org/2023/107) permutation (5 rounds + 1 setup row) |
| **U32** | `split`, `lt`, `and`, `xor`, `log2`, `pow`, `div_mod`, `popcount`, `merkle_step` | up to 33 | Enforces 32-bit arithmetic via bit decomposition |
| **Op Stack** | Stack-depth-changing instructions | 1 | Enforces operand stack consistency |
| **RAM** | `read_mem`, `write_mem`, `sponge_absorb_mem`, `xx_dot_step`, `xb_dot_step` | 1 per word | Enforces random-access memory consistency |
| **Jump Stack** | `call`, `return`, `recurse`, `recurse_or_return` | 1 | Enforces control flow integrity |

### Cross-table lookups

The tables do not exist in isolation. They reference each other through
polynomial constraints. When the Processor table records "I executed a `hash`
instruction at cycle 17," the Hash table must contain a corresponding
6-row block for that hash operation. When the Processor table records
"I read memory address 42," the RAM table must contain a matching read entry.

These cross-table references are enforced by lookup arguments -- polynomial
protocols that prove one table's values appear in another table. If any
cross-reference is missing or inconsistent, the constraint polynomials will
not vanish, and the proof will fail.

### The dominant table rule

The tallest table determines the **padded height** -- the height rounded up
to the next power of 2. All tables are padded to this same height for the
polynomial arithmetic to work. This means a program's proving cost is
determined entirely by its tallest table.

The `--costs` flag reveals the table profile:

```
$ trident build token.tri --costs

Table heights:
  Processor:     3,847 rows
  Hash:          2,418 rows   <-- dominant
  U32:             312 rows
  Op Stack:      3,847 rows
  RAM:           1,204 rows
  Jump Stack:      186 rows

Padded height:   4,096  (next power of 2 above 3,847)
Dominant table:  hash
Estimated prove: ~1.4s
```

In this example, the Processor and Op Stack tables are both 3,847 rows, but
the padded height is 4,096. If a small code change pushed any table past
4,096, the padded height would jump to 8,192 -- doubling proving time.

Cross-reference: [spec.md](spec.md) Section 12 for the full cost model,
[optimization.md](optimization.md) for strategies to reduce table heights.

---

## 5. Commitment: Hiding the Trace

The prover has an execution trace encoded as polynomials. But sending these
polynomials to the verifier would reveal everything -- every secret input,
every intermediate value. The prover needs to commit to the polynomials
without revealing them.

### Step 1: Extend the evaluation domain

Each trace polynomial has degree roughly N-1, where N is the trace length.
The prover evaluates each polynomial at many more points than N -- typically
4x to 8x more. This "blowup" extends the polynomial evaluations from the
trace domain (N points) to a larger evaluation domain (4N to 8N points).

Why? Because the FRI protocol (Section 6) needs these extra evaluations to
test whether the committed values actually come from a low-degree polynomial.
Evaluating at more points makes cheating detectable.

### Step 2: Build a Merkle tree

The prover organizes all the extended evaluations into a
[Merkle tree](https://en.wikipedia.org/wiki/Merkle_tree). A Merkle tree is a
binary tree where each leaf holds a data value and each internal node holds
the hash of its children. The root -- a single hash digest -- is a short,
fixed-size commitment to the entire dataset.

The key property: the prover can later reveal any individual leaf by providing
a **Merkle authentication path** -- the sequence of sibling hashes from the
leaf to the root. The verifier checks the path by recomputing hashes upward.
If the path is valid, the revealed leaf is guaranteed to be part of the
committed dataset. And revealing one leaf exposes nothing about any other leaf.

### Step 3: Publish the Merkle root

The prover publishes the Merkle root. This single digest (5 field elements
in Triton VM) commits the prover to the entire extended trace. The prover
cannot change any evaluation after publishing the root without producing a
different root -- which would be detected.

### Step 4: Zero-knowledge via randomness

For zero-knowledge -- hiding the trace contents -- the prover adds random
polynomials to the trace columns before committing. These "randomizer
polynomials" mask the actual values while preserving constraint satisfaction.
The constraints still hold (because the random contributions cancel in the
constraint equations), but any individual evaluation reveals only randomness,
not the original trace value.

### The hash function: Tip5

The hash function used for these Merkle trees is
[Tip5](https://eprint.iacr.org/2023/107) -- an algebraic hash designed
specifically for STARK arithmetic over the
[Goldilocks field](https://xn--2-umb.com/22/goldilocks/)
(p = 2^64 - 2^32 + 1). Using the same field for both computation and
commitment eliminates expensive field-to-bytes conversions that would be
needed with a general-purpose hash like SHA-256.

Tip5 operates natively on field elements. A single hash call processes 10
field elements and produces a 5-element Digest. In Triton VM, this costs
1 clock cycle and 6 Hash table rows -- the 5 permutation rounds plus 1
setup row. Compare this to SHA-256 in a RISC-V zkVM, which costs thousands
of cycles for the same operation.

Cross-reference: [for-developers.md](for-developers.md) Section 7 for
Merkle tree basics.

---

## 6. FRI: The Heart of the STARK

This is the hardest and most important section. Everything above builds
toward it.

### The problem

The verifier has Merkle commitments to polynomial evaluations. It needs to
confirm that these evaluations actually come from a low-degree polynomial --
not from arbitrary data that happens to satisfy constraints at a few queried
points.

This is the **low-degree test**, also called a **proximity test**: are the
committed values close to a polynomial of the expected degree? If yes, the
arithmetization from Section 3 guarantees the trace is valid. If no, the
prover is cheating.

FRI -- Fast Reed-Solomon Interactive Oracle Proof of Proximity -- solves
this problem using only field arithmetic and hashing. No elliptic curves.
No pairings. No discrete logarithm assumptions.

### The folding trick

The core idea of FRI is elegant. Given a polynomial f(x) of degree at most d:

1. **Split** f(x) into its even and odd parts:
   f(x) = g(x^2) + x * h(x^2), where g and h each have degree at most d/2.
   This decomposition is always possible and unique.

2. **Challenge.** The verifier sends a random field element alpha.

3. **Fold.** The prover computes a new polynomial:
   f'(x) = g(x) + alpha * h(x). This polynomial has degree at most d/2 --
   half the original degree.

4. **Commit.** The prover commits to the evaluations of f' via a new Merkle
   tree and publishes the root.

5. **Repeat.** Apply the same fold to f', halving the degree again. After
   log2(d) rounds, the polynomial is a constant.

```
Round 0: f(x)     degree <= d        committed via Merkle tree
            | fold with alpha_0
Round 1: f1(x)    degree <= d/2      committed via Merkle tree
            | fold with alpha_1
Round 2: f2(x)    degree <= d/4      committed via Merkle tree
            | fold with alpha_2
  ...         ...
Round k: f_k      constant           sent directly
```

After all rounds are committed, the verifier picks random **query positions**
and checks consistency between adjacent rounds. For each query position, the
verifier requests Merkle authentication paths for the corresponding
evaluations in round i and round i+1, then checks that the folding relation
holds: f_{i+1}(x) = g_i(x) + alpha_i * h_i(x).

### Why this works

If the original committed values were close to a degree-d polynomial, then
each folding round produces values close to a polynomial of half the degree.
The final constant is consistent with all prior rounds.

If the original values were NOT close to a low-degree polynomial -- if the
prover committed to garbage -- then the folding rounds will be inconsistent.
The intermediate polynomials will not agree with each other at the queried
positions. The probability of cheating decreases exponentially with the
number of queries. With 80 queries, the probability of a successful cheat
is less than 2^(-80).

The beauty of FRI is that the verifier does very little work: it checks
a logarithmic number of rounds, each requiring a constant number of Merkle
path verifications and field operations. The total verification cost is
O(log^2(d)) -- polylogarithmic in the degree of the original polynomial.

### Why no elliptic curves

FRI uses exactly two cryptographic primitives:

1. **Field arithmetic** -- polynomial evaluation, addition, multiplication
   over the Goldilocks field. These are ordinary 64-bit integer operations
   modulo p = 2^64 - 2^32 + 1.

2. **Hashing** -- Merkle tree construction and authentication path
   verification using Tip5.

No groups. No pairings. No discrete logarithm. No elliptic curves of any
kind. This is what makes STARKs fundamentally different from SNARKs -- and
what makes them quantum-safe (Section 10).

Reference: [Fast Reed-Solomon IOP (Ben-Sasson et al., ECCC 2017/134)](https://eccc.weizmann.ac.il/report/2017/134/).

---

## 7. The Fiat-Shamir Transform

The protocol described above is interactive. The verifier sends random
challenges (the alpha values in FRI, the random evaluation point z for
constraint checking), and the prover responds. This back-and-forth requires
both parties to be online simultaneously.

A real proof system must be non-interactive: the prover produces a proof
document, and anyone can verify it later, offline, without communicating
with the prover.

The [Fiat-Shamir heuristic](https://en.wikipedia.org/wiki/Fiat%E2%80%93Shamir_heuristic)
achieves this transformation. The idea: replace every verifier challenge
with the hash of the entire transcript so far. Since the hash is
deterministic (same input always produces same output) and unpredictable
(the prover cannot control the output), it simulates a random verifier.

Concretely: after the prover publishes a Merkle commitment, the next
challenge is computed as `alpha = Tip5(transcript || commitment)`. The
prover cannot manipulate alpha because it depends on the commitment, which
is already fixed. The verifier can recompute the same alpha from the same
transcript and check consistency.

### Tip5's triple role

In Triton VM, the Tip5 hash function serves three distinct purposes:

1. **Data hashing** inside programs -- the `hash` instruction, `sponge_*`
   operations, and `merkle_step` all invoke Tip5.

2. **Merkle commitments** for trace polynomial evaluations -- the Merkle
   trees that commit the prover to the extended trace are built with Tip5.

3. **Challenge generation** via Fiat-Shamir -- the random challenges that
   drive FRI folding and constraint evaluation are derived from Tip5 hashes
   of the proof transcript.

Using one algebraic hash for all three roles means that Triton VM's execution
and proof generation operate entirely within the same field. No expensive
field-to-bytes conversions. No SHA-256 calls. No context-switching between
different algebraic structures.

This uniformity is what makes recursive verification practical: a Triton VM
program that verifies a STARK proof performs the same Tip5 operations that
the external prover used. The hash calls are native VM instructions, not
software emulations. This is explored further in Section 12.

---

## 8. Putting It All Together

The full pipeline from source code to verified proof:

```
 Trident source (.tri)
        | compile
        v
 TASM assembly (.tasm)
        | execute in Triton VM
        v
 Execution Trace (6 tables)
        | interpolate columns
        v
 Trace Polynomials
        | extend domain + commit via Merkle trees
        v
 Merkle Commitments (Tip5)
        | evaluate constraints, divide by zerofier
        v
 Quotient Polynomials
        | FRI proximity proof (log2(d) folding rounds)
        v
 STARK Proof
        | verify (anyone, anytime, milliseconds)
        v
 Accept / Reject
```

### What the verifier actually checks

The verifier receives the Claim (program hash, public inputs, public outputs)
and the Proof (Merkle roots, FRI commitments, authentication paths, queried
evaluations). It performs four categories of checks:

1. **Merkle root integrity.** The committed Merkle roots are well-formed.
   Every authentication path provided by the prover hashes correctly to the
   claimed root.

2. **AIR satisfaction.** At every queried evaluation point, the constraint
   polynomials (divided by the zerofier) evaluate to values consistent with
   a low-degree quotient polynomial. This confirms the execution trace
   satisfies the AIR -- the transition, boundary, and consistency constraints
   all hold.

3. **FRI consistency.** Across all FRI folding rounds, the evaluations are
   consistent with the folding relation. Each round's committed polynomial
   is the correct fold of the previous round's polynomial at the challenged
   alpha value. This confirms the quotient polynomial is actually low-degree.

4. **Claim binding.** The public inputs and outputs in the Claim match the
   boundary constraints in the trace. The program digest matches the attested
   program hash. The Fiat-Shamir challenges are correctly derived from the
   transcript.

### Concrete numbers

- **Proof size**: typically 100-200 KB. Larger than pairing-based SNARKs
  (~200 bytes for Groth16), but requires no trusted setup and no elliptic
  curves.

- **Verification time**: milliseconds, regardless of the original
  computation's complexity. A program that runs for a billion cycles produces
  a proof that verifies in the same time as a program that runs for a
  thousand cycles.

- **Prover time**: depends on padded height. See Section 11.

Cross-reference: [for-developers.md](for-developers.md) Section 9 for the
CLI walkthrough of build, prove, and verify.

---

## 9. Why No Trusted Setup?

Many SNARK systems -- Groth16, PLONK with KZG commitments, Marlin -- require
a **trusted setup ceremony** before they can be used:

1. Generate a secret value tau (the "toxic waste").
2. Compute powers of tau encrypted on an elliptic curve:
   [tau], [tau^2], [tau^3], ... These become the proving and verification keys.
3. Distribute the keys to provers and verifiers.
4. Destroy tau. If anyone retains tau, they can forge proofs for any
   statement -- and the forgeries are indistinguishable from valid proofs.

This is the "powers of tau ceremony." Real ceremonies involve hundreds of
participants, each contributing randomness, with the guarantee that the setup
is secure as long as at least one participant was honest and destroyed their
contribution. Aleo's ceremony had 2,200+ participants. Zcash's had hundreds.
They work -- but they introduce a category of risk that does not exist in
hash-based systems.

If the ceremony is compromised -- if all participants colluded, or if the
implementation had a bug that leaked tau -- the entire system is broken
silently. Forged proofs look identical to valid proofs. There is no way to
detect the compromise after the fact.

| Property | STARK (FRI) | SNARK (KZG) |
|----------|-------------|-------------|
| Setup required | No | Yes (ceremony) |
| Secret parameters | None | tau ("toxic waste") |
| If setup compromised | N/A | Undetectable forgery |
| "Transparent" | Yes | No |
| Quantum-safe | Yes | No |

FRI avoids this entirely. The verifier's challenges come from hashing the
proof transcript (Fiat-Shamir), not from pre-generated parameters. There is
no secret. There is no ceremony. There is nothing to compromise.

The word "transparent" in STARK -- **S**calable **T**ransparent **AR**gument
of **K**nowledge -- means exactly this: no hidden parameters. Anyone can
verify the proof system's integrity by reading the specification. There are
no secrets embedded in the verification key, no trust assumptions beyond the
hash function's collision resistance.

For infrastructure intended to last decades, this distinction matters. A
trusted setup is a single point of failure that cannot be audited after the
fact. A transparent proof system has no such failure mode.

Cross-reference: [analysis.md](analysis.md) for the full comparison of all
ZK proof systems and their setup requirements.

---

## 10. Why Quantum-Safe?

[Shor's algorithm](https://en.wikipedia.org/wiki/Shor%27s_algorithm) solves
the discrete logarithm problem in polynomial time on a quantum computer. A
sufficiently powerful quantum computer running Shor's algorithm would break:

- **ECDSA** -- transaction signatures on Bitcoin, Ethereum, and every major
  blockchain
- **BN254, BLS12-381** -- the elliptic curves used by pairing-based SNARKs
  (Groth16, KZG commitments)
- **Pasta curves (Pallas/Vesta)** -- used by Mina and Aleo
- **All pairing-based polynomial commitments** -- PLONK, Groth16, Marlin
  verification

The break is total and retroactive. Every Groth16 proof ever generated
becomes forgeable. Every KZG polynomial commitment becomes extractable.

STARK security rests on two assumptions, neither of which involves discrete
logarithm:

1. **Hash collision resistance.** Finding x != y where H(x) = H(y) is
   computationally hard. This is the security foundation of Merkle trees
   and the Fiat-Shamir transform.

2. **FRI soundness.** The proximity test correctly rejects data that is far
   from any low-degree polynomial. This is a combinatorial property of
   Reed-Solomon codes, not a number-theoretic assumption.

[Grover's algorithm](https://en.wikipedia.org/wiki/Grover%27s_algorithm)
-- the other major quantum algorithm -- gives a quadratic speedup for
unstructured search, which applies to hash collision finding. It reduces
2^256 security to 2^128. The standard defense: use hash outputs large enough
that even the Grover-reduced security level is sufficient. Tip5's 5-element
Digest provides 320 bits of output, giving 160 bits of post-quantum collision
resistance -- well above the 128-bit security target.

No migration is needed. [NIST](https://csrc.nist.gov/Projects/post-quantum-cryptography)
has standardized post-quantum algorithms precisely because the threat is real
and the timeline uncertain. STARK proofs are already post-quantum by
construction. The proofs generated today will remain secure against quantum
computers whenever they arrive.

| System | Prover Quantum-Safe | Verifier Quantum-Safe | Migration Path |
|--------|:---:|:---:|---|
| StarkWare/Stwo | Yes (Circle STARKs) | Yes (native STARK) | None needed |
| SP1 | Yes (FRI) | **No** (Groth16/BN254) | Fundamental redesign |
| RISC Zero | Yes (0STARK) | **No** (Groth16/BN254) | Fundamental redesign |
| Aleo | **No** (Pasta curves) | **No** (Pasta curves) | Complete crypto migration |
| Mina | **No** (Pasta curves) | **No** (Pasta curves) | Complete crypto migration |
| **Triton VM** | **Yes** (FRI + Tip5) | **Yes** (native STARK) | **None needed** |

Cross-reference: [vision.md](vision.md) "Quantum Safety Is Not Optional" for
the full argument and comparison table.

---

## 11. Performance: What Does Proving Cost?

Proving time is not mysterious. It follows directly from the trace structure
described in Sections 3-4. The Trident compiler computes it statically from
the source code.

### The formula

From the compiler's cost model (`src/cost.rs`):

```
proving_time = padded_height * 300 * log2(padded_height) * 3ns
```

Where:

- **padded_height** = the next power of 2 above the tallest table's row count.
  This is the single most important number in the cost model.

- **300** = approximate number of columns across all constraint polynomials.
  This is fixed by Triton VM's arithmetization -- it does not depend on the
  program.

- **log2(padded_height)** = the number of FRI folding rounds. Each round
  halves the polynomial degree, so the total number of rounds is logarithmic
  in the padded height.

- **3ns** = approximate time per field operation on reference hardware
  (~1-5 ns per 64-bit field operation depending on CPU).

### The power-of-2 cliff

Because padded height must be a power of 2, small changes in trace height
can cause dramatic cost jumps:

```
Height: 1,024 rows  -->  Padded: 1,024  -->  Proving: ~0.03s
Height: 1,025 rows  -->  Padded: 2,048  -->  Proving: ~0.07s  (doubled!)
Height: 2,048 rows  -->  Padded: 2,048  -->  Proving: ~0.07s
Height: 2,049 rows  -->  Padded: 4,096  -->  Proving: ~0.14s  (doubled again!)
```

One extra row can double the proving time. This is because FRI operates on
evaluation domains that must be powers of 2 -- a fundamental requirement of
the number-theoretic transform used for polynomial arithmetic.

The compiler warns when a program is near a boundary:

```
warning[W0017]: program is 3 rows below padded height boundary
  --> main.tri
   = note: padded_height = 1024 (max table height = 1021)
   = note: adding 4+ rows to any table will double proving cost to 2048
```

### Why the Hash table dominates

Each Tip5 operation adds 6 Hash table rows but only 1 Processor row. A
program with 500 hash calls generates 3,000 Hash table rows but only 500
Processor rows from those same calls. The Hash table grows 6x faster than
the Processor table for hash-heavy code.

This is why hashing is the most expensive operation in Triton VM -- not in
clock cycles (it costs 1 cycle), but in its coprocessor table impact. The
`--costs` and related flags make this visible:

```bash
trident build token.tri --costs      # Full table breakdown
trident build token.tri --hotspots   # Top 5 most expensive functions
trident build token.tri --hints      # Optimization suggestions
trident build token.tri --annotate   # Per-line cost annotations
```

### Program attestation overhead

The program itself must be hashed for integrity -- the `program_digest` in
the Claim is the Tip5 hash of the entire instruction sequence. This
attestation costs additional Hash table rows:

```
attestation_hash_rows = ceil(instruction_count / 10) * 6
```

A 1,000-instruction program adds ceil(1000/10) * 6 = 600 Hash table rows
just for attestation, before any program logic executes. This overhead is
included automatically in the compiler's cost estimates.

### Hash performance across systems

The cost of a single hash operation varies dramatically across ZK systems:

| System | Hash function | Cost per hash | Relative cost |
|--------|--------------|---------------|:---:|
| Triton VM | Tip5 (native) | 1 cc + 6 hash rows | **1x** |
| StarkWare | Poseidon (native) | ~5-10 cc | ~5-10x |
| SP1 | SHA-256 (software) | ~3,000+ cc | ~3,000x |
| RISC Zero | SHA-256 (accelerated) | ~1,000 cc | ~1,000x |

For hash-heavy workloads -- Merkle tree operations, sponge hashing, content
addressing -- this difference dominates total proving cost.

Cross-reference: [spec.md](spec.md) Section 12 for the complete cost model,
[optimization.md](optimization.md) for cost reduction strategies.

---

## 12. Recursive Verification

A STARK proof can verify another STARK proof inside its own execution. The
inner proof's verification becomes part of the outer proof's execution trace,
which is then proved by the outer STARK. Proofs about proofs.

### Why Triton VM excels at this

The FRI verifier's inner loop performs many extension-field dot products:
accumulating weighted sums of polynomial evaluations over the cubic extension
of the Goldilocks field. Triton VM provides native instructions for exactly
these operations:

```
// Extension field dot product from RAM (1 clock cycle, 6 RAM rows)
fn xx_dot_step(acc: XField, ptr_a: Field, ptr_b: Field)
    -> (XField, Field, Field)

// Mixed base/extension field dot product from RAM (1 clock cycle, 4 RAM rows)
fn xb_dot_step(acc: XField, ptr_a: Field, ptr_b: Field)
    -> (XField, Field, Field)
```

In a RISC-V based zkVM (SP1, RISC Zero), each extension-field multiplication
decomposes into dozens of base-field multiplications and additions, each of
which is a separate RISC-V instruction adding rows to the execution trace.
A single `xx_dot_step` in Triton VM replaces hundreds of RISC-V instructions.

The recursive verification cost in Triton VM is approximately **300,000
clock cycles** regardless of the inner proof's original computation
complexity. In SP1 or RISC Zero, the same verification costs millions of
cycles -- an order of magnitude more. This difference comes directly from
the native dot-product instructions and the algebraic hash (Tip5 verification
inside Tip5-based proofs, with no field conversions).

A structural sketch of the recursive verifier in Trident:

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

The verifier reads proof components from non-deterministic (secret) input,
authenticates Merkle commitments using the VM's native `merkle_step`
instruction, and checks FRI proximity using `xx_dot_step`. Neptune Cash has
a working recursive verifier running in production today.

### Applications

- **Proof aggregation.** Combine N individual transaction proofs into one
  constant-size proof. A rollup batches thousands of state transitions and
  proves them all with a single outer proof.

- **Rollup compression.** Prove a batch of state transitions in a single
  proof. The batch proof is the same size regardless of how many transitions
  it contains.

- **Cross-chain bridges.** Verify another chain's proof without replaying
  its history. A Triton VM program reads the external proof from secret
  input, verifies it, and outputs only "valid" or "invalid."

- **Incrementally verifiable computation.** Chain proofs to prove long-running
  computations. Each step proves "I verified the previous step's proof AND
  computed the next increment." The chain can grow indefinitely without any
  single proof growing larger.

Cross-reference: [vision.md](vision.md) "Recursive STARK Verification",
[spec.md](spec.md) Sections 8.4 and 13.4.

---

## 13. Further Reading

### Academic papers

- [Scalable, transparent, and post-quantum computational integrity (Ben-Sasson et al., 2018)](https://eprint.iacr.org/2018/046)
  -- The original STARK paper. Defines the proof system, proves soundness,
  and establishes the "transparent" (no trusted setup) property.

- [Fast Reed-Solomon IOP (Ben-Sasson et al., 2017)](https://eccc.weizmann.ac.il/report/2017/134/)
  -- The FRI protocol. The low-degree proximity test that replaces elliptic
  curve pairings in STARKs.

- [The Tip5 hash function (Aly et al., 2023)](https://eprint.iacr.org/2023/107)
  -- The algebraic hash function designed for STARK arithmetic. Used by
  Triton VM for data hashing, Merkle commitments, and Fiat-Shamir challenges.

- [Goldilocks field (2-adic primes)](https://xn--2-umb.com/22/goldilocks/)
  -- Why p = 2^64 - 2^32 + 1 is ideal for STARK arithmetic: it fits in 64
  bits and has a large power-of-2 subgroup for NTT-based polynomial
  multiplication.

### Accessible introductions

- [Vitalik Buterin's STARK series](https://vitalik.eth.limo/general/2017/11/09/starks_part_1.html)
  -- A multi-part blog walkthrough starting from first principles. Excellent
  for building intuition about polynomial commitments and FRI.

- [STARK anatomy (Aszepieniec)](https://aszepieniec.github.io/stark-anatomy/)
  -- A tutorial implementation of a STARK prover and verifier from scratch.
  The best resource for understanding the code-level details.

### Triton VM and Trident documentation

- [Tutorial](tutorial.md) -- Step-by-step guide from hello world to Merkle
  proofs, cost analysis, and inline assembly.

- [Language Reference](reference.md) -- Quick lookup: cost-per-instruction
  table (Section 7) maps every construct to its table impact.

- [Language Specification](spec.md) -- Section 12 covers cost computation in
  full detail. Section 8.4 covers extension-field dot products. Section 13.4
  gives the recursive verifier sketch.

- [Programming Model](programming-model.md) -- How Trident programs execute
  inside Triton VM. The Claim/Proof structure, public vs. secret input, and
  the divine-and-authenticate pattern.

- [Optimization Guide](optimization.md) -- Strategies for reducing table
  heights and avoiding power-of-2 cliffs. Essential reading after
  understanding the cost model.

- [Error Catalog](errors.md) -- All compiler error messages explained, with
  links back to relevant concepts.

- [Comparative Analysis](analysis.md) -- Triton VM compared to every other
  ZK system: StarkWare, SP1, RISC Zero, Aleo, Mina, NockVM. Covers quantum
  safety, privacy, performance, and ecosystem strength.

- [For Developers](for-developers.md) -- The beginner-friendly introduction.
  Section 8 gives a one-page STARK overview. Section 9 walks through the
  build-prove-verify CLI workflow.

- [For Blockchain Devs](for-blockchain-devs.md) -- Mental model migration
  from Solidity/Anchor/CosmWasm. See "Where's My Gas?" for the cost model
  from a smart contract perspective.

- [Vision](vision.md) -- Why STARK properties -- transparency, quantum
  safety, recursive verification -- matter for real infrastructure intended
  to last decades.

- [Generating Proofs](generating-proofs.md) -- Practical guide: execution
  trace to STARK proof, cost optimization, recursive composition.

- [Verifying Proofs](verifying-proofs.md) -- Proof checking, on-chain
  verification, quantum safety properties.

- [Formal Verification](formal-verification.md) -- Symbolic verification
  of program properties before proving.

- [Triton VM specification](https://triton-vm.org/spec/) -- The target VM's
  instruction set, table structure, and constraint system.
