# Neptune Gold Standard

## ZK-Native Financial Primitives for Triton VM

**Version:** 0.5  
**Date:** February 10, 2026  

### Implementation Status

| Primitive | Status | Example Code |
|-----------|--------|--------------|
| **PLUMB framework** | Implemented | `ext/triton/kernel.tri`, `ext/triton/utxo.tri` |
| **TSP-1** (Fungible tokens) | Implemented | `examples/neptune/type_custom_token.tri` |
| **TSP-2** (NFTs) | Implemented | `examples/nft/nft.tri` |
| **Native currency** | Implemented | `examples/neptune/type_native_currency.tri` |
| **Lock scripts** | Implemented | `examples/neptune/lock_*.tri` (4 variants) |
| **Transaction validation** | Implemented | `examples/neptune/transaction_validation.tri` |
| **Proof composition** | Implemented | `ext/triton/proof.tri`, `examples/neptune/proof_aggregator.tri` |
| **TIDE** (Liquidity) | Design only | Specified below, not yet implemented |
| **COMPASS** (Oracle) | Design only | Specified below, not yet implemented |
| **Hook library** | Design only | Architecture specified, hooks not yet codified |

See the [Tutorial](tutorial.md) for language basics, [Programming Model](programming-model.md) for the Neptune transaction model, and [Deploying a Program](deploying-a-program.md) for deployment workflows.

---

## 1. Philosophy

Neptune's financial layer is not a port of Ethereum's ERC standards. It is designed from first principles for a STARK-provable virtual machine where every state transition produces a cryptographic proof.

Three axioms drive every decision:

1. **Tokens are leaves, not contracts.** A token is not a deployed program with storage. It is a leaf in a Merkle tree whose state transitions are constrained by a circuit. The circuit is the standard. The leaf is the instance.

2. **Liquidity is never locked.** Capital remains in user accounts. DeFi protocols do not custody tokens — they prove valid transformations against user balances via hook composition. One balance can back many strategies simultaneously.

3. **Proofs compose, programs don't call.** There is no `msg.sender` calling a contract. There is a proof that a valid state transition occurred, composed with proofs from hook programs. Composition replaces invocation.

---

## 2. The Gold Standard — Architecture Overview

Neptune's gold standard consists of four primitives, a hook library, and documented patterns for building everything else.

### 2.1 Four Primitives

| Primitive | Name | What it provides | Has own state tree |
|---|---|---|---|
| TSP-1 | Fungible token | Divisible value transfer, supply conservation | Yes — account tree |
| TSP-2 | Non-fungible token | Unique asset ownership, metadata, royalties | Yes — asset tree |
| TIDE | Unified liquidity | Swaps without custody, shared liquidity | Yes — allocation tree |
| COMPASS | Oracle | External data attestation with STARK proofs | Yes — attestation tree |

Each primitive is a circuit with its own Merkle tree. Each produces STARK proofs that compose with each other.

### 2.2 Why Only Four

The hook system + proof composition makes the primitive set very small. A primitive earns its place only if it:

1. Requires its own state structure (Merkle tree) — hooks don't have state
2. Cannot be expressed as a PLUMB deployment + hooks
3. Is depended upon by multiple applications

Everything else is either a **hook program** (reusable constraint logic), a **deployment pattern** (config recipe), or an **application** (composed from primitives + hooks).

### 2.3 Layer Architecture

```
┌─────────────────────────────────────────────────────┐
│  APPLICATIONS                                        │
│  Governance, Lending, Vaults, Stablecoins,           │
│  Multisig, Bridges, Name Service, Staking            │
├─────────────────────────────────────────────────────┤
│  DEPLOYMENT PATTERNS                                 │
│  Documented configs: "to build X, use these hooks"   │
├─────────────────────────────────────────────────────┤
│  HOOK LIBRARY                                        │
│  Reusable ZK programs that compose with operations   │
│  Whitelist, Royalty, Cap, Timelock, Threshold, ...   │
├─────────────────────────────────────────────────────┤
│  PRIMITIVES                                          │
│  TSP-1 (Fungible) │ TSP-2 (NFT) │ TIDE │ COMPASS   │
├─────────────────────────────────────────────────────┤
│  PLUMB FRAMEWORK                                     │
│  Leaf format, Config, Hooks, Auth, 5 Operations      │
└─────────────────────────────────────────────────────┘
```

---

## 3. PLUMB — The Token Framework

**P**ay, **L**ock, **U**pdate, **M**int, **B**urn

PLUMB is the architectural foundation that all Neptune token standards share. It defines:

- **Leaf format** — 10 field elements, hashed to Digest, stored in a binary Merkle tree
- **Config commitment** — 5 authorities + 5 hooks, hashed to Digest
- **Metadata commitment** — standalone descriptive data, hashed to Digest
- **Operation set** — 5 operations (Pay, Lock, Update, Mint, Burn) with uniform proof structure
- **Auth model** — `auth_hash` per leaf + per-operation config-level dual authorization
- **Hook system** — per-operation composable ZK programs
- **Nullifier scheme** — `hash(id, nonce)` for replay prevention
- **Global public state** — `state_root`, `supply`, `config_hash`, `metadata_hash`, `current_time`

### 3.1 Config — Shared by All PLUMB Standards

