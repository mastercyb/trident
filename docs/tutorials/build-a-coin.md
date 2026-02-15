# 🪙 Chapter 2: Build a Coin

*The Builder's Journey -- Chapter 2 of 6*

In Chapter 1 you proved you know a secret. The secret was a password. The
proof was an assertion: `hash(secret) == lock_hash`.

Now the secret is your account's auth key, and the proof is a transaction.
Every token operation -- pay, lock, mint, burn -- is the same pattern: divine
the secret, hash it, prove it matches. Chapter 1, over and over, with more
context each time.

By the end of this chapter you will build a simplified coin with three
operations: pay, mint, and burn. The full production version has five. We
will build the core patterns first, then point you to the complete
implementation.

---

## 🔍 The Account

A coin needs accounts. Each account is a leaf in a Merkle tree, represented as
a hash of five fields:

```trident
leaf = hash(id, balance, nonce, auth_hash, lock_until, 0, 0, 0, 0, 0)
```

| Field | Purpose |
|-------|---------|
| `id` | Unique account identifier |
| `balance` | How many tokens this account holds |
| `nonce` | Replay protection -- increments with every operation |
| `auth_hash` | Hash of the owner's secret key (the "lock" from Chapter 1) |
| `lock_until` | Time-lock timestamp (0 = unlocked) |

The five trailing zeros are padding. The `hash` builtin always takes 10 field
elements. This is the same hash from Chapter 1 -- Tip5, one-way, deterministic.

The entire token state is a Merkle tree of these leaves. The root of the tree
is a single `Digest` that commits to every account. A state transition is an
old root becoming a new root, and the proof demonstrates that the transition
is valid.

Let us write the leaf hash function:

```trident
fn hash_leaf(
    id: Field,
    bal: Field,
    nonce: Field,
    auth: Field,
    lock: Field
) -> Digest {
    hash(id, bal, nonce, auth, lock, 0, 0, 0, 0, 0)
}
```

Five meaningful fields, five zeros. Every operation will use this function to
reconstruct and verify account leaves.

---

## 🔑 Authorization

Here is the authorization function:

```trident
fn verify_auth(auth_hash: Field) {
    let secret: Field = divine()
    let computed: Digest = hash(secret, 0, 0, 0, 0, 0, 0, 0, 0, 0)
    let (h0, _, _, _, _) = computed
    assert_eq(auth_hash, h0)
}
```

Read it carefully. This is Chapter 1.

The `auth_hash` is the lock. The `secret` is the key. The prover divines the
secret, hashes it, and asserts the first element of the digest matches the
stored auth hash. If the prover does not know the secret, the assertion fails
and no proof is generated.

Every operation in the coin calls `verify_auth`. Authorization is not a
separate system. It is the same primitive you already built, embedded inside
each operation.

---

## 📝 Events

Before we write operations, we need two event types. Events record structured
data in the proof trace. The verifier can check that events were emitted
without re-running the program.

```trident
event Nullifier {
    account_id: Field,
    nonce: Field,
}

event SupplyChange {
    old_supply: Field,
    new_supply: Field,
}
```

`Nullifier` prevents replay attacks. Each time an account is mutated, we seal
a nullifier containing the account ID and the old nonce. The verifier tracks
these commitments -- if the same nullifier appears twice, the transaction is
rejected. Because we use `seal` (not `reveal`), the verifier sees only the
hash of the nullifier, not which account was involved.

`SupplyChange` tracks supply accounting. We use `reveal` so the verifier can
confirm the numbers.

---

## 💡 A Balance Check

One more helper before the operations. When we subtract tokens from an account,
we need to ensure the result is non-negative. In a prime field, `sub(5, 10)`
does not give `-5` -- it gives a huge number near `p`. We catch this with a
range check:

```trident
fn assert_non_negative(val: Field) {
    let checked: U32 = as_u32(val)
}
```

`as_u32` converts a field element to a 32-bit unsigned integer. If the value
exceeds 2^32 (which it will if the subtraction wrapped), the conversion fails
and no proof is produced. This is how you enforce `balance >= amount` in a
prime field: subtract, then range-check the result.

---

## ⚡ Operation 1: Pay

Pay transfers tokens from one account to another. It is the most important
operation -- the one that makes a coin useful.

The structure follows a pattern that every operation will share:

1. Read public inputs (what the verifier sees)
2. Divine private inputs (what only the prover knows)
3. Verify the sender's account leaf against the state tree
4. Authorize -- Chapter 1 again
5. Check constraints (balance, time-lock)
6. Compute new leaves
7. Emit events

