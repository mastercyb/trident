# The Trident Standard Library: Complete Architecture

## std.* — A Unified Framework for Verifiable Intelligence, Privacy, and Quantum Computation

---

## The Shape of the Library

```
                        ┌─────────────────────┐
                        │    Applications      │
                        │  std.agent           │
                        │  std.defi            │
                        │  std.science         │
                        └──────────┬──────────┘
                                   │
              ┌────────────────────┼────────────────────┐
              │                    │                     │
    ┌─────────┴────────┐ ┌────────┴────────┐ ┌─────────┴────────┐
    │   Intersections   │ │                 │ │                  │
    │ std.nn_quantum    │ │ std.nn_private  │ │ std.quantum_priv │
    │ (Quantum ML)      │ │ (Private AI)    │ │ (Quantum Crypto) │
    └─────────┬────────┘ └────────┬────────┘ └─────────┬────────┘
              │                    │                     │
    ┌─────────┴────────┐ ┌────────┴────────┐ ┌─────────┴────────┐
    │   Three Pillars   │ │                 │ │                  │
    │ std.nn            │ │ std.private     │ │ std.quantum      │
    │ (Intelligence)    │ │ (Privacy)       │ │ (Quantum)        │
    └─────────┬────────┘ └────────┬────────┘ └─────────┬────────┘
              │                    │                     │
              └────────────────────┼────────────────────┘
                                   │
                        ┌──────────┴──────────┐
                        │ Token Infrastructure │
                        │  std.token           │
                        │  std.coin            │
                        │  std.card            │
                        │  std.skill           │
                        └──────────┬──────────┘
                                   │
                        ┌──────────┴──────────┐
                        │    Foundation        │
                        │  std.field           │
                        │  std.math            │
                        │  std.data            │
                        │  std.graph           │
                        │  std.crypto          │
                        │  std.io              │
                        └─────────────────────┘
```

---

## Layer 0: Foundation

Everything builds on this. These modules provide the mathematical and data infrastructure that all three pillars require.

### std.field — Prime Field Arithmetic

The bedrock. Every computation in Trident reduces to operations over the Goldilocks field $\mathbb{F}_p$ where $p = 2^{64} - 2^{32} + 1$.

```
std.field
├── core            Field type, add, mul, sub, inv, neg, pow
├── ext2            F_{p^2} quadratic extension (complex amplitudes)
├── ext3            F_{p^3} cubic extension (STARK soundness)
├── batch           Batched field operations (SIMD-style parallelism)
├── poly            Polynomial arithmetic over F_p
│   ├── eval        Evaluation, multi-point evaluation
│   ├── interp      Lagrange interpolation
│   ├── ntt         Number Theoretic Transform (FFT over F_p)
│   ├── inv_ntt     Inverse NTT
│   └── commit      Polynomial commitment (FRI)
├── matrix          Matrix operations over F_p and extensions
│   ├── mul         Matrix multiplication
│   ├── transpose   Transpose
│   ├── inv         Matrix inversion (via adjugate or LU over F_p)
│   ├── det         Determinant
│   └── decomp      LU, QR decomposition in field arithmetic
└── random          Deterministic PRG over F_p (for reproducibility)
                    + divine()-based randomness injection
```

### std.math — Mathematical Utilities

Higher-level mathematical functions built on field arithmetic.

```
std.math
├── arithmetic      Modular arithmetic beyond F_p (arbitrary moduli via CRT)
├── number_theory   GCD, Legendre symbol, quadratic residues, primitive roots
├── combinatorics   Binomial coefficients, permutations, combinations in F_p
├── statistics      Mean, variance, covariance, correlation — all in F_p
│   ├── descriptive Central moments, quantiles (via sorting networks)
│   ├── regression  Linear regression over F_p (least squares via matrix ops)
│   └── sampling    Reservoir sampling, stratified sampling with divine()
├── optimization    Optimization algorithms over F_p
│   ├── gradient    Gradient descent, Adam, RMSprop — all field arithmetic
│   ├── linear_prog Simplex method over F_p (exact, no floating-point)
│   ├── convex      Convex optimization (projected gradient, ADMM)
│   └── combinat    Branch and bound, simulated annealing via divine()
├── linalg          Linear algebra beyond basic matrix ops
│   ├── eigen       Eigenvalue computation over F_p (characteristic polynomial)
│   ├── svd         Singular value decomposition (iterative, over F_p)
│   ├── solve       Linear system solving (Gaussian elimination, exact)
│   └── sparse      Sparse matrix operations (CSR/CSC formats)
├── approx          Function approximation in F_p
│   ├── poly_approx Polynomial approximation of transcendentals
│   ├── lookup      Lookup table construction and interpolation
│   ├── piecewise   Piecewise polynomial approximation
│   └── minimax     Minimax approximation for optimal field representations
└── constants       Precomputed field constants (roots of unity, sqrt_inv, etc.)
```

### std.data — Data Structures over $\mathbb{F}_p$

Provable computation needs provable data structures.

