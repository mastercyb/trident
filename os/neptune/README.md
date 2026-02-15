# Neptune

[VM: Triton](../../vm/triton/README.md) | [OS Reference](../../reference/os.md) | [Gold Standard](../../docs/explanation/gold-standard.md)

Neptune is a blockchain where every state transition produces a STARK
proof. No trusted setup. No elliptic curves. Quantum-safe by
construction.

Four properties define Neptune:

- **Programmable.** Arbitrary programs compile to Triton VM and execute
  as provable circuits. Lock scripts, type scripts, token standards,
  proof composition — all written in Trident.
- **Private.** UTXO model with encrypted notifications. Senders prove
  correctness without revealing balances, amounts, or addresses to
  validators. The chain validates proofs, not transactions.
- **Mineable.** Proof-of-work consensus. No staking cartel. No
  validator set. Anyone with hardware can mine blocks and earn coinbase.
- **Quantum-safe.** Lattice-based key encapsulation (generation
  addresses), Tip5 hashing over Goldilocks field, no elliptic curve
  assumptions anywhere in the cryptographic stack.

---

## Programming Model

Programs do not call each other. There is no `msg.sender`, no shared
mutable state, no reentrancy. Every program produces an independent STARK
proof. A verifier composes proofs together. Composition is recursive — a
proof can verify another proof inside it, so any chain of proofs
collapses into a single proof.

All state access follows divine-and-authenticate: the prover divines a
value from secret input, then Merkle-authenticates it against a public
root. If authentication fails, the VM crashes — no proof is generated.
The developer writes `kernel.authenticate_fee(hash)`. The proof
machinery is invisible.

Authorization is explicit. The prover divines a secret and proves
knowledge of it: `hash(secret) == expected`. The secret can be a private
key, a Shamir share, a biometric hash, a hardware attestation, or the
output of another ZK proof. This is account abstraction by default.

---

## Transaction Kernel

Every transaction has a kernel — 8 fields organized as a Merkle tree of
height 3:

| Leaf | Field | Description |
|------|-------|-------------|
| 0 | `inputs` | UTXOs being spent (removal records) |
| 1 | `outputs` | New UTXOs being created (addition records) |
| 2 | `announcements` | Public messages (encrypted UTXO notifications) |
| 3 | `fee` | Transaction fee in NPT (u128) |
| 4 | `coinbase` | Block reward (mining transactions only) |
| 5 | `timestamp` | Transaction timestamp |
| 6 | `mutator_set_hash` | Current UTXO set commitment |
| 7 | `merge_bit` | Whether this is a merged transaction |

The kernel MAST hash is the primary public input for all scripts.

---

## Two Script Types

**Lock scripts** guard a UTXO — they prove the right to spend. Public
input: kernel MAST hash (1 Digest = 5 field elements).

**Type scripts** validate conservation rules — they prove that value is
neither created nor destroyed. Public input: 3 Digests (kernel hash,
input UTXOs hash, output UTXOs hash).

---

## Token Standards: The Gold Standard

Neptune's token system is built on PLUMB — Pay, Lock, Update, Mint,
Burn. Five operations, uniform proof structure, composable hooks.

Two standards cover the entire design space:

| Standard | Name | Conservation law |
|----------|------|------------------|
| TSP-1 | Coin | `sum(balances) = supply` |
| TSP-2 | Card | `owner_count(id) = 1` |

Two conservation laws exist in token systems — divisible supply and
unique ownership. These are mathematically incompatible, so they require
separate circuits. Everything else — liquidity, governance, lending,
oracles, royalties — is a skill. Skills compose through hooks. Standards
define what a token *is*. Skills define what a token *does*.

See the [Gold Standard](../../docs/explanation/gold-standard.md) for
PLUMB, circuit constraints, config model, and the hook system. See the
[Skill Library](../../reference/skill-library.md) for the 23
designed skills.

---

## Directory Structure

2,210 lines of Trident across 17 programs, organized in five layers:

### OS Bindings — `use os.neptune.*`

The foundation. Compiler-supported modules that bind Trident programs to
Neptune's runtime: kernel MAST authentication, UTXO verification,
extension field arithmetic, and recursive proof composition primitives.

