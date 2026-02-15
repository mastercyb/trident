# 🥇 The Gold Standard

## ZK-Native Token Standards

Version: 0.1-draft
Date: February 14, 2026

### Status

This document is a design specification — it describes what we want to
build, not what exists today. The PLUMB framework and token standards
(TSP-1, TSP-2) are architecturally complete.

The 0.1 release target is: deploy basic tokens and interact with them.

The Gold Standard is a Trident-level specification. While the reference
implementation targets Neptune, the standards (PLUMB, TSP-1, TSP-2) and
hook architecture are designed to work on any OS that supports Trident's
Level 2 (provable computation).

| Layer | What | Status | Files |
|-------|------|--------|-------|
| OS bindings | Neptune runtime modules | Compiler support | `os/neptune/kernel.tri`, `utxo.tri`, `proof.tri`, `xfield.tri`, `recursive.tri` |
| Type scripts | Value conservation rules | Compiler support | `os/neptune/types/native_currency.tri` (NPT), `custom_token.tri` (TSP-1) |
| Lock scripts | Spending authorization | Compiler support | `os/neptune/locks/generation.tri`, `symmetric.tri`, `timelock.tri`, `multisig.tri` |
| Transaction validation | Full transaction verification | Compiler support | `os/neptune/programs/transaction_validation.tri` |
| Proof composition | Recursive STARK verification | Compiler support | `os/neptune/programs/proof_aggregator.tri`, `proof_relay.tri` |

See the [Tutorial](../tutorials/tutorial.md) for language basics, [Programming Model](programming-model.md) for the Neptune transaction model, and [Deploying a Program](../guides/deploying-a-program.md) for deployment workflows.

---

## 🔭 1. Philosophy

The Gold Standard is not a port of Ethereum's ERC standards. It is
designed from first principles for STARK-provable virtual machines where
every state transition produces a cryptographic proof.

Three axioms drive every decision:

1. Tokens are leaves, not contracts. A token is not a deployed program with storage. It is a leaf in a Merkle tree whose state transitions are constrained by a circuit. The circuit is the standard. The leaf is the instance.

2. Liquidity is never locked. Capital remains in user accounts. DeFi protocols do not custody tokens — they prove valid transformations against user balances via skill composition. One balance can back many strategies simultaneously.

3. Proofs compose, programs don't call. There is no `msg.sender` calling a contract. There is a proof that a valid state transition occurred, composed with proofs from skill programs. Composition replaces invocation.

---

## 🏗️ 2. Architecture Overview

### 2.1 Two Standards

The Gold Standard defines exactly two token standards. Both are built on PLUMB.

| Standard | Name | What it defines | Conservation law |
|----------|------|-----------------|------------------|
| TSP-1 | Coin | Divisible value transfer | `Σ balances = supply` |
| TSP-2 | Card | Unique asset ownership | `owner_count(id) = 1` |

A standard earns its place by defining a conservation law — an invariant that the circuit enforces on every operation. Divisible supply and unique ownership are incompatible conservation laws, so they require separate circuits. Everything else is a skill.

