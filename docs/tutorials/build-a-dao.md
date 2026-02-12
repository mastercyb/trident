# Chapter 6: Upgrade to a DAO

*The Builder's Journey -- Chapter 6 of 6*

Chapter 1: you proved you know a secret.
Chapter 6: you prove you hold coins, cast a vote, and govern a protocol.

Same primitive. Same three lines. `divine()`, `hash()`, `assert()`.

The secret in Chapter 1 was a password. The secret here is your coin
balance, your identity, and your vote -- all hidden, all proven.

---

## The Governance Problem

Every DAO on a transparent chain has the same flaw: everyone sees
everything. Who voted. How much weight they carry. Which direction
they chose.

This enables three attacks that undermine governance:

**Vote buying.** If your vote is public, someone can pay you to vote a
certain way and verify you did. Private voting makes the receipt
impossible -- the buyer cannot confirm delivery.

**Social coercion.** Peers, employers, protocol teams -- anyone with
social leverage can watch your vote and retaliate. When votes are hidden,
coercion has no target.

**Whale tracking.** Large holders are visible on transparent chains.
Their votes move markets and invite front-running. Private voting hides
the weight, not just the direction.

Private voting fixes all three. The vote is proven valid -- the voter
holds real coins, the weight is correct, the vote is counted exactly
once -- without revealing anything about who cast it.

---

## What We Are Governing

The name service from Chapter 3. Specifically: coin holders from Chapter 2
vote on whether to change a name's resolver. A proposal says "change the
resolver of name X from `old_key` to `new_key`." If the vote passes, the
name's metadata hash is updated.

The coin from Chapter 2 becomes a governance token. No new token needed.
If you hold PLUMB coins, you can vote. Your weight equals your balance.

---

## The Vote Program

Create a file called `vote.tri`:

```
program vote

fn main() {
    // --- Public inputs: what we are voting on ---
    let proposal_hash: Digest = pub_read5()
    let coin_root: Digest = pub_read5()
    let nullifier_root: Digest = pub_read5()

    // --- Secret inputs: the voter's coin account ---
    let voter_id: Field = divine()
    let voter_bal: Field = divine()
    let voter_nonce: Field = divine()
    let voter_auth: Field = divine()
    let voter_lock: Field = divine()

    // Reconstruct the voter's leaf in the coin tree
    let leaf: Digest = hash(
        voter_id, voter_bal, voter_nonce,
        voter_auth, voter_lock, 0, 0, 0, 0, 0
    )

    // Verify the leaf exists in the coin Merkle tree
    let depth: U32 = 32
    let leaf_index: U32 = as_u32(divine())
    let mut idx: U32 = leaf_index
    let mut current: Digest = leaf
    for _ in 0..depth bounded 64 {
        (idx, current) = merkle_step(idx, current)
    }
    assert_digest(current, coin_root)

    // Prove voter identity -- same pattern as Chapter 1
    let auth_secret: Field = divine()
    let auth_hash: Digest = hash(
        auth_secret, 0, 0, 0, 0, 0, 0, 0, 0, 0
    )
    assert_eq(auth_hash[0], voter_auth)

    // The vote: 1 = yes, 0 = no (secret)
    let vote_dir: Field = divine()
    assert(vote_dir == 0 + (vote_dir == 1))

    // Weight = balance (coin-weighted voting)
    let yes_weight: Field = voter_bal * vote_dir
    let no_weight: Field = voter_bal * sub(1, vote_dir)

    // Nullifier prevents double-voting on this proposal
    let proposal_field: Field = proposal_hash[0]
    let nullifier: Digest = hash(
        voter_id, voter_nonce, proposal_field,
        0, 0, 0, 0, 0, 0, 0
    )
    seal VoteNullifier {
        id: voter_id,
        nonce: voter_nonce,
        proposal: proposal_field
    }

    // Public output: yes weight and no weight
    pub_write(yes_weight)
    pub_write(no_weight)
}

event VoteNullifier {
    id: Field,
    nonce: Field,
    proposal: Field
}
```

Forty-five lines. A complete private vote.

---

## What the Verifier Sees

The verifier receives a proof and two public outputs: `yes_weight` and
`no_weight`. That is all.

The verifier does **not** see:

- **Who voted.** `voter_id` is divine -- it never leaves the prover.
- **Their total balance.** `voter_bal` is divine. The verifier knows the
  voter has *at least* as many coins as the weight, but not the exact
  amount.
- **Which direction.** Both `yes_weight` and `no_weight` are published.
  One is zero and the other is the balance -- but the verifier cannot
  link this to a specific voter, so it reveals nothing about any
  individual.

The verifier **does** confirm:

- The voter's leaf exists in the coin tree (Merkle proof against
  `coin_root`).
- The voter knows the auth secret for that leaf (hash preimage proof).
- The vote direction is exactly 0 or 1 (not a fabricated value).
- The weight equals the voter's actual balance (not inflated).
- The nullifier is correctly derived (no double voting).

One proof. Five guarantees. Zero information about the voter.

---

## Walking Through the Code

**Public inputs.** Three digests arrive via `pub_read5()`. The proposal
hash identifies what is being voted on. The coin root is the current state
of the coin Merkle tree -- the same tree from Chapter 2. The nullifier
root tracks which voters have already cast ballots.

**Secret inputs.** Five field elements arrive via `divine()`. These are
the voter's coin leaf: identity, balance, nonce, auth commitment, and
time-lock. The prover supplies them; the verifier never sees them.

**Leaf reconstruction.** The voter's five fields are hashed into a leaf
digest. This is the same leaf format from Chapter 2 -- the coin program
and the vote program share a data structure.

**Merkle verification.** The loop calls `merkle_step` 32 times, walking
from the leaf to the root. If the final digest matches `coin_root`, the
leaf genuinely exists in the coin tree. The voter is not fabricating an
account.

