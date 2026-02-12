# 🔐 Generating Proofs

*From execution trace to cryptographic proof*

> **Lifecycle stage 5 of 6.** This document covers what happens after a Trident
> program has been compiled and executed. The previous stage is Deploying; the
> next stage is [Verifying Proofs](verifying-proofs.md).

A Trident program that compiles, runs, and produces the right output is only
halfway done. The point of writing in Trident is not just to compute a result
-- it is to *prove* the result is correct. This document explains the proof
generation process: what it does, what it costs, and how to control that cost.

---

## 🔐 1. What Is Proof Generation?

When Triton VM executes a compiled Trident program, it does more than produce
output. It records a complete **execution trace** -- every instruction
executed, every stack state, every memory access, every hash permutation. This
trace is the raw material for proof generation.

The **prover** takes this execution trace and converts it into a **STARK
proof**: a compact cryptographic object (roughly 100 KB) that convinces any
verifier of two things:

1. **Correctness.** The program with the given public inputs produced the
   given public outputs. Every instruction was executed faithfully.
2. **Zero knowledge.** The verifier learns nothing beyond the public inputs
   and outputs. Secret inputs, intermediate values, memory contents, and the
   execution trace itself remain hidden.

This is the core value proposition of zero-knowledge computation. The prover
does heavy work once; anyone can verify the result in milliseconds, with no
trust in the prover required.

See [How STARK Proofs Work](../explanation/stark-proofs.md) for the full mathematical
construction, from trace polynomials through FRI and Fiat-Shamir.

---

## 🔧 2. The Proof Pipeline

The end-to-end pipeline from source code to proof:

```trident
Trident source (.tri)
    |
    |  trident build        <-- Trident's responsibility
    v
TASM program (.tasm)
    |
    |  triton-vm execute    <-- Triton VM's responsibility
    v
Execution trace (6 tables)
    |
    |  triton-vm prove
    v
STARK proof + Claim
```

**Trident handles the first arrow.** It compiles `.tri` source into `.tasm`
(Triton Assembly) and performs cost analysis. Once compilation is done,
Trident's job is finished.

**Triton VM handles everything else.** Execution, trace generation, polynomial
interpolation, FRI commitment, and proof assembly are all performed by the
`triton-vm` crate. Trident does not implement any part of the proof system.

The boundary matters because it determines what you can control. Trident gives
you tools to reason about the *shape* of the execution trace (table heights,
hotspots, cost distribution). Triton VM turns that trace into a proof. You
optimize the trace through your Trident code; the proof system is fixed.

The **Claim** -- the public statement that accompanies every proof -- contains
the program digest, the public inputs, and the public outputs:

```text
Claim {
    program_digest: Digest,
    input:  Vec<Field>,
    output: Vec<Field>,
}
```

Everything else (secret inputs, RAM, stack, the trace itself) is hidden by
the zero-knowledge property. See [Programming Model](../explanation/programming-model.md)
for the full execution model.

---

## ⚡ 3. Understanding Proving Cost

Proving time and memory are determined by one number: the **padded height** -- the next power of 2 above the tallest of Triton VM's six execution tables. Only the tallest table matters; reducing a shorter table has zero effect on proving time. See [Optimization Guide](optimization.md) for the full table model, padded height mechanics, and reduction strategies.

### Measuring Cost with Trident

Trident provides three tools for understanding proving cost before you ever
generate a proof:

**`--costs`** shows the height of each of the six tables:

```bash
trident build main.tri --costs
```

This tells you which table is dominant. If the Hash table is at 1200 rows and
everything else is under 500, the padded height is 2048 (next power of 2 above
1200), and your optimization target is the Hash table.

**`--hotspots`** identifies the most expensive functions:

```bash
trident build main.tri --hotspots
```

This shows the top 5 functions ranked by cost contribution, so you know where
to focus.

**`--hints`** suggests specific optimizations:

```bash
trident build main.tri --hints
```

