# 🥇 Neptune Gold Standard

## 🏛️ ZK-Native Token Standards and Capability Library

**Version:** 0.6
**Date:** February 12, 2026

### Implementation Status

| Component | Status | Example Code |
|-----------|--------|--------------|
| **PLUMB framework** | Implemented | `os/neptune/kernel.tri`, `os/neptune/utxo.tri` |
| **TSP-1** (Coin) | Implemented | `examples/neptune/type_custom_token.tri` |
| **TSP-2** (Uniq) | Implemented | `examples/uniq/uniq.tri` |
| **Native currency** | Implemented | `examples/neptune/type_native_currency.tri` |
| **Lock scripts** | Implemented | `examples/neptune/lock_*.tri` (4 variants) |
| **Transaction validation** | Implemented | `examples/neptune/transaction_validation.tri` |
| **Proof composition** | Implemented | `os/neptune/proof.tri`, `examples/neptune/proof_aggregator.tri` |
| **Capability library** | Design only | 23 capabilities specified below |

See the [Tutorial](../tutorials/tutorial.md) for language basics, [Programming Model](programming-model.md) for the Neptune transaction model, and [Deploying a Program](../guides/deploying-a-program.md) for deployment workflows.

---

## 🔭 1. Philosophy

Neptune's financial layer is not a port of Ethereum's ERC standards. It is designed from first principles for a STARK-provable virtual machine where every state transition produces a cryptographic proof.

Three axioms drive every decision:

1. **Tokens are leaves, not contracts.** A token is not a deployed program with storage. It is a leaf in a Merkle tree whose state transitions are constrained by a circuit. The circuit is the standard. The leaf is the instance.

2. **Liquidity is never locked.** Capital remains in user accounts. DeFi protocols do not custody tokens — they prove valid transformations against user balances via capability composition. One balance can back many strategies simultaneously.

3. **Proofs compose, programs don't call.** There is no `msg.sender` calling a contract. There is a proof that a valid state transition occurred, composed with proofs from capability programs. Composition replaces invocation.

---

## 🏗️ 2. Architecture Overview

### 2.1 Two Standards

Neptune has exactly two token standards. Both are built on PLUMB.

| Standard | Name | What it defines | Conservation law |
|----------|------|-----------------|------------------|
| TSP-1 | Coin | Divisible value transfer | `Σ balances = supply` |
| TSP-2 | Uniq | Unique asset ownership | `owner_count(id) = 1` |

A standard earns its place by defining a **conservation law** — an invariant that the circuit enforces on every operation. Divisible supply and unique ownership are incompatible conservation laws, so they require separate circuits. Everything else is a capability.

### 2.2 Capability Library

A **capability** is a skill that a token can acquire. It is a composable package:

- **Hooks** it installs (which PLUMB operations it extends)
- **State tree** it needs (if any — most capabilities are stateless)
- **Config** it requires (which authorities and hooks must be set)
- **Composes with** other capabilities it works alongside

Capabilities are how tokens learn to do things beyond basic transfers. A coin that can provide liquidity has the Liquidity capability. A coin that enforces KYC has the Compliance capability. A uniq that pays creator royalties has the Royalties capability.

The hook system makes this possible. Every PLUMB operation has a hook slot. A capability installs hooks into those slots. Multiple capabilities can coexist on the same token — their hook proofs compose independently.

### 2.3 Why Two Standards, Not Four Primitives

The old model had four "primitives": TSP-1, TSP-2, TIDE (liquidity), COMPASS (oracle). But TIDE and COMPASS fail the standard test — they don't define conservation laws. They define behaviors. A liquidity protocol is something a token *does*, not something a token *is*. An oracle is a service that capabilities *consume*, not a peer of the token standards.

The hook system already supports capability state trees (section 3.5). TIDE's allocation tree and COMPASS's attestation tree are just capability state trees — architecturally identical to any other. They are capabilities, not standards.

### 2.4 Layer Architecture

```text
┌──────────────────────────────────────────────────────────┐
│  RECIPES                                                  │
│  Documented configs: "to build X, use these capabilities" │
├──────────────────────────────────────────────────────────┤
│  CAPABILITY LIBRARY                                       │
│  Composable skills: Liquidity, Oracle, Governance,        │
│  Lending, Compliance, Delegation, Vesting, Royalties,     │
│  Staking, Bridging, Subscription, ...                     │
│                                                           │
│  Each = hooks + optional state tree + config              │
├──────────────────────────────────────────────────────────┤
│  STANDARDS                                                │
│  TSP-1 (Coin)              │  TSP-2 (Uniq)               │
├──────────────────────────────────────────────────────────┤
│  PLUMB FRAMEWORK                                          │
│  Leaf format, Config, Hooks, Auth, 5 Operations           │
└──────────────────────────────────────────────────────────┘
```

---

## 🧩 3. PLUMB — The Token Framework

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

**Composition model:** The token circuit proves state transition validity. The verifier composes the token proof with the hook proof. If `hook == 0`, no external proof required.

### 3.4 Cross-Token Proof Composition

Hooks are not limited to their own token's state. A hook can require proofs from any capability or token as input. The verifier composes all required proofs together.

Example: TOKEN_B's `mint_hook` requires:
1. A valid TOKEN_A pay proof (collateral deposited)
2. A valid Oracle Pricing proof (collateral valuation)
3. A ratio check (mint amount ≤ collateral × price × LTV)

The hook circuit declares its required inputs. The verifier ensures all sub-proofs are valid and their public I/O is consistent (same accounts, same amounts, same timestamps).