```
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

**Composition model:** The token circuit proves state transition validity. The verifier composes the token proof with the hook proof. If `hook == 0`, no external proof required.

### 3.4 Cross-Token Proof Composition

Hooks are not limited to their own token's state. A hook can require proofs from any primitive or application as input. The verifier composes all required proofs together.

Example: TOKEN_B's `mint_hook` requires:
1. A valid TOKEN_A pay proof (collateral deposited)
2. A valid COMPASS price proof (collateral valuation)
3. A ratio check (mint amount ≤ collateral × price × LTV)

The hook circuit declares its required inputs. The verifier ensures all sub-proofs are valid and their public I/O is consistent (same accounts, same amounts, same timestamps).

This is how DeFi works in Neptune: operations on one token compose with operations on other tokens, oracle feeds, and application state — all in a single atomic proof.

### 3.5 Application State Trees

Applications (funds, lending, governance) can maintain their own Merkle trees beyond the four primitives. Application proofs compose with primitive proofs through hooks.

An application state tree follows the same pattern as primitive trees:
- 10-field leaves hashed to Digest
- Binary Merkle tree
- State root committed on-chain
- Operations produce STARK proofs

The difference: application trees are not standardized. Each application defines its own leaf format and constraints. What IS standardized is how application proofs compose with primitive proofs — through the hook system.

### 3.6 Atomic Multi-Tree Commitment

A single Neptune transaction may update multiple Merkle trees:
- TOKEN_A tree (collateral deposited)
- TOKEN_B tree (shares minted)
- COMPASS tree (price read)
- Application tree (position recorded)

The block commits to ALL tree roots atomically via a **state commitment**:

```
block_state = hash(
  token_tree_root_1, token_tree_root_2, ..., token_tree_root_N,
  tide_allocation_root,
  compass_attestation_root,
  app_tree_root_1, ..., app_tree_root_M
)
```

A transaction's composed proof references the old and new state commitment. The block verifier ensures all tree roots transition consistently — no partial updates.

### 3.7 No Approvals

PLUMB has no `approve`, `allowance`, or `transferFrom`. The approve/transferFrom pattern is the largest attack surface in ERC-20. In Neptune:

| Ethereum pattern | Neptune solution |
|---|---|
| DEX swap via `transferFrom` | Two coordinated `pay` ops (TIDE) |
| Lending deposit via `transferFrom` | `pay` to lending account, or `lock` with hook |
| Subscription / recurring payment | Derived auth key satisfying `auth_hash` |
| Meta-transaction / relayer | Anyone with auth secret constructs the proof |
| Multi-step DeFi | Proof composition — all movements proven atomically |

For delegated spending: `auth_hash` derived keys + `pay_hook` tracking cumulative spending per delegate. Strictly more powerful, strictly safer than approve.

### 3.8 Security Properties

1. **No negative balances:** `as_u32()` range check
2. **Replay prevention:** Monotonic nonce + nullifiers
3. **Time-lock enforcement:** `current_time` from block
4. **Lock monotonicity:** Can only extend, not shorten
5. **Supply conservation:** Public invariant
6. **Account abstraction:** `auth_hash` = privkey, Shamir, ZK proof, anything
7. **Config binding:** Every op verifies full config hash
8. **Irreversible renounce:** `admin_auth = 0` = frozen forever
9. **Config-state separation:** Config updates can't touch tree
10. **Hook composability:** Hooks bound to config hash
11. **Symmetric authority:** Every op has authority + hook
12. **Safe defaults:** `mint_auth = 0` = disabled, others `= 0` = permissionless
13. **No approvals:** No allowances, no `transferFrom`, no approval phishing

---

## 4. Two Standards on PLUMB

### 4.1 Why Two, Not One

Fungible and non-fungible tokens have incompatible conservation laws. Forcing both into one circuit creates branching that inflates the Algebraic Execution Table for every proof.

A fungible token with `balance ∈ {0,1}` is not an NFT — it lacks uniqueness proofs, metadata binding, royalty enforcement. An NFT with `supply > 1` is not a fungible token — it lacks divisible arithmetic and range checks.

Two lean circuits always outperform one bloated circuit with conditional branches.

### 4.2 Why Not Three

In Ethereum, ERC-1155 exists because contract deployment is expensive. In Neptune, creating a new token = new config + new tree. Negligible cost. Batching is proof aggregation, not a multi-token contract.

### 4.3 Why This Works for Triton VM

The expensive resource is the circuit (AIR constraints). The cheap resource is leaf data. Both standards use 10-field leaves, same config, same hooks, same proof pipeline. Only constraint polynomials differ. Tooling that understands one understands 90% of the other.

---

## 5. TSP-1 — Fungible Token Standard

*PLUMB implementation for divisible assets*

### 5.1 Account Leaf — 10 field elements

```
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

When `controller ≠ 0`, every operation on this leaf requires a composed proof from the controller program in addition to normal auth. This enables **program-controlled accounts** — leaves that can only be moved by a specific ZK program.

Use cases:
- **Fund accounts:** collateral held by fund program, released only on valid redemption/liquidation proof
- **Escrow:** tokens held until condition is met
- **Protocol treasuries:** spending requires governance proof

The circuit checks: if `leaf.controller ≠ 0`, the verifier must compose with a valid proof from program `controller`. This is additive — both `auth_hash` AND controller must be satisfied.

For a fully program-controlled account (no human key), set `auth_hash` to a known value that the controller program can derive deterministically.

#### Locked-by Field

When `locked_by ≠ 0`, the account's tokens are committed to a specific program. The `lock_data` field carries program-specific state (e.g. which fund position this collateral backs).

Unlike `lock_until` (time-based), `locked_by` is **program-based locking**: only a proof from the `locked_by` program can unlock the account. The lock can be released before `lock_until` if the program authorizes it (e.g. on redemption).

This enables collateral patterns: "these tokens are locked as collateral for fund F, position P, and can only be released when fund F proves the position is closed."

### 5.2 Token Metadata

```
metadata = hash(name_hash, ticker_hash, teaser_hash, site_hash, custom_hash,
                price_oracle, volume_oracle, 0, 0, 0)
```

### 5.3 Circuit Constraints

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

## 6. TSP-2 — Non-Fungible Token Standard

*PLUMB implementation for unique assets*

### 6.1 What Differs from TSP-1

1. **Leaf** represents an asset (unique item), not an account balance
2. **Invariant:** uniqueness (`owner_count(id) = 1`) not divisible supply
3. **Constraints:** `balance` always 0 or 1, metadata/royalty/creator fields active

Operations are still Pay, Lock, Update, Mint, Burn — PLUMB operations. What changes is what the circuit enforces inside each.

### 6.2 Asset Leaf — 10 field elements

