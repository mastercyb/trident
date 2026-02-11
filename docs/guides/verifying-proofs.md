# Verifying Proofs

This is the final stage of the Trident program lifecycle: Writing > Compiling > Running > Deploying > Generating Proofs > **Verifying Proofs**. Everything before this point was about producing a STARK proof. This stage is about checking one.

Given a proof and the public inputs, anyone can verify that a computation was performed correctly -- in milliseconds, without re-executing the program, and without seeing the secret inputs. The original computation may have taken minutes. Verification takes the same time regardless.

For how proofs are generated, see [Generating Proofs](generating-proofs.md). For the underlying proof system, see [How STARK Proofs Work](stark-proofs.md).

---

## 1. What Is Proof Verification?

A STARK proof is a short cryptographic certificate that a specific program, given specific public inputs, produced specific public outputs. Verification is the act of checking that certificate.

The verifier receives two things:

1. **The Claim** -- a public statement: which program was executed (identified by its Tip5 digest), what public inputs it consumed, and what public outputs it produced.

2. **The Proof** -- Merkle roots, FRI commitments, authentication paths, and queried evaluations. Typically 100-200 KB.

The verifier does NOT receive:

- The secret inputs (they remain hidden).
- The source code (only the program digest matters).
- The execution trace (the proof replaces it).

Verification checks that the proof is internally consistent and that it corresponds to the Claim. If the checks pass, the computation was correct. If any check fails, the proof is rejected.

The fundamental asymmetry: proving is expensive (seconds to minutes), verification is cheap (milliseconds). A program that runs for a billion cycles produces a proof that verifies in the same time as a program that runs for a thousand cycles.

---

## 2. Verification Properties

STARK verification provides three guarantees:

### Completeness

If the prover executed the program correctly, the resulting proof will always verify. A valid computation always produces a valid proof. There are no false negatives.

### Soundness

If the prover cheated -- computed the wrong result, fabricated outputs, or tampered with the execution -- the proof will be rejected with overwhelming probability. With 80 FRI queries, the probability of a forged proof passing verification is less than 2^(-80). There are no practical false positives.

### Zero-Knowledge

The proof reveals nothing beyond what the Claim explicitly states: the program digest, the public inputs, and the public outputs. Secret inputs, intermediate values, memory contents, control flow paths -- all hidden. The verifier learns exactly one thing: the computation was correct.

### No Trusted Setup

Unlike SNARK systems (Groth16, PLONK with KZG), STARK verification requires no trusted setup ceremony. There are no secret parameters, no "toxic waste" to destroy, and no ceremony to compromise. The verifier's challenges are derived from the proof transcript itself via the Fiat-Shamir transform. Anyone can verify the proof system's integrity by reading the specification.

This is the "transparent" in STARK -- Scalable Transparent Argument of Knowledge.

See [How STARK Proofs Work](stark-proofs.md), Sections 6 and 9, for the technical foundations of FRI soundness and the transparent setup.

---

## 3. Verifying Triton VM Proofs

Triton VM proofs are verified using the `triton-vm` Rust crate. The verification API requires three inputs:

```rust
use triton_vm::prelude::*;

let claim = Claim {
    program_digest: program.hash(),  // Tip5 digest of the TASM program
    input: public_input,             // Vec<BFieldElement>
    output: public_output,           // Vec<BFieldElement>
};

let verdict = triton_vm::verify(
    Stark::default(),   // proof parameters
    &claim,
    &proof,             // the STARK proof
);

assert!(verdict);
```

### What the verifier checks

Verification performs four categories of checks, as described in [How STARK Proofs Work](stark-proofs.md), Section 8:

1. **Merkle root integrity.** Every authentication path in the proof hashes correctly to the committed Merkle roots.

2. **AIR satisfaction.** At every queried evaluation point, the constraint polynomials -- divided by the zerofier -- evaluate to values consistent with a low-degree quotient. This confirms the execution trace satisfies all transition, boundary, and consistency constraints.

3. **FRI consistency.** Across all folding rounds, the committed polynomial evaluations are consistent with the folding relation. This confirms the quotient polynomial is actually low-degree, not arbitrary data.

4. **Claim binding.** The public inputs and outputs in the Claim match the boundary constraints in the trace. The program digest matches the attested program hash. The Fiat-Shamir challenges are correctly derived from the transcript.

### What the verifier does NOT need

- **Secret inputs.** The verifier never sees values passed via `sec_read()` / `divine()`. They are consumed during execution and hidden by the proof's zero-knowledge property.

- **Source code.** The verifier works with the program digest, not the source. Two different Trident programs that compile to the same TASM produce the same digest. The verifier does not care about the source language.

- **The execution trace.** The entire purpose of the STARK is to replace the trace (potentially millions of rows) with a compact proof (hundreds of kilobytes).

---

## 4. On-Chain Verification