Both standards are available as standard library modules: `std.coin` (TSP-1)
and `std.card` (TSP-2). The shared PLUMB primitives live in `std.token`.
See the [Standard Library reference](../../reference/stdlib.md#layer-05-token-infrastructure).

### 2.2 Skills

A skill is something a token can learn — a composable package of hooks,
optional state, and config that teaches a token a new behavior. Every
PLUMB operation has a hook slot. A skill installs hooks into those slots.
Multiple skills can coexist on the same token — their hook proofs compose
independently.

See the [Skill Library](skill-library.md) for the full catalog of 23
designed skills (Liquidity, Oracle Pricing, Governance, Lending, etc.),
recipes, and proof composition architecture.

All 23 skills ship with the compiler as `std.skill.*` modules — importable
source that developers can use directly, fork, or deploy to an OS's
[Atlas](../../reference/atlas.md).

### 2.3 Why This Is Complete

Two conservation laws exist in token systems. Divisible supply: `Σ balances = supply`. Unique ownership: `owner_count(id) = 1`. These are mathematically incompatible — you cannot enforce both in one circuit without branching that inflates every proof. So there are exactly two standards: TSP-1 and TSP-2.

Everything else a token does — liquidity, oracle pricing, governance, lending, compliance, royalties — is a behavior, not a conservation law. Behaviors compose. Conservation laws don't. A coin that provides liquidity is still a coin. A card that enforces royalties is still a card. The standard defines what the token *is*. Skills define what the token *does*.

This is why two standards plus a skill library covers the entire design space:

- Any divisible asset is TSP-1 + some subset of skills
- Any unique asset is TSP-2 + some subset of skills
- Any DeFi protocol is proof composition between tokens with skills
- Any new financial primitive is a new skill, not a new standard

The model is also complete because tokens are both subjects and objects. A coin can be an acting company — add Governance, Liquidity, Lending, and Staking skills and the token becomes a fully autonomous economic entity that raises capital, trades, lends, and governs itself. A card can be an identity — a name, a reputation, a legal entity, the root of who you are on-chain. The same leaf participates in multiple roles simultaneously through proof composition. No additional primitives are needed because the two standards already cover both sides of every interaction.

A new standard would require a new conservation law — a third mathematical invariant incompatible with both divisible supply and unique ownership. No such invariant exists in token systems. Two is not a simplification. Two is the number.

Both standards share the same foundation: 10-field leaves, same config, same hooks, same proof pipeline. Only constraint polynomials differ. Tooling that understands one understands 90% of the other.

### 2.4 Proven Price

A token knows its supply — the circuit enforces `Σ balances = supply` on every operation. Price should work the same way. In a provable blockchain, every swap is a STARK proof. Price and volume are free byproducts of proven swaps. The question is how to aggregate them into a signal that the token itself can consume.

The answer is protocol fees. Raw volume is trivially inflatable — trade with yourself, back and forth, infinite volume. But every Neptune swap deducts 0.1% (10 basis points) of the trade value in NPT as a protocol fee. Inflating volume costs real money. The proven metric is not "how much was traded" but "how much was paid to trade."

Three proven properties of a Gold Standard token (Neptune reference):

| Property | Invariant | Source |
|----------|-----------|--------|
| Supply | `Σ balances = supply` | Conservation law (per-operation) |
| Price | Fee-weighted TWAP against NPT | Derivation law (per-block aggregation) |
| Liquidity depth | Cumulative fees collected in window | Economic signal (per-block aggregation) |

Supply is a conservation law — enforced per operation. Price is a derivation law — computed per block from proven swap data. Both are circuit-enforced public inputs. Both are available to every hook and every skill without additional proof composition.

#### How It Works

1. Every Liquidity (TIDE) swap proves: token pair, amount in, amount out,
   fee collected. The swap proof is a STARK — the price data is a
   byproduct of proven execution, not a separate attestation.
2. The block producer aggregates all swap proofs for each pair into a
   fee-weighted TWAP. This aggregation is itself a STARK proof — the
   block circuit verifies that the TWAP was correctly derived from the
   individual swap proofs included in the block.
3. The resulting `price` and `fees` become public state for the next block.
4. Any skill can read proven price as a public input — no oracle
   composition required.

Who computes: the block producer (miner). They are already composing all
transaction proofs into the block proof — aggregating swap data into a
TWAP is additional constraint verification within the same block circuit.
The miner cannot fake the TWAP because it must be consistent with the
individual swap proofs they include.

#### Why Protocol Fees, Not Volume

| Signal | Cost to fake | Sybil-resistant |
|--------|-------------|-----------------|
| Volume | Zero (wash trade with yourself) | No |
| Fees paid to LPs | Low (recycle as LP) | Weak |
| Protocol fee deducted | 0.1% of fake volume, non-recoverable | Yes |

The protocol fee is the unforgeable cost — it leaves the trader's hands on every swap. A token with 1,000 NPT in proven fees collected has 1,000 NPT of economic skin behind its price. Skills that consume price (Lending, Stablecoin, Liquidation) can set minimum fee thresholds: "accept this price only if proven fees > X NPT over > N blocks."

#### Protocol Fee

Every swap deducts 0.1% (10 basis points) of trade value in NPT. This is a global protocol constant — uniform across all pairs, not configurable per token.

- On a 10,000 NPT swap: 10 NPT deducted
- To sustain a fake price for 1 hour (6 blocks at 10-minute intervals): 0.1% × volume × 6 blocks
- Total trader cost: 0.1% protocol fee + strategy fee (0.1-0.3%) = 0.2-0.4% total
- Competitive with Uniswap (0.3% + gas + MEV) — Neptune traders save on MEV and gas

Every swap across every token pair strengthens the economic signal for every other token.

#### Bootstrap

New tokens with no swap history have no proven price. Oracle Pricing (COMPASS) serves as the bootstrap mechanism — external attestation until on-chain fee volume is sufficient. The transition is not automatic — skills that consume price decide their own threshold for trusting execution-derived price over oracle-derived price.

#### Price Pair Semantics

All proven prices are denominated in the base blockchain currency (NPT for Neptune). In Trident, the base currency is a target configuration parameter — each OS defines its own base asset.

### 2.5 Layer Architecture

```text
┌──────────────────────────────────────────────────────────┐
│  RECIPES                                                  │
│  Documented configs: "to build X, use these skills" │
├──────────────────────────────────────────────────────────┤
│  SKILL LIBRARY                                            │
│  Composable skills: Liquidity, Oracle, Governance,        │
│  Lending, Compliance, Delegation, Vesting, Royalties,     │
│  Staking, Bridging, Subscription, ...                     │
│                                                           │
│  Each = hooks + optional state tree + config              │
├──────────────────────────────────────────────────────────┤
│  STANDARDS                                                │
│  TSP-1 (Coin)              │  TSP-2 (Card)               │
├──────────────────────────────────────────────────────────┤
│  PLUMB FRAMEWORK                                          │
│  Leaf format, Config, Hooks, Auth, 5 Operations           │
└──────────────────────────────────────────────────────────┘
```

---

## 🧩 3. PLUMB — The Token Framework

Pay, Lock, Update, Mint, Burn

PLUMB is the architectural foundation that all Gold Standard token standards share. It defines:

- Leaf format — 10 field elements, hashed to Digest, stored in a binary Merkle tree
- Config commitment — 5 authorities + 5 hooks, hashed to Digest
- Metadata commitment — standalone descriptive data, hashed to Digest
- Operation set — 5 operations (Pay, Lock, Update, Mint, Burn) with uniform proof structure
- Auth model — `auth_hash` per leaf + per-operation config-level dual authorization
- Hook system — per-operation composable ZK programs
- Nullifier scheme — `hash(id, nonce)` for replay prevention
- Global public state — `state_root`, `supply`, `config_hash`, `metadata_hash`, `current_time`, `price`, `fees`

### 3.1 Config, Operations, and Hooks

Config is a 10-field commitment: 5 authorities (admin, pay, lock, mint,
burn) + 5 hooks (one per operation). Every operation verifies the full
config hash and extracts its dedicated authority and hook. Hooks are
composed ZK programs — the verifier ensures both the token proof and the
hook proof are valid.

See [PLUMB Reference](../../reference/plumb.md#2-token-config--10-field-elements)
for the full config schema, authority semantics, hook table, and proof
envelope.

### 3.2 Cross-Token Proof Composition

Hooks are not limited to their own token's state. A hook can require proofs from any skill or token as input. The verifier composes all required proofs together.

Example: TOKEN_B's `mint_hook` requires:
1. A valid TOKEN_A pay proof (collateral deposited)
2. A valid Oracle Pricing proof (collateral valuation)
3. A ratio check (mint amount ≤ collateral × price × LTV)

The hook circuit declares its required inputs. The verifier ensures all sub-proofs are valid and their public I/O is consistent (same accounts, same amounts, same timestamps).

This is how DeFi works in Neptune: operations on one token compose with operations on other tokens, oracle feeds, and skill state — all in a single atomic proof.

### 3.3 Skill State Trees

Skills that need persistent state maintain their own Merkle trees. A skill state tree follows the same pattern as standard trees:
- 10-field leaves hashed to Digest
- Binary Merkle tree
- State root committed on-chain
- Operations produce STARK proofs

The Liquidity skill's allocation tree, the Oracle Pricing skill's attestation tree, a Governance skill's proposal tree — all are skill state trees. What IS standardized is how skill proofs compose with token proofs through the hook system.

### 3.4 Atomic Multi-Tree Commitment

A single Neptune transaction may update multiple Merkle trees:
- TOKEN_A tree (collateral deposited)
- TOKEN_B tree (shares minted)
- Oracle attestation tree (price read)
- Skill state tree (position recorded)

The block commits to ALL tree roots atomically via a state commitment:

```trident
block_state = hash(
  token_tree_root_1, token_tree_root_2, ..., token_tree_root_N,
  skill_tree_root_1, ..., skill_tree_root_M
)
```

A transaction's composed proof references the old and new state commitment. The block verifier ensures all tree roots transition consistently — no partial updates.

### 3.5 No Approvals

PLUMB has no `approve`, `allowance`, or `transferFrom`. The approve/transferFrom pattern is the largest attack surface in ERC-20. In the Gold Standard:

| Ethereum pattern | Gold Standard solution |
|---|---|
| DEX swap via `transferFrom` | Two coordinated `pay` ops (Liquidity skill) |
| Lending deposit via `transferFrom` | `pay` to lending account, or `lock` with hook |
| Subscription / recurring payment | Derived auth key satisfying `auth_hash` |
| Meta-transaction / relayer | Anyone with auth secret constructs the proof |
| Multi-step DeFi | Proof composition — all movements proven atomically |

For delegated spending: `auth_hash` derived keys + Delegation skill tracking cumulative spending per delegate. Strictly more powerful, strictly safer than approve.

### 3.6 Security Properties

13 properties covering range checks, replay prevention, time-locks,
supply conservation, account abstraction, config binding, irreversible
renounce, and more. See [PLUMB Security Properties](../../reference/plumb.md#10-security-properties)
for the full list.

---

## 🥇 4. TSP-1 — Coin Standard

PLUMB implementation for divisible assets. Conservation law:
`sum(balances) = supply`.

10-field account leaves with balance, nonce, auth, time-lock, controller
(program-controlled accounts), and locked-by (program-based locking).
5 operations: Pay, Lock, Update, Mint, Burn — each with full config
verification and hook composition.

Full specification: [TSP-1 — Coin Reference](../../reference/tsp1-coin.md).

---

## 🥇 5. TSP-2 — Card Standard

PLUMB implementation for unique assets. Conservation law:
`owner_count(id) = 1`.

10-field asset leaves with owner, collection, metadata hash, royalty,
immutable creator, and a flags bitfield gating which operations are
allowed per asset. Same 5 PLUMB operations, different circuit
constraints: uniqueness instead of divisible supply, flag enforcement,
supply caps, and immutable provenance.

Full specification: [TSP-2 — Card Reference](../../reference/tsp2-card.md).

---

## 🛡️ 6. What the Gold Standard Fixes

### vs. ERC-20

| Problem | Solution |
|---|---|
| `approve()` race condition | No approvals — `auth_hash` + Delegation skill |
| Unlimited approval risk | No approvals exist |
| No time-locks | `lock_until` first-class |
| No mint/burn access control | Per-operation authorities |
| Tokens trapped in contracts | Tokens in user accounts |
| ERC-777 hooks = reentrancy | Hooks via proof composition |
| Supply not provable | `supply` conservation in circuit |

### vs. ERC-721

| Problem | Solution |
|---|---|
| Royalties not enforceable | `royalty_bps` + Royalties skill |
| No native collections | `collection_id` in leaf |
| Metadata frozen | `flags.updatable` per asset |
| Separate standard | Same PLUMB framework |

### vs. Uniswap

| Problem | Solution |
|---|---|
| Liquidity locked | Stays in maker accounts (Liquidity skill) |
| Fragmentation | Same capital backs multiple strategies |
| Impermanent loss | Oracle-priced strategies via Oracle Pricing skill |
| MEV | Proof-based, no public mempool |

### vs. Chainlink

| Problem | Solution |
|---|---|
| Trust oracle signers | STARK proof of aggregation (Oracle Pricing skill) |
| Opaque computation | Provable derivation chain |
| Chain-specific | Cross-chain via proof relay |

---

## 🗺️ 7. Roadmap

### 0.1 — Foundation (current target)

Deploy basic tokens and interact with them.

1. PLUMB framework (auth, config, hook slots)
2. TSP-1 circuit (pay, lock, update, mint, burn)
3. TSP-2 circuit (same operations, unique asset semantics)
4. Token deployment tooling
5. Basic wallet integration

### 0.2 — On-Chain Registry + Skills

1. [Atlas](../../reference/atlas.md) — on-chain Merkle registry for
   content-addressed code (store definitions anchored on-chain, provable
   registration and verification).
   Atlas uses TSP-2 Cards: each package is a Card in the OS's Atlas
   collection (`asset_id = hash(name)`,
   `metadata_hash = content_hash(artifact)`). Publishing is minting,
   updating is metadata update, ownership transfer is a pay operation.
   Each OS maintains its own independent Atlas instance.
2. First skills — Supply Cap, Delegation, Compliance
3. Skill composition end-to-end

See the [Skill Library](skill-library.md) for the full skill design space.

---

## ❓ 8. Open Questions

1. **Tree depth.** Depth 20 (~1M leaves). Fixed or variable per token?
2. **Privacy.** How far to push shielded transfers beyond basic pay?
3. **State rent.** Should leaves expire if unused?
4. **Controller recursion.** Can a controller program delegate to another controller? (Likely answer: no — depth = 1, to prevent infinite auth chains.)
5. **Fund share pricing.** Should fund shares use Oracle Pricing feeds for their own price, creating a feedback loop?

---

## 📖 Appendix A: Glossary

| Term | Definition |
|---|---|
| PLUMB | Pay, Lock, Update, Mint, Burn — the token framework |
| TSP-1 | Coin standard (PLUMB implementation for divisible assets) |
| TSP-2 | Card standard (PLUMB implementation for unique assets) |
| Skill | A composable package of hooks + optional state tree + config that teaches a token a new behavior |
| Proven price | Fee-weighted TWAP derived from swap execution, denominated in base currency |
| Proven fees | Cumulative protocol fees collected in aggregation window — economic signal for price confidence |
| Protocol fee | 0.1% (10 bps) of every swap in NPT, global constant |
| Circuit | AIR constraints defining valid state transitions |
| Config | Hashed commitment binding authorities and hooks |
| Hook | Reusable ZK program composed with token proof |
| Leaf | Merkle tree node — account (TSP-1) or asset (TSP-2) |
| Proof composition | Verifying multiple proofs with shared public inputs |
| Controller | Program ID that must co-authorize operations on a leaf |
| State commitment | Block-level hash of all Merkle tree roots |

## 🔗 See Also

- [PLUMB Framework](../../reference/plumb.md) — Shared token framework specification
- [TSP-1 — Coin Reference](../../reference/tsp1-coin.md) — Divisible asset standard
- [TSP-2 — Card Reference](../../reference/tsp2-card.md) — Unique asset standard
- [Skill Library](skill-library.md) — 23 composable token capabilities (DeFi, access control, composition)
- [Tutorial](../tutorials/tutorial.md) — Language basics
- [Programming Model](programming-model.md) — Execution model and stack semantics
- [OS Reference](../../reference/os.md) — OS concepts and `os.token` bindings
- [Multi-Target Compilation](multi-target.md) — One source, every chain
- [Deploying a Program](../guides/deploying-a-program.md) — Deployment workflows