This is how DeFi works in Neptune: operations on one token compose with operations on other tokens, oracle feeds, and capability state — all in a single atomic proof.

### 3.5 Capability State Trees

Capabilities that need persistent state maintain their own Merkle trees. A capability state tree follows the same pattern as standard trees:
- 10-field leaves hashed to Digest
- Binary Merkle tree
- State root committed on-chain
- Operations produce STARK proofs

The Liquidity capability's allocation tree, the Oracle Pricing capability's attestation tree, a Governance capability's proposal tree — all are capability state trees. What IS standardized is how capability proofs compose with token proofs through the hook system.

### 3.6 Atomic Multi-Tree Commitment

A single Neptune transaction may update multiple Merkle trees:
- TOKEN_A tree (collateral deposited)
- TOKEN_B tree (shares minted)
- Oracle attestation tree (price read)
- Capability state tree (position recorded)

The block commits to ALL tree roots atomically via a **state commitment**:

```trident
block_state = hash(
  token_tree_root_1, token_tree_root_2, ..., token_tree_root_N,
  capability_tree_root_1, ..., capability_tree_root_M
)
```

A transaction's composed proof references the old and new state commitment. The block verifier ensures all tree roots transition consistently — no partial updates.

### 3.7 No Approvals

PLUMB has no `approve`, `allowance`, or `transferFrom`. The approve/transferFrom pattern is the largest attack surface in ERC-20. In Neptune:

| Ethereum pattern | Neptune solution |
|---|---|
| DEX swap via `transferFrom` | Two coordinated `pay` ops (Liquidity capability) |
| Lending deposit via `transferFrom` | `pay` to lending account, or `lock` with hook |
| Subscription / recurring payment | Derived auth key satisfying `auth_hash` |
| Meta-transaction / relayer | Anyone with auth secret constructs the proof |
| Multi-step DeFi | Proof composition — all movements proven atomically |

For delegated spending: `auth_hash` derived keys + Delegation capability tracking cumulative spending per delegate. Strictly more powerful, strictly safer than approve.

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

## 📐 4. Two Standards on PLUMB

### 4.1 Why Two, Not One

Coins and uniqs have incompatible conservation laws. Forcing both into one circuit creates branching that inflates the Algebraic Execution Table for every proof.

A coin with `balance ∈ {0,1}` is not a uniq — it lacks uniqueness proofs, metadata binding, royalty enforcement. A uniq with `supply > 1` is not a coin — it lacks divisible arithmetic and range checks.

Two lean circuits always outperform one bloated circuit with conditional branches.

### 4.2 Why Not Three

In Ethereum, ERC-1155 exists because contract deployment is expensive. In Neptune, creating a new token = new config + new tree. Negligible cost. Batching is proof aggregation, not a multi-token contract.

### 4.3 Why This Works for Triton VM

The expensive resource is the circuit (AIR constraints). The cheap resource is leaf data. Both standards use 10-field leaves, same config, same hooks, same proof pipeline. Only constraint polynomials differ. Tooling that understands one understands 90% of the other.

---

## 🥇 5. TSP-1 — Coin Standard

*PLUMB implementation for divisible assets*

### 5.1 Account Leaf — 10 field elements

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

When `controller ≠ 0`, every operation on this leaf requires a composed proof from the controller program in addition to normal auth. This enables **program-controlled accounts** — leaves that can only be moved by a specific ZK program.

Use cases:
- **Fund accounts:** collateral held by fund program, released only on valid redemption/liquidation proof
- **Escrow:** tokens held until condition is met
- **Protocol treasuries:** spending requires governance proof

The circuit checks: if `leaf.controller ≠ 0`, the verifier must compose with a valid proof from program `controller`. This is additive — both `auth_hash` AND controller must be satisfied.

#### Locked-by Field

When `locked_by ≠ 0`, the account's tokens are committed to a specific program. The `lock_data` field carries program-specific state (e.g. which fund position this collateral backs).

Unlike `lock_until` (time-based), `locked_by` is **program-based locking**: only a proof from the `locked_by` program can unlock the account. The lock can be released before `lock_until` if the program authorizes it (e.g. on redemption).

### 5.2 Token Metadata