```
std.data
├── array           Fixed-size arrays (compile-time bounded)
│   ├── sort        Sorting networks (Batcher, bitonic — bounded depth)
│   ├── search      Binary search, interpolation search
│   └── aggregate   Reduce, scan, map — all bounded-loop
├── vector          Variable-length vectors with capacity bound
├── matrix          2D array with row/column operations
├── tensor          N-dimensional tensors (for neural network weights)
│   ├── reshape     View manipulation without data movement
│   ├── slice       Subview extraction
│   ├── broadcast   Broadcasting rules (NumPy-compatible semantics)
│   └── einsum      Einstein summation (general tensor contraction)
├── tree            Merkle trees and authenticated data structures
│   ├── merkle      Standard Merkle tree over Tip5/Poseidon2
│   ├── sparse      Sparse Merkle tree (for large key spaces)
│   ├── append_only Append-only Merkle tree (for logs, histories)
│   └── indexed     Indexed Merkle tree (for efficient updates)
├── map             Key-value maps (hash map over F_p)
│   ├── fixed       Fixed-capacity hash map (compile-time size)
│   └── merkle_map  Merkle-authenticated key-value store
├── accumulator     Cryptographic accumulators
│   ├── rsa         RSA accumulator (membership proofs)
│   └── hash        Hash-based accumulator
├── commitment      Vector commitments
│   ├── merkle      Merkle vector commitment
│   ├── poly        Polynomial commitment (KZG-like over F_p)
│   └── inner_prod  Inner product argument
└── encoding        Serialization / deserialization
    ├── field       Pack/unpack bytes ↔ field elements
    ├── utf8        UTF-8 string handling in F_p
    └── json        JSON parsing into field element structures
```

### std.graph — Graph Algorithms over $\mathbb{F}_p$

Graphs are central to knowledge graphs (Bostrom), social networks, and quantum walk algorithms.

```
std.graph
├── types           Graph representation types
│   ├── adjacency   Adjacency matrix (dense, over F_p)
│   ├── sparse      Sparse adjacency (CSR/COO)
│   ├── edge_list   Edge list representation
│   └── weighted    Weighted graph (edge weights in F_p)
├── algorithms
│   ├── traversal   BFS, DFS (bounded-depth)
│   ├── shortest    Dijkstra, Bellman-Ford over F_p weights
│   ├── pagerank    PageRank / CyberRank (iterative, field arithmetic)
│   ├── spectral    Spectral analysis (eigenvalues of adjacency/Laplacian)
│   ├── matching    Maximum matching (bounded algorithms)
│   ├── flow        Maximum flow / minimum cut
│   └── community   Community detection (spectral, label propagation)
├── random_walk     Classical random walks on graphs
│   ├── standard    Standard random walk
│   ├── lazy        Lazy random walk
│   └── metropolis  Metropolis-Hastings walk
└── quantum_walk    Quantum walks on graphs (bridges to std.quantum)
    ├── coined      Coined quantum walk
    ├── szegedy     Szegedy quantum walk
    └── continuous  Continuous-time quantum walk
```

### std.crypto — Cryptographic Primitives

The security foundation. Most of this already exists in Triton VM; the stdlib exposes it cleanly.

```
std.crypto
├── hash            Hash functions
│   ├── tip5        Tip5 (algebraic hash, STARK-native)
│   ├── poseidon2   Poseidon2 (alternative algebraic hash)
│   ├── rescue      Rescue-Prime (alternative)
│   └── sponge      Sponge construction (generic over permutation)
├── commitment      Commitment schemes
│   ├── pedersen    Pedersen commitment (additive homomorphic)
│   ├── hash_commit Hash-based commitment
│   └── vector      Vector commitment (batched)
├── signature       Digital signatures
│   ├── schnorr     Schnorr signatures over F_p
│   ├── bls         BLS signatures (if pairing available)
│   └── hash_sig    Hash-based signatures (SPHINCS+, post-quantum)
├── merkle          Merkle tree operations (shared with std.data.tree)
├── nullifier       Nullifier computation (for UTXO privacy)
├── proof           STARK proof primitives
│   ├── fri         FRI protocol components
│   ├── air         Algebraic Intermediate Representation
│   ├── verify      STARK verifier (for recursive proofs)
│   └── recursive   Recursive proof composition
└── pq              Post-quantum primitives
    ├── lattice     Lattice-based constructions (if needed)
    └── hash_based  Hash-based constructions (primary)
```

### std.io — Blockchain and External Interaction

How Trident programs interact with the world.

```
std.io
├── pub_input       Public inputs (visible to verifier)
├── pub_output      Public outputs (visible to verifier)
├── divine          Witness injection (private, prover-only)
│   ├── value       Single field element
│   ├── array       Array of field elements
│   ├── struct      Structured witness data
│   └── oracle      Oracle query (for Grover, optimization)
├── storage         On-chain state access
│   ├── read        Read from authenticated storage
│   ├── write       Write to authenticated storage
│   └── merkle_auth Merkle-authenticated state transitions
├── call            Contract-to-contract calls
│   ├── internal    Call within same VM
│   └── cross_chain Cross-chain message passing (Level 1 compatible)
└── time            Block time, timestamps (public inputs)
```