```
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
| `flags` | Field | Bits: transferable, burnable, updatable |

First 5 fields occupy same positions as TSP-1. Last 5 — reserved zeros in TSP-1 — carry per-asset state in TSP-2.

### 6.3 Collection Metadata

```
metadata = hash(name_hash, description_hash, image_hash, site_hash, custom_hash,
                max_supply, royalty_receiver, 0, 0, 0)
```

### 6.4 Circuit Constraints

#### Op 0: Pay (Transfer Ownership)
1–6. Same as TSP-1 Pay, plus:
7. `leaf.flags & TRANSFERABLE`
8. New leaf: `owner_id = new_owner`, `auth_hash = new_auth`, `nonce += 1`
9. If `royalty_bps > 0`: royalty proof composed via `pay_hook`
10. `asset_count` unchanged

#### Op 3: Mint (Originate)
1–2. Same as TSP-1 Mint, plus:
3. `asset_id` not in tree (uniqueness proof)
4. `creator_id` set to minter (immutable forever)
5. `new_asset_count == old_asset_count + 1`

#### Op 4: Burn (Release)
1–4. Same as TSP-1 Burn, plus:
5. `leaf.flags & BURNABLE`
6. Leaf → null
7. `new_asset_count == old_asset_count - 1`

---

## 7. TIDE — Unified Liquidity Protocol

*Tokens In Direct Exchange*

### 7.1 The Problem

Traditional AMMs lock tokens in custodial pool contracts. The same capital cannot simultaneously serve as AMM liquidity, lending collateral, governance votes, and staking weight. Each protocol demands exclusive custody.

Uniswap V3 improved within-pool efficiency. Aqua (1inch, 2025) let one balance back multiple strategies without locking. Neptune makes this architecturally native.

### 7.2 Swaps as Coordinated Pay Operations

PLUMB tokens live in user accounts. There is no pool to lock into. Swaps are two `pay` operations where the `pay_hook` enforces the pricing curve.

```
Alice swaps 100 TOKEN_A for TOKEN_B with maker Bob:

  TOKEN_A Pay: Alice → Bob, amount=100, pay_hook=STRATEGY
  TOKEN_B Pay: Bob → Alice, amount=f(100), pay_hook=STRATEGY

  Composed proof: Token_A ⊗ Token_B ⊗ Strategy → single verification
```

No tokens leave user accounts. No approvals. No router.

### 7.3 Shared Liquidity

Because the AMM is a hook, Bob's balance simultaneously:
- Backs AMM Strategy X and Y
- Serves as lending collateral (via lending hook)
- Counts for governance votes
- Earns staking rewards

Atomic consistency without locks: if two strategies try to move same tokens in one block, second proof fails (Merkle root already changed).

### 7.4 Strategy Registration

```
strategy = hash(maker, token_a, token_b, program, parameters)
```

Immutable once registered. Revoke and re-register to modify.

### 7.5 Virtual Allocations

Allocation tree tracks how much of a maker's balance each strategy can access:

**`Σ(allocations[maker][token]) ≤ balance[maker][token]`**

Overcommitment is safe — every swap proof checks the current balance.

### 7.6 Strategy Programs

Pluggable ZK circuits. Reference implementations:

| Strategy | Description | Key property |
|---|---|---|
| **Constant Product** | x·y = k | Simple, proven, universal |
| **Stable Swap** | Curve-style invariant | Optimized for pegged pairs |
| **Concentrated Liquidity** | Positions in price ranges | Capital-efficient, active management |
| **Oracle-Priced** | Anchored to COMPASS feed | Eliminates impermanent loss — Neptune-unique |

### 7.7 Comparison

| Property | Uniswap V3 | Aqua (1inch) | Neptune TIDE |
|---|---|---|---|
| Custody | Pool contract | Wallet (allowance) | Wallet (Merkle leaf) |
| Execution | EVM call | EVM call | ZK proof composition |
| Multi-strategy | No | Yes | Yes |
| Pricing proof | Trust EVM | Trust EVM | STARK-proven |
| Cross-chain | No | No | Yes (relay proof) |
| Capital in governance | No | Yes | Yes |
| MEV surface | Large | Reduced | Minimal |

---

## 8. COMPASS — Oracle Standard

*External data attestation with STARK proofs*

### 8.1 Why Oracle is a Primitive

Hooks *consume* external data but cannot *produce* it. Someone must commit data, prove its derivation, and make it queryable. Without a standardized oracle:

- TIDE's oracle-priced strategies can't work
- Lending protocols can't calculate liquidation thresholds
- Stablecoins can't maintain their peg
- Synthetics can't track underlying prices
- Any cross-chain verification lacks a data source

The oracle is to DeFi what `auth_hash` is to tokens — the external input everything depends on.

### 8.2 Why This Can't Be a Hook

A hook adds constraints to an existing operation. An oracle is not triggered by a token operation — it has its own lifecycle:

1. Data providers submit attestations
2. Attestations are aggregated (median, TWAP, etc.)
3. The aggregated value is committed to a Merkle tree
4. Any hook or application reads the commitment

This requires its own state tree and its own circuit. It's a primitive.

### 8.3 State Model

#### Attestation Leaf — 10 field elements

```
leaf = hash(feed_id, value, timestamp, provider_id, nonce,
            confidence, source_hash, proof_hash, 0, 0)
```

| Field | Type | Description |
|---|---|---|
| `feed_id` | Field | Unique identifier for the data feed (e.g. hash("BTC/USD")) |
| `value` | Field | The attested value (price, rate, measurement, etc.) |
| `timestamp` | Field | When the value was observed |
| `provider_id` | Field | Identity of data provider (pubkey hash) |
| `nonce` | Field | Monotonic counter per provider per feed |
| `confidence` | Field | Provider's confidence score or precision |
| `source_hash` | Field | Hash of source description (exchange, API, computation) |
| `proof_hash` | Field | Hash of derivation proof (how value was computed) |

#### Feed Config — 10 field elements

```
config = hash(admin_auth, submit_auth, aggregate_auth, 0, 0,
              submit_hook, aggregate_hook, read_hook, 0, 0)
