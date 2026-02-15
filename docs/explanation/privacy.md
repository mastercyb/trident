# The Privacy Trilateral: ZK + FHE + MPC

Three cryptographic technologies combine to provide full-spectrum privacy
over the Goldilocks field.

---

## The Problem

Privacy is three problems wearing one name.

1. **Computational integrity** — prove a result is correct without
   revealing the data that produced it.
2. **Data confidentiality** — compute on data that the computer itself
   cannot see.
3. **Trust distribution** — prevent any single party from having the
   power to compromise the system.

No single cryptographic technology solves all three. Each technology in
the trilateral — ZK, FHE, MPC — solves exactly one, and has a blind spot
that only the other two can fill.

---

## Why One Alone Fails

**ZK alone** proves the result is correct but cannot hide inputs from the
prover. The prover must see the data to generate the proof. ZK hides data
from the *verifier*, not from the *prover*.

**FHE alone** encrypts computation so the processing node never sees
inputs. But it cannot prove the computation was done correctly. The client
receives an encrypted result and must trust that the node actually ran the
correct circuit.

**MPC alone** splits computation across parties so no single party sees
the full input. But it requires all parties to be online, does not produce
a succinct proof for external verifiers, and breaks if enough parties
collude.

Each blind spot is exactly another technology's strength:

```
                       ZK
                     ╱     ╲
               proves       hides
             correctness    witness
                  ╱             ╲
                FHE ─────────── MPC
              hides data      distributes
              from compute    trust

    ZK:  "the answer is correct"
    FHE: "I never saw the question"
    MPC: "no single party saw anything"
```

The triangle is a structural dependency: each vertex requires the other
two to achieve complete privacy.

---

## The Three Technologies

### ZK — Zero-Knowledge Proofs

Prove a statement is true without revealing why it is true.

The prover generates a mathematical proof that a computation was executed
correctly. The proof reveals only the public inputs and the result —
nothing about the private witness. Verification is fast: O(log n) work
regardless of computation size.

Trident targets [STARKs](stark-proofs.md) — hash-based proofs with no
trusted setup and post-quantum security. Every STARK operates over the
Goldilocks field F_p.

**Where ZK applies:**

- **Private transfers.** Prove conservation (inputs = outputs + fee)
  without revealing amounts or owners. The STARK guarantees correctness;
  commitments guarantee privacy.
- **Provable computation.** Every state transition produces a STARK proof.
  Any node verifies any transition without re-executing it. A phone
  verifies what a datacenter computed.
- **Selective disclosure.** Prove properties about state without revealing
  the state. Range proofs, threshold proofs — standard ZK primitives.
- **Recursive verification.** A STARK can verify another STARK inside it.
  Proofs compose: 1,000 transactions collapse into one proof.

### FHE — Fully Homomorphic Encryption

Compute on encrypted data without ever decrypting it.

Data is encrypted under a public key. Arithmetic operations (addition,
multiplication) can be performed directly on ciphertexts. The result,
when decrypted, equals the result of the same operations on plaintexts.

Trident targets TFHE instantiated over the Goldilocks field. The
ciphertext modulus q equals the STARK field characteristic p. The
polynomial ring R_p = F_p[X]/(X^N + 1) used by FHE ciphertexts is a
ring of polynomials with Goldilocks coefficients. FHE operations are
natively field arithmetic — no cross-domain translation.

**Where FHE applies:**

- **Private queries.** Encrypt a query, send it to a node. The node
  performs computation entirely on ciphertexts and returns an encrypted
  result. The node never sees what was queried.
- **Encrypted inference.** A neural network evaluates on FHE-encrypted
  inputs. Linear layers use homomorphic multiply-accumulate. Nonlinear
  activations use Programmable Bootstrapping (PBS).
- **Private links.** Create graph edges where source, target, and weight
  are all encrypted. Aggregate computation still works because focus uses
  only addition (homomorphic) and normalization (achievable via PBS).