```trident
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

## 🥇 6. TSP-2 — Uniq Standard

*PLUMB implementation for unique assets*

### 6.1 What Differs from TSP-1

1. **Leaf** represents an asset (unique item), not an account balance
2. **Invariant:** uniqueness (`owner_count(id) = 1`) not divisible supply
3. **No divisible arithmetic** — no `balance`, no range checks, no splitting
4. **Per-asset state** — metadata, royalty, creator, flags live in the leaf
5. **Creator immutability** — `creator_id` is set at mint and can never change
6. **Flag-gated operations** — transferable, burnable, updatable bits control which PLUMB operations are allowed per asset

Operations are still Pay, Lock, Update, Mint, Burn — PLUMB operations. What changes is what the circuit enforces inside each.

### 6.2 Asset Leaf — 10 field elements

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

Flags are set at mint time and **cannot be changed** after creation. A soulbound credential is minted with `flags = 0`. A game item uses `flags = 31` (all operations). A standard collectible uses `flags = 11` (transferable + burnable + lockable).

#### Collection Binding

When `collection_id ≠ 0`, the asset belongs to a collection identified by its config hash. Collection membership is **immutable after mint**.

#### Creator Immutability

`creator_id` is set at mint and can **never change**. Every subsequent operation preserves it. This provides an unforgeable provenance chain. The Royalties capability depends on this: hooks read `royalty_bps` from the leaf and `royalty_receiver` from collection metadata.

### 6.3 Collection Metadata — 10 field elements

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

### 6.4 Circuit Constraints

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
**Config update:** `old_root == new_root`, admin auth, `admin_auth ≠ 0`, new config fields.
**Metadata update:** Owner auth, `flags & UPDATABLE`, only `metadata_hash` changes, `nonce += 1`.

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

## 🧰 7. Capability Library

### 7.1 What Is a Capability

A capability is a composable package that gives a token a new skill. Every capability has the same anatomy:

| Component | Description |
|-----------|-------------|
| **Skill** | What the token can now do |
| **Hooks** | Which PLUMB hooks it installs |
| **State tree** | Whether it needs its own Merkle tree |
| **Config** | What authorities/hooks must be set |
| **Composes with** | Which other capabilities it works alongside |

A token with no capabilities is a bare TSP-1 or TSP-2 — it can pay, lock, update, mint, and burn. Each capability you add teaches it a new skill.

### 7.2 How Capabilities Compose

Multiple capabilities can be active on the same token simultaneously. When multiple capabilities install hooks on the same operation, their proofs compose independently:

```text
Pay operation with Compliance + Fee-on-Transfer + Liquidity:
  1. Token circuit proves valid balance transfer
  2. Compliance hook proves sender and receiver are whitelisted
  3. Fee-on-Transfer hook proves treasury received its cut
  4. Liquidity hook proves pricing curve was satisfied
  Verifier composes: Token ⊗ Compliance ⊗ Fee ⊗ Liquidity → single proof
```

Convention: access control hooks verify first, then financial hooks, then composition hooks.

### 7.3 Capability Tiers

| Tier | Focus | Capabilities |
|------|-------|-------------|
| **Core** | Skills most tokens want | Supply Cap, Delegation, Vesting, Royalties, Multisig, Timelock |
| **Financial** | DeFi use cases | Liquidity, Oracle Pricing, Vault, Lending, Staking, Stablecoin |
| **Access Control** | Compliance and permissions | Compliance, KYC Gate, Transfer Limits, Controller Gate, Soulbound, Fee-on-Transfer |
| **Composition** | Cross-token interaction | Bridging, Subscription, Burn-to-Redeem, Governance, Batch Operations |

---

## 🔧 8. Core Capabilities

### 8.1 Supply Cap

| | |
|---|---|
| **Skill** | Fixed maximum supply — cryptographically enforced ceiling |
| **Hooks** | `mint_hook` = `MINT_CAP` |
| **State tree** | No |
| **Config** | `mint_auth` must be set (minting enabled) |
| **Composes with** | Everything — most fundamental financial constraint |

The hook verifies: `new_supply <= max_supply` (read from metadata or hardcoded in hook parameters). Without this capability, TSP-1 minting is uncapped. With it, the cap is provably enforced.

### 8.2 Delegation

| | |
|---|---|
| **Skill** | Let others spend on your behalf with limits and expiry |
| **Hooks** | `pay_hook` = `PAY_DELEGATION` |
| **State tree** | Yes — delegation tree |
| **Config** | `pay_hook` must be set |
| **Composes with** | Subscription, Compliance |

Replaces ERC-20's `approve`/`allowance` with bounded, expiring, revocable delegation.

**Delegation leaf:**
```trident
delegation = hash(owner, delegate, token, limit, spent, expiry, 0, 0, 0, 0)
```

On pay, the hook checks: if caller is delegate, verify `spent + amount ≤ limit` and `current_time < expiry`, then `spent += amount`. Owner revokes by changing `auth_hash`.

### 8.3 Vesting

| | |
|---|---|
| **Skill** | Time-locked token release on a schedule |
| **Hooks** | `mint_hook` = `MINT_VESTING` |
| **State tree** | Yes — vesting schedule tree |
| **Config** | `mint_auth` = vesting program |
| **Composes with** | Supply Cap, Governance |

**Vesting schedule leaf:**
```trident
schedule = hash(beneficiary, total_amount, start_time, cliff, duration, claimed, 0, 0, 0, 0)
```

On mint: `elapsed = current_time - start_time`. If `elapsed < cliff`: reject. `vested = total_amount × min(elapsed, duration) / duration`. `amount ≤ vested - claimed`. `claimed += amount`.

### 8.4 Royalties (TSP-2)

| | |
|---|---|
| **Skill** | Enforce creator royalties on every transfer — not optional, not bypassable |
| **Hooks** | `pay_hook` = `PAY_ROYALTY` |
| **State tree** | No — reads `royalty_bps` from leaf, `royalty_receiver` from metadata |
| **Config** | `pay_hook` must be set |
| **Composes with** | Liquidity (marketplace), Oracle Pricing (floor price) |

On every TSP-2 transfer, the hook:
1. Reads `royalty_bps` from the asset leaf
2. Reads `royalty_receiver` from collection metadata
3. Requires a composed TSP-1 pay proof: buyer pays `(sale_price × royalty_bps / 10000)` to `royalty_receiver`

Enforced at the protocol level. No wrapper contract bypass.

### 8.5 Multisig / Threshold

| | |
|---|---|
| **Skill** | Require M-of-N approval for config changes |
| **Hooks** | `update_hook` = `UPDATE_THRESHOLD` |
| **State tree** | No — uses a TSP-1 membership token as the signer set |
| **Config** | `update_hook` must be set |
| **Composes with** | Governance, Timelock |

Deploy a TSP-1 token with `supply = N`, one per signer. On config update, the threshold hook requires M composed pay proofs from token holders. The token IS the membership. The hook IS the threshold logic. Not a separate primitive.

### 8.6 Timelock

| | |
|---|---|
| **Skill** | Mandatory delay period on config changes |
| **Hooks** | `update_hook` = `UPDATE_TIMELOCK` |
| **State tree** | No |
| **Config** | `update_hook` must be set |
| **Composes with** | Multisig, Governance |

Config changes are queued and can only execute after the delay period. Prevents surprise rug-pulls. Commonly combined with Multisig: threshold approval + mandatory delay.

---

## 💰 9. Financial Capabilities

### 9.1 Liquidity (TIDE)

*Tokens In Direct Exchange*

| | |
|---|---|
| **Skill** | Earn on providing liquidity — tokens stay in your account |
| **Hooks** | `pay_hook` = `PAY_STRATEGY` (the pricing curve) |
| **State tree** | Yes — allocation tree |
| **Config** | `pay_hook` must reference a strategy program |
| **Composes with** | Oracle Pricing, Staking, Governance |

#### How It Works

Traditional AMMs lock tokens in custodial pool contracts. The Liquidity capability eliminates custody entirely. Swaps are two `pay` operations where the `pay_hook` enforces the pricing curve:

```text
Alice swaps 100 TOKEN_A for TOKEN_B with maker Bob:

  TOKEN_A Pay: Alice → Bob, amount=100, pay_hook=STRATEGY
  TOKEN_B Pay: Bob → Alice, amount=f(100), pay_hook=STRATEGY

  Composed proof: Token_A ⊗ Token_B ⊗ Strategy → single verification