```

| Field | Type | Description |
|---|---|---|
| `admin_auth` | Field | Feed administrator. `0` = renounced |
| `submit_auth` | Field | Who can submit attestations. `0` = permissionless |
| `aggregate_auth` | Field | Who can trigger aggregation. `0` = permissionless |
| `submit_hook` | Field | Validation on submission (stake requirement, reputation) |
| `aggregate_hook` | Field | Aggregation logic (median, TWAP, weighted average) |
| `read_hook` | Field | Access control on reads (fee, stake, whitelist) |

#### Feed Metadata — 10 field elements

```
metadata = hash(name_hash, pair_hash, decimals, heartbeat, deviation_threshold,
                min_providers, max_staleness, 0, 0, 0)
```

| Field | Type | Description |
|---|---|---|
| `name_hash` | Field | Hash of feed name |
| `pair_hash` | Field | Hash of pair description (e.g. "BTC/USD") |
| `decimals` | Field | Decimal precision of values |
| `heartbeat` | Field | Expected update frequency (seconds) |
| `deviation_threshold` | Field | Min % change to trigger update |
| `min_providers` | Field | Minimum providers for valid aggregation |
| `max_staleness` | Field | Max age before feed is considered stale |

### 8.4 Operations

#### Submit

A provider submits a new attestation.

**Constraints:**
1. Config verified, `submit_auth` and `submit_hook` extracted
2. Provider authorization (if `submit_auth ≠ 0`)
3. `timestamp <= current_time` (no future attestations)
4. `timestamp > leaf.timestamp` (newer than previous from same provider)
5. `nonce == old_nonce + 1`
6. New leaf in attestation tree

The `submit_hook` can enforce: provider must have staked tokens (compose with TSP-1 proof), provider reputation score, value within deviation bounds, etc.

#### Aggregate

Combine multiple provider attestations into a single canonical value.

**Constraints:**
1. Config verified, `aggregate_auth` and `aggregate_hook` extracted
2. Read N provider leaves from tree (Merkle inclusion proofs)
3. `N >= config.min_providers`
4. All attestations within `max_staleness` of `current_time`
5. Compute aggregated value (determined by `aggregate_hook`)
6. Commit aggregated value to feed's canonical slot

The `aggregate_hook` determines the aggregation function:
- **Median** — middle value of sorted submissions
- **TWAP** — time-weighted average over a window
- **Weighted** — reputation-weighted or stake-weighted average
- **Outlier-filtered** — remove values beyond N standard deviations

#### Read (Proof Generation)

Produce a STARK proof that a feed has value V at time T.

This is not an on-chain operation — it's a proof that any hook or application can compose with. The proof attests: "feed F had aggregated value V at Merkle root R, which corresponds to block B."

Any TIDE strategy, lending liquidation, or stablecoin mechanism that needs a price composes with this proof.

### 8.5 The Neptune-Unique Property

In Chainlink or Pyth, oracle data comes with a signature — you trust the signers. In Neptune, oracle data comes with a **STARK proof of its derivation**:

- The aggregation circuit proves the median was correctly computed from N provider submissions
- Each submission can include a `proof_hash` linking to a proof of how the value was derived (e.g. proof that the provider correctly read an exchange API)
- The composed proof covers the entire chain from raw data to aggregated value

This means: a TIDE oracle-priced strategy composes three proofs:
1. Token A pay proof (balance transfer)
2. Token B pay proof (balance transfer)
3. Oracle proof (price is V, provably aggregated from N sources)

The swap price is not trusted — it is **mathematically verified**.

### 8.6 Cross-Chain Oracle

Because oracle proofs are STARKs, they can be relayed to other chains. A price attestation on Neptune can be verified on any chain that can verify STARK proofs, without trusting a bridge or a multisig.

---

## 9. Hook Library

Standard, reusable ZK programs that compose with PLUMB operations. These are reference implementations — anyone can write custom hooks, but these cover the most common needs.

### 9.1 Pay Hooks

| Hook | ID | Description | Composes with |
|---|---|---|---|
| **Whitelist** | `PAY_WHITELIST` | Sender and/or receiver must be in a Merkle set | Membership proof |
| **Blacklist** | `PAY_BLACKLIST` | Sender and/or receiver must NOT be in a Merkle set | Non-membership proof |
| **Transfer Limit** | `PAY_LIMIT` | Max amount per tx or per period per account | Rate tracking state |
| **Royalty** | `PAY_ROYALTY` | Enforce % to creator/receiver on TSP-2 transfers | TSP-1 pay proof (royalty payment) |
| **Soulbound** | `PAY_SOULBOUND` | Reject all transfers unconditionally | None — always fails |
| **Fee-on-Transfer** | `PAY_FEE` | Deduct % to treasury on every transfer | TSP-1 pay proof (fee payment) |
| **Delegation** | `PAY_DELEGATION` | Allow authorized delegates to spend up to limits | Delegation tree |
| **Controller Gate** | `PAY_CONTROLLER` | Verify composed proof from leaf's controller program | Controller proof |
| **Collateral Release** | `PAY_COLLATERAL` | Release locked collateral on valid redemption/liquidation proof | Fund state proof + COMPASS |

#### Whitelist Hook — Detail

The whitelist hook maintains a Merkle tree of approved addresses. On every pay operation, the hook proof verifies:
1. `hash(sender) ∈ whitelist_tree` (Merkle inclusion proof)
2. `hash(receiver) ∈ whitelist_tree` (Merkle inclusion proof)

Config-level authority controls who can add/remove from the whitelist tree.

Use cases: regulated tokens, accredited investor restrictions, sanctioned address blocking.

#### Royalty Hook — Detail

On every TSP-2 pay (ownership transfer), the royalty hook:
1. Reads `royalty_bps` from the asset leaf
2. Reads `royalty_receiver` from collection metadata
3. Requires a composed TSP-1 pay proof: buyer pays `(sale_price × royalty_bps / 10000)` to `royalty_receiver`
4. Sale price is either declared by parties or derived from a COMPASS feed

This enforces royalties at the protocol level — not optional, not bypassable via wrapper contracts.

#### Delegation Hook — Detail

Maintains a delegation tree with leaves:
```
delegation = hash(owner, delegate, token, limit, spent, expiry)
```

On pay, the hook checks:
1. If caller is owner: pass (normal auth)
2. If caller is delegate: verify `spent + amount ≤ limit` and `current_time < expiry`
3. Update `spent += amount`

This replaces ERC-20's approve/allowance with bounded, expiring, revocable delegation.

### 9.2 Mint Hooks

| Hook | ID | Description |
|---|---|---|
| **Supply Cap** | `MINT_CAP` | Enforce `new_supply ≤ max_supply` |
| **Uniqueness** | `MINT_UNIQUE` | For TSP-2: verify `asset_id` not in tree |
| **Allowlist** | `MINT_ALLOWLIST` | Only approved addresses can receive mints |
| **Vesting** | `MINT_VESTING` | Release tokens on schedule (compose with time check) |
| **KYC Gate** | `MINT_KYC` | Require KYC credential (compose with TSP-2 soulbound proof) |
| **Batch** | `MINT_BATCH` | Mint multiple in one proof (recursive composition) |
| **Fund Mint** | `MINT_FUND` | Mint shares against collateral + oracle price proof |

#### Supply Cap Hook — Detail

Simple but critical. The hook verifies:
1. Read `max_supply` from metadata (or hardcoded in hook parameters)
2. `new_supply <= max_supply`

Without this hook, TSP-1 minting is uncapped. With it, the cap is cryptographically enforced.

#### Vesting Hook — Detail

Maintains a vesting schedule:
```
schedule = hash(beneficiary, total_amount, start_time, cliff, duration, claimed)
```

On mint, the hook:
1. `elapsed = current_time - start_time`
2. If `elapsed < cliff`: reject
3. `vested = total_amount × min(elapsed, duration) / duration`
4. `amount ≤ vested - claimed`
5. `claimed += amount`

### 9.3 Burn Hooks

| Hook | ID | Description |
|---|---|---|
| **Burn Tax** | `BURN_TAX` | Send % to treasury instead of destroying |
| **Burn-to-Redeem** | `BURN_REDEEM` | Burning proves eligibility (compose with mint or unlock) |
| **Minimum** | `BURN_MINIMUM` | Enforce minimum burn amount |
| **Fund Redeem** | `BURN_FUND_REDEEM` | Burn shares → release collateral at oracle-evaluated rate |
| **Liquidation** | `BURN_LIQUIDATE` | Partial burn at discount when health_factor < 1 |

#### Burn-to-Redeem — Detail

A powerful pattern: burn a TSP-2 asset to receive something else.

The hook produces a receipt proof. This receipt composes with a mint operation on another token:
```
Burn(TSP-2 item) → receipt proof ⊗ Mint(TSP-1 reward token)
```

Use cases: burn NFT to claim physical goods, burn ticket to access event, burn old token to receive upgraded version.

### 9.4 Lock Hooks

| Hook | ID | Description |
|---|---|---|
| **Max Duration** | `LOCK_MAX` | Prevent locks beyond a maximum timestamp |
| **Lock Rewards** | `LOCK_REWARDS` | Compose with reward distribution |
| **Rental** | `LOCK_RENTAL` | TSP-2: lock asset with temporary usage rights |
| **Program Lock** | `LOCK_PROGRAM` | Lock tokens to a program (sets `locked_by` + `lock_data`) |

### 9.5 Update Hooks

| Hook | ID | Description |
|---|---|---|
| **Timelock** | `UPDATE_TIMELOCK` | Config changes require delay period |
| **Threshold** | `UPDATE_THRESHOLD` | Multiple auth proofs required (M-of-N) |
| **Migration** | `UPDATE_MIGRATION` | One-time config migration with safety checks |

#### Threshold Hook — Detail

The multisig pattern. Does not require a separate primitive:

1. Deploy a TSP-1 token with supply = N, one per signer
2. On config update, the threshold hook requires M composed pay proofs from token holders
3. Each pay proof proves: "I hold 1 token in this multisig set and I authorize this update"

The token IS the membership. The hook IS the threshold logic.

---

## 10. Deployment Patterns

Documented configurations combining primitives + hooks.

### 10.1 Simple Fungible Token

```
Standard: TSP-1
Config:
  admin_auth: hash(admin)
  pay_auth: 0                     // permissionless
  lock_auth: 0
  mint_auth: hash(minter)
  burn_auth: 0
  all hooks: 0