PBS is where the [Rosetta Stone](vision.md#the-rosetta-stone) identity
manifests most clearly. PBS evaluates a lookup table on encrypted data by
encoding the function as a test polynomial and blind-rotating it by the
encrypted input. The same lookup table that the STARK uses for proof
authentication and the neural network uses for activation is the FHE
bootstrap function.

### MPC — Multi-Party Computation

Multiple parties jointly compute a function without any party learning
any other party's input.

Each party holds a share of the secret. Parties exchange messages
according to a protocol. At the end, each party learns the output and
nothing else. Security holds as long as fewer than a threshold of parties
collude.

Trident targets Shamir secret sharing over F_p for threshold schemes.
Poseidon2 was chosen specifically for MPC compatibility — its x^7 S-box
has multiplicative depth 3, requiring only 3 communication rounds per
hash evaluation in secret-shared protocols.

**Where MPC applies:**

- **Threshold key management.** The FHE decryption key is split across
  guardians via Shamir sharing. Decryption requires a threshold (e.g.,
  3-of-5). No individual guardian can decrypt alone. The key is born
  distributed and never exists in complete form.
- **Private aggregation.** Multiple parties compute aggregate statistics
  (average stake, total contribution, consensus ranking) without revealing
  individual values.
- **Distributed randomness.** Unpredictable, unbiasable random values
  via MPC-based beacons. Each participant commits to a random value, then
  all values are combined via MPC.

---

## How They Combine

Each pair fills the other's gap. All three together provide full-spectrum
privacy.

### ZK + FHE: Verifiable Encrypted Computation

Compute on data you cannot see. Prove you did it right.

```
1. Client encrypts input under FHE:     ct = Enc(pk, data)
2. Server evaluates circuit on ct:       ct' = Eval(circuit, ct)
3. Server generates STARK proof:         π = Prove(circuit, ct, ct')
4. Client verifies proof:                Verify(π) → accept/reject
5. Client decrypts result:               result = Dec(sk, ct')
```

FHE operations over R_p are arithmetic operations over F_p — the same
operations that STARK constraints express. The STARK proof covers the FHE
evaluation without cross-domain translation.

### ZK + MPC: Distributed Proving

Multiple parties jointly generate a proof without any party seeing the
full witness.

```
1. Each party holds secret share:        [x]_i = share_i(x)
2. Parties run MPC to evaluate circuit:  [y]_i = MPC_Eval(circuit, [x]_i)
3. Parties jointly construct STARK:      π = MPC_Prove([trace]_i)
4. Anyone verifies proof:                Verify(π) → accept/reject
```

### FHE + MPC: Threshold Encrypted Computation

Compute on encrypted data where no single entity can decrypt, ever.

```
1. Key generation via MPC:               (pk, [sk]_i) = MPC_KeyGen()
2. Client encrypts under public key:     ct = Enc(pk, data)
3. Any node evaluates on ciphertext:     ct' = Eval(circuit, ct)
4. Threshold decryption via MPC:         result = MPC_Dec([sk]_i, ct')
```

### ZK + FHE + MPC: The Full Trilateral

All three together. Private verifiable inference on encrypted data:

```
1. MPC key ceremony:     Guardians generate (pk, [sk]_i)
2. FHE encryption:       Alice encrypts data: ct = Enc(pk, data)
3. FHE evaluation:       Node runs model: ct' = Model(ct)
4. STARK proof:          Node generates proof π of correct execution
5. Threshold decryption: Guardians cooperate: result = MPC_Dec([sk]_i, ct')
6. Verification:         Anyone checks Verify(π) → accept
```

Alice's data never exposed (FHE). Result provably correct (ZK). No single
point of key compromise (MPC). Model weights can also be private (FHE on
both sides). Proof is post-quantum secure (STARK). Phone can verify
datacenter's work (O(log n) verification).

---

## Privacy Tiers

Full trilateral privacy is not required for every operation. Privacy is
opt-in and escalating.

### Tier 0 — Transparent

Everything public. ZK only (proof of correctness, not privacy).

### Tier 1 — Private Ownership

Who owns what is hidden. Amounts are hidden. Technologies: ZK with
Poseidon2 commitments and nullifiers. Spending reveals only a nullifier
(preventing double-spend) and creates new commitments. STARK proof
guarantees conservation without revealing values or owners.

### Tier 2 — Private Computation

Inputs and intermediate values hidden even from the computing node.
Technologies: ZK + FHE. User encrypts inputs, node evaluates on
ciphertexts, STARK attests to correct evaluation.

### Tier 3 — Distributed Trust

Keys and secrets distributed across guardians. Technologies: ZK + FHE +
MPC. Threshold key management, distributed randomness, multi-guardian
recovery. For threat models that include physical compromise of individual
nodes.

---

## The Algebraic Foundation

The trilateral holds together because all three technologies operate over
the same field.

| Technology | Algebraic home | Key operation | Field primitive |
|------------|---------------|---------------|-----------------|
| ZK (STARK) | F_p polynomial constraints | FRI commitment | `ntt` + `p2r` |
| FHE (TFHE) | R_p = F_p[X]/(X^N+1) | Programmable Bootstrapping | `ntt` + `lut` |
| MPC (Shamir) | F_p secret shares | Threshold reconstruction | `fma` |

All three use Poseidon2 for commitments and hashing — its x^7 S-box is
efficient in ZK (7 constraints), viable in MPC (depth 3), and evaluable
under FHE. All three benefit from NTT acceleration — the same butterfly
network serves FRI folding (ZK), polynomial multiplication (FHE), and
verifiable secret-share refresh (MPC).

Four hardware primitives accelerate the entire stack:

- `fma` (field multiply-accumulate) — STARK constraints, FHE polynomial
  arithmetic, MPC share recombination
- `ntt` (Number-Theoretic Transform) — FRI commitment, PBS polynomial
  multiply, convolution
- `p2r` (Poseidon2 round) — commitment hashing, nullifier derivation,
  MPC-friendly randomness
- `lut` (lookup table) — STARK lookup argument, FHE test polynomial,
  neural activation

---

## Design Choices

### Why STARKs over SNARKs

SNARKs produce smaller proofs (~200 bytes vs ~200 KB) but require trusted
setup and rely on elliptic curve assumptions that quantum computers break.
STARKs are transparent (no ceremony), hash-based (post-quantum), and native
to the Goldilocks field.

### Why TFHE over BGV/CKKS

TFHE's Programmable Bootstrapping evaluates an arbitrary function during
noise refresh, eliminating separate bootstrapping and evaluation steps.
When instantiated over Goldilocks, PBS uses the same lookup table as the
STARK and the neural network — the Rosetta Stone identity.

### Why Poseidon2 over SHA-256 or Tip5

SHA-256 is 50-100x more expensive inside a STARK circuit. Tip5 is fast in
STARKs but uses lookup-based S-boxes impossible for MPC and FHE. Poseidon2's
x^7 power map is the only S-box design simultaneously efficient in ZK,
viable in MPC, and evaluable under FHE. Optimized at the architecture level,
not the component level.

### Why Goldilocks over BN254 or BabyBear

BN254 is optimized for elliptic curve pairings (quantum-broken). BabyBear
(31-bit) is too small for FHE noise management. Goldilocks is the sweet
spot: 64-bit (one CPU register), prime (proper field), NTT-friendly (2^32
roots of unity), and large enough for FHE. No other field satisfies all
four constraints simultaneously.

---

## Threat Model

| Threat | Technology | Defense |
|--------|-----------|---------|
| Node sees user data | FHE | Computation on encrypted data |
| Node returns wrong result | ZK | STARK proof of correct execution |
| Single key holder compromised | MPC | Threshold key distribution |
| Quantum computer breaks crypto | ZK (STARK) | Hash-based, no elliptic curves |
| Transaction graph surveillance | ZK | Commitments + nullifiers |
| Minority node collusion | MPC | Threshold security |
| Physical server access | FHE + MPC | Data encrypted + key distributed |

---

## See Also

- [Quantum Computing](quantum.md) — the quantum pillar
- [Verifiable AI](trident-ai-zkml-deep-dive.md) — the AI pillar
- [Vision](vision.md) — three revolutions, one field
- [STARK Proofs](stark-proofs.md) — how STARKs work
