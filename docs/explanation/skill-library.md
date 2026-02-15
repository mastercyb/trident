# Skill Library

## Composable Token Capabilities for the Gold Standard

Version: 0.1-draft | Date: February 14, 2026

### Status

This document is a design specification -- none of the 23 skills below
are implemented. Implementation follows after the PLUMB foundation
(basic token deploy and interact) is production-tested. Community
contributions welcome.

Skills extend tokens defined by the [Gold Standard](gold-standard.md).
A token without skills is a bare TSP-1 (Coin) or TSP-2 (Card) -- it
can pay, lock, update, mint, and burn. Each skill you add teaches it a
new behavior through the PLUMB hook system.

> See [Skill Reference](../../reference/skills.md) for complete spec
> tables, recipes, hook IDs, and glossary.

---

## 1. What Is a Skill

A skill is a composable package that teaches a token a new behavior.
Every skill has the same anatomy:

| Component | Description |
|-----------|-------------|
| Skill | What the token can now do |
| Hooks | Which PLUMB hooks it installs |
| State tree | Whether it needs its own Merkle tree |
| Config | What authorities/hooks must be set |
| Composes with | Which other skills it works alongside |

### 1.1 Skill Namespace: `std.skill.*`

All 23 official skills ship as importable Trident source under
`std.skill`. Each is a `.tri` module with three usage paths:

- **Import** (`use std.skill.liquidity`): inlined into your circuit
  at compile time.
- **Fork**: copy from `std/skill/`, modify, compile your own version.
- **Deploy**: publish to the OS's
  [Atlas](../../reference/atlas.md);
  other tokens reference by content hash or Atlas name.

