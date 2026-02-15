# The Standard Library

## One Field, One Proof, One Library

Version: 0.1-draft
Date: February 15, 2026

### Status

This document explains the design philosophy behind Trident's standard
library. For the complete module catalog and API trees, see the
[Standard Library Reference](../../reference/stdlib.md).

---

## Why One Field Rules Everything

Three computational revolutions -- AI, zero-knowledge privacy, and quantum
computing -- appear unrelated. They share a common algebraic foundation:
prime field arithmetic.

Neural network inference is matrix multiplication over a field. STARK
proving is polynomial arithmetic over a field. Quantum simulation is
unitary matrix operations over a field extension. Trident's stdlib
exploits this convergence: one implementation of matrix multiply serves
neural networks, polynomial commitment, and quantum gate application.

The Goldilocks field F_p (p = 2^64 - 2^32 + 1) is the bedrock. Every
computation reduces to operations over this field. Every function compiles
to an arithmetic circuit. Every circuit produces a STARK proof.

Most standard libraries organize around data structures: lists, maps,
strings, I/O. Trident's stdlib organizes around a mathematical insight.
The field is the universal substrate. Cryptographic hashing, neural
network inference, quantum gate simulation, and token accounting all
reduce to the same field operations. A library designed around this
convergence shares implementations where traditional libraries duplicate
them.

A matrix multiply in `std.field` serves `std.nn` (weight application),
`std.quantum` (gate simulation), and `std.private` (FHE polynomial
operations). One function, three domains, zero code duplication. The
convergence is mathematical, and the library makes it structural.

---

## The Layer Architecture

The stdlib is organized into five layers, each depending only on layers
below:

```
Layer 3: Applications     std.agent, std.defi, std.science
Layer 2: Intersections    std.nn_private, std.nn_quantum, std.quantum_private
Layer 1: Three Pillars    std.nn, std.private, std.quantum
Layer 0.5: Tokens         std.token, std.coin, std.card, std.skill.*
Layer 0: Foundation       std.field, std.math, std.data, std.graph, std.crypto
```

Why layers? Because the dependency graph must be a DAG. `std.nn` depends
on `std.field` (matrix operations) and `std.data` (tensors). `std.defi`
depends on `std.coin` and `std.nn` (credit models). Flatten everything
and the dependency tangle becomes unmaintainable. Layers enforce
discipline: you can use anything below you, nothing above you.

The layer structure also mirrors adoption. Foundation ships first -- and
parts already exist as `std/crypto/`. Token infrastructure ships next --
reference implementations exist in `os/neptune/standards/`. The three
pillars, intersections, and applications follow as the ecosystem matures.
Each layer is independently useful. You do not need `std.quantum` to
build a token. You do not need `std.nn` to write a privacy protocol.
But when you reach the intersections, the shared foundation means
combining pillars requires composition, not reimplementation.

---

## Token Infrastructure: The Economic Substrate

Tokens live in the stdlib at Layer 0.5, between Foundation and the
Pillars. This placement is a deliberate architectural decision.

Every provable application eventually moves value. DeFi protocols trade
it. AI marketplaces price it. Scientific certificates attest it. Putting
tokens at Layer 0.5 means every higher layer can assume token operations
exist, the way every layer assumes field arithmetic exists. A neural
network marketplace (`std.defi`) composes `std.nn` inference proofs with
`std.coin` payment proofs. If tokens lived at Layer 1 alongside `std.nn`,
this composition would create a circular dependency.

The PLUMB framework (Pay, Lock, Update, Mint, Burn) provides five
operations that cover the entire token lifecycle. Two standards implement
PLUMB with different conservation laws:

- `std.coin` (TSP-1): divisible value, where the sum of all balances
  equals total supply
- `std.card` (TSP-2): unique ownership, where every asset has exactly
  one owner