---

## Layer 0.5: Token Infrastructure

Tokens are the economic foundation. While Layer 0 provides mathematical and cryptographic primitives, and Layer 1 provides computational pillars, the token layer provides the economic substrate — standards for value transfer, unique asset ownership, and composable token behaviors.

All token modules build on the PLUMB framework (Pay, Lock, Update, Mint, Burn) — five operations that cover every token lifecycle event. See the [PLUMB reference](plumb.md) for the shared framework, [TSP-1 Coin](tsp1-coin.md) and [TSP-2 Card](tsp2-card.md) for standard-specific constraints.

### std.token — PLUMB Framework Primitives

The shared foundation for all token standards. Defines the config model, leaf structure, authorization, hook system, and proof composition.

```
std.token
├── config          Token configuration (5 authorities + 5 hooks)
│   ├── authority   Authority types (disabled, required, optional)
│   ├── hook        Hook program references (content hash or registry name)
│   └── validate    Config hash computation and verification
├── leaf            Token leaf structure (10-field standard layout)
│   ├── read        Leaf field access
│   ├── write       Leaf field mutation (within circuit constraints)
│   └── hash        Leaf hash computation for Merkle inclusion
├── auth            Authorization primitives
│   ├── verify      Auth hash verification (divine + hash + assert)
│   ├── dual        Dual authorization (account + config authority)
│   └── controller  Controller-based authorization
├── hook            Hook system
│   ├── signal      Signal a hook program for proof composition
│   ├── compose     Compose multiple hook proofs
│   └── verify      Verify hook proof is valid for operation
├── tree            Merkle tree operations for token state
│   ├── include     Inclusion proof (leaf exists in tree)
│   ├── update      Update proof (old leaf → new leaf)
│   └── root        Root computation and verification
└── event           Standard token events
    ├── nullifier   Nullifier emission (UTXO consumption)
    ├── supply      Supply change tracking
    └── state       State transition logging
```

### std.coin — TSP-1 Coin Standard

Divisible value transfer. Conservation law: `sum(balances) = supply`. Every operation that changes a balance must preserve total supply (except mint and burn, which adjust it).

```
std.coin
├── account         Account leaf (10 fields: account_id, balance, nonce, auth_hash,
│   │                lock_until, controller, locked_by, lock_data, reserved x2)
│   ├── create      Account creation with initial balance
│   ├── read        Account field access
│   └── validate    Account leaf invariant checking
├── ops             PLUMB operations for coins
│   ├── pay         Transfer: debit sender, credit receiver, preserve sum
│   ├── lock        Time-lock: extend lock_until, set locked_by
│   ├── update      Config update: admin-only, rehash config
│   ├── mint        Create value: credit recipient, increase supply
│   └── burn        Destroy value: debit holder, decrease supply
├── conservation    Supply conservation enforcement
│   ├── check       Verify sum(inputs) = sum(outputs) ± mint/burn
│   └── supply      Global supply tracking (supply tree)
├── metadata        Token metadata
│   ├── name        Token name and symbol
│   ├── decimals    Decimal precision
│   └── supply_cap  Maximum supply (if capped)
└── events          Coin-specific events
    ├── transfer    Balance transfer event
    ├── mint        Supply increase event
    └── burn        Supply decrease event
```

See [TSP-1 Coin reference](tsp1-coin.md) for the complete specification.

### std.card — TSP-2 Card Standard

Unique asset ownership. Conservation law: `owner_count(id) = 1`. Every asset has exactly one owner at all times.

```
std.card
├── asset           Asset leaf (10 fields: asset_id, owner_id, nonce, auth_hash,
│   │                lock_until, collection_id, metadata_hash, royalty_bps, creator_id, flags)
│   ├── create      Asset creation at mint
│   ├── read        Asset field access
│   └── validate    Asset leaf invariant checking
├── ops             PLUMB operations for cards
│   ├── pay         Transfer ownership: change owner_id, enforce royalties
│   ├── lock        Time-lock: extend lock_until
│   ├── update      Metadata update: change metadata_hash (if UPDATABLE flag set)
│   ├── mint        Create asset: assign asset_id, owner, creator, flags (permanent)
│   └── burn        Destroy asset: remove from tree (if BURNABLE flag set)
├── flags           Asset capability flags (set at mint, immutable)
│   ├── TRANSFERABLE  Can be transferred (bit 0)
│   ├── BURNABLE      Can be burned (bit 1)
│   ├── UPDATABLE     Metadata can change (bit 2)
│   ├── LOCKABLE      Can be time-locked (bit 3)
│   └── MINTABLE      Collection can mint more (bit 4)
├── collection      Collection management
│   ├── create      Create collection with config
│   ├── metadata    Collection-level metadata
│   └── supply      Collection supply tracking
└── events          Card-specific events
    ├── transfer    Ownership transfer event
    ├── metadata    Metadata update event
    ├── mint        Asset creation event
    └── burn        Asset destruction event
```