In [Neptune Cash](https://neptune.cash), every transaction carries a STARK proof of its validity. Miners verify these proofs as part of block validation. The consensus rule is simple: if the proof verifies against the claimed program and public inputs, the transaction is valid.

### How it works

1. A user constructs a transaction with private details (amounts, accounts, authorization secrets).
2. The user proves the transaction's validity by executing the transaction validation program in Triton VM and generating a STARK proof.
3. The proof and public inputs are broadcast to the network.
4. Every miner and full node verifies the proof before including the transaction in a block.
5. No node ever sees the private details. They see only "this transaction is valid" -- backed by cryptographic proof.

### Why this scales

Verification takes milliseconds regardless of the transaction's complexity. A simple transfer and a complex multi-party exchange produce proofs that verify in the same time. Miners do not re-execute transaction logic; they check proofs. This decouples validation cost from computation cost.

The proof also serves as a permanent certificate. Any node joining the network later can verify historical transactions without trusting the nodes that originally validated them.

---

## 5. Recursive Verification

Triton VM can verify STARK proofs inside the VM itself. A Trident program can read a proof from secret input, verify it, and output only the verdict. The verification of *that* program produces another STARK proof -- a proof about a proof.

### Why this matters

- **Proof aggregation.** Batch N individual proofs into one. Instead of verifying N proofs separately, verify one aggregate proof. The aggregate proof is the same size regardless of N.

- **Rollup compression.** Prove a batch of state transitions in a single proof. Thousands of transactions become one constant-size certificate.

- **Incrementally verifiable computation.** Chain proofs for long-running computations. Each step proves "I verified the previous step's proof AND computed the next increment." The chain grows indefinitely; proof size stays constant.

- **Cross-chain bridges.** Verify another chain's proof without replaying its history. Read the external proof, verify it inside Triton VM, output the result.

### Performance

Recursive verification in Triton VM costs approximately 300,000 clock cycles regardless of the inner proof's original computation complexity. This efficiency comes from Triton VM's native instructions for extension-field dot products (`xx_dot_step`, `xb_dot_step`) and its algebraic hash function (Tip5). In RISC-V based zkVMs, the same verification costs millions of cycles.

Neptune Cash uses recursive verification in production today -- aggregating transaction proofs for block validation.

See [How STARK Proofs Work](stark-proofs.md), Section 12, for the full technical details of recursive verification.

---

## 6. Cross-Target Verification

Trident's architecture is designed for multiple compilation targets. The same Trident program compiled to different targets produces different assembly, different execution traces, and different proofs. But the semantic guarantee is the same: the computation was correct.

Each target has its own verification procedure:

| Target | Proof System | Verification Method |
|--------|-------------|-------------------|
| Triton VM (`--target triton`) | STARK (FRI + Tip5) | `triton_vm::verify()` |
| Future targets (Miden, Cairo, RISC-V zkVMs) | Target-specific | Target-specific verifier |

Currently, only the Triton VM target is implemented. When additional targets are added, each will bring its own proof format and verification API. The Trident compiler's universal core ensures that a program's semantics are preserved across targets -- a correct program on one target is correct on all targets -- but the proofs are not interchangeable between targets.

See [Vision](vision.md) for the universal compilation architecture and the roadmap for additional targets.

---

## 7. Quantum Safety

STARK proofs are secure against quantum computers. This is not a future migration plan -- it is a property of the current system.

### What quantum computers threaten

Shor's algorithm breaks the discrete logarithm problem, which underlies:

- ECDSA (transaction signatures on most blockchains)
- BN254 and BLS12-381 (elliptic curves used by Groth16 and KZG-based SNARKs)
- Pasta curves (used by Mina and Aleo)

The break is total and retroactive. Every Groth16 proof ever generated becomes forgeable. Every KZG commitment becomes extractable.

### Why STARKs are immune

STARK verification relies on exactly two primitives:

1. **Hash collision resistance** -- finding collisions in Tip5 is computationally hard. Grover's algorithm provides at most a quadratic speedup, reducing 2^256 security to 2^128. Tip5's 320-bit output provides 160 bits of post-quantum collision resistance, well above the 128-bit security target.

2. **FRI soundness** -- the proximity test correctly rejects data far from any low-degree polynomial. This is a combinatorial property of Reed-Solomon codes, not a number-theoretic assumption. Quantum computers offer no advantage.

No elliptic curves. No pairings. No discrete logarithm. Proofs generated today remain secure against quantum computers whenever they arrive.

| System | Prover Quantum-Safe | Verifier Quantum-Safe |
|--------|:---:|:---:|
| Triton VM (Trident's target) | Yes | Yes |
| SP1, RISC Zero | Yes (FRI) | No (Groth16 wrapping) |
| Aleo, Mina | No (Pasta curves) | No (Pasta curves) |

See [How STARK Proofs Work](stark-proofs.md), Section 10, for the full quantum safety analysis.

---

## Complete Lifecycle

This is the final stage. Here is the complete journey of a Trident program, from source to verified proof:

| Stage | What Happens | Guide |
|-------|-------------|-------|
| **1. Writing** | Author a `.tri` program with types, modules, and functions | [Tutorial](tutorial.md) |
| **2. Compiling** | `trident build` translates `.tri` to TASM assembly | [Compiling a Program](compiling-a-program.md) |
| **3. Running** | Execute the TASM in Triton VM, producing an execution trace | [Running a Program](running-a-program.md) |
| **4. Deploying** | Distribute the compiled program for use by provers and verifiers | [Deploying a Program](deploying-a-program.md) |
| **5. Generating Proofs** | The STARK prover compresses the execution trace into a proof | [Generating Proofs](generating-proofs.md) |
| **6. Verifying Proofs** | Anyone checks the proof in milliseconds -- this document | *You are here* |

### Deep dives

- [How STARK Proofs Work](stark-proofs.md) -- The full proof system: execution traces, arithmetization, FRI, Fiat-Shamir, recursive verification.
- [Programming Model](programming-model.md) -- The Claim/Proof structure, public vs. secret input, and the divine-and-authenticate pattern.
- [Vision](vision.md) -- Why Trident exists: quantum safety, no trusted setup, universal targets, provable programs for everyone.
- [Language Reference](reference.md) -- Quick lookup for types, instructions, and costs.
- [Tutorial](tutorial.md) -- Step-by-step walkthrough from hello world to Merkle proofs.
