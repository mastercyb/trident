# Trident: Three Revolutions. One Field.

---

## The Problem

Three computational revolutions -- quantum computing, privacy, and
artificial intelligence -- are advancing in isolation. Each builds its own
toolchain, its own field arithmetic, its own proof systems. Quantum teams
design qudit gates over prime fields. Privacy teams build ZK/FHE/MPC over
prime fields. AI teams quantize neural networks into field arithmetic for
verifiable inference. They share a common algebraic foundation and do not
know it. The isolation is artificial. The convergence is structural.

No language exists at their intersection. Quantum programming languages
cannot produce zero-knowledge proofs. ZK languages cannot express quantum
circuits. Neither can run neural network inference natively. Every team
reinvents field arithmetic from scratch, in incompatible toolchains, for
one application at a time.

Meanwhile, the only blockchain that passes all four tests -- quantum-safe,
private, programmable, mineable --
is [Neptune Cash](https://neptune.cash/) running on
[Triton VM](https://triton-vm.org/). Neptune's team wrote a recursive
STARK verifier, a transaction engine, and a Proof-of-Work blockchain in
raw assembly. It works, but raw TASM does not scale beyond a handful of
developers. Trident exists to make Neptune accessible -- and to unify the
three revolutions that share its field.

---

## The Discovery

The Goldilocks field (p = 2^64 - 2^32 + 1) was chosen for classical STARK
efficiency: it fits in 64-bit CPU words, allows fast modular reduction, and
has a multiplicative group with 2^32 roots of unity for efficient NTTs.

That this same choice simultaneously optimizes for quantum advantage,
private computation, and field-native AI is the discovery — a mathematical
inevitability arising from the shared requirement of reversible computation
with complete arithmetic.

**Quantum** -- requires prime-dimensional state spaces. Every quantum
operation must be unitary -- reversible, norm-preserving, with no information
destruction. When the dimension is prime, the Hilbert space has no invariant
subspaces under the generalized Pauli group. No decoherence channels form.
Every gate touches the full state space. A 2025 paper in *Nature
Communications* proved that constant-depth quantum circuits over
prime-dimensional qudits unconditionally surpass classical biased threshold
circuits, and this advantage is robust to noise across all prime dimensions.
Quantum advantage demands prime dimensions by structural necessity.

**Privacy** -- requires reversible computation with complete arithmetic.
Zero-knowledge proofs need the verifier to trace from output back to input.
Fully homomorphic encryption needs polynomial rings over a prime field for
ciphertext operations. Multi-party computation needs Shamir secret sharing
over a finite field. All three demand that every nonzero element has a
multiplicative inverse and no information is destroyed. ZK, FHE, and MPC
all operate natively over the same Goldilocks field -- no cross-domain
translation, one proof system covers everything.

**AI** -- requires nonlinear functions over fixed fields. Neural networks
expressed in field arithmetic produce STARK proofs alongside their outputs.
Weights, activations, and gradients are field elements from the start -- no
float-to-field quantization, no precision loss, no impedance mismatch
between the computation and the proof system. The same lookup table that
provides hash security provides neural network expressiveness.

The cyclic group Z/pZ for prime p is the shared algebraic skeleton.
Classically, it defines the additive group of the field. Quantum
mechanically, it defines the computational basis and generalized Pauli
operators of a p-dimensional qudit. In neural networks, it defines the
native arithmetic of provable inference. These are the same object viewed
from three sides.

> Both classical provability and quantum mechanics require reversible
> computation with complete arithmetic. Both require prime fields. Trident
> makes prime field elements its fundamental primitive. The convergence
> follows from a theorem about prime numbers.

See [Quantum Computing](quantum.md) for the full structural argument.

---

## What Trident Is

Trident is a minimal, security-first language for provable computation --
and the first programming language positioned at the convergence point of
all three revolutions.

Source code compiles through a [54-operation IR](../reference/ir.md) that
lowers to a target VM. The first target is Triton VM. The roadmap includes
quantum, ML, ZK, and classical backends.

The design constraints are deliberate:

- Bounded loops. Every loop has a compile-time bound. No infinite execution.
  This is simultaneously a ZK constraint (finite circuit), a neural network
  layer iterator, and a quantum circuit depth bound.
- No heap, no recursion, no dynamic dispatch. All data has known size.
- Fixed-width types. `Field`, `U32`, `Bool`, `Digest`, fixed arrays, structs.
- Cost transparency. Proving cost computable from source before execution.

These constraints make every program a fixed, bounded computation -- exactly
what a STARK prover requires, exactly what a quantum circuit executes, and
exactly what makes neural network inference provable.

Three key primitives bridge the three worlds:

- **`divine()`** -- non-deterministic witness injection. For privacy: injects
  secret data. For AI: injects model weights and optimization results. For
  quantum: maps to oracle queries, enabling Grover speedup on witness search.
  Same mechanism, different semantics, one proof.

- **Bounded loops** -- every loop has a compile-time bound. Simultaneously a
  ZK constraint (finite circuit), a neural network layer iterator, and a
  quantum circuit depth bound.

- **Lookup tables** -- the Rosetta Stone mechanism. One table serves as
  cryptographic S-box, neural activation, FHE bootstrap function, and STARK
  authentication.

The compiler is ~36K lines of Rust with 5 runtime dependencies. 618 tests.
The roadmap includes 20 VM targets and 25 OS targets. For architecture
details, see [Multi-Target Compilation](multi-target.md). For hash
performance and quantum safety comparisons, see
[Comparative Analysis](provable-computing.md).

---

## Three Pillars: Quantum, Privacy, AI

```
                         TRIDENT

              Quantum ---- Privacy ---- AI
              |     |        |           |
          security  advantage|       field-native
          hash-based NTT=QFT |       neural networks
          STARK    qudit sim |       provable inference
          proofs   QML, VQE  |
                     FHE + ZK + MPC
```

### Quantum: The Shield and the Sword

The Quantum pillar faces both directions. It shields Trident programs against
quantum computers that will break elliptic curve cryptography. And it
harnesses quantum computation as a resource.

**Security** -- Every Trident proof is a STARK: hash-based, transparent, no
trusted setup. Security reduces to collision resistance of Poseidon2. Grover's
quadratic speedup against hashing is manageable by doubling output size. Every
SNARK system in production has an expiration date. Hash-based STARKs do not.

**Advantage** -- The same field that provides quantum security opens the door
to quantum computation. A quantum gate on a p-dimensional qudit is a unitary
matrix over the quadratic extension F_{p^2} -- two F_p operations per
component. Quantum simulation lives natively in the same field as everything
else. The Number-Theoretic Transform (NTT) over F_p is the exact discrete
analog of the Quantum Fourier Transform (QFT) -- same butterfly network, same
twiddle factors, same hardware. The NTT engine that accelerates STARK proofs
simultaneously accelerates quantum circuit simulation.

The standard quantum approach decomposes a single Toffoli gate into ~8,000
T-gates because of the mismatch between binary dimension and the gate's
algebraic structure. In prime dimension p, the generalized Toffoli is a single
native gate. One matrix multiplication over F_{p^2}. Four orders of magnitude
reduction in gate count.

The same program that runs on Triton VM today can have its proof generation
quantum-accelerated tomorrow, with zero source code changes.

### Privacy: ZK + FHE + MPC

Privacy is a requirement. Three cryptographic technologies
work in concert:

- **ZK** (Zero-Knowledge Proofs) -- prove a statement is true while keeping the
  evidence sealed. STARKs over F_p.
- **FHE** (Fully Homomorphic Encryption) -- compute on data that remains
  encrypted throughout. TFHE over the Goldilocks ring R_p.
- **MPC** (Multi-Party Computation) -- jointly compute a function where every
  party's input stays private. Shamir sharing over F_p.

Each technology's strength fills exactly the gap where another needs support.
Together they cover the full spectrum:

| Tier | What's Protected | Technologies |
|------|-----------------|--------------|
| 0 -- Transparent | Open computation, proven correct | ZK (correctness proofs) |
| 1 -- Private Ownership | Record ownership, amounts, transaction graph | ZK (commitments + nullifiers) |
| 2 -- Private Computation | Inputs, intermediates, query content | ZK + FHE |
| 3 -- Distributed Trust | Keys distributed, threshold-secured secrets | ZK + FHE + MPC |

All three technologies operate over the same Goldilocks field. No cross-domain
translation. One proof system covers everything.

### AI: Field-Native Intelligence

Neural networks in Trident run natively over the Goldilocks field. Weights,
activations, and outputs are field elements from the start -- the natural
language of the proof system. Inference produces a STARK proof alongside its
result. Anyone can verify that a model produced a specific output from specific
inputs, while the model weights remain private and the input data stays
encrypted.

The `std.nn` library provides 15+ neural network operations: matrix multiply,
attention, convolutions, normalization, lookup-table activations. No
float-to-field quantization. No precision loss. No impedance mismatch.

Why this matters: existing zkML approaches (EZKL, others) start from
floating-point models, convert to field arithmetic (losing precision), and use
SNARKs (quantum-vulnerable). Trident starts from field-native arithmetic, uses
STARKs (post-quantum), and incurs zero quantization overhead.

---

## The Rosetta Stone

A single lookup table over F_p simultaneously functions as four mechanisms:

```
                  T_f : {0, ..., D-1} -> F_p
                  +========================+
                  |  0 -> f(0)             |
                  |  1 -> f(1)             |
                  |  ...                   |
                  |  D-1 -> f(D-1)         |
                  +============+===========+
                               |
            +------------------+------------------+
            |                  |                  |
    +-------+-------+  +------+------+  +---------+---------+
    | STARK reads:  |  | NN reads:   |  | FHE reads:        |
    | lookup table  |  | activation  |  | test polynomial   |
    | for LogUp     |  | layer       |  | for bootstrap     |
    +---------------+  +-------------+  +-------------------+
                               |
                    +----------+----------+
                    | Crypto reads:       |
                    | S-box for hash      |
                    | round function      |
                    +---------------------+
```

Every interesting computation requires nonlinearity. Linear functions cannot
distinguish, classify, decide, or protect. The lookup table is the universal
mechanism for introducing arbitrary nonlinearity into field-arithmetic systems.
Each domain discovered it independently. They arrived at the same mathematical
object from four different directions.

The unification is most vivid in a concrete scenario: a program performing
neural network inference on FHE-encrypted data with a STARK correctness proof
uses the same ReLU table for the activation function (NN layer), the
bootstrapping (FHE evaluation), and the authentication (STARK lookup). Three
roles. One array of field elements.

---

## The Vision

The bet is fivefold:

**Quantum-native, not just quantum-safe.** Every SNARK system has an
expiration date. Hash-based STARKs don't need migration. But the deeper
point: the same prime field arithmetic that makes programs provable makes
them optimal for quantum execution. Trident programs are not merely safe
against quantum attacks -- they are structurally ready to be quantum-
accelerated. `Field` maps to a qudit register. `divine()` maps to an oracle
query. Bounded loops map to fixed-depth circuits. The same program runs on
Triton VM today and has its proof generation quantum-accelerated tomorrow.

**Privacy as a trilateral.** ZK proves correctness while hiding evidence.
FHE computes on encrypted data without seeing it. MPC distributes trust
across independent parties. Each technology fills the gap where the others
need support. All three operate over the same Goldilocks field. The full
privacy spectrum -- from transparent proofs to threshold-secured secrets --
is available from genesis.

**AI as first-class citizen.** Neural networks in field arithmetic, not
floats. Provable inference. Private model evaluation on encrypted data.
The same prover that validates transactions validates model outputs. The
same verifier that checks balances checks neural network inference.
Intelligence and verification share a single mathematical home.

**Developer experience determines adoption.** Triton VM is the right
foundation. Raw TASM is the wrong interface. Cairo proved this for StarkWare.
Trident proves it for the only OS that gets all four properties right.

**No program should be stranded on one VM.** The universal core compiles to
any target. Backend extensions add power without limiting portability.
Choosing Trident is not choosing a single ecosystem -- it is choosing all
of them.

---

## What Becomes Possible

Each pillar alone is powerful. The unification over a single field makes
their intersections -- capabilities that draw on two or three pillars
simultaneously -- emerge naturally.

**Quantum x AI** -- Hybrid classical-quantum neural networks. Quantum walks
on knowledge graphs for quadratic speedup in convergence. Verifiable quantum
chemistry: VQE for drug discovery produces STARK proofs anyone can verify on
a phone.

**Quantum x Privacy** -- Every privacy mechanism is quantum-resistant by
construction. FHE ciphertexts are lattice-based over F_p. ZK proofs are
hash-based STARKs. MPC uses Shamir sharing over F_p. The quantum future is
an ally, not a threat.

**Privacy x AI** -- Neural networks evaluate on FHE-encrypted inputs. The
model owner's IP stays protected. The data owner's information stays sealed.
A STARK proof attests correct evaluation. Anyone verifies on a phone. From
here: a private AI marketplace where models and data meet inside encrypted
computation, verified by zero-knowledge proofs, with keys distributed via MPC.

**All three** -- A diagnostic AI runs on FHE-encrypted medical data.
Computation is quantum-accelerated. A STARK proof attests correct execution.
The decryption key is held by an MPC threshold group. The patient receives a
provably correct diagnosis that only they can read. Every property -- privacy,
correctness, quantum security, quantum advantage -- flows from the same field.

---

## The Strategic Position

```text
           Expressiveness
                |
   Rust/C++  *  |
                |     Cairo *
                |                 Quantum-native
                |          Trident *  ............. Field-native AI
                |                     Provable inference
      Circom *  |     Noir *          Private computation
                |                     Three-revolution convergence
                +-------------------------------------->  Provability
```

Not the most expressive language. Not the most minimal circuit DSL. The
convergence point for provable programs that are simultaneously quantum-
native, AI-compatible, and privacy-preserving. Every trend -- more zkVMs,
ZK expanding beyond crypto, AI demanding verifiability, quantum computers
approaching cryptographic relevance, regulatory pressure for auditable code
-- makes this position stronger.

---

## See Also

- [Quantum Computing](quantum.md) -- Deep structural necessity argument
- [For Offchain Devs](for-offchain-devs.md) -- Zero-knowledge programming from scratch
- [For Onchain Devs](for-onchain-devs.md) -- From Solidity/Cairo/Anchor to Trident
- [Multi-Target Compilation](multi-target.md) -- One source, every chain
- [Comparative Analysis](provable-computing.md) -- Triton VM vs every other ZK system
- [How STARK Proofs Work](stark-proofs.md) -- From execution traces to quantum-safe proofs
- [Gold Standard](gold-standard.md) -- Token standards (TSP-1, TSP-2) and capability library
- [Language Reference](../reference/language.md) -- Types, operators, builtins, grammar
- [IR Reference](../reference/ir.md) -- 54 TIR operations, 4 lowering paths
