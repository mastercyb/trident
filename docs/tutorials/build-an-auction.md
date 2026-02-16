# 🔨 Chapter 5: Auction Names with Hidden Bids

*The Builder's Journey -- Chapter 5 of 6*

Chapter 3 built names. But who gets "cyber"? First come first served
rewards speed, not value. English auctions reward the deepest pockets
but punish honest bidders. Both leak information: in an English auction,
every bid is public, and the winner always overpays relative to what
was necessary.

Vickrey auctions fix this: bid your true value in secret, winner pays
the second-highest price. With ZK, the bids are genuinely hidden -- not
commit-reveal where you see commits on-chain and infer bid ranges, but
truly private. Losing bids are never revealed to anyone. Not the
auctioneer, not the winner, not the public.

The secret: your bid amount. The proof: it is higher than the second
price.

---

## 🧠 Why Vickrey

Sealed-bid, second-price: every bidder submits one secret bid. The highest bidder wins but pays the second-highest price. Your dominant strategy is to bid your true value -- bidding higher risks overpaying, bidding lower risks losing. With ZK, bids are genuinely hidden: `divine()` values that never leave the prover's machine.

---

## 🔍 The Three Phases

A Vickrey auction runs in three phases:

1. Commit -- Bidders submit `hash(bid_amount, salt, auth)`. The
   commitment is public. The bid amount is secret.
2. Reveal -- The winner proves their bid is at least the second
   price, without revealing the actual bid or any losing bids.
3. Settle -- The name transfers to the winner. The winner pays the
   second price using coin from Chapter 2.

---

## 🔑 Phase 1: Commit

Each bidder commits to their bid by publishing a hash. The bid amount
and salt are secret. Nobody -- not even the auctioneer -- knows what
anyone bid.

```trident
program auction_commit

fn main() {
    // Public inputs
    let name_hash: Digest = pub_read5()        // which name is being auctioned
    let bidder_auth: Field = pub_read()         // bidder's public auth hash

    // Secret inputs
    let bid_amount: Field = divine()            // the actual bid
    let salt: Field = divine()                  // commitment randomness

    // Compute commitment: hash(bid, salt, auth, 0, 0, 0, 0, 0, 0, 0)
    let commitment: Digest = hash(bid_amount, salt, bidder_auth,
                                  0, 0, 0, 0, 0, 0, 0)

    // Public output: the commitment
    pub_write5(commitment.0, commitment.1, commitment.2,
               commitment.3, commitment.4)
}
```

This is Chapter 1 again. A secret goes in, a hash comes out. The
commitment is published on-chain. The bid amount and salt exist only
in the bidder's memory.

The salt prevents dictionary attacks. Without it, an attacker could
hash every plausible bid amount and compare against the published
commitment. With a random salt, the commitment reveals nothing about
the bid.

The `bidder_auth` field binds the commitment to a specific bidder. This
prevents someone from copying your commitment and claiming it as theirs
during the reveal phase.

---

## 🔍 Phase 2: Reveal (Winner Only)

This is where ZK earns its keep. On a transparent chain, every bidder
must reveal their bid in the reveal phase. All bids become public. The
auctioneer, the other bidders, and the entire world learn what everyone
was willing to pay. That information leaks into future auctions, future
negotiations, future markets.

Here, only the winner reveals -- and even they do not reveal the bid
amount. They prove three things:

1. My commitment matches what I submitted in Phase 1.
2. My bid is at least the second-highest price.
3. I control the auth key bound to the commitment.

The losing bidders do nothing. Their bids remain secret forever.

```trident
program auction_reveal

fn main() {
    // Public inputs
    let name_hash: Digest = pub_read5()        // which name
    let second_price: Field = pub_read()        // second-highest bid
    let bid_commitment: Digest = pub_read5()    // winner's commitment from Phase 1

    // Secret inputs
    let bid_amount: Field = divine()            // actual bid (never revealed)
    let salt: Field = divine()                  // commitment randomness
    let bidder_auth: Field = divine()           // bidder's auth key hash

    // 1. Verify commitment matches Phase 1
    let computed: Digest = hash(bid_amount, salt, bidder_auth,
                                0, 0, 0, 0, 0, 0, 0)
    assert_digest(computed, bid_commitment)

    // 2. Prove bid >= second_price (winner condition)
    let margin: Field = sub(bid_amount, second_price)
    assert_non_negative(margin)

    // 3. Prove bidder identity (Chapter 1 pattern)
    verify_auth(bidder_auth)

    // Output: winner pays second_price, not their actual bid
    pub_write(second_price)
}

fn verify_auth(auth_hash: Field) {
    let secret: Field = divine()
    let computed: Digest = hash(secret, 0, 0, 0, 0, 0, 0, 0, 0, 0)
    let (h0, _, _, _, _) = computed
    assert_eq(auth_hash, h0)
}

fn assert_non_negative(val: Field) {
    let _: U32 = as_u32(val)
}
```

Walk through it.

`let bid_commitment: Digest = pub_read5()` -- The winner's commitment
from Phase 1, visible to the verifier. This anchors the proof to a
specific on-chain commitment.