See [TSP-2 Card reference](tsp2-card.md) for the complete specification.

### std.skill — Composable Token Skills

Skills are composable packages that teach tokens new behaviors through the PLUMB hook system. The `std.skill` module ships 23 official skill implementations with the compiler. Each skill is importable Trident source — developers can use them directly, fork and customize, or deploy modified versions to the on-chain registry.

Three usage modes for any skill:
- **Import**: `use std.skill.liquidity` — inline the skill code at compile time
- **Fork**: Copy the source, modify it, compile your own version
- **Deploy**: Publish a compiled skill to the OS's [on-chain registry](os.md#per-os-on-chain-registry), reference it by content hash or name in token config hooks

```
std.skill
├── core                        Skills most tokens want
│   ├── supply_cap              Fixed maximum supply
│   ├── delegation              Authorized third-party operations
│   ├── vesting                 Time-released token distribution
│   ├── royalties               Creator royalties on Card transfers
│   ├── multisig                Multi-signature authorization
│   └── timelock                Time-delayed operations
├── financial                   DeFi capabilities
│   ├── liquidity               Automated market making (TIDE)
│   ├── oracle                  Price feed integration (COMPASS)
│   ├── vault                   Yield-bearing token wrappers
│   ├── lending                 Collateralized lending
│   ├── staking                 Stake-for-reward mechanisms
│   └── stablecoin              Peg maintenance
├── access                      Compliance and permissions
│   ├── compliance              Whitelist/blacklist enforcement
│   ├── kyc_gate                KYC verification gate
│   ├── transfer_limits         Per-transaction and periodic limits
│   ├── controller_gate         Institutional custody controls
│   ├── soulbound               Non-transferable binding
│   └── fee_on_transfer         Automatic fee collection
└── composition                 Cross-token interaction
    ├── bridging                Cross-OS asset bridging
    ├── subscription            Recurring payment streams
    ├── burn_to_redeem          Burn one token to receive another
    ├── governance              Voting and proposal systems
    └── batch                   Atomic multi-operation bundles
```

See the [Skill Library](../docs/explanation/skill-library.md) for detailed specifications of all 23 skills, recipes, and proof composition architecture.

---

## Layer 1: The Three Pillars

### std.nn — Intelligence

Neural network primitives. Everything is field arithmetic. Everything is provable.

```
std.nn
├── layer                   Neural network layers
│   ├── linear              Dense layer: y = Wx + b over F_p
│   ├── conv1d              1D convolution
│   ├── conv2d              2D convolution
│   ├── depthwise_conv      Depthwise separable convolution
│   ├── embedding           Token embedding (lookup table)
│   ├── positional          Positional encoding over F_p
│   └── recurrent           GRU/LSTM cells (bounded unroll)
│
├── attention               Transformer components
│   ├── scaled_dot_product  Core attention: softmax(QK^T/√d)V
│   ├── multi_head          Multi-head attention
│   ├── causal_mask         Causal masking for autoregressive models
│   ├── flash               Memory-efficient attention (chunked)
│   ├── cross               Cross-attention (encoder-decoder)
│   └── rotary              Rotary position embeddings (RoPE) in F_p
│
├── activation              Nonlinear activation functions
│   ├── relu                ReLU via lookup table
│   ├── gelu                GELU via lookup table
│   ├── silu                SiLU/Swish via lookup table
│   ├── softmax             Softmax via field exp + normalization
│   ├── sigmoid             Sigmoid via lookup table
│   ├── tanh                Tanh via lookup table
│   ├── tip5_sbox           Tip5 S-box as activation (crypto-native)
│   └── custom              User-defined lookup table activation
│
├── norm                    Normalization layers
│   ├── layer_norm          LayerNorm: (x - μ) / σ in F_p
│   ├── batch_norm          BatchNorm with running statistics
│   ├── rms_norm            RMSNorm (simpler, used in LLaMA)
│   └── group_norm          GroupNorm
│
├── loss                    Loss functions
│   ├── cross_entropy       Cross-entropy over F_p
│   ├── mse                 Mean squared error
│   ├── mae                 Mean absolute error
│   ├── kl_divergence       KL divergence
│   └── contrastive         Contrastive loss (for embeddings)
│
├── optim                   Optimizers (training in F_p)
│   ├── sgd                 Stochastic gradient descent
│   ├── adam                Adam optimizer over F_p
│   ├── rmsprop             RMSprop
│   ├── schedule            Learning rate scheduling
│   └── gradient            Gradient computation
│       ├── backprop        Standard backpropagation
│       ├── param_shift     Parameter shift rule (for quantum layers)
│       └── finite_diff     Finite difference (for non-differentiable layers)
│
├── model                   Pre-built model architectures
│   ├── mlp                 Multi-layer perceptron
│   ├── cnn                 Convolutional neural network
│   ├── transformer         Transformer (encoder, decoder, enc-dec)
│   ├── diffusion           Diffusion model components
│   └── gnn                 Graph neural network
│       ├── gcn             Graph Convolutional Network
│       ├── gat             Graph Attention Network
│       └── message_pass    Generic message passing
│
├── data                    Data handling for ML
│   ├── dataset             Dataset abstraction (bounded iteration)
│   ├── batch               Batching with padding
│   ├── augment             Data augmentation (deterministic, provable)
│   └── tokenize            Tokenization (BPE, WordPiece) in F_p
│
└── onnx                    ONNX interoperability
    ├── import              ONNX → Trident model
    ├── export              Trident model → ONNX
    └── ops                 ONNX operator mappings
        ├── supported       Operator support matrix
        └── custom          Custom operator registration
```

