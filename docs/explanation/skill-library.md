# Skill Library

## Composable Token Capabilities for the Gold Standard

Version: 0.1-draft
Date: February 14, 2026

### Status

This document is a design specification — none of the 23 skills below
are implemented. This section specifies the design space — what tokens
should be able to learn. Implementation follows after the PLUMB
foundation (basic token deploy and interact) is production-tested.
Community contributions welcome.

Skills extend tokens defined by the [Gold Standard](gold-standard.md).
A token without skills is a bare TSP-1 (Coin) or TSP-2 (Card) — it can
pay, lock, update, mint, and burn. Each skill you add teaches it a new
behavior through the PLUMB hook system.

---

## 1. What Is a Skill

A skill is a composable package that teaches a token a new behavior. Every skill has the same anatomy:

| Component | Description |
|-----------|-------------|
| Skill | What the token can now do |
| Hooks | Which PLUMB hooks it installs |
| State tree | Whether it needs its own Merkle tree |
| Config | What authorities/hooks must be set |
| Composes with | Which other skills it works alongside |

---

## 2. How Skills Compose

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

### Hook Composition Ordering

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
in the same transaction, the block's atomic state commitment ensures
all tree updates are applied together or not at all.

---

## 3. Skill Tiers

| Tier | Focus | Skills |
|------|-------|-------------|
| Core | Skills most tokens want | Supply Cap, Delegation, Vesting, Royalties, Multisig, Timelock |
| Financial | DeFi use cases | Liquidity, Oracle Pricing, Vault, Lending, Staking, Stablecoin |
| Access Control | Compliance and permissions | Compliance, KYC Gate, Transfer Limits, Controller Gate, Soulbound, Fee-on-Transfer |
| Composition | Cross-token interaction | Bridging, Subscription, Burn-to-Redeem, Governance, Batch Operations |

---

## 4. Core Skills

### 4.1 Supply Cap

| | |
|---|---|
| Skill | Fixed maximum supply — cryptographically enforced ceiling |
| Hooks | `mint_hook` = `MINT_CAP` |
| State tree | No |
| Config | `mint_auth` must be set (minting enabled) |
| Composes with | Everything — most fundamental financial constraint |

The hook verifies: `new_supply <= max_supply` (read from metadata or hardcoded in hook parameters). Without this skill, TSP-1 minting is uncapped. With it, the cap is provably enforced.

### 4.2 Delegation

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

### 4.3 Vesting

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

### 4.4 Royalties (TSP-2)

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

### 4.5 Multisig / Threshold

| | |
|---|---|
| Skill | Require M-of-N approval for config changes |
| Hooks | `update_hook` = `UPDATE_THRESHOLD` |
| State tree | No — uses a TSP-1 membership token as the signer set |
| Config | `update_hook` must be set |
| Composes with | Governance, Timelock |

Deploy a TSP-1 token with `supply = N`, one per signer. On config update, the threshold hook requires M composed pay proofs from token holders. The token IS the membership. The hook IS the threshold logic. Not a separate primitive.

### 4.6 Timelock

| | |
|---|---|
| Skill | Mandatory delay period on config changes |
| Hooks | `update_hook` = `UPDATE_TIMELOCK` |
| State tree | No |
| Config | `update_hook` must be set |
| Composes with | Multisig, Governance |

Config changes are queued and can only execute after the delay period. Prevents surprise rug-pulls. Commonly combined with Multisig: threshold approval + mandatory delay.

---

## 5. Financial Skills

### 5.1 Liquidity (TIDE)

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

Every swap deducts 0.1% (10 basis points) of trade value in NPT. This is a global protocol constant, not configurable per token or per strategy. The fee serves as the foundation for Sybil-resistant price discovery (see [Gold Standard](gold-standard.md) section 2.4).

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

### 5.2 Oracle Pricing (COMPASS)

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

### 5.3 Vault / Yield-Bearing

| | |
|---|---|
| Skill | Deposit asset, receive shares at exchange rate (ERC-4626 as a skill) |
| Hooks | `mint_hook` = `VAULT_DEPOSIT`, `burn_hook` = `VAULT_WITHDRAW` |
| State tree | No — exchange rate derived from `total_assets / total_shares` |
| Config | `mint_auth` = vault program |
| Composes with | Oracle Pricing, Lending, Staking |