```

No tokens leave user accounts. No approvals. No router.

#### Shared Liquidity

Because the AMM is a hook, Bob's balance simultaneously:
- Backs AMM Strategy X and Y
- Serves as lending collateral (via Lending capability)
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

**`Σ(allocations[maker][token]) ≤ balance[maker][token]`**

Overcommitment is safe — every swap proof checks the current balance.

#### Strategy Programs

Pluggable ZK circuits. Reference implementations:

| Strategy | Description | Key property |
|---|---|---|
| **Constant Product** | x·y = k | Simple, proven, universal |
| **Stable Swap** | Curve-style invariant | Optimized for pegged pairs |
| **Concentrated Liquidity** | Positions in price ranges | Capital-efficient, active management |
| **Oracle-Priced** | Anchored to Oracle Pricing feed | Eliminates impermanent loss |

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

### 9.2 Oracle Pricing (COMPASS)

*External data attestation with STARK proofs*

| | |
|---|---|
| **Skill** | Price feeds with STARK-proven aggregation — verified, not trusted |
| **Hooks** | Consumed by other capabilities (mint_hook, pay_hook compose with oracle proofs) |
| **State tree** | Yes — attestation tree |
| **Config** | Feed config (submit_auth, aggregate_auth, hooks) |
| **Composes with** | Liquidity, Lending, Stablecoin, Bridging |

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

**Submit:** A provider submits a new attestation. Constraints: provider authorization, `timestamp <= current_time`, newer than previous, `nonce == old_nonce + 1`. The `submit_hook` can enforce staking requirements, reputation scores, deviation bounds.

**Aggregate:** Combine multiple attestations into a canonical value. Constraints: N leaves from tree, `N >= min_providers`, all within `max_staleness`. The `aggregate_hook` determines the function: median, TWAP, weighted average, outlier-filtered.

**Read:** Produce a STARK proof that feed F has value V at time T. Not an on-chain operation — a proof that any capability can compose with.

#### The Neptune-Unique Property

In Chainlink or Pyth, oracle data comes with a signature — you trust the signers. In Neptune, oracle data comes with a **STARK proof of its derivation**. The aggregation circuit proves the median was correctly computed from N submissions. The composed proof covers the entire chain from raw data to aggregated value. Swap prices are not trusted — they are **mathematically verified**.

#### Cross-Chain Oracle

Oracle proofs are STARKs. They can be relayed to other chains and verified without trusting a bridge or multisig.

### 9.3 Vault / Yield-Bearing

| | |
|---|---|
| **Skill** | Deposit asset, receive shares at exchange rate (ERC-4626 as a capability) |
| **Hooks** | `mint_hook` = `VAULT_DEPOSIT`, `burn_hook` = `VAULT_WITHDRAW` |
| **State tree** | No — exchange rate derived from `total_assets / total_shares` |
| **Config** | `mint_auth` = vault program |
| **Composes with** | Oracle Pricing, Lending, Staking |

On deposit: mint shares proportional to deposited assets. On withdrawal: burn shares, release proportional assets. Inflation attack defense built into the hook (initial offset at deployment).

### 9.4 Lending / Collateral

| | |
|---|---|
| **Skill** | Use tokens as collateral to borrow against |
| **Hooks** | `mint_hook` = `FUND_MINT`, `burn_hook` = `FUND_REDEEM` + `BURN_LIQUIDATE` |
| **State tree** | Yes — position tree (user, collateral, debt, health_factor) |
| **Config** | `mint_auth` = lending program |
| **Composes with** | Oracle Pricing (mandatory), Liquidity (liquidation swaps) |

**Supply flow:**
1. TOKEN_A pay to fund account (`controller = FUND_PROGRAM`, `locked_by = FUND_PROGRAM`)
2. Oracle Pricing proves TOKEN_A price = V
3. Fund program records position in its state tree
4. TOKEN_B mint to borrower: `amount = collateral × price × ltv_ratio`

**Liquidation:** If `health_factor < 1` (checked via Oracle Pricing), anyone can prove the condition and execute — liquidator covers debt, receives collateral at discount.

### 9.5 Staking

| | |
|---|---|
| **Skill** | Lock tokens to earn rewards |
| **Hooks** | `lock_hook` = `LOCK_REWARDS`, `mint_hook` = `STAKE_DEPOSIT`, `burn_hook` = `STAKE_WITHDRAW` |
| **State tree** | Optional — reward distribution state |
| **Config** | `lock_auth` may be set for mandatory staking |
| **Composes with** | Liquidity (staked tokens back strategies), Governance |

Combined with Vault capability for a liquid staking token (LST): deposit native token → receive LST that appreciates as staking rewards accrue.

### 9.6 Stablecoin

| | |
|---|---|
| **Skill** | Maintain a peg through collateral + oracle pricing |
| **Hooks** | `mint_hook` = `STABLECOIN_MINT`, `burn_hook` = `STABLECOIN_REDEEM` |
| **State tree** | Yes — collateral position tree |
| **Config** | `mint_auth` = minting program |
| **Composes with** | Oracle Pricing (mandatory), Lending, Liquidity |

Mint hook composes with: Oracle Pricing proof (collateral price), TSP-1 lock proof (collateral locked), collateral ratio check (e.g. 150% minimum). Burn hook releases collateral proportional to burn amount.

---

## 🔐 10. Access Control Capabilities

### 10.1 Compliance (Whitelist / Blacklist)

| | |
|---|---|
| **Skill** | Restrict who can send/receive tokens |
| **Hooks** | `pay_hook` = `PAY_WHITELIST` or `PAY_BLACKLIST` |
| **State tree** | Yes — approved/blocked address Merkle set |
| **Config** | `pay_auth` may enforce dual auth |
| **Composes with** | KYC Gate, Delegation |

**Whitelist:** On every pay, hook proves `hash(sender) ∈ whitelist_tree` and `hash(receiver) ∈ whitelist_tree` via Merkle inclusion proofs.

**Blacklist:** Non-membership proofs — proves addresses are NOT in the blocked set.

Use cases: regulated tokens, accredited investor restrictions, sanctioned address blocking.

### 10.2 KYC Gate

| | |
|---|---|
| **Skill** | Require verified identity credential to mint or receive |
| **Hooks** | `mint_hook` = `MINT_KYC` |
| **State tree** | No — composes with a TSP-2 soulbound credential proof |
| **Config** | `mint_auth` must be set |
| **Composes with** | Compliance, Soulbound |

The hook requires a composed proof that the recipient holds a valid soulbound credential (TSP-2 with `flags = 0`).

### 10.3 Transfer Limits

| | |
|---|---|
| **Skill** | Cap transfer amounts per transaction or per time period |
| **Hooks** | `pay_hook` = `PAY_LIMIT` |
| **State tree** | Yes — rate tracking per account |
| **Config** | `pay_hook` must be set |
| **Composes with** | Compliance, Delegation |

### 10.4 Controller Gate

| | |
|---|---|
| **Skill** | Require a specific program's proof to move tokens |
| **Hooks** | `pay_hook` = `PAY_CONTROLLER` |
| **State tree** | No — reads `controller` from leaf |
| **Config** | `leaf.controller` must be set |
| **Composes with** | Lending (program-controlled collateral), Vault |

Verifies a composed proof from the leaf's `controller` program. Enables escrow, protocol treasuries, and program-controlled accounts.

### 10.5 Soulbound (TSP-2)

| | |
|---|---|
| **Skill** | Make assets permanently non-transferable |
| **Hooks** | `pay_hook` = `PAY_SOULBOUND` (always rejects) |
| **State tree** | No |
| **Config** | `pay_hook` set |
| **Composes with** | KYC Gate (credential issuance) |

Also achievable without a hook: mint with `flags = 0` (TRANSFERABLE bit clear). The hook version works for TSP-1 tokens that lack per-leaf flags.

### 10.6 Fee-on-Transfer

| | |
|---|---|
| **Skill** | Deduct a percentage to treasury on every transfer |
| **Hooks** | `pay_hook` = `PAY_FEE` |
| **State tree** | No — composes with TSP-1 pay proof for fee payment |
| **Config** | `pay_hook` set, treasury address in metadata |
| **Composes with** | Compliance, Liquidity |

---

## 🔗 11. Composition Capabilities

### 11.1 Bridging

| | |
|---|---|
| **Skill** | Cross-chain portability via STARK proof relay |
| **Hooks** | `mint_hook` = `BRIDGE_LOCK_PROOF`, `burn_hook` = `BRIDGE_RELEASE_PROOF` |
| **State tree** | No — proofs relay directly |
| **Config** | `mint_auth` = bridge program, `burn_auth` = bridge program |
| **Composes with** | Oracle Pricing (cross-chain price verification) |

Mint on destination chain requires STARK proof of lock on source chain. Burn on destination produces proof for release on source chain. No trusted bridge or multisig.

### 11.2 Subscription / Streaming Payments

| | |
|---|---|
| **Skill** | Recurring authorized payments on a schedule |
| **Hooks** | `pay_hook` = `PAY_DELEGATION` (with rate-limiting) |
| **State tree** | Delegation tree (reuses Delegation capability) |
| **Config** | `pay_hook` set |
| **Composes with** | Delegation (required) |

Service provider registers as delegate with monthly `limit` and `expiry`. Each period, service calls pay using delegation authority. Hook enforces rate limit. User revokes by changing `auth_hash`.

### 11.3 Burn-to-Redeem

| | |
|---|---|
| **Skill** | Burn one asset to claim another |
| **Hooks** | `burn_hook` = `BURN_REDEEM` |
| **State tree** | No — produces receipt proof |
| **Config** | `burn_hook` set |
| **Composes with** | Any mint operation |

The hook produces a receipt proof that composes with a mint operation on another token:

```text
Burn(TSP-2 item) → receipt proof ⊗ Mint(TSP-1 reward token)
```

Use cases: burn uniq to claim physical goods, burn ticket for event access, burn old token for upgraded version, crafting (burn materials → mint result).

### 11.4 Governance

| | |
|---|---|
| **Skill** | Vote with your tokens, propose and execute protocol changes |
| **Hooks** | `update_hook` = `UPDATE_TIMELOCK` + `UPDATE_THRESHOLD` |
| **State tree** | Yes — proposal tree |
| **Config** | `admin_auth` = governance program |
| **Composes with** | Timelock, Multisig, Staking (vote weight = staked balance) |

Uses historical Merkle roots as free balance snapshots. Flow:
1. Create proposal → commit to proposal tree
2. Snapshot current TSP-1 state root at proposal creation
3. Vote → voter proves balance at snapshot root (Merkle inclusion)
4. Tally → aggregation circuit counts votes, verifies quorum
5. Execute → queue behind timelock, then execute config updates

No governance primitive needed. Balance snapshots are free — every historical Merkle root is a snapshot.

### 11.5 Batch Operations

| | |
|---|---|
| **Skill** | Mint or transfer multiple tokens in one proof |
| **Hooks** | `mint_hook` = `MINT_BATCH` |
| **State tree** | No — recursive proof composition |
| **Config** | `mint_hook` set |
| **Composes with** | Supply Cap |

Multiple mints composed into a single recursive STARK proof. Useful for airdrops, collection launches, and batch distributions.

---

## 📋 12. Recipes

Recipes are documented configurations that combine a standard with capabilities to build specific token types. Pick a standard, pick capabilities, deploy.

### 12.1 Simple Coin

```text
Standard: TSP-1    Capabilities: none
Config: admin_auth=hash(admin), mint_auth=hash(minter), all others=0
```

The simplest token. Anyone can transfer and burn. Admin can update config. Authorized minter mints.

### 12.2 Immutable Money

```text
Standard: TSP-1    Capabilities: none
Config: admin_auth=0 (renounced), mint_auth=0 (disabled), all others=0
```

After genesis mint, nothing can change. Pure permissionless sound money. The config hash is verifiably immutable.

### 12.3 Regulated Token

```text
Standard: TSP-1    Capabilities: Compliance, KYC Gate, Multisig
Config: pay_auth=hash(compliance), pay_hook=PAY_WHITELIST,
        mint_hook=MINT_KYC, update_hook=UPDATE_THRESHOLD
