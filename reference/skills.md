# Skill Reference

[<- Standard Library](stdlib.md) | [Token Standards: TSP-1](tsp1-coin.md) | [TSP-2](tsp2-card.md)

Skills extend tokens defined by the [Gold Standard](../docs/explanation/gold-standard.md).
See the [Skill Library explanation](../docs/explanation/skill-library.md) for
design philosophy, composition model, and deep dives.

---

## Skill Anatomy

| Component | Description |
|-----------|-------------|
| Skill | What the token can now do |
| Hooks | Which PLUMB hooks it installs |
| State tree | Whether it needs its own Merkle tree |
| Config | What authorities/hooks must be set |
| Composes with | Which other skills it works alongside |

## Skill Tiers

| Tier | Focus | Skills |
|------|-------|-------------|
| Core | Skills most tokens want | Supply Cap, Delegation, Vesting, Royalties, Multisig, Timelock |
| Financial | DeFi use cases | Liquidity, Oracle Pricing, Vault, Lending, Staking, Stablecoin |
| Access Control | Compliance and permissions | Compliance, KYC Gate, Transfer Limits, Controller Gate, Soulbound, Fee-on-Transfer |
| Composition | Cross-token interaction | Bridging, Subscription, Burn-to-Redeem, Governance, Batch Operations |

---

## Core Skills

### Supply Cap

| | |
|---|---|
| Skill | Fixed maximum supply -- cryptographically enforced ceiling |
| Hooks | `mint_hook` = `MINT_CAP` |
| State tree | No |
| Config | `mint_auth` must be set (minting enabled) |
| Composes with | Everything -- most fundamental financial constraint |

### Delegation

| | |
|---|---|
| Skill | Let others spend on your behalf with limits and expiry |
| Hooks | `pay_hook` = `PAY_DELEGATION` |
| State tree | Yes -- delegation tree |
| Config | `pay_hook` must be set |
| Composes with | Subscription, Compliance |

Delegation leaf:
```trident
delegation = hash(owner, delegate, token, limit, spent, expiry, 0, 0, 0, 0)
```

### Vesting

| | |
|---|---|
| Skill | Time-locked token release on a schedule |
| Hooks | `mint_hook` = `MINT_VESTING` |
| State tree | Yes -- vesting schedule tree |
| Config | `mint_auth` = vesting program |
| Composes with | Supply Cap, Governance |

Vesting schedule leaf:
```trident
schedule = hash(beneficiary, total_amount, start_time, cliff, duration, claimed, 0, 0, 0, 0)
```

### Royalties (TSP-2)

| | |
|---|---|
| Skill | Enforce creator royalties on every transfer -- not optional, not bypassable |
| Hooks | `pay_hook` = `PAY_ROYALTY` |
| State tree | No -- reads `royalty_bps` from leaf, `royalty_receiver` from metadata |
| Config | `pay_hook` must be set |
| Composes with | Liquidity (marketplace), Oracle Pricing (floor price) |

### Multisig / Threshold

| | |
|---|---|
| Skill | Require M-of-N approval for config changes |
| Hooks | `update_hook` = `UPDATE_THRESHOLD` |
| State tree | No -- uses a TSP-1 membership token as the signer set |
| Config | `update_hook` must be set |
| Composes with | Governance, Timelock |

### Timelock

| | |
|---|---|
| Skill | Mandatory delay period on config changes |
| Hooks | `update_hook` = `UPDATE_TIMELOCK` |
| State tree | No |
| Config | `update_hook` must be set |
| Composes with | Multisig, Governance |

---

## Financial Skills

### Liquidity (TIDE)

*Tokens In Direct Exchange*

| | |
|---|---|
| Skill | Earn on providing liquidity -- tokens stay in your account |
| Hooks | `pay_hook` = `PAY_STRATEGY` (the pricing curve) |
| State tree | Yes -- allocation tree |
| Config | `pay_hook` must reference a strategy program |
| Composes with | Oracle Pricing, Staking, Governance |

Strategy registration:
```trident
strategy = hash(maker, token_a, token_b, program, parameters)
```

Strategy programs:

| Strategy | Description | Key property |
|---|---|---|
| Constant Product | x*y = k | Simple, proven, universal |
| Stable Swap | Curve-style invariant | Optimized for pegged pairs |
| Concentrated Liquidity | Positions in price ranges | Capital-efficient, active management |
| Oracle-Priced | Anchored to Oracle Pricing feed | Eliminates impermanent loss |

### Oracle Pricing (COMPASS)

*External data attestation with STARK proofs*