On deposit: mint shares proportional to deposited assets. On withdrawal: burn shares, release proportional assets. Inflation attack defense built into the hook (initial offset at deployment).

### 5.4 Lending / Collateral

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

### 5.5 Staking

| | |
|---|---|
| Skill | Lock tokens to earn rewards |
| Hooks | `lock_hook` = `LOCK_REWARDS`, `mint_hook` = `STAKE_DEPOSIT`, `burn_hook` = `STAKE_WITHDRAW` |
| State tree | Optional — reward distribution state |
| Config | `lock_auth` may be set for mandatory staking |
| Composes with | Liquidity (staked tokens back strategies), Governance |

Combined with Vault skill for a liquid staking token (LST): deposit native token → receive LST that appreciates as staking rewards accrue.

### 5.6 Stablecoin

| | |
|---|---|
| Skill | Maintain a peg through collateral + oracle pricing |
| Hooks | `mint_hook` = `STABLECOIN_MINT`, `burn_hook` = `STABLECOIN_REDEEM` |
| State tree | Yes — collateral position tree |
| Config | `mint_auth` = minting program |
| Composes with | Oracle Pricing (mandatory), Lending, Liquidity |

Mint hook composes with: Oracle Pricing proof (collateral price), TSP-1 lock proof (collateral locked), collateral ratio check (e.g. 150% minimum). Burn hook releases collateral proportional to burn amount.

---

## 6. Access Control Skills

### 6.1 Compliance (Whitelist / Blacklist)

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

### 6.2 KYC Gate

| | |
|---|---|
| Skill | Require verified identity credential to mint or receive |
| Hooks | `mint_hook` = `MINT_KYC` |
| State tree | No — composes with a TSP-2 soulbound credential proof |
| Config | `mint_auth` must be set |
| Composes with | Compliance, Soulbound |

The hook requires a composed proof that the recipient holds a valid soulbound credential (TSP-2 with `flags = 0`).

### 6.3 Transfer Limits

| | |
|---|---|
| Skill | Cap transfer amounts per transaction or per time period |
| Hooks | `pay_hook` = `PAY_LIMIT` |
| State tree | Yes — rate tracking per account |
| Config | `pay_hook` must be set |
| Composes with | Compliance, Delegation |

### 6.4 Controller Gate

| | |
|---|---|
| Skill | Require a specific program's proof to move tokens |
| Hooks | `pay_hook` = `PAY_CONTROLLER` |
| State tree | No — reads `controller` from leaf |
| Config | `leaf.controller` must be set |
| Composes with | Lending (program-controlled collateral), Vault |

Verifies a composed proof from the leaf's `controller` program. Enables escrow, protocol treasuries, and program-controlled accounts.

### 6.5 Soulbound (TSP-2)

| | |
|---|---|
| Skill | Make assets permanently non-transferable |
| Hooks | `pay_hook` = `PAY_SOULBOUND` (always rejects) |
| State tree | No |
| Config | `pay_hook` set |
| Composes with | KYC Gate (credential issuance) |

Also achievable without a hook: mint with `flags = 0` (TRANSFERABLE bit clear). The hook version works for TSP-1 tokens that lack per-leaf flags.

### 6.6 Fee-on-Transfer

| | |
|---|---|
| Skill | Deduct a percentage to treasury on every transfer |
| Hooks | `pay_hook` = `PAY_FEE` |
| State tree | No — composes with TSP-1 pay proof for fee payment |
| Config | `pay_hook` set, treasury address in metadata |
| Composes with | Compliance, Liquidity |

---

## 7. Composition Skills

### 7.1 Bridging

| | |
|---|---|
| Skill | Cross-chain portability via STARK proof relay |
| Hooks | `mint_hook` = `BRIDGE_LOCK_PROOF`, `burn_hook` = `BRIDGE_RELEASE_PROOF` |
| State tree | No — proofs relay directly |
| Config | `mint_auth` = bridge program, `burn_auth` = bridge program |
| Composes with | Oracle Pricing (cross-chain price verification) |

Mint on destination chain requires STARK proof of lock on source chain. Burn on destination produces proof for release on source chain. No trusted bridge or multisig.