These are the only two conservation laws in token systems. Divisible
supply and unique ownership are mathematically incompatible -- you cannot
enforce both in one circuit without branching that inflates every proof.
A third standard would require a third conservation law incompatible with
both. No such invariant exists.

Everything beyond conservation is a skill. Liquidity, governance, oracle
pricing, compliance, royalties -- 23 composable behaviors that tokens
learn through the hook system. Skills compose through STARK proof
composition: each hook generates an independent proof, the verifier
checks all proofs together. No execution ordering, no reentrancy, no
`msg.sender`. See the [Skill Library](skill-library.md) for the full
design space and the [Gold Standard](gold-standard.md) for the PLUMB
framework.

---

## The Three Pillars

Three domains form Layer 1, chosen because they represent the three
computational revolutions that field arithmetic serves.

### std.nn -- Intelligence

Every neural network layer is a matrix multiplication followed by a
nonlinear activation. Matrix multiplication is native to field
arithmetic -- multiply-accumulate over F_p elements, directly
expressible as arithmetic circuit gates.

The key insight concerns activation functions. ReLU, GELU, and SiLU are
implemented as lookup tables over F_p, proven via the same lookup
argument that authenticates Tip5's cryptographic S-box. The STARK
mechanism that validates a hash function's nonlinearity is identical to
the mechanism that validates a neural network's nonlinearity. The proof
cost is constant regardless of the activation's mathematical complexity.
Custom activations designed specifically for field arithmetic
expressiveness cost the same to prove as standard ones.

Training is included, not just inference. `std.nn.optim` provides
optimizers (SGD, Adam) over F_p, enabling provable training -- a STARK
proof that a model was trained with a specific algorithm on specific data
for a specific number of steps. Current zkML frameworks (EZKL, DeepProve)
prove inference only, leaving a trust gap between training claims and
verifiable reality. See [Verifiable AI](ai.md) for the full argument
against the float-to-field quantization pipeline.

### std.private -- Privacy

Zero-knowledge privacy extends beyond "prove I know a secret." The
privacy pillar provides composable patterns across three cryptographic
technologies that share the Goldilocks field:

- ZK (STARKs) proves correctness while hiding the witness
- FHE (TFHE over R_p) computes on data that remains encrypted throughout
- MPC (Shamir sharing over F_p) distributes trust across parties

Each technology fills the gap where another needs support. Together they
cover the full spectrum from transparent proofs to threshold-secured
secrets. The compliance module (`std.private.compliance`) bridges the
gap between absolute privacy and regulatory reality -- private
transactions that selectively reveal data to auditors, aggregate
threshold reporting without exposing individual values.

See [The Privacy Trilateral](privacy.md) for why all three technologies
are necessary and how they compose.

### std.quantum -- Quantum Power

Quantum computing primitives with dual compilation: classical simulation
(Triton VM + STARK proof) for development, and quantum execution
(Cirq/hardware) for production. The same quantum circuit definition
runs on both backends.

The mathematical foundation is direct. A quantum gate on a
p-dimensional qudit is a unitary matrix over the quadratic extension
F_{p^2} -- two F_p operations per component. The Number-Theoretic
Transform that accelerates STARK proofs is the exact discrete analog of
the Quantum Fourier Transform -- same butterfly network, same twiddle
factors. The NTT engine that accelerates polynomial commitment
simultaneously accelerates quantum circuit simulation.

The STARK proof format is identical regardless of backend. You verify
correctness without knowing whether the computation ran classically or
quantumly. See [Quantum Computing](quantum.md) for the structural
argument about prime fields and quantum advantage.

---

## The Intersections

The real power emerges where pillars meet. Each intersection combines
two domains to create capabilities that neither achieves alone.

### std.nn_private -- Private AI

The intersection that makes Trident competitive with existing zkML
frameworks on their own ground, then surpasses them. Private inference
where model weights remain secret. Provable fairness -- demonstrate
equal outcomes across demographic groups without revealing individual
predictions. Provable robustness -- certify that no adversarial example
within an epsilon-ball causes misclassification.