```text
fn pay() {
    // --- Public inputs (verifier sees these) ---
    let old_root: Digest = pub_read5()
    let new_root: Digest = pub_read5()
    let supply: Field = pub_read()
    let current_time: Field = pub_read()
    let amount: Field = pub_read()

    // --- Sender account (prover divines these) ---
    let s_id: Field = divine()
    let s_bal: Field = divine()
    let s_nonce: Field = divine()
    let s_auth: Field = divine()
    let s_lock: Field = divine()

    // Verify sender leaf exists in the tree
    let s_leaf: Digest = hash_leaf(s_id, s_bal, s_nonce, s_auth, s_lock)
    let s_leaf_expected: Digest = divine5()
    assert_digest(s_leaf, s_leaf_expected)

    // Authorize -- this is Chapter 1
    verify_auth(s_auth)

    // Time-lock check: current_time >= lock_until
    let time_diff: Field = sub(current_time, s_lock)
    assert_non_negative(time_diff)

    // Balance check: sender has enough
    let new_s_bal: Field = sub(s_bal, amount)
    assert_non_negative(new_s_bal)

    // --- Receiver account (prover divines these) ---
    let r_id: Field = divine()
    let r_bal: Field = divine()
    let r_nonce: Field = divine()
    let r_auth: Field = divine()
    let r_lock: Field = divine()

    // Verify receiver leaf exists in the tree
    let r_leaf: Digest = hash_leaf(r_id, r_bal, r_nonce, r_auth, r_lock)
    let r_leaf_expected: Digest = divine5()
    assert_digest(r_leaf, r_leaf_expected)

    // --- Compute new leaves ---
    let new_s_nonce: Field = s_nonce + 1
    let new_s_leaf: Digest = hash_leaf(
        s_id, new_s_bal, new_s_nonce, s_auth, s_lock
    )
    let new_r_bal: Field = r_bal + amount
    let new_r_leaf: Digest = hash_leaf(
        r_id, new_r_bal, r_nonce, r_auth, r_lock
    )

    // Verify new leaves
    let new_s_expected: Digest = divine5()
    assert_digest(new_s_leaf, new_s_expected)
    let new_r_expected: Digest = divine5()
    assert_digest(new_r_leaf, new_r_expected)

    // Nullifier prevents replay
    seal Nullifier { account_id: s_id, nonce: s_nonce }

    // Supply unchanged in a transfer
    reveal SupplyChange { old_supply: supply, new_supply: supply }
}
```

Walk through the key moments.

Public inputs. The verifier sees the old state root, the new state root,
the total supply, the current timestamp, and the transfer amount. These are the
claim: "the state transitioned from old_root to new_root by moving `amount`
tokens."

Divine the sender. The prover secretly inputs the sender's account fields.
Nobody else sees these. The prover then hashes them into a leaf and verifies
that leaf against the tree. If the prover lies about the balance, the leaf hash
will not match, and the proof fails.

Authorize. `verify_auth(s_auth)` is Chapter 1 embedded in a payment. The
prover divines the secret key, hashes it, asserts it matches the sender's
`auth_hash`. Only the account owner can produce this proof.

Balance and time-lock. Subtract the amount from the balance and range-check
the result. Subtract the lock time from the current time and range-check that.
Both use the same pattern: `sub` then `as_u32`.

New leaves. Compute what the sender and receiver accounts look like after
the transfer. The sender's balance decreases, the receiver's balance increases,
and the sender's nonce increments by 1.

Nullifier. `seal Nullifier { ... }` emits a sealed (hashed) event. The
verifier sees the commitment but not the contents. If the prover tries to
replay this proof, the same nullifier appears twice and the verifier rejects it.

Supply. A transfer does not change the total supply. We reveal this fact
so the verifier can confirm it.

---

## ⚡ Operation 2: Mint

Mint creates new tokens. It is simpler than pay -- there is no sender to
debit, only a recipient to credit. But it requires a different kind of
authorization: a mint authority.

```trident
fn mint() {
    // --- Public inputs ---
    let old_root: Digest = pub_read5()
    let new_root: Digest = pub_read5()
    let old_supply: Field = pub_read()
    let new_supply: Field = pub_read()
    let amount: Field = pub_read()
    let mint_auth: Field = pub_read()

    // Mint authorization -- Chapter 1 again, different key
    verify_auth(mint_auth)

    // Supply accounting
    let expected_supply: Field = old_supply + amount
    assert_eq(new_supply, expected_supply)

    // --- Recipient account ---
    let r_id: Field = divine()
    let r_bal: Field = divine()
    let r_nonce: Field = divine()
    let r_auth: Field = divine()
    let r_lock: Field = divine()

    // Verify old recipient leaf
    let r_leaf: Digest = hash_leaf(r_id, r_bal, r_nonce, r_auth, r_lock)
    let r_leaf_expected: Digest = divine5()
    assert_digest(r_leaf, r_leaf_expected)

    // New recipient leaf (balance increased)
    let new_r_bal: Field = r_bal + amount
    let new_r_leaf: Digest = hash_leaf(
        r_id, new_r_bal, r_nonce, r_auth, r_lock
    )

    // Verify new leaf
    let new_r_expected: Digest = divine5()
    assert_digest(new_r_leaf, new_r_expected)

    // Supply change
    reveal SupplyChange { old_supply: old_supply, new_supply: new_supply }
}
```