Skills build on `std.token`, `std.coin` (TSP-1), and `std.card`
(TSP-2). See the [Standard Library reference](../../reference/stdlib.md#layer-05-token-infrastructure)
for the full module catalog.

## 2. How Skills Compose

Multiple skills can be active on the same token simultaneously.
When multiple skills install hooks on the same operation, their proofs
compose independently:

```text
Pay with Compliance + Fee-on-Transfer + Liquidity:
  1. Token circuit proves valid balance transfer
  2. Compliance hook proves sender/receiver whitelisted
  3. Fee-on-Transfer hook proves treasury received its cut
  4. Liquidity hook proves pricing curve satisfied
  Verifier: Token * Compliance * Fee * Liquidity -> single proof
```

### Hook Composition Ordering

STARK proof composition is **commutative** -- each hook proof is
independently generated and verified. Hooks don't call each other;
they produce separate proofs the verifier checks together. The verifier
confirms every declared hook has a valid proof, public I/O is consistent
across sub-proofs, and all Merkle roots chain correctly.

Contradictory hooks (e.g., Soulbound rejects transfers while Liquidity
allows a swap) cause composition failure -- both proofs cannot be
simultaneously valid. This catches misconfigured tokens at proof time.
When multiple hooks modify different state trees in one transaction,
the block's atomic state commitment ensures all updates apply together
or not at all.

## 3. Skill Tiers

| Tier | Focus | Skills |
|------|-------|--------|
| Core | Skills most tokens want | Supply Cap, Delegation, Vesting, Royalties, Multisig, Timelock |
| Financial | DeFi use cases | Liquidity, Oracle Pricing, Vault, Lending, Staking, Stablecoin |
| Access Control | Compliance and permissions | Compliance, KYC Gate, Transfer Limits, Controller Gate, Soulbound, Fee-on-Transfer |
| Composition | Cross-token interaction | Bridging, Subscription, Burn-to-Redeem, Governance, Batch Operations |

## 4. Choosing Skills for Your Token

Start with the simplest configuration, then add skills as needs emerge.

- **Fixed supply?** Supply Cap.
- **Spending on behalf of others?** Delegation.
- **Team/investor vesting?** Vesting.
- **NFT creator royalties?** Royalties (TSP-2).
- **Multi-party control?** Multisig + Timelock.
- **Swap/trade support?** Liquidity (TIDE).
- **Price feeds?** Oracle Pricing (COMPASS) -- required for lending
  and stablecoins.
- **Regulated token?** Compliance + KYC Gate.
- **Cross-chain?** Bridging.

## 5. Core Skills at a Glance

**Supply Cap** -- Cryptographically enforced ceiling; mint hook
verifies `new_supply <= max_supply`.

**Delegation** -- Bounded, expiring, revocable allowances replacing
ERC-20 `approve`/`allowance`.

**Vesting** -- Time-locked token release with cliff and linear unlock.

**Royalties (TSP-2)** -- Creator royalties enforced at protocol level
on every transfer via composed pay proof.

**Multisig** -- M-of-N approval for config changes using a TSP-1
membership token as the signer set.

**Timelock** -- Mandatory delay on config changes. Commonly paired
with Multisig.

## 6. Financial Skills

### 6.1 Liquidity (TIDE) -- How Swaps Work Without Custody

Traditional AMMs lock tokens in custodial pool contracts. The
Liquidity skill (*Tokens In Direct Exchange*) eliminates custody.
Swaps are two `pay` operations where the `pay_hook` enforces the
pricing curve:

```text
Alice swaps 100 TOKEN_A for TOKEN_B with maker Bob:
  TOKEN_A Pay: Alice -> Bob, amount=100, pay_hook=STRATEGY
  TOKEN_B Pay: Bob -> Alice, amount=f(100), pay_hook=STRATEGY
  Composed proof: Token_A * Token_B * Strategy -> single verification
```

No tokens leave user accounts. No approvals. No router.

#### Protocol Fee

Every swap deducts 0.1% (10 bps) of trade value in NPT -- a global
protocol constant for Sybil-resistant price discovery (see
[Gold Standard](gold-standard.md) section 2.4). Strategy fee (paid
to LPs) is separate. Total trader cost: 0.2-0.4%.

#### Shared Liquidity

Because the AMM is a hook, a maker's balance simultaneously backs AMM
strategies, serves as lending collateral, counts for governance votes,
and earns staking rewards. If two strategies try to move the same
tokens in one block, the second proof fails (Merkle root changed).

#### Virtual Allocations

The allocation tree tracks how much of a maker's balance each strategy
can access. Overcommitment is safe -- every swap proof checks the
current balance. Strategies are pluggable ZK circuits (constant product,
stable swap, concentrated liquidity, oracle-priced), immutable once
registered.

### 6.2 Oracle Pricing (COMPASS) -- Verified, Not Trusted

#### Why Oracle Pricing Needs a State Tree

Hooks *consume* external data but cannot *produce* it. Someone must
commit data, prove its derivation, and make it queryable. The oracle
is to DeFi what `auth_hash` is to tokens -- the external input
everything depends on.

#### The STARK-Unique Property

In Chainlink or Pyth, oracle data comes with a signature -- you trust
the signers. In the Gold Standard, oracle data comes with a STARK
proof of its derivation. The aggregation circuit proves the median was
correctly computed from N submissions. The composed proof covers the
entire chain from raw data to aggregated value.

#### Cross-Chain Oracle

Oracle proofs are STARKs. They can be relayed to other chains and
verified without trusting a bridge or multisig.

### 6.3 Other Financial Skills

**Vault** -- Deposit asset, receive shares at an exchange rate
(ERC-4626 as a skill).

**Lending** -- Use tokens as collateral to borrow. Requires Oracle
Pricing for health factor checks.

**Staking** -- Lock tokens to earn rewards. Combined with Vault for
liquid staking tokens (LSTs).

**Stablecoin** -- Maintain a peg through collateral + oracle pricing.

## 7. Access Control Skills at a Glance

**Compliance** -- Restrict who can send or receive via Merkle
inclusion/non-membership proofs (whitelist or blacklist).

**KYC Gate** -- Require a verified soulbound credential (TSP-2) to
mint or receive.

**Transfer Limits** -- Cap amounts per transaction or per time period.

**Controller Gate** -- Require a specific program's proof to move
tokens. Enables escrow and program-controlled accounts.

**Soulbound (TSP-2)** -- Permanently non-transferable assets.

**Fee-on-Transfer** -- Deduct a percentage to treasury on every
transfer.

## 8. Composition Skills at a Glance

**Bridging** -- Cross-chain portability via STARK proof relay.

**Subscription** -- Recurring authorized payments via Delegation with
rate-limiting.

**Burn-to-Redeem** -- Burn one asset to claim another (physical goods,
crafting, token migration).

**Governance** -- Vote with tokens using historical Merkle roots as
free balance snapshots.

**Batch Operations** -- Mint or transfer multiple tokens in one
recursive STARK proof.

## 9. Proof Composition

```text
+------------------------------------------+
|      Composed Transaction Proof          |
|                                          |
|  +----------+  +----------+             |
|  | Token A   |  | Token B   |             |
|  | Pay Proof |  | Pay Proof |             |
|  +-----+----+  +-----+----+             |
|        +------+-------+                  |
|        +------v------+                   |
|        | Skill Proof |                   |
|        +------+------+                   |
|  +------------v------------+             |
|  |  Oracle Pricing Proof   |             |
|  +------------+------------+             |
|        +------v------+                   |
|        | Allocation  |                   |
|        +-------------+                   |
+------------------------------------------+
```

All sub-proofs are independently verifiable. Public I/O must be
consistent, Merkle roots must chain, and Triton VM recursive
verification collapses the composition into a single relayable STARK.

## 10. Implementation Roadmap

Skills require the [Gold Standard](gold-standard.md) foundation to be
stable. Priority based on dependency order:

**First skills** (unblock the rest):
- Supply Cap -- validates the hook mechanism
- Delegation -- enables subscription and spending limits
- Compliance -- enables regulated tokens

**Financial skills** (require working tokens):
- Liquidity (TIDE) -- enables proven price
- Oracle Pricing (COMPASS) -- enables lending and stablecoins
- Vault, Staking, Lending, Stablecoin

**Composition skills** (require working financial skills):
- Governance, Bridging, Burn-to-Redeem, Batch Operations

The skill library maps the full design space. The architecture allows
anyone to implement a skill as a ZK program that composes through the
hook system.

## 11. Open Questions

1. **Skill versioning.** Upgrade in place, or deploy a new one?
2. **Skill discovery.** How does a wallet know which skills a token has?
3. **Skill dependencies.** Enforce that Lending requires Oracle Pricing?
4. **Multi-hop swaps.** Atomic A->B->C in one proof, or sequential?
5. **Strategy liveness.** Keeper mechanism for dead strategies?

## See Also

- [Skill Reference](../../reference/skills.md) -- Spec tables, recipes, hook IDs, glossary
- [The Gold Standard](gold-standard.md) -- PLUMB framework, TSP-1 (Coin), TSP-2 (Card)
- [Programming Model](programming-model.md) -- Execution model and stack semantics
- [OS Reference](../../reference/os.md) -- OS concepts and `os.token` bindings
- [Multi-Target Compilation](multi-target.md) -- One source, every chain
- [Deploying a Program](../guides/deploying-a-program.md) -- Deployment workflows