| | |
|---|---|
| Skill | Price feeds with STARK-proven aggregation -- verified, not trusted |
| Hooks | Consumed by other skills (mint_hook, pay_hook compose with oracle proofs) |
| State tree | Yes -- attestation tree |
| Config | Feed config (submit_auth, aggregate_auth, hooks) |
| Composes with | Liquidity, Lending, Stablecoin, Bridging |

Attestation leaf -- 10 field elements:
```trident
leaf = hash(feed_id, value, timestamp, provider_id, nonce,
            confidence, source_hash, proof_hash, 0, 0)
```

Feed config -- 10 field elements:
```trident
config = hash(admin_auth, submit_auth, aggregate_auth, 0, 0,
              submit_hook, aggregate_hook, read_hook, 0, 0)
```

Feed metadata -- 10 field elements:
```trident
metadata = hash(name_hash, pair_hash, decimals, heartbeat, deviation_threshold,
                min_providers, max_staleness, 0, 0, 0)
```

Operations:

- **Submit**: Provider submits a new attestation. Constraints: provider authorization, `timestamp <= current_time`, newer than previous, `nonce == old_nonce + 1`. The `submit_hook` can enforce staking requirements, reputation scores, deviation bounds.
- **Aggregate**: Combine multiple attestations into a canonical value. Constraints: N leaves from tree, `N >= min_providers`, all within `max_staleness`. The `aggregate_hook` determines the function: median, TWAP, weighted average, outlier-filtered.
- **Read**: Produce a STARK proof that feed F has value V at time T. Not an on-chain operation -- a proof that any skill can compose with.

### Vault / Yield-Bearing

| | |
|---|---|
| Skill | Deposit asset, receive shares at exchange rate (ERC-4626 as a skill) |
| Hooks | `mint_hook` = `VAULT_DEPOSIT`, `burn_hook` = `VAULT_WITHDRAW` |
| State tree | No -- exchange rate derived from `total_assets / total_shares` |
| Config | `mint_auth` = vault program |
| Composes with | Oracle Pricing, Lending, Staking |

### Lending / Collateral

| | |
|---|---|
| Skill | Use tokens as collateral to borrow against |
| Hooks | `mint_hook` = `FUND_MINT`, `burn_hook` = `FUND_REDEEM` + `BURN_LIQUIDATE` |
| State tree | Yes -- position tree (user, collateral, debt, health_factor) |
| Config | `mint_auth` = lending program |
| Composes with | Oracle Pricing (mandatory), Liquidity (liquidation swaps) |

### Staking

| | |
|---|---|
| Skill | Lock tokens to earn rewards |
| Hooks | `lock_hook` = `LOCK_REWARDS`, `mint_hook` = `STAKE_DEPOSIT`, `burn_hook` = `STAKE_WITHDRAW` |
| State tree | Optional -- reward distribution state |
| Config | `lock_auth` may be set for mandatory staking |
| Composes with | Liquidity (staked tokens back strategies), Governance |

### Stablecoin

| | |
|---|---|
| Skill | Maintain a peg through collateral + oracle pricing |
| Hooks | `mint_hook` = `STABLECOIN_MINT`, `burn_hook` = `STABLECOIN_REDEEM` |
| State tree | Yes -- collateral position tree |
| Config | `mint_auth` = minting program |
| Composes with | Oracle Pricing (mandatory), Lending, Liquidity |

---

## Access Control Skills

### Compliance (Whitelist / Blacklist)

| | |
|---|---|
| Skill | Restrict who can send/receive tokens |
| Hooks | `pay_hook` = `PAY_WHITELIST` or `PAY_BLACKLIST` |
| State tree | Yes -- approved/blocked address Merkle set |
| Config | `pay_auth` may enforce dual auth |
| Composes with | KYC Gate, Delegation |

### KYC Gate

| | |
|---|---|
| Skill | Require verified identity credential to mint or receive |
| Hooks | `mint_hook` = `MINT_KYC` |
| State tree | No -- composes with a TSP-2 soulbound credential proof |
| Config | `mint_auth` must be set |
| Composes with | Compliance, Soulbound |

### Transfer Limits

| | |
|---|---|
| Skill | Cap transfer amounts per transaction or per time period |
| Hooks | `pay_hook` = `PAY_LIMIT` |
| State tree | Yes -- rate tracking per account |
| Config | `pay_hook` must be set |
| Composes with | Compliance, Delegation |

### Controller Gate

| | |
|---|---|
| Skill | Require a specific program's proof to move tokens |
| Hooks | `pay_hook` = `PAY_CONTROLLER` |
| State tree | No -- reads `controller` from leaf |
| Config | `leaf.controller` must be set |
| Composes with | Lending (program-controlled collateral), Vault |

### Soulbound (TSP-2)

| | |
|---|---|
| Skill | Make assets permanently non-transferable |
| Hooks | `pay_hook` = `PAY_SOULBOUND` (always rejects) |
| State tree | No |
| Config | `pay_hook` set |
| Composes with | KYC Gate (credential issuance) |

