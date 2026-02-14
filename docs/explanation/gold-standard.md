# 🥇 The Gold Standard

## ZK-Native Token Standards and Skill Library

Version: 0.1-draft
Date: February 14, 2026

### Status

This document is a design specification — it describes what we want to
build, not what exists today. The PLUMB framework and token standards
(TSP-1, TSP-2) are architecturally complete. The skill library is a
design-phase catalog — none of the 23 skills are implemented yet.

The 0.1 release target is: deploy basic tokens and interact with them.
Skills and capabilities come later, after the foundation is battle-tested.

The Gold Standard is a Trident-level specification. While the reference
implementation targets Neptune, the standards (PLUMB, TSP-1, TSP-2) and
skill architecture are designed to work on any OS that supports Trident's
Level 2 (provable computation).

| Layer | What | Status | Files |
|-------|------|--------|-------|
| OS bindings | Neptune runtime modules | Compiler support | `os/neptune/kernel.tri`, `utxo.tri`, `proof.tri`, `xfield.tri`, `recursive.tri` |
| Type scripts | Value conservation rules | Compiler support | `examples/neptune/type_native_currency.tri` (NPT), `type_custom_token.tri` (TSP-1) |
| Lock scripts | Spending authorization | Compiler support | `examples/neptune/lock_generation.tri`, `lock_symmetric.tri`, `lock_timelock.tri`, `lock_multisig.tri` |
| Transaction validation | Full transaction verification | Compiler support | `examples/neptune/transaction_validation.tri` |
| Proof composition | Recursive STARK verification | Compiler support | `examples/neptune/proof_aggregator.tri`, `proof_relay.tri` |
| Skill library | Token capabilities (DeFi, access control) | Design only | 23 skills specified, 0 implemented |

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

### 2.2 Skill Library

A skill is something a token can learn. It is a composable package:

- Hooks it installs (which PLUMB operations it extends)
- State tree it needs (if any — most skills are stateless)
- Config it requires (which authorities and hooks must be set)
- Composes with other skills it works alongside

Skills are how tokens learn to do things beyond basic transfers. A coin that can provide liquidity has the Liquidity skill. A coin that enforces KYC has the Compliance skill. A card that pays creator royalties has the Royalties skill.

The hook system makes this possible. Every PLUMB operation has a hook slot. A skill installs hooks into those slots. Multiple skills can coexist on the same token — their hook proofs compose independently.

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

## 🧰 6. Skill Library (Design Phase)

*None of the skills below are implemented. This section specifies the
design space — what tokens should be able to learn. Implementation
follows after the PLUMB foundation (basic token deploy and interact)
is production-tested. Community contributions welcome.*

### 6.1 What Is a Skill

A skill is a composable package that teaches a token a new behavior. Every skill has the same anatomy:

| Component | Description |
|-----------|-------------|
| Skill | What the token can now do |
| Hooks | Which PLUMB hooks it installs |
| State tree | Whether it needs its own Merkle tree |
| Config | What authorities/hooks must be set |
| Composes with | Which other skills it works alongside |

A token with no skills is a bare TSP-1 or TSP-2 — it can pay, lock, update, mint, and burn. Each skill you add teaches it a new behavior.

### 6.2 How Skills Compose

Multiple skills can be active on the same token simultaneously. When
multiple skills install hooks on the same operation, their proofs compose
independently:

```text
Pay operation with Compliance + Fee-on-Transfer + Liquidity:
  1. Token circuit proves valid balance transfer
  2. Compliance hook proves sender and receiver are whitelisted
  3. Fee-on-Transfer hook proves treasury received its cut
  4. Liquidity hook proves pricing curve was satisfied
  Verifier composes: Token ⊗ Compliance ⊗ Fee ⊗ Liquidity → single proof
```

#### Hook Composition Ordering

Hook ordering is a non-problem. Unlike contract calls (which execute
sequentially and can reenter each other), STARK proof composition is
**commutative** — each hook proof is independently generated and
independently verified. There is no execution order because hooks don't
call each other. They produce separate proofs that the verifier checks
together.

The config declares which hooks are active. The prover generates all
required sub-proofs. The verifier checks:

1. Every declared hook has a valid proof
2. Public I/O is consistent across all sub-proofs (same accounts, amounts,
   timestamps)
3. All Merkle roots chain correctly