The compiler analyzes your code and emits actionable hints (e.g., "batch these
hash calls," "this loop bound is too loose").

These three commands form a feedback loop: measure, identify, optimize, measure
again. Use `--save-costs` and `--compare` to track improvements over time.

See [How STARK Proofs Work](../explanation/stark-proofs.md), Section 11, for the exact
proving cost formula.

---

## ⚡ 4. Optimizing for Proof Generation

Every optimization in Trident ultimately serves one goal: reducing the padded
height of the execution trace. The full details are in the
[Optimization Guide](optimization.md); here is a brief summary of the key
strategies organized by table:

**Hash Table** -- Often the tallest. Each `tip5` call adds 6 rows regardless
of how many inputs you use. Batch multiple values into a single call (up to 10
field elements). For more than 10 elements, use the sponge API
(`sponge_init` / `sponge_absorb` / `sponge_squeeze`) to stream data without
repeated hash invocations. Reduce Merkle tree depth where possible -- each
level costs 6 rows.

**Processor Table** -- Grows with every instruction executed. Loops are the
primary driver. Move invariant computations outside loop bodies, use the
tightest possible `bounded` values, and minimize iteration counts.

**U32 Table** -- Grows with range checks and bitwise operations. Use `Field`
arithmetic instead of `U32` when range checks are not required. Minimize
`as_u32` conversions.

**Op Stack Table** -- Grows with deep stack access. Keep hot variables shallow,
minimize struct sizes, and keep live variable counts under 16 field elements to
avoid spilling to RAM.

**RAM Table** -- Grows with memory read/write operations. Prefer stack-based
access when feasible. Use `sponge_absorb_mem` to hash data directly from RAM
without reading each element onto the stack first.

**Jump Stack Table** -- Grows with function calls and branches. Inline small
functions called in tight loops. Reduce branching inside loops.

The [Optimization Guide](optimization.md) covers each strategy in detail,
including code examples and cost comparisons.

---

## ▶️ 5. Proving with Triton VM

Once you have a compiled `.tasm` program, proof generation is handled by the
`triton-vm` Rust crate. The basic flow:

```rust
use triton_vm::prelude::*;

// 1. Load the program
let program = Program::from_code(tasm_source)?;

// 2. Define inputs
let public_input = PublicInput::new(vec![/* field elements */]);
let secret_input = NonDeterminism::new(vec![/* field elements */]);

// 3. Execute and prove
let (stark, claim, proof) = triton_vm::prove(
    Stark::default(),
    &claim,
    &program,
    public_input,
    secret_input,
)?;
```

The three inputs to the prover are:

1. **The compiled program** (`.tasm` output from `trident build`)
2. **Public inputs** (visible to the verifier, included in the Claim)
3. **Secret inputs** (hidden from the verifier, used during execution only)

The prover returns a **Claim** and a **Proof**. Together, these are everything
a verifier needs. The program source, the secret inputs, and the execution
trace are not required for verification.

Refer to the [Triton VM documentation](https://triton-vm.org/) and the
`triton-vm` crate documentation for the full API, configuration options, and
performance tuning.

---

## 🔄 6. Recursive Proofs

Triton VM supports **recursive STARK verification**: you can verify a STARK
proof *inside* the VM itself. This means a Trident program can take a proof as
secret input, verify it, and produce a new proof that the verification
succeeded.

This enables **proof composition**: chain multiple computations together by
proving that each step's proof is valid. Use cases include incremental
computation, proof aggregation, and bootstrapping trust across independent
programs.

Recursive verification is made possible by dedicated VM instructions
(`xx_dot_step` and `xb_dot_step`) that accelerate the inner-product
computations at the core of STARK verification. Without these builtins,
verifying a STARK inside the VM would be prohibitively expensive.

Recursive proofs are currently advanced and experimental territory. The proving
cost of recursive verification is significant (the verifier program itself
produces a large execution trace), and the tooling is still maturing. But the
capability is real and functional today.

---

## 📊 7. Proof Size and Performance

**Proof size.** STARK proofs are approximately 100 KB. This is larger than
SNARK proofs (which are typically under 1 KB) but comes with two important
advantages: STARKs require **no trusted setup** (no ceremony, no toxic waste)
and are **quantum-safe** (security relies on hash functions, not elliptic curve
assumptions).

**Proving time.** Scales with the padded height of the execution trace. A
program whose tallest table has 1,000 rows (padded to 1,024) proves in
seconds. A program with 1,000,000 rows (padded to 1,048,576) takes
proportionally longer. The exact relationship depends on hardware, but the
scaling is roughly linear in padded height.

**Verification time.** Constant with respect to program complexity --
milliseconds regardless of whether the original computation took seconds or
hours. This asymmetry between proving cost and verification cost is the
fundamental property that makes the system useful.

**Memory.** The prover must hold the entire execution trace and its polynomial
representation in memory. For large programs, this can be substantial. Reducing
the padded height reduces memory proportionally.

For the full mathematical details -- why proofs are this size, why verification
is fast, why no trusted setup is needed -- see
[How STARK Proofs Work](../explanation/stark-proofs.md).

---

## 🚀 Next Step

Once a proof is generated, it must be verified.

**Next:** [Verifying Proofs](verifying-proofs.md) -- how verifiers check
proofs, what the verification algorithm does, and how on-chain verification
works.