```

### 12.4 Art Collection

```text
Standard: TSP-2    Capabilities: Royalties, Supply Cap
Config: pay_hook=PAY_ROYALTY, mint_hook=MINT_CAP+MINT_UNIQUE
Flags per asset: transferable=1, burnable=1, updatable=0
```

### 12.5 Soulbound Credential

```text
Standard: TSP-2    Capabilities: Soulbound
Config: mint_auth=hash(issuer), pay_hook=PAY_SOULBOUND
Flags: transferable=0, burnable=0, updatable=0
```

### 12.6 Game Item Collection

```text
Standard: TSP-2    Capabilities: Royalties, Burn-to-Redeem (crafting)
Config: mint_auth=hash(game_server), pay_hook=GAME_RULES,
        mint_hook=ITEM_GEN, update_hook=ITEM_EVOLUTION
Flags: transferable=1, burnable=1, updatable=1
```

### 12.7 Yield-Bearing Vault

```text
Standard: TSP-1    Capabilities: Vault
Config: mint_auth=hash(vault_program),
        mint_hook=VAULT_DEPOSIT, burn_hook=VAULT_WITHDRAW
```

### 12.8 Governance Token

```text
Standard: TSP-1    Capabilities: Governance, Timelock, Multisig
Config: admin_auth=hash(governance_program),
        update_hook=UPDATE_TIMELOCK+UPDATE_THRESHOLD
