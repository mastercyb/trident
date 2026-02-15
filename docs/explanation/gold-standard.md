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

### 2.2 Skills

A skill is something a token can learn — a composable package of hooks,
optional state, and config that teaches a token a new behavior. Every
PLUMB operation has a hook slot. A skill installs hooks into those slots.
Multiple skills can coexist on the same token — their hook proofs compose
independently.

See the [Skill Library](skill-library.md) for the full catalog of 23
designed skills (Liquidity, Oracle Pricing, Governance, Lending, etc.),
recipes, and proof composition architecture.

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

### 3.1 Config — Shared by All PLUMB Standards

```trident
config = hash(admin_auth, pay_auth, lock_auth, mint_auth, burn_auth,
              pay_hook, lock_hook, update_hook, mint_hook, burn_hook)
```

| Field | Type | Description |
|---|---|---|
| `admin_auth` | Field | Admin secret hash. `0` = renounced (permanently immutable) |
| `pay_auth` | Field | Config-level pay authority. `0` = account auth only |
| `lock_auth` | Field | Config-level lock authority. `0` = account auth only |
| `mint_auth` | Field | Config-level mint authority. `0` = minting disabled |
| `burn_auth` | Field | Config-level burn authority. `0` = account auth only |
| `pay_hook` | Field | External program ID for pay logic (`0` = none) |
| `lock_hook` | Field | External program ID for lock logic (`0` = none) |
| `update_hook` | Field | External program ID for update logic (`0` = none) |
| `mint_hook` | Field | External program ID for mint logic (`0` = none) |
| `burn_hook` | Field | External program ID for burn logic (`0` = none) |

#### Authority Semantics

| Operation type | Auth = 0 | Auth ≠ 0 |
|---|---|---|
| Account ops (pay, lock, burn) | Account auth only (permissionless) | Dual auth: account + config authority |
| Config ops (mint) | Operation disabled | Config authority required |
| Config ops (update) | Renounced (permanently frozen) | Admin authority required |

### 3.2 Operations

All 5 operations follow the same proof envelope:
1. Divine 10 config fields, hash, assert match against public `config_hash`
2. Extract dedicated authority and hook
3. Verify authorization (account-level, and config-level if dual auth)
4. Apply state transition to leaf(s)
5. Update Merkle root
6. Emit public I/O for composition

### 3.3 Hook System

| Hook | Triggered by | Description |
|---|---|---|
| `pay_hook` | Every pay | Custom logic on transfers |
| `lock_hook` | Every lock | Custom lock logic |
| `update_hook` | Every config update | Governance over config changes |
| `mint_hook` | Every mint | Supply and minting logic |
| `burn_hook` | Every burn | Burn conditions and effects |

Composition model: The token circuit proves state transition validity. The verifier composes the token proof with the hook proof. If `hook == 0`, no external proof required.

### 3.4 Cross-Token Proof Composition

Hooks are not limited to their own token's state. A hook can require proofs from any skill or token as input. The verifier composes all required proofs together.

Example: TOKEN_B's `mint_hook` requires:
1. A valid TOKEN_A pay proof (collateral deposited)
2. A valid Oracle Pricing proof (collateral valuation)
3. A ratio check (mint amount ≤ collateral × price × LTV)

The hook circuit declares its required inputs. The verifier ensures all sub-proofs are valid and their public I/O is consistent (same accounts, same amounts, same timestamps).

This is how DeFi works in Neptune: operations on one token compose with operations on other tokens, oracle feeds, and skill state — all in a single atomic proof.

### 3.5 Skill State Trees

Skills that need persistent state maintain their own Merkle trees. A skill state tree follows the same pattern as standard trees:
- 10-field leaves hashed to Digest
- Binary Merkle tree
- State root committed on-chain
- Operations produce STARK proofs

The Liquidity skill's allocation tree, the Oracle Pricing skill's attestation tree, a Governance skill's proposal tree — all are skill state trees. What IS standardized is how skill proofs compose with token proofs through the hook system.

### 3.6 Atomic Multi-Tree Commitment

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

### 3.7 No Approvals