### std.private — Privacy

Zero-knowledge privacy primitives. Not just "ZK proofs" — a complete toolkit for building private applications.

```
std.private
├── witness                 Private data management
│   ├── inject              Inject private witness (wraps divine())
│   ├── constrain           Constrain witness values
│   ├── range_proof         Prove value in range without revealing it
│   └── membership          Prove set membership without revealing element
│
├── identity                Identity and credential systems
│   ├── credential          Anonymous credential issuance and verification
│   ├── selective_disclose  Reveal only specific attributes
│   ├── age_proof           Prove age > threshold without revealing DOB
│   ├── identity_commit     Commit to identity without revealing it
│   └── revocation          Credential revocation (via accumulators)
│
├── transaction             Private value transfer
│   ├── utxo                UTXO-based private transactions
│   ├── nullifier           Nullifier management (prevent double-spend)
│   ├── amount_hiding       Hidden transaction amounts
│   ├── sender_hiding       Hidden sender identity
│   ├── receiver_hiding     Hidden receiver identity
│   └── script_hiding       Hidden lock/type scripts
│
├── computation             Private computation patterns
│   ├── blind               Blind computation (compute on data you can't see)
│   ├── mpc                 Multi-party computation building blocks
│   │   ├── secret_share    Secret sharing over F_p
│   │   ├── threshold       Threshold schemes
│   │   └── garbled         Garbled circuit components
│   ├── auction             Private auction protocols
│   │   ├── sealed_bid      Sealed-bid auction
│   │   ├── vickrey         Second-price auction
│   │   └── combinatorial   Combinatorial auction
│   └── voting              Private voting
│       ├── ballot          Ballot creation and encryption
│       ├── tally           Verifiable tallying
│       └── eligibility     Voter eligibility proofs
│
├── data                    Private data operations
│   ├── private_set_ops     Private set intersection, union, difference
│   ├── private_compare     Compare private values (>, <, ==)
│   ├── private_aggregate   Aggregate private data (sum, mean without revealing individual values)
│   └── private_search      Search over private data (index without revealing query)
│
├── compliance              Regulatory compliance with privacy
│   ├── audit_trail         Auditable but private transaction history
│   ├── selective_audit     Allow auditor to see specific fields only
│   ├── threshold_report    Report when aggregate exceeds threshold
│   └── sanctions_check     Prove address not on sanctions list (without revealing address)
│
└── proof                   Proof management
    ├── compose             Proof composition (combine multiple proofs)
    ├── recursive           Recursive proof (proof of proof)
    ├── aggregate           Aggregate multiple proofs into one
    └── selective           Selective disclosure from existing proof
```

### std.quantum — Quantum Power

Quantum computing primitives with dual compilation: classical simulation (Triton VM + STARK) and quantum execution (Cirq/hardware).

