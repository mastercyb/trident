# Trident for Developers

A guide to Trident and zero-knowledge programming for developers coming from
Rust, Python, Go, JavaScript, C++, or any conventional language. No prior
knowledge of cryptography, ZK proofs, or field arithmetic is assumed.

Trident is a universal language for provable computation. It currently targets
Triton VM by default, with a multi-target architecture designed to compile to
any zkVM (Miden, Cairo, SP1/RISC Zero, and others) from a single source.

---

## 1. What Is a Zero-Knowledge Proof?

A zero-knowledge proof lets you convince someone that a statement is true
without revealing *why* it is true. The classic analogy: imagine you have a
colorblind friend and two balls -- one red, one green. You want to prove the
balls are different colors without telling your friend which is which. You hand
them both balls behind their back, they randomly swap (or not), and show them
to you. You can always tell whether they swapped. After enough rounds, your
friend is convinced the balls are different colors -- but still has no idea
which one is red.

In programming terms: a prover runs a computation with some secret inputs and
produces a small certificate (the "proof"). Anyone can check that certificate
in milliseconds and be convinced the computation was done correctly -- without
learning what the secret inputs were. Trident is the language you use to write
that computation. Because Trident targets multiple zkVMs, the same source can
be compiled and proved on whichever backend suits your deployment.

---

## 2. What Is a Field Element?

In normal programming, integers are 32-bit or 64-bit values that overflow
silently or throw exceptions. In Trident, the basic numeric type is a **field
element** -- an integer that wraps around at a specific prime number instead of
at a power of two.