PLUMB has no `approve`, `allowance`, or `transferFrom`. The approve/transferFrom pattern is the largest attack surface in ERC-20. In the Gold Standard:

| Ethereum pattern | Gold Standard solution |
|---|---|
| DEX swap via `transferFrom` | Two coordinated `pay` ops (Liquidity skill) |
| Lending deposit via `transferFrom` | `pay` to lending account, or `lock` with hook |
| Subscription / recurring payment | Derived auth key satisfying `auth_hash` |
| Meta-transaction / relayer | Anyone with auth secret constructs the proof |
| Multi-step DeFi | Proof composition — all movements proven atomically |

For delegated spending: `auth_hash` derived keys + Delegation skill tracking cumulative spending per delegate. Strictly more powerful, strictly safer than approve.

### 3.8 Security Properties

1. No negative balances: `as_u32()` range check
2. Replay prevention: Monotonic nonce + nullifiers
3. Time-lock enforcement: `current_time` from block
4. Lock monotonicity: Can only extend, not shorten
5. Supply conservation: Public invariant
6. Account abstraction: `auth_hash` = privkey, Shamir, ZK proof, anything
7. Config binding: Every op verifies full config hash
8. Irreversible renounce: `admin_auth = 0` = frozen forever
9. Config-state separation: Config updates can't touch tree
10. Hook composability: Hooks bound to config hash
11. Symmetric authority: Every op has authority + hook
12. Safe defaults: `mint_auth = 0` = disabled, others `= 0` = permissionless
13. No approvals: No allowances, no `transferFrom`, no approval phishing

---

## 🥇 4. TSP-1 — Coin Standard

*PLUMB implementation for divisible assets*

### 4.1 Account Leaf — 10 field elements

```trident
leaf = hash(account_id, balance, nonce, auth_hash, lock_until,
            controller, locked_by, lock_data, 0, 0)
```

| Field | Type | Description |
|---|---|---|
| `account_id` | Field | Unique account identifier (pubkey hash) |
| `balance` | Field | Token balance (U32 range) |
| `nonce` | Field | Monotonic counter |
| `auth_hash` | Field | Hash of authorization secret |
| `lock_until` | Field | Timestamp lock (0 = unlocked) |
| `controller` | Field | Program ID that must co-authorize operations (`0` = owner only) |
| `locked_by` | Field | Program ID that locked this account (`0` = not program-locked) |
| `lock_data` | Field | Program-specific lock data (position ID, collateral ratio, etc.) |
| *reserved* | Field×2 | Extension space |

#### Controller Field

When `controller ≠ 0`, every operation on this leaf requires a composed proof from the controller program in addition to normal auth. This enables program-controlled accounts — leaves that can only be moved by a specific ZK program.

Use cases:
- Fund accounts: collateral held by fund program, released only on valid redemption/liquidation proof
- Escrow: tokens held until condition is met
- Protocol treasuries: spending requires governance proof

The circuit checks: if `leaf.controller ≠ 0`, the verifier must compose with a valid proof from program `controller`. This is additive — both `auth_hash` AND controller must be satisfied.

#### Locked-by Field

When `locked_by ≠ 0`, the account's tokens are committed to a specific program. The `lock_data` field carries program-specific state (e.g. which fund position this collateral backs).

Unlike `lock_until` (time-based), `locked_by` is program-based locking: only a proof from the `locked_by` program can unlock the account. The lock can be released before `lock_until` if the program authorizes it (e.g. on redemption).

### 4.2 Token Metadata

```trident
metadata = hash(name_hash, ticker_hash, teaser_hash, site_hash, custom_hash,
                price_oracle, volume_oracle, 0, 0, 0)
```

### 4.3 Circuit Constraints

#### Op 0: Pay
1. Config verified, `pay_auth` and `pay_hook` extracted
2. Sender leaf verifies against `old_root`
3. `hash(secret) == sender.auth_hash`
4. If `pay_auth ≠ 0`, dual auth required
5. `current_time >= sender.lock_until`
6. `sender.balance >= amount` (range check via `as_u32`)
7. Sender: `balance -= amount`, `nonce += 1`
8. Receiver: `balance += amount`
9. New leaves → `new_root`
10. Supply unchanged