```
std.quantum
├── state                   Quantum state management
│   ├── qstate              Qstate<N, D> type (amplitudes in F_{p^2})
│   ├── init                State initialization (|0⟩, uniform, custom)
│   ├── normalize           State normalization
│   ├── fidelity            State fidelity computation
│   ├── entropy             Von Neumann entropy
│   └── partial_trace       Partial trace (reduce subsystem)
│
├── gate                    Quantum gate library
│   ├── pauli               Generalized Pauli gates (X, Z for prime dim)
│   ├── hadamard            Generalized Hadamard (QFT on single qudit)
│   ├── phase               Phase gates (parametrized)
│   ├── rotation            Rotation gates (arbitrary axis)
│   ├── controlled          Controlled gates (arbitrary control values)
│   ├── swap                SWAP and sqrt-SWAP
│   ├── toffoli             Generalized Toffoli (multi-controlled)
│   └── custom              User-defined unitary (matrix specification)
│
├── circuit                 Circuit construction and manipulation
│   ├── builder             Circuit builder API
│   ├── compose             Sequential composition
│   ├── parallel            Parallel composition (tensor product)
│   ├── inverse             Circuit inversion (adjoint)
│   ├── control             Add control qudits to existing circuit
│   ├── optimize            Gate cancellation, commutation, fusion
│   └── depth               Circuit depth analysis
│
├── measure                 Measurement
│   ├── computational       Measurement in computational basis
│   ├── arbitrary           Measurement in arbitrary basis
│   ├── partial             Measure subset of qudits
│   ├── expectation         Expectation value of observable
│   └── sample              Repeated sampling (divine()-based)
│
├── algorithm               Standard quantum algorithms
│   ├── qft                 Quantum Fourier Transform
│   ├── grover              Grover's search
│   │   ├── search          Basic search
│   │   ├── amplitude_amp   Amplitude amplification (generalized)
│   │   └── counting        Quantum counting
│   ├── phase_est           Quantum Phase Estimation
│   ├── walk                Quantum walks (bridges std.graph)
│   │   ├── discrete        Discrete-time quantum walk
│   │   ├── continuous      Continuous-time quantum walk
│   │   └── search          Quantum walk search
│   ├── shor                Shor's factoring (period finding subroutine)
│   ├── hhl                 HHL linear systems algorithm
│   └── swap_test           SWAP test (state comparison)
│
├── chemistry               Quantum chemistry
│   ├── hamiltonian         Molecular Hamiltonian construction
│   │   ├── molecular       Electronic structure Hamiltonians
│   │   ├── ising           Ising model Hamiltonians
│   │   └── hubbard         Hubbard model
│   ├── ansatz              Variational circuit ansatze
│   │   ├── uccsd           Unitary Coupled Cluster
│   │   ├── hardware_eff    Hardware-efficient ansatz
│   │   └── adapt           ADAPT-VQE ansatz construction
│   └── vqe                 Variational Quantum Eigensolver
│
├── optimization            Quantum optimization
│   ├── qaoa                QAOA
│   │   ├── maxcut          MaxCut problem
│   │   ├── portfolio       Portfolio optimization
│   │   └── scheduling      Job scheduling
│   ├── quantum_annealing   Simulated quantum annealing (classical sim)
│   └── grover_opt          Grover-based optimization
│
├── error                   Error models and mitigation
│   ├── noise_model         Depolarizing, dephasing, amplitude damping
│   ├── error_correct       Qudit error correction codes
│   ├── mitigation          Error mitigation techniques
│   │   ├── zne             Zero-noise extrapolation
│   │   ├── pec             Probabilistic error cancellation
│   │   └── dd              Dynamical decoupling
│   └── tomography          State tomography (characterize quantum state)
│
└── compile                 Compilation backends
    ├── simulate            Classical state vector simulation
    ├── cirq                Google Cirq (qutrit/qudit circuits)
    ├── quforge             QuForge (GPU-accelerated simulation)
    └── hardware            Hardware-specific compilation
        ├── trapped_ion     Innsbruck trapped-ion native gates
        ├── supercond       Superconducting transmon native gates
        └── photonic        Photonic quantum computing
```

---

## Layer 2: The Intersections

This is where the real power emerges. Each intersection combines two pillars to create capabilities impossible with either alone.

### std.nn_private — Private AI

The intersection of intelligence and privacy. Verifiable machine learning where models and/or data remain secret.

```
std.nn_private
├── inference               Private inference patterns
│   ├── private_model       Inference with private weights (model IP protected)
│   ├── private_input       Inference with private data (user privacy)
│   ├── private_both        Both model and input private
│   └── selective_reveal    Reveal specific intermediate values to auditor
│
├── training                Private training
│   ├── private_data        Train on data prover can see, verifier can't
│   ├── federated           Federated learning over F_p
│   │   ├── aggregate       Secure aggregation of gradients
│   │   ├── differential    Differential privacy in F_p
│   │   └── verify          Verify each participant's contribution
│   ├── proof_of_training   Prove model trained on claimed data/hyperparams
│   └── proof_of_accuracy   Prove model achieves claimed accuracy
│
├── marketplace             Model marketplace primitives
│   ├── model_commit        Commit to model without revealing weights
│   ├── accuracy_proof      Prove accuracy on test set (test set public or private)
│   ├── inference_service   On-chain inference with private weights
│   ├── payment             Pay-per-inference smart contracts
│   └── licensing           Proof of model provenance and licensing
│
├── fairness                Provable model fairness
│   ├── demographic_parity  Prove equal outcomes across groups
│   ├── equalized_odds      Prove equal error rates across groups
│   ├── feature_exclusion   Prove protected features not used
│   └── counterfactual      Prove decision unchanged if protected attribute changed
│
├── robustness              Provable model robustness
│   ├── adversarial_cert    Certify no adversarial example within ε-ball
│   ├── backdoor_detect     Prove model free of backdoor triggers
│   └── distribution_shift  Detect and prove distribution shift
│
└── explainability          Provable explanations
    ├── feature_importance  Prove which features drove the decision
    ├── attention_map       Prove attention distribution (for transformers)
    ├── counterfactual      Prove minimal input change to flip decision
    └── reasoning_trace     Full execution trace as explanation (STARK-native)
```

### std.nn_quantum — Quantum Machine Learning

The intersection of intelligence and quantum computing. Neural networks that leverage quantum mechanical effects.