**Auth proof.** The voter divines a secret and hashes it. If the hash
matches `voter_auth`, the voter controls this account. This is Chapter 1:
`divine()`, `hash()`, `assert()`. The same three lines.

**Vote direction.** The voter divines 0 or 1. The assertion constrains it
to exactly those two values -- no other field element is accepted.

**Weight calculation.** `yes_weight` is the balance times the direction.
`no_weight` is the balance times `(1 - direction)`. Exactly one is
nonzero. The voter's full balance goes in one direction.

**Nullifier.** A hash of the voter's identity, nonce, and proposal. The
sealed event commits to these values. If the same voter tries to vote
again on the same proposal, the nullifier collision is detected. One
voter, one vote.

**Public output.** Two field elements: the yes weight and the no weight.
These are the only things that cross the wire.

---

## The Tally

After all votes are submitted, the tallier sums the results:

```
program tally

fn main() {
    let num_votes: U32 = as_u32(pub_read())
    let mut total_yes: Field = 0
    let mut total_no: Field = 0

    for i in 0..num_votes bounded 1024 {
        let yes_w: Field = divine()
        let no_w: Field = divine()
        total_yes = total_yes + yes_w
        total_no = total_no + no_w
    }

    // The result: did the proposal pass?
    let (yes_hi, yes_lo): (U32, U32) = split(total_yes)
    let (no_hi, no_lo): (U32, U32) = split(total_no)
    let passed: Bool = yes_lo > no_lo

    pub_write(total_yes)
    pub_write(total_no)
}
```

The tally program aggregates every individual vote proof's output. Each
voter's `yes_weight` and `no_weight` feed into a running sum. The final
totals are published. Anyone can verify the tally proof independently.

In a production system, you would additionally verify each individual vote
proof inside the tally using proof composition (Tier 3) -- the tally
program would recursively verify that each weight came from a valid vote
proof. For this tutorial, the structure is what matters: individual proofs
feed a collective result.

---

## Executing the Proposal

If the vote passes, the name resolver must update. This is where the
chapters compose.

The name service from Chapter 3 has an `update` operation. The coin from
Chapter 2 provides the governance token. The vote from this chapter
proves the holders decided. The execution links them:

1. The vote tally proof proves `total_yes > total_no`.
2. The name update proof changes the resolver from `old_key` to `new_key`.
3. The two proofs compose: vote result authorizes the name change.

In Chapter 3, every name has an owner and a metadata hash (the resolver).
The `update` operation requires authorization. Set the name's update
authority to the DAO -- then only a passing vote can trigger the change.

The composition is:

```
DAO_vote_tally  compose  Name_update
```

One proof that says: the holders voted yes, therefore the resolver changes.
No multisig. No admin key. No trusted party. The math authorizes the
update.

---

## The Full Circle

You started Chapter 1 with four lines:

```
let lock_hash: Digest = pub_read5()
let secret: Field = divine()
let computed: Digest = hash(secret, 0, 0, 0, 0, 0, 0, 0, 0, 0)
assert_digest(computed, lock_hash)
```

Divine the secret. Hash it. Assert it matches. The verifier confirms
without seeing.

Six chapters later, you have built:

- A private coin with five operations -- pay, lock, update, mint, burn
  (Chapter 2)
- A name service with ownership and resolver (Chapter 3)
- A non-custodial AMM with hidden reserves (Chapter 4)
- A sealed-bid auction with fair price discovery (Chapter 5)
- Private governance where no one sees who voted (this chapter)

Every single one is those same four lines with more context.

The coin's `pay` operation: divine your auth key, hash it, assert it
matches the account commitment. The name's `update` operation: divine
your ownership key, hash it, assert it matches. The AMM's invariant
check: divine the reserve position, compute the product, assert it
holds. The auction's bid: divine the price, hash it, assert it matches
the sealed commitment. The vote: divine your balance and identity, hash
them into a leaf, assert the leaf exists in the tree.

`divine()`. `hash()`. `assert()`.

The secret changes. The context grows. The primitive never does.

---

## What You Built (Complete Application)

| Chapter | Program | Secret | What Is Proven |
|---------|---------|--------|----------------|
| 1. Prove a Secret | `secret.tri` | Password | Knowledge of preimage |
| 2. Build a Coin | `coin.tri` | Account auth key | Valid state transition |
| 3. Build a Name | `name.tri` | Name ownership key | Name ownership + resolver |
| 4. Build a Strategy | `strategy.tri` | Reserve position | Invariant holds (x * y = k) |
| 5. Auction Names | `auction.tri` | Bid amount | Bid >= second price |
| 6. Upgrade to a DAO | `vote.tri` | Balance + identity + vote | Valid weighted vote |

Six programs. Six secrets. One primitive.

You have a liquid DAO where coin holders privately govern their name
service. The coin is the governance token. The vote is hidden. The
result is public. The resolver updates when the math says it should.

The first line of Chapter 1: "You are about to learn the most powerful
primitive in cryptography."

You just used it to build a private, quantum-safe DAO.

Same primitive. Same three lines.

---

## What Is Next

You have built a complete private web3 application. To go further:

- [Language Tour](tutorial.md) -- complete syntax reference with examples
- [Agent Briefing](../reference/briefing.md) -- compact cheat-sheet for
  the full language
- [Language Reference](../reference/language.md) -- sponge, Merkle,
  extension fields, proof composition
- [Gold Standard](../explanation/gold-standard.md) -- production patterns
  for PLUMB, TIDE, and COMPASS
- [OS Reference](../reference/os.md) -- portable runtime APIs across 25
  operating systems
- `examples/` -- production implementations of `coin.tri` and `uniq.tri`