Notice the structural similarity to pay. The public inputs differ -- mint
tracks supply changes instead of transfer amounts. The authorization targets
a mint authority key instead of a personal account key. But the core is
identical: divine, hash, assert.

The supply accounting is explicit: `new_supply == old_supply + amount`. The
verifier sees both supply values and the amount. If the arithmetic does not
hold, no proof.

---

## ⚡ Operation 3: Burn

Burn destroys tokens. It is the mirror of pay, but instead of crediting a
receiver, the tokens vanish and the supply decreases.

```trident
fn burn() {
    // --- Public inputs ---
    let old_root: Digest = pub_read5()
    let new_root: Digest = pub_read5()
    let old_supply: Field = pub_read()
    let new_supply: Field = pub_read()
    let current_time: Field = pub_read()
    let amount: Field = pub_read()

    // --- Account to burn from ---
    let a_id: Field = divine()
    let a_bal: Field = divine()
    let a_nonce: Field = divine()
    let a_auth: Field = divine()
    let a_lock: Field = divine()

    // Verify account leaf
    let a_leaf: Digest = hash_leaf(a_id, a_bal, a_nonce, a_auth, a_lock)
    let a_leaf_expected: Digest = divine5()
    assert_digest(a_leaf, a_leaf_expected)

    // Authorize -- account owner must consent to burn
    verify_auth(a_auth)

    // Time-lock check
    let time_diff: Field = sub(current_time, a_lock)
    assert_non_negative(time_diff)

    // Balance check
    let new_a_bal: Field = sub(a_bal, amount)
    assert_non_negative(new_a_bal)

    // Supply accounting
    let expected_supply: Field = sub(old_supply, amount)
    assert_eq(new_supply, expected_supply)

    // New leaf
    let new_a_nonce: Field = a_nonce + 1
    let new_a_leaf: Digest = hash_leaf(
        a_id, new_a_bal, new_a_nonce, a_auth, a_lock
    )

    // Verify new leaf
    let new_a_expected: Digest = divine5()
    assert_digest(new_a_leaf, new_a_expected)

    // Nullifier
    seal Nullifier { account_id: a_id, nonce: a_nonce }

    // Supply change
    reveal SupplyChange { old_supply: old_supply, new_supply: new_supply }
}
```

Burn combines patterns from both pay and mint. Like pay, it requires the
account owner's authorization and checks the time-lock and balance. Like
mint, it tracks supply changes -- but in the opposite direction: `new_supply
== old_supply - amount` (expressed as `sub(old_supply, amount)` because there
is no `-` operator).

---

## 📝 The Full Program

The complete program combines the functions above with an entry point that dispatches by opcode:

```trident
fn main() {
    let op: Field = pub_read()
    if op == 0 { pay() }
    else if op == 3 { mint() }
    else if op == 4 { burn() }
}
```

We kept opcodes 0, 3, and 4 to match the production numbering. The two missing operations:

Lock (op 1) -- Time-locks an account's tokens until a future timestamp. Locks can only be extended, never shortened.

Update (op 2) -- Changes the token's configuration. Setting `admin_auth = 0` permanently renounces control -- no secret hashes to 0, so the config becomes immutable forever.

---

## ⚡ Build and Test

```bash
trident build coin.tri --target triton -o coin.tasm
trident build coin.tri --costs
trident build coin.tri --hotspots
```

The pay operation will be the most expensive -- it hashes the most leaves and performs the most I/O.

---

## ✅ What You Learned

Accounts are Merkle leaves. `hash(id, balance, nonce, auth, lock, 0, 0,
0, 0, 0)` -- five meaningful fields, five zeros, one digest. The entire ledger
is a tree of these leaves, committed to by a single root hash.

Authorization is Chapter 1. `verify_auth` divines a secret, hashes it,
and asserts the hash matches. The same four-line pattern from `secret.tri`,
called inside every operation.

Nullifiers prevent replay. `seal Nullifier { account_id, nonce }` emits a
sealed commitment. The verifier tracks these. If a nullifier repeats, the
transaction is rejected.

---

## 🏗️ The Production Version

This tutorial built a simplified coin to show the core patterns. The
production implementation adds:

- Config commitment -- a hash of 5 authorities and 5 hooks, verified by
  every operation to bind the proof to a specific token
- Dual authorization -- config-level authority on top of account-level
  auth, enabling regulated tokens
- Per-operation hooks -- external program IDs that compose with the token
  proof at the verifier level
- Admin renounce -- setting `admin_auth = 0` permanently freezes the
  config, enforced by hash preimage infeasibility

For the complete implementation with all 5 operations, config authorities,
hooks, and dual auth, see `os/neptune/standards/coin.tri` (535 lines) and its
specification at [TSP-1 — Coin](../reference/tsp1-coin.md).

---

## 🔮 Next

[Chapter 3: Build a Name Service](build-a-name.md) -- The coin gives you
money. Now you need identity. You will mint unique names that resolve to
public keys -- like ENS, but private and quantum-safe.