```

The simplest possible token. Anyone can transfer and burn. Admin can update config. Authorized minter can mint.

### 10.2 Immutable Money

```
Standard: TSP-1
Config:
  admin_auth: 0                   // renounced — forever frozen
  mint_auth: 0                    // no more minting
  all other auth: 0
  all hooks: 0
```

After genesis mint, nothing can change. Pure permissionless sound money. No admin, no minting, no hooks. The config hash is verifiably immutable.

### 10.3 Regulated Token

```
Standard: TSP-1
Config:
  admin_auth: hash(admin)
  pay_auth: hash(compliance)      // dual auth on transfers
  lock_auth: hash(regulator)      // regulator can freeze
  mint_auth: hash(treasury)
  burn_auth: hash(compliance)
Hooks:
  pay_hook: PAY_WHITELIST          // sender+receiver must be approved
  mint_hook: MINT_KYC              // KYC required
  update_hook: UPDATE_THRESHOLD    // multi-party approval for config changes
```

### 10.4 NFT Art Collection

```
Standard: TSP-2
Config:
  admin_auth: hash(collection_admin)
  pay_auth: 0
  lock_auth: 0
  mint_auth: hash(artist)
  burn_auth: 0
Hooks:
  pay_hook: PAY_ROYALTY            // enforces royalty on every transfer
  mint_hook: MINT_CAP + MINT_UNIQUE // max supply + uniqueness