### Fee-on-Transfer

| | |
|---|---|
| Skill | Deduct a percentage to treasury on every transfer |
| Hooks | `pay_hook` = `PAY_FEE` |
| State tree | No -- composes with TSP-1 pay proof for fee payment |
| Config | `pay_hook` set, treasury address in metadata |
| Composes with | Compliance, Liquidity |

---

## Composition Skills

### Bridging

| | |
|---|---|
| Skill | Cross-chain portability via STARK proof relay |
| Hooks | `mint_hook` = `BRIDGE_LOCK_PROOF`, `burn_hook` = `BRIDGE_RELEASE_PROOF` |
| State tree | No -- proofs relay directly |
| Config | `mint_auth` = bridge program, `burn_auth` = bridge program |
| Composes with | Oracle Pricing (cross-chain price verification) |

### Subscription / Streaming Payments

| | |
|---|---|
| Skill | Recurring authorized payments on a schedule |
| Hooks | `pay_hook` = `PAY_DELEGATION` (with rate-limiting) |
| State tree | Delegation tree (reuses Delegation skill) |
| Config | `pay_hook` set |
| Composes with | Delegation (required) |

### Burn-to-Redeem

| | |
|---|---|
| Skill | Burn one asset to claim another |
| Hooks | `burn_hook` = `BURN_REDEEM` |
| State tree | No -- produces receipt proof |
| Config | `burn_hook` set |
| Composes with | Any mint operation |

### Governance

| | |
|---|---|
| Skill | Vote with your tokens, propose and execute protocol changes |
| Hooks | `update_hook` = `UPDATE_TIMELOCK` + `UPDATE_THRESHOLD` |
| State tree | Yes -- proposal tree |
| Config | `admin_auth` = governance program |
| Composes with | Timelock, Multisig, Staking (vote weight = staked balance) |

### Batch Operations

| | |
|---|---|
| Skill | Mint or transfer multiple tokens in one proof |
| Hooks | `mint_hook` = `MINT_BATCH` |
| State tree | No -- recursive proof composition |
| Config | `mint_hook` set |
| Composes with | Supply Cap |

---

## Recipes

### Simple Coin

```text
Standard: TSP-1    Skills: none
Config: admin_auth=hash(admin), mint_auth=hash(minter), all others=0
```

### Immutable Money

```text
Standard: TSP-1    Skills: none
Config: admin_auth=0 (renounced), mint_auth=0 (disabled), all others=0
```

### Regulated Token

```text
Standard: TSP-1    Skills: Compliance, KYC Gate, Multisig
Config: pay_auth=hash(compliance), pay_hook=PAY_WHITELIST,
        mint_hook=MINT_KYC, update_hook=UPDATE_THRESHOLD
```

### Art Collection

```text
Standard: TSP-2    Skills: Royalties, Supply Cap
Config: pay_hook=PAY_ROYALTY, mint_hook=MINT_CAP+MINT_UNIQUE
Flags per asset: transferable=1, burnable=1, updatable=0
```

### Soulbound Credential

```text
Standard: TSP-2    Skills: Soulbound
Config: mint_auth=hash(issuer), pay_hook=PAY_SOULBOUND
Flags: transferable=0, burnable=0, updatable=0
```

### Game Item Collection

```text
Standard: TSP-2    Skills: Royalties, Burn-to-Redeem (crafting)
Config: mint_auth=hash(game_server), pay_hook=GAME_RULES,
        mint_hook=ITEM_GEN, update_hook=ITEM_EVOLUTION
Flags: transferable=1, burnable=1, updatable=1
```

### Yield-Bearing Vault

```text
Standard: TSP-1    Skills: Vault
Config: mint_auth=hash(vault_program),
        mint_hook=VAULT_DEPOSIT, burn_hook=VAULT_WITHDRAW
```

### Governance Token

```text
Standard: TSP-1    Skills: Governance, Timelock, Multisig
Config: admin_auth=hash(governance_program),
        update_hook=UPDATE_TIMELOCK+UPDATE_THRESHOLD
```

### Stablecoin

```text
Standard: TSP-1    Skills: Stablecoin, Oracle Pricing
Config: mint_auth=hash(minting_program),
        mint_hook=STABLECOIN_MINT, burn_hook=STABLECOIN_REDEEM
```

### Wrapped / Bridged Asset

```text
Standard: TSP-1    Skills: Bridging
Config: mint_auth=hash(bridge), burn_auth=hash(bridge),
        mint_hook=BRIDGE_LOCK_PROOF, burn_hook=BRIDGE_RELEASE_PROOF
```

### Liquid Staking Token