```

### 12.9 Stablecoin

```text
Standard: TSP-1    Capabilities: Stablecoin, Oracle Pricing
Config: mint_auth=hash(minting_program),
        mint_hook=STABLECOIN_MINT, burn_hook=STABLECOIN_REDEEM
```

### 12.10 Wrapped / Bridged Asset

```text
Standard: TSP-1    Capabilities: Bridging
Config: mint_auth=hash(bridge), burn_auth=hash(bridge),
        mint_hook=BRIDGE_LOCK_PROOF, burn_hook=BRIDGE_RELEASE_PROOF
```

### 12.11 Liquid Staking Token

```text
Standard: TSP-1    Capabilities: Staking, Vault
Config: mint_auth=hash(staking_program),
        mint_hook=STAKE_DEPOSIT, burn_hook=STAKE_WITHDRAW
```

### 12.12 Subscription Service

```text
Standard: TSP-1    Capabilities: Delegation, Subscription
Config: pay_hook=PAY_DELEGATION
```

### 12.13 Collateralized Fund

```text
Standard: TSP-1 (collateral) + TSP-1 (shares)
Capabilities: Lending, Oracle Pricing, Liquidity

Supply: TOKEN_A pay → fund_account (controller=FUND), Oracle price proof,
        fund state recorded, TOKEN_B minted to supplier