If any hook proof is missing or invalid, the composed proof fails. If two
hooks have contradictory requirements (e.g., Soulbound says "reject all
transfers" while Liquidity says "allow this swap"), the composition simply
fails — both proofs cannot be simultaneously valid. This is correct
behavior: contradictory hooks mean a misconfigured token, caught at proof
time, not at runtime.

The one constraint: when multiple hooks modify **different state trees**
in the same transaction, the block's atomic state commitment (section 3.6)
ensures all tree updates are applied together or not at all.

### 6.3 Skill Tiers

| Tier | Focus | Skills |
|------|-------|-------------|
| Core | Skills most tokens want | Supply Cap, Delegation, Vesting, Royalties, Multisig, Timelock |
| Financial | DeFi use cases | Liquidity, Oracle Pricing, Vault, Lending, Staking, Stablecoin |
| Access Control | Compliance and permissions | Compliance, KYC Gate, Transfer Limits, Controller Gate, Soulbound, Fee-on-Transfer |
| Composition | Cross-token interaction | Bridging, Subscription, Burn-to-Redeem, Governance, Batch Operations |

---

## 🔧 7. Core Skills (Design Phase)

### 7.1 Supply Cap

| | |
|---|---|
| Skill | Fixed maximum supply — cryptographically enforced ceiling |
| Hooks | `mint_hook` = `MINT_CAP` |
| State tree | No |
| Config | `mint_auth` must be set (minting enabled) |
| Composes with | Everything — most fundamental financial constraint |

The hook verifies: `new_supply <= max_supply` (read from metadata or hardcoded in hook parameters). Without this skill, TSP-1 minting is uncapped. With it, the cap is provably enforced.

### 7.2 Delegation

| | |
|---|---|
| Skill | Let others spend on your behalf with limits and expiry |
| Hooks | `pay_hook` = `PAY_DELEGATION` |
| State tree | Yes — delegation tree |
| Config | `pay_hook` must be set |
| Composes with | Subscription, Compliance |

Replaces ERC-20's `approve`/`allowance` with bounded, expiring, revocable delegation.

Delegation leaf:
```trident
delegation = hash(owner, delegate, token, limit, spent, expiry, 0, 0, 0, 0)
```

On pay, the hook checks: if caller is delegate, verify `spent + amount ≤ limit` and `current_time < expiry`, then `spent += amount`. Owner revokes by changing `auth_hash`.

### 7.3 Vesting

| | |
|---|---|
| Skill | Time-locked token release on a schedule |
| Hooks | `mint_hook` = `MINT_VESTING` |
| State tree | Yes — vesting schedule tree |
| Config | `mint_auth` = vesting program |
| Composes with | Supply Cap, Governance |

Vesting schedule leaf:
```trident
schedule = hash(beneficiary, total_amount, start_time, cliff, duration, claimed, 0, 0, 0, 0)
```

On mint: `elapsed = current_time - start_time`. If `elapsed < cliff`: reject. `vested = total_amount × min(elapsed, duration) / duration`. `amount ≤ vested - claimed`. `claimed += amount`.

### 7.4 Royalties (TSP-2)

| | |
|---|---|
| Skill | Enforce creator royalties on every transfer — not optional, not bypassable |
| Hooks | `pay_hook` = `PAY_ROYALTY` |
| State tree | No — reads `royalty_bps` from leaf, `royalty_receiver` from metadata |
| Config | `pay_hook` must be set |
| Composes with | Liquidity (marketplace), Oracle Pricing (floor price) |

On every TSP-2 transfer, the hook:
1. Reads `royalty_bps` from the asset leaf
2. Reads `royalty_receiver` from collection metadata
3. Requires a composed TSP-1 pay proof: buyer pays `(sale_price × royalty_bps / 10000)` to `royalty_receiver`

Enforced at the protocol level. No wrapper contract bypass.

### 7.5 Multisig / Threshold

| | |
|---|---|
| Skill | Require M-of-N approval for config changes |
| Hooks | `update_hook` = `UPDATE_THRESHOLD` |
| State tree | No — uses a TSP-1 membership token as the signer set |
| Config | `update_hook` must be set |
| Composes with | Governance, Timelock |

Deploy a TSP-1 token with `supply = N`, one per signer. On config update, the threshold hook requires M composed pay proofs from token holders. The token IS the membership. The hook IS the threshold logic. Not a separate primitive.

### 7.6 Timelock

| | |
|---|---|
| Skill | Mandatory delay period on config changes |
| Hooks | `update_hook` = `UPDATE_TIMELOCK` |
| State tree | No |
| Config | `update_hook` must be set |
| Composes with | Multisig, Governance |

Config changes are queued and can only execute after the delay period. Prevents surprise rug-pulls. Commonly combined with Multisig: threshold approval + mandatory delay.

---

## 💰 8. Financial Skills (Design Phase)

### 8.1 Liquidity (TIDE)

*Tokens In Direct Exchange*

| | |
|---|---|
| Skill | Earn on providing liquidity — tokens stay in your account |
| Hooks | `pay_hook` = `PAY_STRATEGY` (the pricing curve) |
| State tree | Yes — allocation tree |
| Config | `pay_hook` must reference a strategy program |
| Composes with | Oracle Pricing, Staking, Governance |

#### How It Works

Traditional AMMs lock tokens in custodial pool contracts. The Liquidity skill eliminates custody entirely. Swaps are two `pay` operations where the `pay_hook` enforces the pricing curve:

```text
Alice swaps 100 TOKEN_A for TOKEN_B with maker Bob:

  TOKEN_A Pay: Alice → Bob, amount=100, pay_hook=STRATEGY
  TOKEN_B Pay: Bob → Alice, amount=f(100), pay_hook=STRATEGY

  Composed proof: Token_A ⊗ Token_B ⊗ Strategy → single verification
```

No tokens leave user accounts. No approvals. No router.

#### Protocol Fee

Every swap deducts 0.1% (10 basis points) of trade value in NPT. This is a global protocol constant, not configurable per token or per strategy. The fee serves as the foundation for Sybil-resistant price discovery (section 2.4).

The strategy fee (paid to LPs) is separate and set per-strategy. Total trader cost: 0.1% protocol fee + 0.1-0.3% strategy fee = 0.2-0.4% total.

#### Shared Liquidity

Because the AMM is a hook, Bob's balance simultaneously:
- Backs AMM Strategy X and Y
- Serves as lending collateral (via Lending skill)
- Counts for governance votes
- Earns staking rewards

Atomic consistency without locks: if two strategies try to move same tokens in one block, the second proof fails (Merkle root already changed).

#### Strategy Registration

```trident
strategy = hash(maker, token_a, token_b, program, parameters)
```

Immutable once registered. Revoke and re-register to modify.

#### Virtual Allocations

Allocation tree tracks how much of a maker's balance each strategy can access:

`Σ(allocations[maker][token]) ≤ balance[maker][token]`

Overcommitment is safe — every swap proof checks the current balance.

#### Strategy Programs

Pluggable ZK circuits. Reference implementations:

| Strategy | Description | Key property |
|---|---|---|
| Constant Product | x·y = k | Simple, proven, universal |
| Stable Swap | Curve-style invariant | Optimized for pegged pairs |
| Concentrated Liquidity | Positions in price ranges | Capital-efficient, active management |
| Oracle-Priced | Anchored to Oracle Pricing feed | Eliminates impermanent loss |

#### Comparison

| Property | Uniswap V3 | Aqua (1inch) | Neptune Liquidity |
|---|---|---|---|
| Custody | Pool contract | Wallet (allowance) | Wallet (Merkle leaf) |
| Execution | EVM call | EVM call | ZK proof composition |
| Multi-strategy | No | Yes | Yes |
| Pricing proof | Trust EVM | Trust EVM | STARK-proven |
| Cross-chain | No | No | Yes (relay proof) |
| Capital in governance | No | Yes | Yes |
| MEV surface | Large | Reduced | Minimal |

### 8.2 Oracle Pricing (COMPASS)

*External data attestation with STARK proofs*

| | |
|---|---|
| Skill | Price feeds with STARK-proven aggregation — verified, not trusted |
| Hooks | Consumed by other skills (mint_hook, pay_hook compose with oracle proofs) |
| State tree | Yes — attestation tree |
| Config | Feed config (submit_auth, aggregate_auth, hooks) |
| Composes with | Liquidity, Lending, Stablecoin, Bridging |

#### Why Oracle Pricing Needs a State Tree

Hooks *consume* external data but cannot *produce* it. Someone must commit data, prove its derivation, and make it queryable. The oracle is to DeFi what `auth_hash` is to tokens — the external input everything depends on.

#### Attestation Leaf — 10 field elements

```trident
leaf = hash(feed_id, value, timestamp, provider_id, nonce,
            confidence, source_hash, proof_hash, 0, 0)
```

#### Feed Config — 10 field elements

```trident
config = hash(admin_auth, submit_auth, aggregate_auth, 0, 0,
              submit_hook, aggregate_hook, read_hook, 0, 0)
```

#### Feed Metadata — 10 field elements

```trident
metadata = hash(name_hash, pair_hash, decimals, heartbeat, deviation_threshold,
                min_providers, max_staleness, 0, 0, 0)
```

#### Operations

Submit: A provider submits a new attestation. Constraints: provider authorization, `timestamp <= current_time`, newer than previous, `nonce == old_nonce + 1`. The `submit_hook` can enforce staking requirements, reputation scores, deviation bounds.

Aggregate: Combine multiple attestations into a canonical value. Constraints: N leaves from tree, `N >= min_providers`, all within `max_staleness`. The `aggregate_hook` determines the function: median, TWAP, weighted average, outlier-filtered.

Read: Produce a STARK proof that feed F has value V at time T. Not an on-chain operation — a proof that any skill can compose with.

#### The STARK-Unique Property

In Chainlink or Pyth, oracle data comes with a signature — you trust the signers. In the Gold Standard, oracle data comes with a STARK proof of its derivation. The aggregation circuit proves the median was correctly computed from N submissions. The composed proof covers the entire chain from raw data to aggregated value. Swap prices are not trusted — they are mathematically verified.

#### Cross-Chain Oracle

Oracle proofs are STARKs. They can be relayed to other chains and verified without trusting a bridge or multisig.

### 8.3 Vault / Yield-Bearing

| | |
|---|---|
| Skill | Deposit asset, receive shares at exchange rate (ERC-4626 as a skill) |
| Hooks | `mint_hook` = `VAULT_DEPOSIT`, `burn_hook` = `VAULT_WITHDRAW` |
| State tree | No — exchange rate derived from `total_assets / total_shares` |
| Config | `mint_auth` = vault program |
| Composes with | Oracle Pricing, Lending, Staking |

On deposit: mint shares proportional to deposited assets. On withdrawal: burn shares, release proportional assets. Inflation attack defense built into the hook (initial offset at deployment).

### 8.4 Lending / Collateral

| | |
|---|---|
| Skill | Use tokens as collateral to borrow against |
| Hooks | `mint_hook` = `FUND_MINT`, `burn_hook` = `FUND_REDEEM` + `BURN_LIQUIDATE` |
| State tree | Yes — position tree (user, collateral, debt, health_factor) |
| Config | `mint_auth` = lending program |
| Composes with | Oracle Pricing (mandatory), Liquidity (liquidation swaps) |

Supply flow:
1. TOKEN_A pay to fund account (`controller = FUND_PROGRAM`, `locked_by = FUND_PROGRAM`)
2. Oracle Pricing proves TOKEN_A price = V
3. Fund program records position in its state tree
4. TOKEN_B mint to borrower: `amount = collateral × price × ltv_ratio`

Liquidation: If `health_factor < 1` (checked via Oracle Pricing), anyone can prove the condition and execute — liquidator covers debt, receives collateral at discount.

### 8.5 Staking

| | |
|---|---|
| Skill | Lock tokens to earn rewards |
| Hooks | `lock_hook` = `LOCK_REWARDS`, `mint_hook` = `STAKE_DEPOSIT`, `burn_hook` = `STAKE_WITHDRAW` |
| State tree | Optional — reward distribution state |
| Config | `lock_auth` may be set for mandatory staking |
| Composes with | Liquidity (staked tokens back strategies), Governance |

Combined with Vault skill for a liquid staking token (LST): deposit native token → receive LST that appreciates as staking rewards accrue.

### 8.6 Stablecoin

| | |
|---|---|
| Skill | Maintain a peg through collateral + oracle pricing |
| Hooks | `mint_hook` = `STABLECOIN_MINT`, `burn_hook` = `STABLECOIN_REDEEM` |
| State tree | Yes — collateral position tree |
| Config | `mint_auth` = minting program |
| Composes with | Oracle Pricing (mandatory), Lending, Liquidity |

Mint hook composes with: Oracle Pricing proof (collateral price), TSP-1 lock proof (collateral locked), collateral ratio check (e.g. 150% minimum). Burn hook releases collateral proportional to burn amount.

---

## 🔐 9. Access Control Skills (Design Phase)

### 9.1 Compliance (Whitelist / Blacklist)

| | |
|---|---|
| Skill | Restrict who can send/receive tokens |
| Hooks | `pay_hook` = `PAY_WHITELIST` or `PAY_BLACKLIST` |
| State tree | Yes — approved/blocked address Merkle set |
| Config | `pay_auth` may enforce dual auth |
| Composes with | KYC Gate, Delegation |

Whitelist: On every pay, hook proves `hash(sender) ∈ whitelist_tree` and `hash(receiver) ∈ whitelist_tree` via Merkle inclusion proofs.

Blacklist: Non-membership proofs — proves addresses are NOT in the blocked set.

Use cases: regulated tokens, accredited investor restrictions, sanctioned address blocking.

### 9.2 KYC Gate

| | |
|---|---|
| Skill | Require verified identity credential to mint or receive |
| Hooks | `mint_hook` = `MINT_KYC` |
| State tree | No — composes with a TSP-2 soulbound credential proof |
| Config | `mint_auth` must be set |
| Composes with | Compliance, Soulbound |

The hook requires a composed proof that the recipient holds a valid soulbound credential (TSP-2 with `flags = 0`).

### 9.3 Transfer Limits

| | |
|---|---|
| Skill | Cap transfer amounts per transaction or per time period |
| Hooks | `pay_hook` = `PAY_LIMIT` |
| State tree | Yes — rate tracking per account |
| Config | `pay_hook` must be set |
| Composes with | Compliance, Delegation |

### 9.4 Controller Gate

| | |
|---|---|
| Skill | Require a specific program's proof to move tokens |
| Hooks | `pay_hook` = `PAY_CONTROLLER` |
| State tree | No — reads `controller` from leaf |
| Config | `leaf.controller` must be set |
| Composes with | Lending (program-controlled collateral), Vault |

Verifies a composed proof from the leaf's `controller` program. Enables escrow, protocol treasuries, and program-controlled accounts.

### 9.5 Soulbound (TSP-2)

| | |
|---|---|
| Skill | Make assets permanently non-transferable |
| Hooks | `pay_hook` = `PAY_SOULBOUND` (always rejects) |
| State tree | No |
| Config | `pay_hook` set |
| Composes with | KYC Gate (credential issuance) |

Also achievable without a hook: mint with `flags = 0` (TRANSFERABLE bit clear). The hook version works for TSP-1 tokens that lack per-leaf flags.

### 9.6 Fee-on-Transfer

| | |
|---|---|
| Skill | Deduct a percentage to treasury on every transfer |
| Hooks | `pay_hook` = `PAY_FEE` |
| State tree | No — composes with TSP-1 pay proof for fee payment |
| Config | `pay_hook` set, treasury address in metadata |
| Composes with | Compliance, Liquidity |

---

## 🔗 10. Composition Skills (Design Phase)

### 10.1 Bridging

| | |
|---|---|
| Skill | Cross-chain portability via STARK proof relay |
| Hooks | `mint_hook` = `BRIDGE_LOCK_PROOF`, `burn_hook` = `BRIDGE_RELEASE_PROOF` |
| State tree | No — proofs relay directly |
| Config | `mint_auth` = bridge program, `burn_auth` = bridge program |
| Composes with | Oracle Pricing (cross-chain price verification) |

Mint on destination chain requires STARK proof of lock on source chain. Burn on destination produces proof for release on source chain. No trusted bridge or multisig.

### 10.2 Subscription / Streaming Payments

| | |
|---|---|
| Skill | Recurring authorized payments on a schedule |
| Hooks | `pay_hook` = `PAY_DELEGATION` (with rate-limiting) |
| State tree | Delegation tree (reuses Delegation skill) |
| Config | `pay_hook` set |
| Composes with | Delegation (required) |

Service provider registers as delegate with monthly `limit` and `expiry`. Each period, service calls pay using delegation authority. Hook enforces rate limit. User revokes by changing `auth_hash`.

### 10.3 Burn-to-Redeem

| | |
|---|---|
| Skill | Burn one asset to claim another |
| Hooks | `burn_hook` = `BURN_REDEEM` |
| State tree | No — produces receipt proof |
| Config | `burn_hook` set |
| Composes with | Any mint operation |

The hook produces a receipt proof that composes with a mint operation on another token:

```text
Burn(TSP-2 item) → receipt proof ⊗ Mint(TSP-1 reward token)
```

Use cases: burn card to claim physical goods, burn ticket for event access, burn old token for upgraded version, crafting (burn materials → mint result).

### 10.4 Governance

| | |
|---|---|
| Skill | Vote with your tokens, propose and execute protocol changes |
| Hooks | `update_hook` = `UPDATE_TIMELOCK` + `UPDATE_THRESHOLD` |
| State tree | Yes — proposal tree |
| Config | `admin_auth` = governance program |
| Composes with | Timelock, Multisig, Staking (vote weight = staked balance) |

Uses historical Merkle roots as free balance snapshots. Flow:
1. Create proposal → commit to proposal tree
2. Snapshot current TSP-1 state root at proposal creation
3. Vote → voter proves balance at snapshot root (Merkle inclusion)
4. Tally → aggregation circuit counts votes, verifies quorum
5. Execute → queue behind timelock, then execute config updates

No governance primitive needed. Balance snapshots are free — every historical Merkle root is a snapshot.

### 10.5 Batch Operations

| | |
|---|---|
| Skill | Mint or transfer multiple tokens in one proof |
| Hooks | `mint_hook` = `MINT_BATCH` |
| State tree | No — recursive proof composition |
| Config | `mint_hook` set |
| Composes with | Supply Cap |

Multiple mints composed into a single recursive STARK proof. Useful for airdrops, collection launches, and batch distributions.

---

## 📋 11. Recipes

Recipes are documented configurations that combine a standard with skills to build specific token types. Pick a standard, pick skills, deploy.

### 11.1 Simple Coin

```text
Standard: TSP-1    Skills: none
Config: admin_auth=hash(admin), mint_auth=hash(minter), all others=0
```

The simplest token. Anyone can transfer and burn. Admin can update config. Authorized minter mints.

### 11.2 Immutable Money

```text
Standard: TSP-1    Skills: none
Config: admin_auth=0 (renounced), mint_auth=0 (disabled), all others=0
```

After genesis mint, nothing can change. Pure permissionless sound money. The config hash is verifiably immutable.

### 11.3 Regulated Token

```text
Standard: TSP-1    Skills: Compliance, KYC Gate, Multisig
Config: pay_auth=hash(compliance), pay_hook=PAY_WHITELIST,
        mint_hook=MINT_KYC, update_hook=UPDATE_THRESHOLD
```

### 11.4 Art Collection

```text
Standard: TSP-2    Skills: Royalties, Supply Cap
Config: pay_hook=PAY_ROYALTY, mint_hook=MINT_CAP+MINT_UNIQUE
Flags per asset: transferable=1, burnable=1, updatable=0
```

### 11.5 Soulbound Credential

```text
Standard: TSP-2    Skills: Soulbound
Config: mint_auth=hash(issuer), pay_hook=PAY_SOULBOUND
Flags: transferable=0, burnable=0, updatable=0
```

### 11.6 Game Item Collection

```text
Standard: TSP-2    Skills: Royalties, Burn-to-Redeem (crafting)
Config: mint_auth=hash(game_server), pay_hook=GAME_RULES,
        mint_hook=ITEM_GEN, update_hook=ITEM_EVOLUTION
Flags: transferable=1, burnable=1, updatable=1
```

### 11.7 Yield-Bearing Vault

```text
Standard: TSP-1    Skills: Vault
Config: mint_auth=hash(vault_program),
        mint_hook=VAULT_DEPOSIT, burn_hook=VAULT_WITHDRAW
```

### 11.8 Governance Token

```text
Standard: TSP-1    Skills: Governance, Timelock, Multisig
Config: admin_auth=hash(governance_program),
        update_hook=UPDATE_TIMELOCK+UPDATE_THRESHOLD
```

### 11.9 Stablecoin

```text
Standard: TSP-1    Skills: Stablecoin, Oracle Pricing
Config: mint_auth=hash(minting_program),
        mint_hook=STABLECOIN_MINT, burn_hook=STABLECOIN_REDEEM
```

### 11.10 Wrapped / Bridged Asset

```text
Standard: TSP-1    Skills: Bridging
Config: mint_auth=hash(bridge), burn_auth=hash(bridge),
        mint_hook=BRIDGE_LOCK_PROOF, burn_hook=BRIDGE_RELEASE_PROOF
```

### 11.11 Liquid Staking Token

```text
Standard: TSP-1    Skills: Staking, Vault
Config: mint_auth=hash(staking_program),
        mint_hook=STAKE_DEPOSIT, burn_hook=STAKE_WITHDRAW
```

### 11.12 Subscription Service

```text
Standard: TSP-1    Skills: Delegation, Subscription
Config: pay_hook=PAY_DELEGATION
```

### 11.13 Collateralized Fund

```text
Standard: TSP-1 (collateral) + TSP-1 (shares)
Skills: Lending, Oracle Pricing, Liquidity

Supply: TOKEN_A pay → fund_account (controller=FUND), Oracle price proof,
        fund state recorded, TOKEN_B minted to supplier
Redeem: TOKEN_B burn, Oracle price proof, TOKEN_A released from fund_account
Liquidation: health_factor < 1 proven, liquidator covers debt, receives collateral
```

### 11.14 Card Marketplace

```text
Standard: TSP-2 + TSP-1
Skills: Royalties, Oracle Pricing, Liquidity

Seller transfers card to buyer:
  TSP-2 Pay (asset transfer) + TSP-1 Pay (payment) + TSP-1 Pay (royalty)
  Composed proof: TSP-2 ⊗ TSP-1(payment) ⊗ TSP-1(royalty) → single verification
```

### 11.15 Prediction Market

```text
Standard: N × TSP-1 (outcome tokens)
Skills: Oracle Pricing, Liquidity, Burn-to-Redeem

Create: deploy N tokens (one per outcome), mint requires equal buy-in
Trade: Liquidity strategies for outcome pairs
Resolve: Oracle attests outcome, winning token redeemable 1:1
Redeem: burn winner (burn_hook verifies resolution), receive payout
```

### 11.16 Name Service

```text
Standard: TSP-2    Skills: none (just metadata schema)
Register: mint TSP-2 where asset_id=hash(name), metadata_hash=hash(resolution)
Resolve: Merkle inclusion proof for hash(name) in collection tree
Transfer: standard TSP-2 pay
Update: TSP-2 metadata update (if flags.updatable=1)
```

---

## 🧩 12. Proof Composition Architecture

### 12.1 The Composition Stack

```text
┌─────────────────────────────────────────┐
│           Composed Transaction Proof     │
│                                         │
│  ┌──────────┐  ┌──────────┐            │
│  │ Token A   │  │ Token B   │            │
│  │ Pay Proof │  │ Pay Proof │            │
│  └────┬─────┘  └────┬─────┘            │
│       │              │                   │
│       └──────┬───────┘                   │
│              │                           │
│       ┌──────▼──────┐                    │
│       │ Skill  │                    │
│       │   Proof     │                    │
│       └──────┬──────┘                    │
│              │                           │
│  ┌───────────▼───────────┐              │
│  │   Oracle Pricing      │              │
│  │    Skill Proof   │              │
│  └───────────┬───────────┘              │
│              │                           │
│       ┌──────▼──────┐                    │
│       │ Allocation  │                    │
│       │   Proof     │                    │
│       └─────────────┘                    │
└─────────────────────────────────────────┘
```

### 12.2 Composition Rules

1. All sub-proofs independently verifiable
2. Public I/O consistent across sub-proofs (amounts, accounts, timestamps)
3. Merkle roots chain correctly
4. Triton VM recursive verification → entire composition = single STARK proof
5. Single proof relayable cross-chain

---

## 🛡️ 13. What Neptune Fixes

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

## 🏷️ 14. Naming Convention

| Component | Name | Role |
|---|---|---|
| Framework | PLUMB | Pay, Lock, Update, Mint, Burn |
| Standard | TSP-1 (Coin) | PLUMB implementation for divisible assets |
| Standard | TSP-2 (Card) | PLUMB implementation for unique assets |
| Skill | Liquidity (TIDE) | Tokens In Direct Exchange — swaps without custody |
| Skill | Oracle Pricing (COMPASS) | External data attestation with STARK proofs |
| Skill | *[23 total]* | See Skill Library (sections 7-10) |

---

## 🗺️ 15. Implementation Roadmap

### 0.1 — Foundation (current target)

Deploy basic tokens and interact with them. No skills. The goal is to
validate the PLUMB framework, the two standards, and the proof pipeline
end-to-end.

1. PLUMB framework (auth, config, hook slots)
2. TSP-1 circuit (pay, lock, update, mint, burn)
3. TSP-2 circuit (same operations, unique asset semantics)
4. Token deployment tooling
5. Basic wallet integration

Everything below is post-0.1 — sequenced by dependency, not by deadline.

### Post-0.1 — Skills (community-driven)

Skills are the extensibility layer. They require the foundation to be
stable. Suggested priority based on dependency order:

**First skills** (unblock the rest):
- Supply Cap — simplest skill, validates the hook mechanism
- Delegation — enables subscription and spending limits
- Compliance — enables regulated tokens

**Financial skills** (require working tokens):
- Liquidity (TIDE) — enables proven price
- Oracle Pricing (COMPASS) — enables lending and stablecoins
- Vault, Staking, Lending, Stablecoin

**Composition skills** (require working financial skills):
- Governance, Bridging, Burn-to-Redeem, Batch Operations

The skill library is intentionally large — it maps the full design space.
Not all skills need to be built by the core team. The architecture is
designed so that anyone can implement a skill as a ZK program that
composes through the hook system.

---

## ❓ 16. Open Questions for the Community

The Gold Standard specifies the design space. These questions are
intentionally left open — they require real-world usage, community input,
and implementation experience to answer well. Contributions welcome.

1. **Tree depth.** Depth 20 (~1M leaves). Fixed or variable per token?
2. **Multi-hop swaps.** Atomic A->B->C in one composed proof, or sequential?
3. **Privacy.** How far to push shielded transfers beyond basic pay?
4. **State rent.** Should leaves expire if unused?
5. **Strategy liveness.** Keeper mechanism for dead Liquidity strategies?
6. **Skill versioning.** Can a skill be upgraded, or must you deploy a new one?
7. **Skill discovery.** How does a wallet know which skills a token has?
8. **Skill dependencies.** Should the system enforce that Lending requires Oracle Pricing, or is that the deployer's responsibility?
9. **Controller recursion.** Can a controller program delegate to another controller? (Likely answer: no — depth = 1, to prevent infinite auth chains.)
10. **Fund share pricing.** Should fund shares use Oracle Pricing feeds for their own price, creating a feedback loop?

Note: hook composition ordering (previously listed here) is answered in
section 6.2 — STARK proof composition is commutative, so ordering is a
non-problem.

---

## 📖 Appendix A: Glossary

| Term | Definition |
|---|---|
| PLUMB | Pay, Lock, Update, Mint, Burn — the token framework |
| TSP-1 | Coin standard (PLUMB implementation for divisible assets) |
| TSP-2 | Card standard (PLUMB implementation for unique assets) |
| Skill | A composable package of hooks + optional state tree + config that teaches a token a new behavior |
| Recipe | A documented configuration combining a standard + skills to build a specific token type |
| TIDE | Codename for the Liquidity skill — Tokens In Direct Exchange |
| COMPASS | Codename for the Oracle Pricing skill |
| Proven price | Fee-weighted TWAP derived from swap execution, denominated in base currency |
| Proven fees | Cumulative protocol fees collected in aggregation window — economic signal for price confidence |
| Protocol fee | 0.1% (10 bps) of every swap in NPT, global constant |
| Circuit | AIR constraints defining valid state transitions |
| Config | Hashed commitment binding authorities and hooks |
| Hook | Reusable ZK program composed with token proof |
| Leaf | Merkle tree node — account (TSP-1) or asset (TSP-2) |
| Proof composition | Verifying multiple proofs with shared public inputs |
| Strategy | Pricing program defining an AMM curve (Liquidity skill) |
| Allocation | Virtual balance assigned to a strategy |
| Attestation | Oracle data point with provenance proof |
| Feed | An Oracle Pricing data stream (e.g. BTC/USD price) |
| Controller | Program ID that must co-authorize operations on a leaf |
| State commitment | Block-level hash of all Merkle tree roots |

## 📋 Appendix B: Skill Quick Reference

### Core

| Skill | Hooks | State Tree | Composes With |
|---|---|---|---|
| Supply Cap | `mint_hook` | No | Everything |
| Delegation | `pay_hook` | Yes (delegation tree) | Subscription, Compliance |
| Vesting | `mint_hook` | Yes (schedule tree) | Supply Cap, Governance |
| Royalties | `pay_hook` | No | Liquidity, Oracle Pricing |
| Multisig | `update_hook` | No (membership token) | Governance, Timelock |
| Timelock | `update_hook` | No | Multisig, Governance |

### Financial

| Skill | Hooks | State Tree | Composes With |
|---|---|---|---|
| Liquidity (TIDE) | `pay_hook` | Yes (allocation tree) | Oracle Pricing, Staking, Governance |
| Oracle Pricing (COMPASS) | — | Yes (attestation tree) | Liquidity, Lending, Stablecoin, Bridging |
| Vault | `mint_hook`, `burn_hook` | No | Oracle Pricing, Lending, Staking |
| Lending | `mint_hook`, `burn_hook` | Yes (position tree) | Oracle Pricing, Liquidity |
| Staking | `lock_hook`, `mint_hook`, `burn_hook` | Optional | Liquidity, Governance |
| Stablecoin | `mint_hook`, `burn_hook` | Yes (collateral tree) | Oracle Pricing, Lending, Liquidity |

### Access Control

| Skill | Hooks | State Tree | Composes With |
|---|---|---|---|
| Compliance | `pay_hook` | Yes (address set) | KYC Gate, Delegation |
| KYC Gate | `mint_hook` | No | Compliance, Soulbound |
| Transfer Limits | `pay_hook` | Yes (rate tracking) | Compliance, Delegation |
| Controller Gate | `pay_hook` | No | Lending, Vault |
| Soulbound | `pay_hook` | No | KYC Gate |
| Fee-on-Transfer | `pay_hook` | No | Compliance, Liquidity |

### Composition

| Skill | Hooks | State Tree | Composes With |
|---|---|---|---|
| Bridging | `mint_hook`, `burn_hook` | No | Oracle Pricing |
| Subscription | `pay_hook` | Yes (delegation tree) | Delegation |
| Burn-to-Redeem | `burn_hook` | No | Any mint operation |
| Governance | `update_hook` | Yes (proposal tree) | Timelock, Multisig, Staking |
| Batch Operations | `mint_hook` | No | Supply Cap |

## 📋 Appendix C: Hook ID Reference

### Pay Hooks
| ID | Skill |
|---|---|
| `PAY_WHITELIST` | Compliance |
| `PAY_BLACKLIST` | Compliance |
| `PAY_LIMIT` | Transfer Limits |
| `PAY_ROYALTY` | Royalties |
| `PAY_SOULBOUND` | Soulbound |
| `PAY_FEE` | Fee-on-Transfer |
| `PAY_DELEGATION` | Delegation / Subscription |
| `PAY_CONTROLLER` | Controller Gate |
| `PAY_STRATEGY` | Liquidity (TIDE) |
| `PAY_COLLATERAL` | Lending (collateral release) |

### Mint Hooks
| ID | Skill |
|---|---|
| `MINT_CAP` | Supply Cap |
| `MINT_UNIQUE` | TSP-2 uniqueness check |
| `MINT_ALLOWLIST` | Compliance (mint-side) |
| `MINT_VESTING` | Vesting |
| `MINT_KYC` | KYC Gate |
| `MINT_BATCH` | Batch Operations |
| `MINT_FUND` | Lending (mint shares) |
| `VAULT_DEPOSIT` | Vault |
| `STAKE_DEPOSIT` | Staking |
| `STABLECOIN_MINT` | Stablecoin |
| `BRIDGE_LOCK_PROOF` | Bridging |

### Burn Hooks
| ID | Skill |
|---|---|
| `BURN_TAX` | Fee-on-Transfer (burn-side) |
| `BURN_REDEEM` | Burn-to-Redeem |
| `BURN_MINIMUM` | Transfer Limits (burn-side) |
| `BURN_FUND_REDEEM` | Lending (redeem shares) |
| `BURN_LIQUIDATE` | Lending (liquidation) |
| `VAULT_WITHDRAW` | Vault |
| `STAKE_WITHDRAW` | Staking |
| `STABLECOIN_REDEEM` | Stablecoin |
| `BRIDGE_RELEASE_PROOF` | Bridging |

### Lock Hooks
| ID | Skill |
|---|---|
| `LOCK_MAX` | Transfer Limits (max lock duration) |
| `LOCK_REWARDS` | Staking |
| `LOCK_RENTAL` | Composition (TSP-2 rental) |
| `LOCK_PROGRAM` | Controller Gate (program lock) |

### Update Hooks
| ID | Skill |
|---|---|
| `UPDATE_TIMELOCK` | Timelock |
| `UPDATE_THRESHOLD` | Multisig |
| `UPDATE_MIGRATION` | Composition (one-time migration) |

## 🔗 See Also

- [Tutorial](../tutorials/tutorial.md) — Language basics
- [Programming Model](programming-model.md) — Execution model and stack semantics
- [OS Reference](../reference/os.md) — OS concepts and `os.token` bindings
- [Multi-Target Compilation](multi-target.md) — One source, every chain
- [Deploying a Program](../guides/deploying-a-program.md) — Deployment workflows