```text
Standard: TSP-1    Skills: Staking, Vault
Config: mint_auth=hash(staking_program),
        mint_hook=STAKE_DEPOSIT, burn_hook=STAKE_WITHDRAW
```

### Subscription Service

```text
Standard: TSP-1    Skills: Delegation, Subscription
Config: pay_hook=PAY_DELEGATION
```

### Collateralized Fund

```text
Standard: TSP-1 (collateral) + TSP-1 (shares)
Skills: Lending, Oracle Pricing, Liquidity

Supply: TOKEN_A pay -> fund_account (controller=FUND), Oracle price proof,
        fund state recorded, TOKEN_B minted to supplier
Redeem: TOKEN_B burn, Oracle price proof, TOKEN_A released from fund_account
Liquidation: health_factor < 1 proven, liquidator covers debt, receives collateral
```

### Card Marketplace

```text
Standard: TSP-2 + TSP-1
Skills: Royalties, Oracle Pricing, Liquidity

Seller transfers card to buyer:
  TSP-2 Pay (asset transfer) + TSP-1 Pay (payment) + TSP-1 Pay (royalty)
  Composed proof: TSP-2 + TSP-1(payment) + TSP-1(royalty) -> single verification
```

### Prediction Market

```text
Standard: N x TSP-1 (outcome tokens)
Skills: Oracle Pricing, Liquidity, Burn-to-Redeem

Create: deploy N tokens (one per outcome), mint requires equal buy-in
Trade: Liquidity strategies for outcome pairs
Resolve: Oracle attests outcome, winning token redeemable 1:1
Redeem: burn winner (burn_hook verifies resolution), receive payout
```

### Name Service

```text
Standard: TSP-2    Skills: none (just metadata schema)
Register: mint TSP-2 where asset_id=hash(name), metadata_hash=hash(resolution)
Resolve: Merkle inclusion proof for hash(name) in collection tree
Transfer: standard TSP-2 pay
Update: TSP-2 metadata update (if flags.updatable=1)
```

---

## Proof Composition

### Composition Stack

```text
+-------------------------------------------+
|           Composed Transaction Proof       |
|                                            |
|  +----------+  +----------+               |
|  | Token A   |  | Token B   |              |
|  | Pay Proof |  | Pay Proof |              |
|  +-----+----+  +-----+----+               |
|        |              |                    |
|        +------+-------+                    |
|               |                            |
|        +------v------+                     |
|        | Skill       |                     |
|        |   Proof     |                     |
|        +------+------+                     |
|               |                            |
|  +------------v------------+               |
|  |   Oracle Pricing        |               |
|  |    Skill Proof          |               |
|  +------------+------------+               |
|               |                            |
|        +------v------+                     |
|        | Allocation  |                     |
|        |   Proof     |                     |
|        +-------------+                     |
+-------------------------------------------+
```

### Composition Rules

1. All sub-proofs independently verifiable
2. Public I/O consistent across sub-proofs (amounts, accounts, timestamps)
3. Merkle roots chain correctly
4. Triton VM recursive verification -- entire composition = single STARK proof
5. Single proof relayable cross-chain

---

## Naming Convention

| Component | Name | Role |
|---|---|---|
| Skill | Liquidity (TIDE) | Tokens In Direct Exchange -- swaps without custody |
| Skill | Oracle Pricing (COMPASS) | External data attestation with STARK proofs |
| Skill | *[23 total]* | See Core, Financial, Access Control, Composition sections |

---

## Quick Reference

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
| Oracle Pricing (COMPASS) | -- | Yes (attestation tree) | Liquidity, Lending, Stablecoin, Bridging |
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

## Hook ID Reference

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
| TIDE | Codename for the Liquidity skill -- Tokens In Direct Exchange |
| COMPASS | Codename for the Oracle Pricing skill |
| Hook | Reusable ZK program composed with token proof |
| Strategy | Pricing program defining an AMM curve (Liquidity skill) |
| Allocation | Virtual balance assigned to a strategy |
| Attestation | Oracle data point with provenance proof |
| Feed | An Oracle Pricing data stream (e.g. BTC/USD price) |

---

## See Also

- [Skill Library -- Design & Philosophy](../docs/explanation/skill-library.md) -- Why skills exist, how they compose, deep dives on TIDE and COMPASS
- [The Gold Standard](../docs/explanation/gold-standard.md) -- PLUMB framework, TSP-1 (Coin), TSP-2 (Card)
- [Standard Library](stdlib.md) -- `std.skill.*` module catalog
- [OS Reference](os.md) -- `os.token` bindings and per-OS registry
- [TSP-1 Coin](tsp1-coin.md) -- Coin leaf format, config, operations
- [TSP-2 Card](tsp2-card.md) -- Card leaf format, config, operations