Redeem: TOKEN_B burn, Oracle price proof, TOKEN_A released from fund_account
Liquidation: health_factor < 1 proven, liquidator covers debt, receives collateral
```

### 12.14 Uniq Marketplace

```text
Standard: TSP-2 + TSP-1
Capabilities: Royalties, Oracle Pricing, Liquidity

Seller transfers uniq to buyer:
  TSP-2 Pay (asset transfer) + TSP-1 Pay (payment) + TSP-1 Pay (royalty)
  Composed proof: TSP-2 ⊗ TSP-1(payment) ⊗ TSP-1(royalty) → single verification
```

### 12.15 Prediction Market

```text
Standard: N × TSP-1 (outcome tokens)
Capabilities: Oracle Pricing, Liquidity, Burn-to-Redeem

Create: deploy N tokens (one per outcome), mint requires equal buy-in
Trade: Liquidity strategies for outcome pairs
Resolve: Oracle attests outcome, winning token redeemable 1:1
Redeem: burn winner (burn_hook verifies resolution), receive payout
```

### 12.16 Name Service

```text
Standard: TSP-2    Capabilities: none (just metadata schema)
Register: mint TSP-2 where asset_id=hash(name), metadata_hash=hash(resolution)
Resolve: Merkle inclusion proof for hash(name) in collection tree
Transfer: standard TSP-2 pay
Update: TSP-2 metadata update (if flags.updatable=1)
```

---

## 🧩 13. Proof Composition Architecture

### 13.1 The Composition Stack

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
│       │ Capability  │                    │
│       │   Proof     │                    │
│       └──────┬──────┘                    │
│              │                           │
│  ┌───────────▼───────────┐              │
│  │   Oracle Pricing      │              │
│  │    Capability Proof   │              │
│  └───────────┬───────────┘              │
│              │                           │
│       ┌──────▼──────┐                    │
│       │ Allocation  │                    │
│       │   Proof     │                    │
│       └─────────────┘                    │
└─────────────────────────────────────────┘
```

### 13.2 Composition Rules

1. All sub-proofs independently verifiable
2. Public I/O consistent across sub-proofs (amounts, accounts, timestamps)
3. Merkle roots chain correctly
4. Triton VM recursive verification → entire composition = single STARK proof
5. Single proof relayable cross-chain

---

## 🛡️ 14. What Neptune Fixes

### vs. ERC-20

| Problem | Solution |
|---|---|
| `approve()` race condition | No approvals — `auth_hash` + Delegation capability |
| Unlimited approval risk | No approvals exist |
| No time-locks | `lock_until` first-class |
| No mint/burn access control | Per-operation authorities |
| Tokens trapped in contracts | Tokens in user accounts |
| ERC-777 hooks = reentrancy | Hooks via proof composition |
| Supply not provable | `supply` conservation in circuit |

### vs. ERC-721

| Problem | Solution |
|---|---|
| Royalties not enforceable | `royalty_bps` + Royalties capability |
| No native collections | `collection_id` in leaf |
| Metadata frozen | `flags.updatable` per asset |
| Separate standard | Same PLUMB framework |

### vs. Uniswap

| Problem | Solution |
|---|---|
| Liquidity locked | Stays in maker accounts (Liquidity capability) |
| Fragmentation | Same capital backs multiple strategies |
| Impermanent loss | Oracle-priced strategies via Oracle Pricing capability |
| MEV | Proof-based, no public mempool |

### vs. Chainlink

| Problem | Solution |
|---|---|
| Trust oracle signers | STARK proof of aggregation (Oracle Pricing capability) |
| Opaque computation | Provable derivation chain |
| Chain-specific | Cross-chain via proof relay |

---

## 🏷️ 15. Naming Convention

| Component | Name | Role |
|---|---|---|
| Framework | **PLUMB** | **P**ay, **L**ock, **U**pdate, **M**int, **B**urn |
| Standard | **TSP-1** (Coin) | PLUMB implementation for divisible assets |
| Standard | **TSP-2** (Uniq) | PLUMB implementation for unique assets |
| Capability | **Liquidity** (TIDE) | Tokens In Direct Exchange — swaps without custody |
| Capability | **Oracle Pricing** (COMPASS) | External data attestation with STARK proofs |
| Capability | *[23 total]* | See Capability Library (sections 8-11) |

---

## 🗺️ 16. Implementation Roadmap

### Phase 0 — Genesis
1. PLUMB framework (auth, config, hook composition)
2. TSP-1 circuit
3. Token deployment tooling
4. Core capabilities: Supply Cap, Compliance, Soulbound

### Phase 1 — Ownership
5. TSP-2 circuit
6. Capabilities: Royalties, Delegation, Supply Cap (for collections)
7. Wallet integration (both standards)

### Phase 2 — Financial
8. Liquidity capability (allocation tree, constant product, stable swap)
9. Vault capability
10. Staking capability

### Phase 3 — Oracle
11. Oracle Pricing capability (attestation tree, submit, aggregate, read)
12. Median + TWAP aggregation hooks

### Phase 4 — Composition
13. Oracle-priced Liquidity strategy
14. Lending capability
15. Stablecoin capability
16. Cross-chain proof relay (Bridging)