Flags per asset: transferable=1, burnable=1, updatable=0
```

### 10.5 Soulbound Credential

```
Standard: TSP-2
Config:
  admin_auth: hash(issuer)
  mint_auth: hash(issuer)
  pay_hook: PAY_SOULBOUND          // blocks all transfers
Flags: transferable=0, burnable=0, updatable=0
```

### 10.6 Game Item Collection

```
Standard: TSP-2
Config:
  admin_auth: hash(game_admin)
  mint_auth: hash(game_server)
Hooks:
  pay_hook: custom GAME_RULES      // validates in-game transfer rules
  mint_hook: custom ITEM_GEN       // item generation + class supply
  update_hook: ITEM_EVOLUTION      // allows metadata updates (leveling, crafting)
Flags: transferable=1, burnable=1, updatable=1
```

### 10.7 Vault / Yield-Bearing Token

```
Standard: TSP-1 (the share token)
Config:
  mint_auth: hash(vault_program)
  burn_auth: 0
Hooks:
  mint_hook: VAULT_DEPOSIT         // deposit asset → receive shares at exchange rate
  burn_hook: VAULT_WITHDRAW        // burn shares → receive asset at exchange rate
```

The vault hook maintains exchange rate state: `total_assets / total_shares`. On deposit, it mints shares proportional to deposited assets. On withdrawal, it burns shares and releases proportional assets.

This is ERC-4626 as a deployment pattern, not a separate standard. The virtual shares defense against inflation attacks is built into the hook (initial offset at deployment).

### 10.8 Multisig

```
Standard: TSP-1 (membership token, supply = N)
Config:
  mint_auth: 0                    // fixed membership
  pay_auth: 0
Hooks:
  none on the membership token itself

Usage: any config that needs M-of-N sets
  update_hook: UPDATE_THRESHOLD referencing this membership token
```

Not a primitive. Not a separate standard. Just a token + a hook.

### 10.9 Governance Token

```
Standard: TSP-1
Config:
  admin_auth: hash(governance_program)
  mint_auth: hash(governance_program)
Hooks:
  update_hook: UPDATE_TIMELOCK + UPDATE_THRESHOLD
```

Governance is an application that:
1. Uses historical Merkle roots as balance snapshots (free — inherent to architecture)
2. Maintains a proposal tree (application state, not a primitive)
3. Composes vote proofs with balance inclusion proofs against snapshot root
4. Queues execution behind a timelock hook
5. Uses threshold hook for emergency actions

### 10.10 Stablecoin

```
Standard: TSP-1
Config:
  mint_auth: hash(minting_program)
  burn_auth: 0
Hooks:
  mint_hook: STABLECOIN_MINT       // requires collateral proof + COMPASS price feed
  burn_hook: STABLECOIN_REDEEM     // releases collateral proportional to burn
```

The mint hook composes with:
- COMPASS oracle proof (collateral price)
- TSP-1 lock proof (collateral locked)
- Collateral ratio check (e.g. 150% minimum)

The burn hook composes with:
- TSP-1 pay proof (release collateral to burner)
- Exchange rate verification

### 10.11 Wrapped / Bridged Asset

```
Standard: TSP-1
Config:
  mint_auth: hash(bridge_program)
  burn_auth: hash(bridge_program)
Hooks:
  mint_hook: BRIDGE_LOCK_PROOF     // requires STARK proof of lock on source chain
  burn_hook: BRIDGE_RELEASE_PROOF  // produces proof for release on source chain
```

### 10.12 Liquid Staking Token

```
Standard: TSP-1 (the LST)
Config:
  mint_auth: hash(staking_program)
Hooks:
  mint_hook: STAKE_DEPOSIT         // deposit native token → receive LST
  burn_hook: STAKE_WITHDRAW        // burn LST → queue unstaking
```

Combined with a vault pattern for the exchange rate. The LST appreciates as staking rewards accrue.

### 10.13 Subscription / Streaming Payments

```
Standard: TSP-1
Config:
  pay_hook: PAY_DELEGATION
```

Service provider registers as delegate with:
- `limit`: monthly subscription amount
- `expiry`: subscription end date

Each month, service calls pay using delegation authority. Hook enforces rate limit. User revokes by changing `auth_hash`.

### 10.14 Fund / Collateralized Minting

The canonical DeFi pattern: supply one token, receive another based on oracle-evaluated price. Exercises all four primitives.

```
Tokens:
  TOKEN_A: TSP-1 (collateral asset, e.g. ETH)
  TOKEN_B: TSP-1 (fund shares / stablecoin / synthetic)

TOKEN_B Config:
  mint_auth: hash(fund_program)
  burn_auth: 0
Hooks:
  mint_hook: FUND_MINT             // requires collateral + price proof
  burn_hook: FUND_REDEEM           // releases collateral on burn
```

**Supply flow (all four primitives composed):**

```
1. Alice does TOKEN_A pay:
     From: Alice → fund_account
     fund_account.controller = FUND_PROGRAM
     fund_account.locked_by = FUND_PROGRAM
     fund_account.lock_data = hash(position_id)

2. COMPASS proves TOKEN_A price = V

3. Fund program updates position in its application state tree:
     position = { alice, collateral_amount, price, ltv_ratio, health_factor }

4. TOKEN_B mint to Alice:
     mint_hook composes with:
       ⊗ TOKEN_A pay proof (collateral deposited)
       ⊗ COMPASS price proof (valuation)
       ⊗ Fund state proof (position recorded)
     amount = collateral × price × ltv_ratio
```

**Redemption flow:**

```
1. Alice does TOKEN_B burn (amount = shares to redeem)

2. COMPASS proves current TOKEN_A price

3. Fund program computes owed = f(shares_burned, current_price, fund_state)
   Updates position in application state tree

4. TOKEN_A pay: fund_account → Alice
   Authorized by fund_account.controller = FUND_PROGRAM
   fund_account.locked_by cleared (if position fully closed)