```
std.nn_quantum
├── encoding                Classical data → quantum state
│   ├── amplitude           Amplitude encoding (exponential compression)
│   ├── angle               Angle encoding (rotation gates)
│   ├── basis               Basis encoding (computational basis)
│   ├── iqp                 IQP encoding (instantaneous quantum polynomial)
│   └── kernel              Quantum kernel feature map
│
├── layer                   Quantum neural network layers
│   ├── variational         Parametrized rotation + entangling
│   ├── strongly_entangling Strongly entangling layers
│   ├── random              Random quantum circuit layers
│   ├── convolution         Quantum convolution (periodic structure)
│   └── pooling             Quantum pooling (measurement + reduction)
│
├── model                   Quantum model architectures
│   ├── qnn                 Pure quantum neural network
│   ├── hybrid              Hybrid classical-quantum model
│   │   ├── classical_head  Classical input → quantum body → classical output
│   │   ├── quantum_head    Quantum input → classical body
│   │   └── interleaved     Alternating classical and quantum layers
│   ├── qkernel             Quantum kernel methods
│   │   ├── qsvm            Quantum support vector machine
│   │   └── qgpr            Quantum Gaussian process regression
│   ├── qgan                Quantum generative adversarial network
│   ├── qbm                 Quantum Boltzmann machine
│   └── qtransformer        Quantum-enhanced transformer
│       ├── quantum_attn    Quantum attention mechanism
│       └── quantum_ffn     Quantum feed-forward network
│
├── train                   Quantum model training
│   ├── param_shift         Parameter shift rule for gradients
│   ├── adjoint             Adjoint differentiation
│   ├── spsa                Simultaneous perturbation stochastic approx
│   ├── natural_gradient    Quantum natural gradient
│   └── barren_plateau      Barren plateau detection and mitigation
│
├── advantage               Quantum advantage analysis
│   ├── expressibility      Circuit expressibility metrics
│   ├── entangling_power    Entanglement generation capacity
│   ├── classical_shadow    Classical shadow tomography for efficiency
│   └── kernel_alignment    Quantum vs classical kernel comparison
│
└── application             Domain-specific quantum ML
    ├── molecular_property  Molecular property prediction
    ├── drug_binding        Drug-target binding affinity
    ├── financial_opt       Financial portfolio optimization
    ├── graph_classify      Graph classification (molecular, social)
    └── anomaly_detect      Quantum anomaly detection
```

### std.quantum_private — Quantum Cryptography

The intersection of quantum computing and privacy. Post-quantum protocols, quantum key distribution, quantum-secure computation.

```
std.quantum_private
├── qkd                     Quantum Key Distribution
│   ├── bb84                BB84 protocol
│   ├── e91                 E91 (entanglement-based)
│   ├── b92                 B92 (simplified)
│   ├── sifting              Key sifting (matching bases)
│   ├── error_est           Quantum bit error rate estimation
│   └── privacy_amp         Privacy amplification
│
├── quantum_commit          Quantum commitment schemes
│   ├── qubit_commit        Commitment using quantum states
│   ├── string_commit       Quantum string commitment
│   └── timed               Timed quantum commitment (auto-reveal)
│
├── quantum_coin            Quantum coin flipping
│   ├── strong              Strong coin flipping
│   └── weak                Weak coin flipping
│
├── quantum_oblivious       Quantum oblivious transfer
│   ├── one_of_two          1-out-of-2 oblivious transfer
│   └── rabin               Rabin oblivious transfer
│
├── quantum_random          Quantum randomness
│   ├── qrng                Quantum random number generation
│   ├── certifiable         Certifiable randomness (Bell test + proof)
│   ├── beacon              Quantum random beacon (on-chain)
│   └── vrf                 Verifiable random function (quantum-enhanced)
│
├── pq_crypto               Post-quantum classical cryptography
│   ├── hash_sig            Hash-based signatures (STARK-native)
│   ├── lattice             Lattice-based constructions
│   │   ├── kyber           Kyber key encapsulation
│   │   └── dilithium       Dilithium signatures
│   └── code_based          Code-based cryptography (McEliece)
│
└── quantum_mpc             Quantum multi-party computation
    ├── quantum_secret_share Quantum secret sharing
    ├── verifiable_qc       Verifiable quantum computation
    │   ├── blind           Blind quantum computing (compute without seeing)
    │   └── verified        Verified delegated quantum computing
    └── quantum_auction     Quantum sealed-bid auction (no-cloning security)
```

---

## Layer 3: Applications

Pre-built application modules that compose foundation, pillars, and intersections.

### std.agent — Autonomous Verifiable Agents