### Phase 5 — Ecosystem
17. Remaining capabilities (Governance, Timelock, Multisig, Vesting, Batch, Burn-to-Redeem, Subscription)
18. Recipe tooling and deployment templates
19. Reference implementations for all recipes

---

## ❓ 17. Open Questions

1. **Tree depth:** Depth 20 (~1M leaves). Fixed or variable?
2. **Multi-hop swaps:** Atomic A→B→C or sequential?
3. **Privacy:** How far to push shielded transfers?
4. **State rent:** Should leaves expire?
5. **Strategy liveness:** Keeper mechanism for dead strategies?
6. **Capability versioning:** Can a capability be upgraded, or must you deploy a new one?
7. **Capability discovery:** How does a wallet know which capabilities a token has?
8. **Hook chaining:** When multiple capabilities install hooks on the same operation, what is the proof composition order?
9. **Capability dependencies:** Should the system enforce that Lending requires Oracle Pricing, or is that the deployer's responsibility?
10. **Controller recursion:** Can a controller program delegate to another controller?
11. **Fund share pricing:** Should fund shares use Oracle Pricing feeds for their own price, creating a feedback loop?

---

## 📖 Appendix A: Glossary

| Term | Definition |
|---|---|
| **PLUMB** | Pay, Lock, Update, Mint, Burn — the token framework |
| **TSP-1** | Coin standard (PLUMB implementation for divisible assets) |
| **TSP-2** | Uniq standard (PLUMB implementation for unique assets) |
| **Capability** | A composable package of hooks + optional state tree + config that gives a token a new skill |
| **Recipe** | A documented configuration combining a standard + capabilities to build a specific token type |
| **TIDE** | Codename for the Liquidity capability — Tokens In Direct Exchange |
| **COMPASS** | Codename for the Oracle Pricing capability |
| **Circuit** | AIR constraints defining valid state transitions |
| **Config** | Hashed commitment binding authorities and hooks |
| **Hook** | Reusable ZK program composed with token proof |
| **Leaf** | Merkle tree node — account (TSP-1) or asset (TSP-2) |
| **Proof composition** | Verifying multiple proofs with shared public inputs |
| **Strategy** | Pricing program defining an AMM curve (Liquidity capability) |
| **Allocation** | Virtual balance assigned to a strategy |
| **Attestation** | Oracle data point with provenance proof |
| **Feed** | An Oracle Pricing data stream (e.g. BTC/USD price) |
| **Controller** | Program ID that must co-authorize operations on a leaf |
| **State commitment** | Block-level hash of all Merkle tree roots |

## 📋 Appendix B: Capability Quick Reference

### Core

| Capability | Hooks | State Tree | Composes With |
|---|---|---|---|
| Supply Cap | `mint_hook` | No | Everything |
| Delegation | `pay_hook` | Yes (delegation tree) | Subscription, Compliance |
| Vesting | `mint_hook` | Yes (schedule tree) | Supply Cap, Governance |
| Royalties | `pay_hook` | No | Liquidity, Oracle Pricing |
| Multisig | `update_hook` | No (membership token) | Governance, Timelock |
| Timelock | `update_hook` | No | Multisig, Governance |

### Financial

| Capability | Hooks | State Tree | Composes With |
|---|---|---|---|
| Liquidity (TIDE) | `pay_hook` | Yes (allocation tree) | Oracle Pricing, Staking, Governance |
| Oracle Pricing (COMPASS) | — | Yes (attestation tree) | Liquidity, Lending, Stablecoin, Bridging |
| Vault | `mint_hook`, `burn_hook` | No | Oracle Pricing, Lending, Staking |
| Lending | `mint_hook`, `burn_hook` | Yes (position tree) | Oracle Pricing, Liquidity |
| Staking | `lock_hook`, `mint_hook`, `burn_hook` | Optional | Liquidity, Governance |
| Stablecoin | `mint_hook`, `burn_hook` | Yes (collateral tree) | Oracle Pricing, Lending, Liquidity |

### Access Control

| Capability | Hooks | State Tree | Composes With |
|---|---|---|---|
| Compliance | `pay_hook` | Yes (address set) | KYC Gate, Delegation |
| KYC Gate | `mint_hook` | No | Compliance, Soulbound |
| Transfer Limits | `pay_hook` | Yes (rate tracking) | Compliance, Delegation |
| Controller Gate | `pay_hook` | No | Lending, Vault |
| Soulbound | `pay_hook` | No | KYC Gate |
| Fee-on-Transfer | `pay_hook` | No | Compliance, Liquidity |

### Composition

| Capability | Hooks | State Tree | Composes With |
|---|---|---|---|
| Bridging | `mint_hook`, `burn_hook` | No | Oracle Pricing |
| Subscription | `pay_hook` | Yes (delegation tree) | Delegation |
| Burn-to-Redeem | `burn_hook` | No | Any mint operation |
| Governance | `update_hook` | Yes (proposal tree) | Timelock, Multisig, Staking |
| Batch Operations | `mint_hook` | No | Supply Cap |

## 📋 Appendix C: Hook ID Reference

### Pay Hooks
| ID | Capability |
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
| ID | Capability |
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
| ID | Capability |
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
| ID | Capability |
|---|---|
| `LOCK_MAX` | Transfer Limits (max lock duration) |
| `LOCK_REWARDS` | Staking |
| `LOCK_RENTAL` | Composition (TSP-2 rental) |
| `LOCK_PROGRAM` | Controller Gate (program lock) |

### Update Hooks
| ID | Capability |
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