The standout capability: explainability via zero-knowledge proofs. A
STARK proof contains the complete execution trace -- every neuron
activation, every attention weight. The trace IS the explanation. Not
a post-hoc approximation like SHAP or LIME, but the actual computation
path, mathematically guaranteed honest, with model weights remaining
private through `divine()` witness injection.

### std.nn_quantum -- Quantum Machine Learning

Hybrid classical-quantum models where quantum circuits handle attention
computation and classical layers handle expressiveness. The potential
advantage is concrete: quantum attention mechanisms may achieve O(n*sqrt(n))
complexity versus classical O(n^2). Barren plateau detection and
mitigation strategies are STARK-proven to have been applied correctly --
a verifiable certificate that the quantum optimization did not silently
fail.

### std.quantum_private -- Quantum Cryptography

Certified randomness is the flagship application. Generate a random
number on quantum hardware, use a Bell inequality violation to certify
it is genuine, produce a STARK proof of the Bell test, publish on-chain
as a certified random beacon. Physics-guaranteed randomness with
mathematical proof of certification. No trust in hardware manufacturers,
no trust in the randomness service -- the Bell violation is the proof.

---

## Applications: Where It All Converges

Layer 3 composes everything below into domain-specific modules.

**std.agent** -- autonomous verifiable agents. Every decision produces a
STARK proof covering perception (what inputs the agent received),
reasoning (what model produced the decision), and action (what the agent
did). Safety constraints are proven in the STARK -- a trading agent with
a budget cap provably cannot exceed it, because the proof would be
invalid if the constraint were violated. The guarantee is mathematical,
not a software check that could be bypassed.

**std.defi** -- decentralized finance with exact arithmetic. No
floating-point rounding in interest rate models. Oracle prices backed by
STARK-proven swap data (see the [Gold Standard](gold-standard.md)
section on proven price). Private compliance -- KYC verification without
revealing identity, regulatory reporting on aggregate thresholds without
exposing individual positions.

**std.science** -- verifiable computational science. Molecular
simulations that produce proof certificates. Carbon credits backed by
quantum chemistry calculations, STARK-proven, settled on-chain.
Reproducibility as a cryptographic property rather than a social norm.

---

## What Ships Today

The stdlib specification spans 19 modules and roughly 200 submodules.
Today, a fraction exists as working code.

**Shipping (std/crypto/):** sha256, keccak256, ecdsa, secp256k1,
ed25519, poseidon, poseidon2, merkle, bigint, auth -- 10 modules
covering the cryptographic foundation. Plus `std/io/storage.tri` and
`std/target.tri`.

**Reference implementations (os/neptune/standards/):** `coin.tri`,
`card.tri`, and `plumb.tri` -- the token infrastructure that will
become `std.token`, `std.coin`, `std.card` once generalized across
targets.

**Designed, not yet implemented:** `std.field`, `std.math`, `std.data`,
`std.graph` (foundation beyond crypto). `std.nn`, `std.private`,
`std.quantum` (pillars). All intersections and applications.

The path forward: foundation modules first, because they serve all three
pillars. Token infrastructure next, because it provides the economic
substrate for everything above. Then pillars and intersections as the
ecosystem matures. The layer architecture means any module can be
implemented independently as long as it depends only on layers below.
Community contributions drive the timeline -- the architecture is the
invitation.

---

## See Also

- [Standard Library Reference](../../reference/stdlib.md) -- Complete module trees and API catalog
- [Skill Library](skill-library.md) -- Composable token skills design philosophy
- [Skill Reference](../../reference/skills.md) -- Skill specifications, recipes, hook IDs
- [The Gold Standard](gold-standard.md) -- PLUMB framework, TSP-1, TSP-2
- [OS Reference](../../reference/os.md) -- Runtime bindings and per-OS registry