```
std.agent
├── core                    Agent framework
│   ├── perceive            Perception: sensor data → features (std.nn)
│   ├── reason              Reasoning: features → plan (std.nn.attention)
│   ├── decide              Decision: plan → action (std.nn + std.math.optimization)
│   ├── act                 Action: execute on-chain (std.io)
│   └── prove               Proof: entire cycle → STARK
│
├── policy                  Policy management
│   ├── frozen              Frozen policy (weights committed, immutable)
│   ├── adaptive            Adaptive policy (on-chain learning, proven updates)
│   ├── multi_agent         Multi-agent coordination (game-theoretic)
│   └── hierarchical        Hierarchical policies (meta-policy selects sub-policy)
│
├── safety                  Provable agent safety
│   ├── constraint          Hard constraints on actions (proven in STARK)
│   ├── invariant           State invariants (never violated)
│   ├── budget              Resource budgets (gas, value, risk)
│   └── kill_switch         Provable shutdown conditions
│
├── memory                  Agent memory
│   ├── episodic            Experience replay (Merkle-authenticated)
│   ├── semantic            Knowledge base (graph, bridges std.graph)
│   └── working             Working memory (bounded, proven)
│
└── type                    Agent type specializations
    ├── trading             DeFi trading agent
    ├── keeper              Liquidation / maintenance agent
    ├── oracle              Data oracle agent
    ├── governance          DAO governance agent
    └── search              Knowledge graph search agent (Bostrom)
```

### std.defi — Decentralized Finance

```
std.defi
├── amm                     Automated market makers
│   ├── constant_product    x·y = k (Uniswap v2 style)
│   ├── concentrated        Concentrated liquidity (Uniswap v3 style)
│   ├── curve               StableSwap curve
│   └── quantum_amm         Quantum-optimized liquidity (QAOA pricing)
│
├── lending                 Lending protocols
│   ├── overcollateral      Standard overcollateralized lending
│   ├── undercollateral     Undercollateralized (requires std.nn credit model)
│   ├── liquidation         Liquidation logic
│   └── interest            Interest rate models over F_p
│
├── derivatives             Derivative instruments
│   ├── option              Options (Black-Scholes in F_p, or quantum pricing)
│   ├── future              Futures contracts
│   ├── perpetual           Perpetual swaps
│   └── exotic              Exotic derivatives (quantum Monte Carlo pricing)
│
├── risk                    Risk management
│   ├── var                 Value at Risk (std.nn model + std.private)
│   ├── stress_test         Scenario analysis (proven model execution)
│   ├── correlation         Correlation analysis (std.math.statistics)
│   └── quantum_risk        Quantum-accelerated risk computation
│
└── compliance              DeFi compliance
    ├── kyc_private         Private KYC (prove identity without revealing it)
    ├── aml_check           AML screening (private set intersection)
    ├── reporting           Regulatory reporting (selective disclosure)
    └── audit               Auditable private transactions
```

### std.science — Verifiable Computational Science

```
std.science
├── chemistry               Molecular computation
│   ├── molecule            Molecular specification
│   ├── ground_state        Ground state energy (VQE)
│   ├── dynamics            Molecular dynamics simulation
│   ├── binding             Binding affinity prediction
│   └── reaction            Reaction pathway analysis
│
├── materials               Materials science
│   ├── crystal             Crystal structure analysis
│   ├── band_structure      Electronic band structure
│   ├── thermal             Thermal property computation
│   └── mechanical          Mechanical property prediction
│
├── ecology                 Ecological modeling
│   ├── carbon              Carbon absorption modeling
│   ├── biodiversity        Biodiversity index computation
│   ├── population          Population dynamics
│   └── network             Ecological network analysis (mycorrhizal)
│
├── climate                 Climate modeling
│   ├── atmospheric         Atmospheric chemistry
│   ├── ocean               Ocean circulation
│   └── land_use            Land use change modeling
│
└── certificate             Scientific certificates
    ├── carbon_credit       Proven carbon credit
    ├── biodiversity_token  Proven biodiversity assessment
    ├── material_spec       Proven material specification
    └── drug_candidate      Proven pharmaceutical computation
```

### Module Count

```
Foundation:    6 modules    (field, math, data, graph, crypto, io)
Token:         4 modules    (token, coin, card, skill)
Pillars:       3 modules    (nn, private, quantum)
Intersections: 3 modules    (nn_private, nn_quantum, quantum_private)
Applications:  3 modules    (agent, defi, science)
─────────────────────────────
Total:        19 modules

Estimated submodules:  ~200
Estimated functions:   ~2,000
Estimated LoC:         ~50,000-100,000
```

### Dependency Graph

```
std.agent ──────► std.nn ──────────► std.field
    │                │                   ▲
    │                ▼                   │
    ├───► std.nn_private ──► std.private ┤
    │                │           │       │
    │                ▼           ▼       │
    ├───► std.nn_quantum ──► std.quantum ┤
    │                            │       │
    │                            ▼       │
    └───► std.quantum_private ───────────┤
                                         │
std.coin ───────► std.token ─────────────┤
std.card ───────► std.token              │
std.skill.* ────► std.token              │
std.token ──────► std.crypto ────────────┤
                                         │
std.defi ───────► std.coin ──────────────┤
                  std.card               │
                                         │
std.defi ───────► std.math ──────────────┤
    │                                    │
    ▼                                    │
std.science ────► std.data ──────────────┤
                                         │
                  std.graph ─────────────┤
                                         │
                  std.crypto ────────────┤
                                         │
                  std.io ────────────────┘
```

See [Standard Library Design Philosophy](../docs/explanation/stdlib.md) for
the rationale behind the layer architecture, intersection design, and token
infrastructure decisions.