### 7.2 Subscription / Streaming Payments

| | |
|---|---|
| Skill | Recurring authorized payments on a schedule |
| Hooks | `pay_hook` = `PAY_DELEGATION` (with rate-limiting) |
| State tree | Delegation tree (reuses Delegation skill) |
| Config | `pay_hook` set |
| Composes with | Delegation (required) |

Service provider registers as delegate with monthly `limit` and `expiry`. Each period, service calls pay using delegation authority. Hook enforces rate limit. User revokes by changing `auth_hash`.

### 7.3 Burn-to-Redeem

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

### 7.4 Governance

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

### 7.5 Batch Operations

| | |
|---|---|
| Skill | Mint or transfer multiple tokens in one proof |
| Hooks | `mint_hook` = `MINT_BATCH` |
| State tree | No — recursive proof composition |
| Config | `mint_hook` set |
| Composes with | Supply Cap |

Multiple mints composed into a single recursive STARK proof. Useful for airdrops, collection launches, and batch distributions.

---

## 8. Recipes

Recipes are documented configurations that combine a standard with skills to build specific token types. Pick a standard, pick skills, deploy.

### 8.1 Simple Coin

```text
Standard: TSP-1    Skills: none
Config: admin_auth=hash(admin), mint_auth=hash(minter), all others=0
```

The simplest token. Anyone can transfer and burn. Admin can update config. Authorized minter mints.

### 8.2 Immutable Money

```text
Standard: TSP-1    Skills: none
Config: admin_auth=0 (renounced), mint_auth=0 (disabled), all others=0
```

After genesis mint, nothing can change. Pure permissionless sound money. The config hash is verifiably immutable.

### 8.3 Regulated Token

```text
Standard: TSP-1    Skills: Compliance, KYC Gate, Multisig
Config: pay_auth=hash(compliance), pay_hook=PAY_WHITELIST,
        mint_hook=MINT_KYC, update_hook=UPDATE_THRESHOLD
```

### 8.4 Art Collection

```text
Standard: TSP-2    Skills: Royalties, Supply Cap
Config: pay_hook=PAY_ROYALTY, mint_hook=MINT_CAP+MINT_UNIQUE
Flags per asset: transferable=1, burnable=1, updatable=0
```

### 8.5 Soulbound Credential

```text
Standard: TSP-2    Skills: Soulbound
Config: mint_auth=hash(issuer), pay_hook=PAY_SOULBOUND
Flags: transferable=0, burnable=0, updatable=0
```

### 8.6 Game Item Collection

```text
Standard: TSP-2    Skills: Royalties, Burn-to-Redeem (crafting)
Config: mint_auth=hash(game_server), pay_hook=GAME_RULES,
        mint_hook=ITEM_GEN, update_hook=ITEM_EVOLUTION
Flags: transferable=1, burnable=1, updatable=1
```

### 8.7 Yield-Bearing Vault

```text
Standard: TSP-1    Skills: Vault
Config: mint_auth=hash(vault_program),
        mint_hook=VAULT_DEPOSIT, burn_hook=VAULT_WITHDRAW
```

### 8.8 Governance Token

```text
Standard: TSP-1    Skills: Governance, Timelock, Multisig
Config: admin_auth=hash(governance_program),
        update_hook=UPDATE_TIMELOCK+UPDATE_THRESHOLD
```

### 8.9 Stablecoin

```text
Standard: TSP-1    Skills: Stablecoin, Oracle Pricing
Config: mint_auth=hash(minting_program),
        mint_hook=STABLECOIN_MINT, burn_hook=STABLECOIN_REDEEM
```

### 8.10 Wrapped / Bridged Asset

```text
Standard: TSP-1    Skills: Bridging
Config: mint_auth=hash(bridge), burn_auth=hash(bridge),
        mint_hook=BRIDGE_LOCK_PROOF, burn_hook=BRIDGE_RELEASE_PROOF
```

### 8.11 Liquid Staking Token

```text
Standard: TSP-1    Skills: Staking, Vault
Config: mint_auth=hash(staking_program),
        mint_hook=STAKE_DEPOSIT, burn_hook=STAKE_WITHDRAW
```

### 8.12 Subscription Service

```text
Standard: TSP-1    Skills: Delegation, Subscription
Config: pay_hook=PAY_DELEGATION
```