`let bid_amount: Field = divine()` -- The actual bid. This is the
secret. It enters the prover's machine and never leaves. The verifier
never sees it. It does not appear in the proof.

`assert_digest(computed, bid_commitment)` -- The commitment check.
The prover recomputes the hash from their secret inputs and asserts it
matches the public commitment from Phase 1. This prevents the winner
from changing their bid after seeing the second price.

`let margin: Field = sub(bid_amount, second_price)` -- Compute
the difference. If the bid is at least the second price, this is a
non-negative value.

`assert_non_negative(margin)` -- Proves the winner condition: bid >= second price.

`verify_auth(bidder_auth)` -- Proves the bidder's identity. Both helpers are from Chapter 2.

`pub_write(second_price)` -- The output. The winner pays the second
price, not their actual bid. This is the Vickrey mechanism: the price
you pay is independent of what you bid, so you have no reason to bid
anything other than your true value.

Notice what is absent. There is no loop over bidders. There is no
decryption of losing bids. There is no reveal phase for anyone except
the winner. The losing bidders' secrets stay divine -- they were conjured
by the prover's machine and they vanish when the machine halts.

---

## 🧩 Phase 3: Settle

Settlement composes three proofs, each from a previous chapter:

1. Auction proof (this chapter) -- The winner proved their bid
   exceeds the second price.
2. Coin pay proof (Chapter 2) -- The winner transfers `second_price`
   coins to the seller.
3. Name transfer proof (Chapter 3) -- The seller transfers the name
   to the winner.

```text
program auction_settle

fn main() {
    // Auction result (from auction_reveal proof)
    let name_hash: Digest = pub_read5()
    let payment_amount: Field = pub_read()      // second_price

    // Coin payment: winner --> seller (Chapter 2 pattern)
    let old_coin_root: Digest = pub_read5()
    let new_coin_root: Digest = pub_read5()
    verify_coin_transfer(old_coin_root, new_coin_root, payment_amount)

    // Name transfer: seller --> winner (Chapter 3 pattern)
    let old_name_root: Digest = pub_read5()
    let new_name_root: Digest = pub_read5()
    let winner_auth: Field = pub_read()
    verify_name_transfer(old_name_root, new_name_root,
                         name_hash, winner_auth)

    // Output: new state roots
    pub_write5(new_coin_root.0, new_coin_root.1, new_coin_root.2,
               new_coin_root.3, new_coin_root.4)
    pub_write5(new_name_root.0, new_name_root.1, new_name_root.2,
               new_name_root.3, new_name_root.4)
}
```

The details of `verify_coin_transfer` and `verify_name_transfer` follow
the patterns from Chapters 2 and 3: divine the account leaves, verify
Merkle paths against the old root, apply the transfer, recompute leaves,
and verify the new root. The auction adds one new element: the payment
amount is the `second_price` from the reveal proof, not a value chosen
by the sender.

Three proofs compose into one verified settlement. The verifier checks
one proof and knows: the auction was fair, the payment was made, and the
name changed hands. No intermediary. No escrow. No trust.

---

## 💡 What Makes This Impossible Without ZK

On transparent chains, commit-reveal leaks information: commitment counts, gas patterns, and the full reveal phase expose all bids. With Trident, bid amounts are `divine()` -- they exist only in the prover's memory. Only the winner proves. Losing bids stay secret forever.

---

## ⚡ Build It

```nu
trident build auction_commit.tri --target triton -o auction_commit.tasm
trident build auction_reveal.tri --target triton -o auction_reveal.tasm
trident build auction_settle.tri --target triton -o auction_settle.tasm
```

Check the costs:

```nu
trident build auction_reveal.tri --target triton --costs
```

The reveal program is the most expensive of the three -- it has a hash
(the commitment recomputation), a range check (the bid comparison), and
an auth verification (the identity proof). Even so, the cost is modest:
two hashes and one U32 conversion. The auction is cheap because the
mechanism is simple. Vickrey's elegance translates directly into proving
efficiency.

---

## ✅ What You Learned

- Vickrey auctions are sealed-bid, second-price, and
  incentive-compatible. Your dominant strategy is to bid your true value.
- Commit -- `hash(bid, salt, auth)` publishes a commitment. Nobody
  sees the bid. The salt prevents dictionary attacks. The auth binds the
  commitment to the bidder.
- Reveal -- The winner proves `bid >= second_price` without revealing
  the bid. `sub` computes the margin. `as_u32` range-checks it. One
  line proves the winner condition.
- Settle -- Compose with coin pay (Chapter 2) and name transfer
  (Chapter 3). Three proofs, one verified settlement.
- Losing bids stay secret forever. Not encrypted. Not obfuscated.
  Never computed outside the bidder's machine.
- The secret is your bid -- `divine()`, `hash()`, `assert` -- the
  Chapter 1 pattern, once more.

---

## 🔮 Next

[Chapter 6: Upgrade to a DAO](build-a-dao.md) -- Your coin has holders,
your names have owners, your liquidity is flowing, your auctions are fair.
One piece remains: governance. Token holders vote to change the name
resolver -- and the votes stay private too.