#### Op 1: Lock(time)
1. Config verified, `lock_auth` and `lock_hook` extracted
2. Account auth required
3. If `lock_auth ≠ 0`, dual auth
4. `lock_until_time >= leaf.lock_until` (extend only)
5. Leaf: `lock_until = lock_until_time`, `nonce += 1`

#### Op 2: Update
1. `old_root == new_root` (state unchanged)
2. Old config verified, `update_hook` extracted
3. `hash(admin_secret) == old_config.admin_auth`
4. `admin_auth ≠ 0` (not renounced)
5. New config fields → `new_config`

#### Op 3: Mint
1. Config verified, `mint_auth` and `mint_hook` extracted
2. `hash(mint_secret) == config.mint_auth`
3. `new_supply == old_supply + amount`
4. Recipient: `balance += amount`

#### Op 4: Burn
1. Config verified, `burn_auth` and `burn_hook` extracted
2. Account auth required
3. If `burn_auth ≠ 0`, dual auth
4. `current_time >= leaf.lock_until`
5. `leaf.balance >= amount`
6. `new_supply == old_supply - amount`
7. Leaf: `balance -= amount`, `nonce += 1`

---

## 🥇 5. TSP-2 — Card Standard

*PLUMB implementation for unique assets*

### 5.1 What Differs from TSP-1

1. Leaf represents an asset (unique item), not an account balance
2. Invariant: uniqueness (`owner_count(id) = 1`) not divisible supply
3. No divisible arithmetic — no `balance`, no range checks, no splitting
4. Per-asset state — metadata, royalty, creator, flags live in the leaf
5. Creator immutability — `creator_id` is set at mint and can never change
6. Flag-gated operations — transferable, burnable, updatable bits control which PLUMB operations are allowed per asset

Operations are still Pay, Lock, Update, Mint, Burn — PLUMB operations. What changes is what the circuit enforces inside each.

### 5.2 Asset Leaf — 10 field elements

```trident
leaf = hash(asset_id, owner_id, nonce, auth_hash, lock_until,
            collection_id, metadata_hash, royalty_bps, creator_id, flags)
```

| Field | Type | Description |
|---|---|---|
| `asset_id` | Field | Globally unique asset identifier |
| `owner_id` | Field | Current owner (account_id hash) |
| `nonce` | Field | Monotonic counter |
| `auth_hash` | Field | Hash of owner's authorization secret |
| `lock_until` | Field | Timestamp lock (0 = unlocked) |
| `collection_id` | Field | Collection membership (0 = standalone) |
| `metadata_hash` | Field | Hash of item metadata |
| `royalty_bps` | Field | Royalty basis points (0-10000) |
| `creator_id` | Field | Original creator (immutable after mint) |
| `flags` | Field | Bits: transferable (0), burnable (1), updatable (2), lockable (3), mintable (4) |

First 5 fields occupy same positions as TSP-1. Last 5 — reserved zeros in TSP-1 — carry per-asset state in TSP-2.

#### Flags Bitfield

| Bit | Name | When set | When clear |
|-----|------|----------|------------|
| 0 | `TRANSFERABLE` | Pay (transfer) allowed | Pay rejected |
| 1 | `BURNABLE` | Burn allowed | Burn rejected |
| 2 | `UPDATABLE` | Metadata update allowed | Metadata frozen forever |
| 3 | `LOCKABLE` | Lock (time-lock) allowed | Lock rejected |
| 4 | `MINTABLE` | Re-mint into collection allowed | Collection closed to new mints |

Flags are set at mint time and cannot be changed after creation. A soulbound credential is minted with `flags = 0`. A game item uses `flags = 31` (all operations). A standard collectible uses `flags = 11` (transferable + burnable + lockable).

#### Collection Binding

When `collection_id ≠ 0`, the asset belongs to a collection identified by its config hash. Collection membership is immutable after mint.

#### Creator Immutability

`creator_id` is set at mint and can never change. Every subsequent operation preserves it. This provides an unforgeable provenance chain. The Royalties skill depends on this: hooks read `royalty_bps` from the leaf and `royalty_receiver` from collection metadata.