All composed into single atomic proof.
```

**Liquidation flow:**

```
1. COMPASS proves TOKEN_A price dropped → health_factor < 1

2. Liquidator provides TOKEN_B (partial debt coverage)

3. Fund program authorizes collateral release at discount:
   TOKEN_A pay: fund_account → liquidator
   Authorized by controller = FUND_PROGRAM

4. Fund state tree: position updated or closed

All composed — liquidator submits single proof.
```

**Key properties:**
- Collateral is in `fund_account` with `controller = FUND_PROGRAM` — only the fund program can move it
- `locked_by` tracks which program locked the collateral and `lock_data` links to the position
- Fund shares (TOKEN_B) are standard TSP-1 — tradeable via TIDE
- Fund_account's TOKEN_A can back TIDE strategies while serving as collateral (fund as maker)
- Liquidation is permissionless — anyone can prove health_factor < 1 and execute
- The entire supply/redeem/liquidate flow is one atomic composed proof

---

## 11. Application Patterns

Applications are composed from primitives + hooks but have their own state and logic beyond what hooks provide. They are not part of the gold standard — they are built on it.

### 11.1 Governance

**Components used:** TSP-1 (governance token), historical Merkle roots (snapshots), UPDATE_TIMELOCK hook, UPDATE_THRESHOLD hook

**Application state:** Proposal tree (separate Merkle tree managed by governance program)

**Flow:**
1. Create proposal: commit to proposal tree
2. Snapshot: record current TSP-1 state root at proposal creation
3. Vote: voter proves balance at snapshot root (Merkle inclusion proof against historical root), casts vote
4. Tally: aggregation circuit counts votes, verifies quorum
5. Execute: if passed, queue behind timelock, then execute config updates

No governance primitive needed. Balance snapshots are free (every historical Merkle root is a snapshot).

### 11.2 Lending / Borrowing

**Components used:** TSP-1 (asset tokens + receipt tokens), COMPASS (price feeds), TIDE (liquidation swaps)

**Application state:** Position tree (collateral, debt, health factors per user)

**Flow:**
1. Supply: user pays TSP-1 into lending pool account, receives receipt token (vault pattern)
2. Borrow: user locks collateral (TSP-1 lock), mints debt token, receives borrowed asset
3. Interest: receipt token exchange rate appreciates over time
4. Liquidation: if health factor < 1 (checked via COMPASS price), liquidator can repay debt and receive collateral at discount via TIDE swap

Lending is an application, not a primitive. It composes TSP-1 + COMPASS + TIDE.

### 11.3 Name Service

**Components used:** TSP-2 (names as unique assets)

**Flow:**
1. Register: mint a TSP-2 asset where `asset_id = hash(name)`, `metadata_hash = hash(resolution_record)`
2. Resolve: Merkle inclusion proof for `hash(name)` in the name collection tree
3. Transfer: standard TSP-2 pay (ownership transfer)
4. Update resolution: TSP-2 metadata update (if `flags.updatable = 1`)

Name service is just an NFT collection with a specific metadata schema.

### 11.4 Prediction Markets

**Components used:** TSP-1 (outcome tokens), COMPASS (resolution oracle), TIDE (trading)

**Flow:**
1. Create market: deploy N TSP-1 tokens (one per outcome), mint_hook requires equal buy-in
2. Trade: TIDE strategies for outcome token pairs
3. Resolve: COMPASS oracle attests outcome, winning token becomes redeemable 1:1
4. Redeem: burn winning token (burn_hook verifies oracle resolution), receive payout

### 11.5 Insurance / Options

**Components used:** TSP-2 (option/policy as unique asset), TSP-1 (premium/collateral), COMPASS (price triggers)

**Flow:**
1. Writer mints TSP-2 option, locks collateral via TSP-1 lock
2. Buyer purchases option via TIDE swap
3. Exercise: at expiry, COMPASS proves price condition, burn-to-redeem releases collateral

---

## 12. Proof Composition Architecture

### 12.1 The Composition Stack

```
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
│       │  Strategy   │                    │
│       │   Proof     │                    │
│       └──────┬──────┘                    │
│              │                           │
│  ┌───────────▼───────────┐              │
│  │     COMPASS Oracle    │              │
│  │       Price Proof     │              │
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

## 13. What Neptune Fixes

### vs. ERC-20

| Problem | Solution |
|---|---|
| `approve()` race condition | No approvals — `auth_hash` + delegation hook |
| Unlimited approval risk | No approvals exist |
| No time-locks | `lock_until` first-class |
| No mint/burn access control | Per-operation authorities |
| Tokens trapped in contracts | Tokens in user accounts |
| ERC-777 hooks = reentrancy | Hooks via proof composition |
| Supply not provable | `supply` conservation in circuit |

### vs. ERC-721

| Problem | Solution |
|---|---|
| Royalties not enforceable | `royalty_bps` + PAY_ROYALTY hook |
| No native collections | `collection_id` in leaf |
| Metadata frozen | `flags.updatable` per asset |
| Separate standard | Same PLUMB framework |

### vs. Uniswap

| Problem | Solution |
|---|---|
| Liquidity locked | Stays in maker accounts |
| Fragmentation | Same capital backs multiple strategies |
| Impermanent loss | Oracle-priced strategies via COMPASS |
| MEV | Proof-based, no public mempool |

### vs. Chainlink

| Problem | Solution |
|---|---|
| Trust oracle signers | STARK proof of aggregation |
| Opaque computation | Provable derivation chain |
| Chain-specific | Cross-chain via proof relay |

---

## 14. Naming Convention

| Component | Name | Role |
|---|---|---|
| Framework | **PLUMB** | **P**ay, **L**ock, **U**pdate, **M**int, **B**urn |
| TSP-1 | Fungible token | PLUMB implementation for divisible assets |
| TSP-2 | Non-fungible token | PLUMB implementation for unique assets |
| Liquidity | **TIDE** | **T**okens **I**n **D**irect **E**xchange |
| Oracle | **COMPASS** | External data attestation with STARK proofs |

---

## 15. Implementation Roadmap