| File | Lines | What it does |
|------|-------|-------------|
| `kernel.tri` | 91 | Read kernel MAST hash, authenticate individual fields (fee, timestamp, inputs, outputs) via Merkle proofs |
| `utxo.tri` | 19 | Authenticate divined UTXO data against expected digest |
| `xfield.tri` | 28 | Extension field construction, inverse, dot-product steps (XField * XField, XField * BField) |
| `recursive.tri` | 94 | Inner product accumulation, claim reading, FRI commitment verification — building blocks for recursive proof verification |
| `proof.tri` | 160 | End-to-end proof composition: parse claims, hash public I/O, FRI verification chain, inner proof verification, proof aggregation |

### Token Standards — `standards/`

The two PLUMB implementations. Each is a complete token circuit with all
five operations, config management, hook slots, nullifiers, and
conservation law enforcement.

| File | Lines | Standard |
|------|-------|----------|
| `coin.tri` | 535 | TSP-1 — fungible token. Account leaves, balance arithmetic, time-locks, configurable authorities, composable hooks |
| `card.tri` | 746 | TSP-2 — unique asset. Per-asset metadata, royalties, creator immutability, flag-gated operations, collection binding |

### Lock Scripts — `locks/`

Spending authorization programs. Each proves the right to spend a UTXO
by demonstrating knowledge of a secret.

| File | Lines | Mechanism |
|------|-------|-----------|
| `generation.tri` | 33 | Hash-preimage lock (lattice-based KEM, post-quantum) |
| `symmetric.tri` | 22 | 5-field preimage (320-bit entropy, shared symmetric key) |
| `multisig.tri` | 50 | 2-of-3 threshold — prove knowledge of 2 out of 3 preimages |
| `timelock.tri` | 33 | Time-locked UTXO — authenticate timestamp, assert `now >= release` |

### Type Scripts — `types/`

Conservation law enforcement. Each proves that a transaction neither
creates nor destroys value beyond what the rules allow.

| File | Lines | Rule |
|------|-------|------|
| `native_currency.tri` | 46 | NPT conservation: `sum(inputs) + coinbase = sum(outputs) + fee` |
| `custom_token.tri` | 75 | TSP-1 token conservation: `sum(input_balances) = sum(output_balances)` |

### Programs — `programs/`

Standalone programs for transaction orchestration and proof composition.

| File | Lines | Purpose |
|------|-------|---------|
| `transaction_validation.tri` | 119 | Full Neptune transaction verification — validate all lock scripts, type scripts, and kernel integrity |
| `recursive_verifier.tri` | 116 | Complete recursive STARK verifier — verify an inner proof inside the current execution |
| `proof_aggregator.tri` | 28 | Batch N proofs into a single outer proof |
| `proof_relay.tri` | 15 | Verify and forward a single proof (simplest composition program) |

---

## Runtime Parameters

| Parameter | Value |
|-----------|-------|
| VM | Triton (Goldilocks field, 2^64 - 2^32 + 1) |
| Runtime binding | `os.neptune.*` |
| Account model | UTXO |
| Storage | Merkle-authenticated (divine-and-authenticate) |
| Transactions | Proof-based (STARK per script) |
| Cost model | Table rows (proving cost, computed from source) |
| Addresses | Generation (`nolga`, post-quantum) and Symmetric (`nolsa`, shared key) |
| Hashing | Tip5 (algebraic, ZK-native) |

---

## See Also

- [Gold Standard](../../docs/explanation/gold-standard.md) — PLUMB framework, TSP-1/TSP-2 circuits, hook system, proven price
- [Skill Library](../../reference/skill-library.md) — 23 composable token capabilities
- [Programming Model](../../docs/explanation/programming-model.md) — Divine-and-authenticate, stack semantics
- [For Onchain Devs](../../docs/explanation/for-onchain-devs.md) — Mental model migration from Solidity
- [Deploying a Program](../../docs/guides/deploying-a-program.md) — Build and deploy workflows
- [Triton VM](../../vm/triton/README.md) — The underlying provable virtual machine
- [OS Reference](../../reference/os.md) — Portable `os.*` API and per-OS lowering tables