### 5.3 Collection Metadata — 10 field elements

```trident
metadata = hash(name_hash, description_hash, image_hash, site_hash, custom_hash,
                max_supply, royalty_receiver, 0, 0, 0)
```

| Field | Type | Description |
|---|---|---|
| `name_hash` | Field | Hash of collection name |
| `description_hash` | Field | Hash of collection description |
| `image_hash` | Field | Hash of collection image/avatar |
| `site_hash` | Field | Hash of collection website URL |
| `custom_hash` | Field | Hash of application-specific data |
| `max_supply` | Field | Maximum number of assets (0 = unlimited) |
| `royalty_receiver` | Field | Account that receives royalties on transfers |
| *reserved* | Field×3 | Extension space |

### 5.4 Circuit Constraints

All 5 operations follow the PLUMB proof envelope (section 3.2).

#### Op 0: Pay (Transfer Ownership)
1. Config verified, `pay_auth` and `pay_hook` extracted
2. Asset leaf verified against `old_root`
3. `hash(secret) == leaf.auth_hash`
4. If `pay_auth ≠ 0`: dual auth required
5. `current_time >= leaf.lock_until`
6. `leaf.flags & TRANSFERABLE`
7. `collection_id`, `creator_id`, `royalty_bps`, `metadata_hash`, `flags` unchanged
8. New leaf: `owner_id = new_owner`, `auth_hash = new_auth`, `nonce += 1`
9. New leaf → `new_root`
10. Nullifier emitted: `hash(asset_id, nonce)`

#### Op 1: Lock (Time-Lock Asset)
1. Config verified, `lock_auth` and `lock_hook` extracted
2. Owner auth required
3. If `lock_auth ≠ 0`: dual auth
4. `leaf.flags & LOCKABLE`
5. `lock_until_time >= leaf.lock_until` (extend only)
6. All immutable fields unchanged
7. Leaf: `lock_until = lock_until_time`, `nonce += 1`

#### Op 2: Update (Config or Metadata)
Config update: `old_root == new_root`, admin auth, `admin_auth ≠ 0`, new config fields.
Metadata update: Owner auth, `flags & UPDATABLE`, only `metadata_hash` changes, `nonce += 1`.

#### Op 3: Mint (Originate)
1. Config verified, `mint_auth` and `mint_hook` extracted
2. `mint_auth ≠ 0` (minting enabled)
3. Mint authorization
4. `asset_id` not in tree (non-membership proof)
5. `creator_id = minter_id` (immutable forever)
6. `collection_id`, `flags`, `royalty_bps` set (immutable after mint)
7. `flags & MINTABLE`
8. `nonce = 0`, `lock_until = 0`
9. New leaf → `new_root`
10. `new_asset_count == old_asset_count + 1`
11. If `max_supply ≠ 0`: `new_asset_count <= max_supply`

#### Op 4: Burn (Release)
1. Config verified, `burn_auth` and `burn_hook` extracted
2. Owner auth required
3. If `burn_auth ≠ 0`: dual auth
4. `current_time >= leaf.lock_until`
5. `leaf.flags & BURNABLE`
6. Leaf → null (Merkle deletion)
7. `new_asset_count == old_asset_count - 1`
8. Nullifier emitted: `hash(asset_id, nonce)`

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

1. On-chain Merkle registry for content-addressed code (store definitions
   anchored on-chain, provable registration and verification)
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

- [TSP-1 — Coin Reference](../../reference/tsp1-coin.md) — Canonical coin standard specification
- [TSP-2 — Card Reference](../../reference/tsp2-card.md) — Canonical card standard specification
- [Skill Library](skill-library.md) — 23 composable token capabilities (DeFi, access control, composition)
- [Tutorial](../tutorials/tutorial.md) — Language basics
- [Programming Model](programming-model.md) — Execution model and stack semantics
- [OS Reference](../../reference/os.md) — OS concepts and `os.token` bindings
- [Multi-Target Compilation](multi-target.md) — One source, every chain
- [Deploying a Program](../guides/deploying-a-program.md) — Deployment workflows