### 8.13 Collateralized Fund

```text
Standard: TSP-1 (collateral) + TSP-1 (shares)
Skills: Lending, Oracle Pricing, Liquidity

Supply: TOKEN_A pay → fund_account (controller=FUND), Oracle price proof,
        fund state recorded, TOKEN_B minted to supplier
Redeem: TOKEN_B burn, Oracle price proof, TOKEN_A released from fund_account
Liquidation: health_factor < 1 proven, liquidator covers debt, receives collateral
```

### 8.14 Card Marketplace

```text
Standard: TSP-2 + TSP-1
Skills: Royalties, Oracle Pricing, Liquidity

Seller transfers card to buyer:
  TSP-2 Pay (asset transfer) + TSP-1 Pay (payment) + TSP-1 Pay (royalty)
  Composed proof: TSP-2 ⊗ TSP-1(payment) ⊗ TSP-1(royalty) → single verification
```

### 8.15 Prediction Market

```text
Standard: N × TSP-1 (outcome tokens)
Skills: Oracle Pricing, Liquidity, Burn-to-Redeem

Create: deploy N tokens (one per outcome), mint requires equal buy-in
Trade: Liquidity strategies for outcome pairs
Resolve: Oracle attests outcome, winning token redeemable 1:1
Redeem: burn winner (burn_hook verifies resolution), receive payout
```

### 8.16 Name Service

```text
Standard: TSP-2    Skills: none (just metadata schema)
Register: mint TSP-2 where asset_id=hash(name), metadata_hash=hash(resolution)
Resolve: Merkle inclusion proof for hash(name) in collection tree
Transfer: standard TSP-2 pay
Update: TSP-2 metadata update (if flags.updatable=1)
```

---

## 9. Proof Composition Architecture

### 9.1 The Composition Stack

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

### 9.2 Composition Rules

1. All sub-proofs independently verifiable
2. Public I/O consistent across sub-proofs (amounts, accounts, timestamps)
3. Merkle roots chain correctly
4. Triton VM recursive verification → entire composition = single STARK proof
5. Single proof relayable cross-chain

---

## 10. Naming Convention

| Component | Name | Role |
|---|---|---|
| Skill | Liquidity (TIDE) | Tokens In Direct Exchange — swaps without custody |
| Skill | Oracle Pricing (COMPASS) | External data attestation with STARK proofs |
| Skill | *[23 total]* | See sections 4-7 |

---

## 11. Implementation Roadmap

Skills require the [Gold Standard](gold-standard.md) foundation to be
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

## 12. Open Questions

1. **Skill versioning.** Can a skill be upgraded, or must you deploy a new one?
2. **Skill discovery.** How does a wallet know which skills a token has?
3. **Skill dependencies.** Should the system enforce that Lending requires Oracle Pricing, or is that the deployer's responsibility?
4. **Multi-hop swaps.** Atomic A->B->C in one composed proof, or sequential?
5. **Strategy liveness.** Keeper mechanism for dead Liquidity strategies?

---

## Appendix A: Skill Quick Reference

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

## Appendix B: Hook ID Reference

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

---

## Glossary

| Term | Definition |
|---|---|
| Skill | A composable package of hooks + optional state tree + config that teaches a token a new behavior |
| Recipe | A documented configuration combining a standard + skills to build a specific token type |
| TIDE | Codename for the Liquidity skill — Tokens In Direct Exchange |
| COMPASS | Codename for the Oracle Pricing skill |
| Hook | Reusable ZK program composed with token proof |
| Strategy | Pricing program defining an AMM curve (Liquidity skill) |
| Allocation | Virtual balance assigned to a strategy |
| Attestation | Oracle data point with provenance proof |
| Feed | An Oracle Pricing data stream (e.g. BTC/USD price) |

---

## See Also

- [The Gold Standard](gold-standard.md) — PLUMB framework, TSP-1 (Coin), TSP-2 (Card)
- [Programming Model](programming-model.md) — Execution model and stack semantics
- [OS Reference](../../reference/os.md) — OS concepts and `os.token` bindings
- [Multi-Target Compilation](multi-target.md) — One source, every chain
- [Deploying a Program](../guides/deploying-a-program.md) — Deployment workflows