Each target VM uses its own prime. Triton VM (the default target) uses
`p = 2^64 - 2^32 + 1`, known as the
[Goldilocks prime](https://xn--2-umb.com/22/goldilocks/). Think of it as a
64-bit integer where arithmetic wraps at `p` instead of at `2^64`:

```
0 + 1       = 1                  (same as normal)
5 * 3       = 15                 (same as normal)
0 - 1       = 18446744069414584320   (wraps to p - 1, not -1)
p - 1 + 1   = 0                  (wraps back around)
```

Why a prime? Because prime fields have a mathematical property that makes them
ideal for proof systems: every nonzero element has a multiplicative inverse.
You can always "divide" (multiply by the inverse), which is essential for the
polynomial math underlying STARK proofs. Powers of two do not have this
property -- even numbers have no inverse mod `2^64`.

In practice, you can mostly treat `Field` values like normal integers for
addition and multiplication. The key differences:

- There is no subtraction operator. `sub(a, b)` computes `a + (p - b)`, which
  is the field-arithmetic equivalent. Trident makes this explicit to prevent
  surprises.
- There are no negative numbers. What you think of as `-1` is actually
  `p - 1`.
- Comparison operators like `<` and `>` require converting to `U32` first,
  because "less than" is not meaningful in a prime field (is `p - 1` less than
  `2`?).

If all of this feels strange: it is. But after a few programs, it becomes
natural. Think of `Field` as "an integer that wraps at a big prime" and you
will be fine.

---

## 3. Why Does Every Loop Need a Bound?

In conventional programming, you write `while` loops that run until some
condition is met. The runtime figures out how many iterations actually happen.
Trident does not allow this.

Every loop in Trident requires an explicit maximum bound:

```
for i in 0..n bounded 100 {
    process(i)
}
```

The reason is the **execution trace**. When a zkVM runs your program, it
records every single instruction executed -- every addition, every comparison,
every stack operation -- into a giant table called the execution trace. This
trace is what gets turned into a proof.

The proof system needs to know the size of this trace *before* execution. It
allocates memory, sets up polynomial commitments, and prepares the algebraic
machinery based on that size. An unbounded loop means an unpredictable trace
size, which means the proof system cannot be set up.

Think of it this way: in a normal program, you pay for computation with CPU
time, and you find out the cost after the program finishes. In a ZK program,
the cost must be known (or at least bounded) before execution starts. The bound
is the price tag. `bounded 100` means "this loop will generate at most 100
iterations' worth of trace rows, and the proof system will allocate
accordingly."

If the actual runtime value of `n` turns out to be 50, the trace still has
room for 100 iterations. The unused rows are padded. This is a deliberate
trade-off: you pay for the worst case in exchange for deterministic,
predictable proving cost.

---

## 4. Why No Recursion?

Recursion is just an unbounded loop in disguise. A recursive function can call
itself to arbitrary depth depending on runtime inputs. The compiler cannot
predict the trace length, so it cannot set up the proof system.

```
// This is NOT allowed in Trident:
fn factorial(n: Field) -> Field {
    if n == 0 { return 1 }
    n * factorial(n - 1)    // ERROR: recursion detected
}
```

The fix is always a bounded loop:

```
fn factorial(n: Field) -> Field {
    let mut result: Field = 1
    for i in 1..n bounded 20 {
        result = result * i
    }
    result
}
```

This is a hard constraint of the proof system, not a limitation of the
compiler. Bounded loops are the universal replacement. If you find yourself
wanting recursion, ask: "What is the maximum depth this could reach?" That
depth becomes your bound.

---

## 5. Why No Heap?

Trident has no `malloc`, no `free`, no garbage collector, no dynamically-sized
data structures. Every piece of data has a size known at compile time.

This is because the target VMs (such as Triton VM) use **stack machines with
fixed-size RAM**. The execution model has:

- An **operand stack** (16 elements directly accessible, with automatic spill
  to memory for deeper values)
- **RAM** (word-addressed, each cell holds one field element)
- No heap allocator, no pointer arithmetic, no dynamic dispatch

All variables in Trident are either on the stack or at fixed RAM addresses. The
compiler decides which. Arrays have compile-time-known lengths. Structs have
compile-time-known field layouts.

Why? The same reason as bounded loops: determinism. Dynamic memory allocation
means unpredictable memory access patterns, which means unpredictable trace
growth in the RAM table. The proof system needs to know the RAM table size in
advance.

What you give up: `Vec`, `HashMap`, `String`, trees, linked lists, and every
other dynamically-sized data structure you are used to.

What you get: guaranteed deterministic memory usage, zero memory bugs, and
exact cost predictions.

In practice, most ZK programs are surprisingly small. You are not building a
web server. You are proving a specific computation -- hashing some data,
verifying a Merkle proof, checking that token balances add up. Fixed-size
arrays and structs cover nearly all of it.

---

## 6. What Is "Divine"?

This is the concept that trips up most newcomers. In a normal program, all
inputs come from the same place -- the user, a file, a network socket. In
Trident, there are two kinds of input:

- **Public input** (`pub_read`): visible to both the prover and the verifier.
  The verifier sees these values and can check them.
- **Secret input** (`divine`): visible only to the prover. The verifier never
  sees these values.

The word "divine" comes from "divination" -- the prover conjures a value out of
thin air. The program then *checks* that the value is correct using assertions.

The analogy: imagine you are taking a math exam. The teacher (verifier) gives
you a problem (public input). You are allowed to whisper the answer to yourself
(divine the value), but you still have to show all your work to prove the
answer is correct. The teacher checks your work, not the whisper.

Here is a concrete example:

```
fn prove_knows_square_root(x: Field) {
    let s: Field = divine()       // prover whispers: "the answer is 7"
    assert(s * s == x)            // program proves: 7 * 7 = 49 = x
}
```

The verifier sees `x` (say, 49) and the proof. The verifier does NOT see `s`
(7). But the proof guarantees that the prover knew some `s` where `s * s == x`.

This pattern -- divine a value, then constrain it with assertions -- is the
foundation of all ZK programming. The prover does the expensive work of finding
the answer. The program does the cheap work of checking it. If any assertion
fails, no proof is generated, and the computation is rejected.

The divine-and-verify pattern shows up everywhere:

- **Merkle proofs**: divine the sibling hashes, verify the path hashes up to
  the root
- **Square roots**: divine the root, verify `root * root == input`
- **Preimage checks**: divine the preimage, verify `hash(preimage) ==
  known_hash`
- **Transaction validation**: divine the transaction details, verify the
  balances add up

---

## 7. What Is a Merkle Tree and Why Does It Matter?

A Merkle tree is a data structure where a single hash (the "root") represents
an entire collection of data. You can prove that a specific piece of data is in
the collection by providing a short path of hashes, without revealing anything
else in the collection.

Here is what it looks like:

```
                    Root
                   /    \
                  /      \
               H(AB)    H(CD)
               /  \      /  \
              /    \    /    \
             A      B  C      D
```

Each leaf (A, B, C, D) is a piece of data. Each internal node is the hash of
its two children: `H(AB) = hash(A, B)`, and so on up to the root. The root is
a single hash that uniquely represents the entire tree.

To prove that leaf B is in the tree, you provide:

1. B itself
2. A (the sibling)
3. H(CD) (the uncle)

The verifier computes: `hash(A, B) -> H(AB)`, then `hash(H(AB), H(CD)) ->
Root`, and checks that the result matches the known root. This is called a
**Merkle proof** or **authentication path**.

Why does this matter for Trident?

In zero-knowledge systems, you often need to prove things about large datasets
(account balances, UTXO sets, transaction histories) without revealing the
entire dataset. A Merkle tree lets you commit to the whole dataset with one
hash (the root), then selectively prove individual entries.

Triton VM has native instructions for Merkle tree operations (`merkle_step`),
making them extremely efficient -- one hash instruction per tree level. In
Trident:

```
use std.crypto.merkle

fn verify_membership(root: Digest, leaf: Digest, index: U32, depth: U32) {
    std.crypto.merkle.verify(root, leaf, index, depth)
}
```

Under the hood, this divines the sibling hashes from secret input and walks up
the tree level by level, checking each hash. The verifier only sees the root
(public input) and the proof. The leaf, the siblings, and the rest of the tree
remain hidden.

---

## 8. What Is a STARK?

STARK stands for **Scalable Transparent ARgument of Knowledge**. It is the
proof system that Triton VM uses. Here is what each word means in practical
terms:

- **Scalable**: the proof is small and fast to verify, even for enormous
  computations. A computation that takes minutes to run produces a proof that
  takes milliseconds to check.
- **Transparent**: no trusted setup. Some older proof systems (SNARKs) require
  a one-time ceremony where secret parameters are generated and then
  destroyed. If the ceremony is compromised, the entire system is broken.
  STARKs do not need this -- they rely only on hash functions.
- **ARgument of Knowledge**: the proof demonstrates that the prover actually
  performed the computation, not just that the answer is correct.

Think of a STARK as a compression algorithm for computation. The prover runs
the full program, records every step in the execution trace, and then
compresses that trace into a small proof using polynomial math and hash
functions. The verifier decompresses just enough to be convinced the trace was
valid.

Key properties that matter for you as a developer:

- **No trusted setup**: you do not need to trust anyone to set up the system.
  Deploy and verify with no ceremony.
- **Hash-based security**: STARKs use only hash functions (specifically Tip5 in
  Triton VM), not elliptic curve cryptography. This makes them resistant to
  quantum computers, which can break elliptic curve systems.
- **Post-quantum safe**: when (if) quantum computers become practical, STARK
  proofs remain secure. Elliptic-curve-based SNARKs do not.
- **Proof size**: STARK proofs are larger than SNARK proofs (kilobytes vs.
  hundreds of bytes), but this is usually an acceptable trade-off for the
  security and simplicity benefits.

For the full technical picture -- arithmetization, polynomial commitments,
FRI folding, and how it all connects to Triton VM's six execution tables --
see [How STARK Proofs Work](stark-proofs.md).

---

## 9. What Happens When You Build, Prove, and Verify?

Here is the full lifecycle of a Trident program, step by step.

### Step 1: Write your program

```
program balance_check

fn main() {
    let declared_balance: Field = pub_read()
    let secret_a: Field = divine()
    let secret_b: Field = divine()
    assert(secret_a + secret_b == declared_balance)
    pub_write(1)
}
```

This program proves: "I know two secret values that add up to the declared
balance."

### Step 2: Compile to TASM

```bash
trident build balance_check.tri -o balance_check.tasm
trident build balance_check.tri -o balance_check.tasm --target triton   # Explicit target (default)
```

The `--target` flag selects which backend the compiler emits code for. The
default is `triton`. When targeting Triton VM, the compiler translates your
Trident source directly into TASM (Triton Assembly) -- the instruction set of
Triton VM. There is no intermediate representation. Each Trident construct maps
predictably to specific TASM instructions. The output file is human-readable
assembly. Other targets (e.g., `--target miden`) emit the corresponding VM's
assembly from the same source.

### Step 3: The prover executes the program

The prover runs the TASM program inside Triton VM with:

- **Public input**: the declared balance (say, `100`)
- **Secret input**: the two secret values (say, `37` and `63`)

The VM executes every instruction and records the full **execution trace** --
every stack state, every memory access, every hash operation. This trace is a
large table (potentially millions of rows).

### Step 4: The STARK prover compresses the trace into a proof

The prover takes the execution trace and uses the STARK protocol (polynomial
commitments, FRI folding, Fiat-Shamir hashing) to compress it into a compact
proof. This is the most computationally expensive step -- it can take seconds
to minutes depending on the trace size.

The proof contains:

- Commitments to the trace polynomials
- Query responses at random evaluation points
- FRI proximity proofs

The proof does NOT contain the secret inputs, the execution trace, or any
intermediate values.

### Step 5: Anyone verifies the proof

The verifier receives:

- The **Claim**: program hash, public input (`100`), public output (`1`)
- The **Proof**: the STARK proof from step 4

The verifier runs a fast algorithm that checks the proof against the claim. It
does not re-execute the program. It checks mathematical relationships between
the commitments and queries. This takes milliseconds.

If the check passes: the verifier is convinced that some prover ran the program
correctly with inputs that produced the claimed output -- without knowing what
the secret inputs were.

If the check fails: the proof is rejected. Either the prover made an error, or
the program's assertions failed.

### The CLI commands in sequence

```bash
# 1. Compile (default target: triton)
trident build balance_check.tri -o balance_check.tasm

# 1b. Compile for a different target
trident build balance_check.tri --target miden -o balance_check.masm

# 2. See what it will cost to prove
trident build balance_check.tri --costs
trident build balance_check.tri --target miden --costs

# 3. The proving and verification steps happen in the target VM's runtime,
#    outside of the Trident compiler. Trident's job ends at producing
#    the assembly file. The VM toolchain (e.g., Triton VM Rust library)
#    handles execution, proof generation, and verification.
```

---

## 10. The Mental Model: Compute Expensive, Verify Cheap

This is the single most important idea in zero-knowledge programming. Once you
internalize it, everything else follows.

**The prover does the heavy lifting once. Anyone can verify instantly.**

In conventional programming, if you want to convince someone a computation is
correct, they have to re-run it. If the computation takes an hour, verification
takes an hour. If a million people want to verify, that is a million hours of
computation.

With ZK proofs, the prover runs the computation once (an hour of work) and
produces a proof. Verification takes milliseconds, no matter how complex the
original computation was. A million verifiers spend a few seconds total.

This asymmetry is why ZK proofs are useful:

- **Blockchains**: a miner proves a transaction is valid. Every node verifies
  the proof instead of re-executing the transaction. Consensus becomes cheap.
- **Privacy**: the prover proves they have sufficient funds without revealing
  their balance. The verifier checks the proof without seeing the numbers.
- **Compression**: a rollup proves it executed 10,000 transactions correctly.
  The base chain verifies one proof instead of replaying 10,000 transactions.

In Trident, this asymmetry shows up in the `divine` pattern. The prover does
the expensive work of finding the right values (searching, sorting, computing
inverses). The program does the cheap work of checking those values are
correct. The proof captures that the checks passed.

---

## 11. Your First Trident Program

Let us build a complete program from scratch, explaining every line.

```
program my_first

fn main() {
    let a: Field = pub_read()
    let b: Field = pub_read()
    let sum: Field = a + b
    let product: Field = a * b
    pub_write(sum)
    pub_write(product)
}
```

Line by line:

**`program my_first`** -- Every Trident file starts with either `program`
(executable, has a `main` function) or `module` (library, no `main`). The name
`my_first` is the program identifier.

**`fn main() {`** -- The entry point. Every program must have exactly one
`main` function. It takes no arguments and returns nothing. All I/O happens
through `pub_read`, `pub_write`, and `divine`.

**`let a: Field = pub_read()`** -- Read one field element from public input.
The verifier will see this value as part of the claim. `Field` is the base
numeric type -- an integer mod `p = 2^64 - 2^32 + 1`.

**`let b: Field = pub_read()`** -- Read a second public input. Public inputs
are consumed sequentially -- the first `pub_read` gets the first value, the
second gets the second, and so on.

**`let sum: Field = a + b`** -- Field addition. This compiles to a single TASM
`add` instruction. If the result exceeds `p`, it wraps around (mod `p`).

**`let product: Field = a * b`** -- Field multiplication. Single TASM `mul`
instruction. Same wrapping behavior.

**`pub_write(sum)`** -- Write the sum to public output. The verifier sees this
value.

**`pub_write(product)`** -- Write the product to public output.

Build it:

```bash
trident build my_first.tri -o my_first.tasm
```

See the cost:

```bash
trident build my_first.tri --costs
```

This program is trivial -- a handful of instructions, negligible proving cost.
But it demonstrates the complete pattern: read public inputs, compute, write
public outputs. The prover runs this with concrete values (say, `a=3, b=7`),
the trace records every step, the STARK prover compresses the trace, and any
verifier can confirm the outputs (10 and 21) are correct for those inputs.

Now let us add secret input:

```
program secret_sum

fn main() {
    let target: Field = pub_read()
    let s1: Field = divine()
    let s2: Field = divine()
    assert(s1 + s2 == target)
    pub_write(1)
}
```

This program proves: "I know two secret numbers that add up to the public
target." The verifier sees the target and the output (`1` = success) but never
learns `s1` or `s2`. The `assert` is the constraint -- if the prover provides
wrong secret values, the assertion fails, no proof is generated, and the
computation is rejected.

---

## 12. What Makes Trident Different?

If you are coming from Rust, Python, Go, JavaScript, or C++, Trident will feel
restrictive. Here is an honest accounting of what you give up and what you get.

### What you give up

| Feature | In Rust/Python/etc. | In Trident |
|---------|---------------------|------------|
| Dynamic memory | `Vec`, `HashMap`, heap allocation | Fixed-size arrays and structs only |
| Unbounded loops | `while`, `loop`, arbitrary `for` | All loops require `bounded N` |
| Recursion | Arbitrary recursion depth | Not allowed; use bounded loops |
| Strings | `String`, `&str`, string processing | Not available (no string ops in the VM) |
| Floating point | `f32`, `f64` | Not available (field arithmetic only) |
| Generics | `<T: Trait>`, type parameters | Size-generic functions only (`<N>`) |
| Exceptions | `try/catch`, `Result`, `?` | `assert` only -- failure = no proof |
| Concurrency | Threads, async/await | Single-threaded, sequential execution |
| Standard library | Huge ecosystem | Layered std modules (`std.core`, `std.crypto`, `std.io`) for VM primitives |

### What you gain

**Provability.** Every execution of your program produces a mathematical proof
that the computation was correct. No one has to trust the prover. Anyone can
verify in milliseconds.

**Privacy.** Secret inputs (`divine`) are never revealed to the verifier. You
can prove properties of data without exposing the data itself.

**Quantum safety.** STARK proofs are based on hash functions, not elliptic
curves. When quantum computers arrive, your proofs remain secure.

**Multi-target deployment.** The same Trident source compiles to multiple zkVMs
via `--target`. Write your program once, then deploy to Triton VM, Miden VM,
Cairo, or other backends without rewriting. The universal core of the language
is portable across all targets; backend extensions let you access
target-specific capabilities when needed.

**Cost certainty.** The compiler tells you exactly how much proving will cost
before you run anything. `trident build --costs` gives you the precise trace
size, dominant table, and estimated proving time. No surprises.

**Determinism.** No garbage collection pauses, no memory allocation failures,
no undefined behavior. The same program with the same inputs always produces
the same trace with the same cost.

**Auditability.** Trident compiles directly to TASM with no intermediate
representation. Every language construct maps predictably to specific
instructions. A security auditor can verify the translation in days, not
months.

### The honest trade-off

Trident is not a general-purpose language. You would not build a web server, a
game engine, or a data pipeline in it. It is a purpose-built tool for writing
provable computations -- programs where correctness, privacy, and
verifiability matter more than expressiveness.

The restrictions (bounded loops, no heap, no recursion) are not limitations of
the language design. They are fundamental requirements of the proof system.
Any language targeting a STARK VM faces the same constraints. Trident makes
them explicit rather than hiding them behind abstractions. The multi-target
architecture means these constraints are enforced uniformly across all
backends -- programs that compile for one target will compile for any other
target that supports the same feature set.

---

## 13. Further Reading

### Trident documentation

- [Tutorial](tutorial.md) -- Step-by-step guide: types, functions, modules,
  I/O, hashing, events, testing, cost analysis
- [Language Reference](reference.md) -- Quick lookup: types, operators,
  builtins, grammar, CLI flags
- [Language Specification](spec.md) -- Complete reference for all language
  constructs, the type system, module system, grammar, and cost model
- [Programming Model](programming-model.md) -- How programs run inside Triton
  VM, the Neptune blockchain model, script types, and data flow
- [Optimization Guide](optimization.md) -- Strategies for reducing proving
  cost across all six Triton VM tables
- [Error Catalog](errors.md) -- Every compiler error message explained with
  fixes
- [For Blockchain Devs](for-blockchain-devs.md) -- If you come from Solidity,
  Anchor, or CosmWasm, start here instead
- [Compiling a Program](compiling-a-program.md) -- Build pipeline, cost
  analysis, and error handling
- [Formal Verification](formal-verification.md) -- Prove program properties
  for all inputs via symbolic execution and SMT
- [Content-Addressed Code](content-addressed.md) -- Poseidon2 hashing, UCM
  codebase manager, verification caching
- [Universal Design](universal-design.md) -- Multi-target architecture,
  backend extensions, and the universal core
- [Vision](vision.md) -- Why Trident exists and what you can build with it
- [Comparative Analysis](analysis.md) -- Triton VM vs. every other ZK system

### External resources

- [Triton VM](https://triton-vm.org/) -- The default target virtual machine
- [Triton VM specification](https://triton-vm.org/spec/) -- The TASM
  instruction set
- [Neptune Cash](https://neptune.cash/) -- Production blockchain built on
  Triton VM
- [tasm-lib](https://github.com/TritonVM/tasm-lib) -- Reusable TASM snippets
  used by the standard library
- [Tip5 hash function](https://eprint.iacr.org/2023/107) -- The algebraic hash
  function native to Triton VM
- [FRI protocol](https://eccc.weizmann.ac.il/report/2017/134/) -- The
  proximity proof at the heart of STARKs
- [Goldilocks prime](https://xn--2-umb.com/22/goldilocks/) -- Why
  `2^64 - 2^32 + 1`
- [How STARK Proofs Work](stark-proofs.md) -- Deep dive into the proof
  system underlying Triton VM
- [Vyper](https://docs.vyperlang.org/) -- The language philosophy that inspired
  Trident's "deliberate limitation" approach