### Phase 0 — Genesis
1. PLUMB framework (auth, config, hook composition)
2. TSP-1 circuit
3. Token deployment tooling
4. Hook library: PAY_WHITELIST, MINT_CAP, PAY_SOULBOUND

### Phase 1 — Ownership
5. TSP-2 circuit
6. Hook library: PAY_ROYALTY, MINT_UNIQUE, PAY_DELEGATION
7. Wallet integration (both standards)

### Phase 2 — Exchange
8. TIDE allocation tree circuit
9. Constant product strategy
10. Stable swap strategy
11. Strategy registration + aggregator

### Phase 3 — Oracle
12. COMPASS attestation tree circuit
13. Submit + Aggregate operations
14. Median aggregation hook
15. TWAP aggregation hook

### Phase 4 — Provable Pricing
16. Oracle-priced TIDE strategy (compose COMPASS + TIDE)
17. Concentrated liquidity strategy
18. Cross-chain proof relay

### Phase 5 — Ecosystem
19. Remaining hook library (UPDATE_TIMELOCK, UPDATE_THRESHOLD, LOCK_REWARDS, BURN_REDEEM, MINT_VESTING)
20. Deployment pattern documentation and tooling
21. Application reference implementations (governance, vault, stablecoin)

---

## 16. Open Questions

1. **Tree depth:** Depth 20 (~1M leaves). Fixed or variable?
2. **Allocation tree:** Separate or integrated into token tree?
3. **Multi-hop swaps:** Atomic A→B→C or sequential?
4. **Privacy:** How far to push shielded transfers?
5. **State rent:** Should leaves expire?
6. **Strategy liveness:** Keeper mechanism for dead strategies?
7. **TSP-2 naming:** Distinctive name for the NFT standard.
8. **COMPASS provider incentives:** How are oracle providers rewarded?
9. **Hook versioning:** Immutability or upgrade path?
10. **Controller recursion:** Can a controller program delegate to another controller? Or is one level sufficient?
11. **Fund share pricing:** Should fund share tokens use COMPASS feeds for their own price, creating a feedback loop? Or must share price be derived purely from collateral + position state?

---

## Appendix A: Glossary

| Term | Definition |
|---|---|
| PLUMB | Pay, Lock, Update, Mint, Burn — the token framework |
| TSP-1 | Fungible token standard (PLUMB implementation) |
| TSP-2 | Non-fungible token standard (PLUMB implementation) |
| TIDE | Tokens In Direct Exchange — unified liquidity |
| COMPASS | Oracle — external data attestation with STARK proofs |
| Circuit | AIR constraints defining valid state transitions |
| Config | Hashed commitment binding authorities and hooks |
| Hook | Reusable ZK program composed with token proof |
| Leaf | Merkle tree node — account (TSP-1) or asset (TSP-2) |
| Proof composition | Verifying multiple proofs with shared public inputs |
| Strategy | Pricing program defining an AMM curve |
| Allocation | Virtual balance assigned to a strategy |
| Maker | Liquidity provider who registers strategies |
| Taker | User who executes swaps |
| Attestation | Oracle data point with provenance proof |
| Feed | A COMPASS data stream (e.g. BTC/USD price) |
| Deployment pattern | Config recipe for building a specific token type |
| Application | Software built on primitives + hooks with own state |
| Controller | Program ID that must co-authorize operations on a leaf |
| Locked-by | Program ID that holds a program-based lock on a leaf |
| State commitment | Block-level hash of all Merkle tree roots |
| Fund pattern | Collateralized minting: supply TOKEN_A → receive TOKEN_B at oracle price |

## Appendix B: Hook Library Quick Reference

### Pay Hooks
| ID | Purpose | Composable with |
|---|---|---|
| `PAY_WHITELIST` | Approved addresses only | Membership Merkle tree |
| `PAY_BLACKLIST` | Blocked addresses excluded | Non-membership proof |
| `PAY_LIMIT` | Max amount per tx/period | Rate tracking state |
| `PAY_ROYALTY` | Creator royalty on NFT transfers | TSP-1 pay proof |
| `PAY_SOULBOUND` | Block all transfers | — |
| `PAY_FEE` | Fee-on-transfer to treasury | TSP-1 pay proof |
| `PAY_DELEGATION` | Delegated spending with limits | Delegation tree |
| `PAY_CONTROLLER` | Verify controller program proof | Controller proof |
| `PAY_COLLATERAL` | Release locked collateral on redemption/liquidation | Fund state + COMPASS |

### Mint Hooks
| ID | Purpose |
|---|---|
| `MINT_CAP` | Max supply enforcement |
| `MINT_UNIQUE` | TSP-2 uniqueness verification |
| `MINT_ALLOWLIST` | Approved recipients only |
| `MINT_VESTING` | Time-based release schedule |
| `MINT_KYC` | KYC credential required |
| `MINT_BATCH` | Multiple mints in one proof |
| `MINT_FUND` | Mint shares against collateral + oracle price |

### Burn Hooks
| ID | Purpose |
|---|---|
| `BURN_TAX` | % to treasury on burn |
| `BURN_REDEEM` | Burn proves eligibility for receipt |
| `BURN_MINIMUM` | Enforce minimum burn amount |
| `BURN_FUND_REDEEM` | Burn shares → release collateral at oracle rate |
| `BURN_LIQUIDATE` | Partial burn at discount when health_factor < 1 |

### Lock Hooks
| ID | Purpose |
|---|---|
| `LOCK_MAX` | Maximum lock duration |
| `LOCK_REWARDS` | Compose with reward distribution |
| `LOCK_RENTAL` | Temporary usage rights (TSP-2) |
| `LOCK_PROGRAM` | Lock tokens to a program (sets `locked_by` + `lock_data`) |

### Update Hooks
| ID | Purpose |
|---|---|
| `UPDATE_TIMELOCK` | Config changes require delay |
| `UPDATE_THRESHOLD` | M-of-N approval required |
| `UPDATE_MIGRATION` | One-time migration with safety checks |