# Chapter 3: Build a Name Service

*The Builder's Journey -- Chapter 3 of 6*

Chapter 1: prove you know a secret. Chapter 2: the secret unlocks your
coins. Now: the secret proves you own a name -- and the name resolves to
whatever you want.

A name service maps human-readable names to public keys. ENS does this on
Ethereum. We will do it on Neptune -- private, quantum-safe, and provable.
The name itself is a unique asset. Owning it means knowing the secret behind
its auth hash. Chapter 1 again.

---

## Names as Unique Assets

A coin (TSP-1) is fungible. One coin of the same denomination is identical to
another. A name is not. The name "cyber" is distinct from the name "neptune" --
there is exactly one of each, and ownership matters.

Neptune represents unique assets using the TSP-2 pattern. Where a coin leaf
has 5 meaningful fields and pads the rest with zeros, a uniq leaf uses all 10:

| Field | Coin (TSP-1) | Name (TSP-2) |
|-------|-------------|---------------|
| 1 | account id | asset_id = hash of the name string |
| 2 | balance | owner_id |
| 3 | nonce | nonce |
| 4 | auth_hash | auth_hash (Chapter 1 -- the ownership secret) |
| 5 | lock_until | lock_until |
| 6 | 0 | collection_id (the name registry) |
| 7 | 0 | metadata_hash (what the name resolves to) |
| 8 | 0 | royalty_bps |
| 9 | 0 | creator_id |
| 10 | 0 | flags (TRANSFERABLE + UPDATABLE = 5) |

The key insight: `metadata_hash` is the resolver. It holds the hash of the
public key (or any record) that the name points to. When you "resolve" a name,
you look up its leaf and read the metadata. When you "update" a name, you prove
ownership and swap in a new metadata hash.

---

## The Name Leaf

Every name is a leaf in a Merkle tree. The leaf is the hash of all 10 fields:

```trident
fn hash_leaf(
    asset_id: Field,
    owner_id: Field,
    nonce: Field,
    auth_hash: Field,
    lock_until: Field,
    collection_id: Field,
    metadata_hash: Field,
    royalty_bps: Field,
    creator_id: Field,
    flags: Field
) -> Digest {
    hash(
        asset_id,
        owner_id,
        nonce,
        auth_hash,
        lock_until,
        collection_id,
        metadata_hash,
        royalty_bps,
        creator_id,
        flags
    )
}
```

For a name called "cyber" that resolves to public key `pk`:

- `asset_id = hash("cyber")[0]` -- the content hash of the name string
- `owner_id` -- identifies the current owner
- `nonce` -- incremented on every state change (prevents replay)
- `auth_hash = hash(owner_secret)[0]` -- the ownership proof (Chapter 1)
- `lock_until = 0` -- no time lock
- `collection_id` -- identifies which name registry this belongs to
- `metadata_hash = hash(pk)[0]` -- what the name resolves to
- `royalty_bps = 0` -- no royalties for this tutorial
- `creator_id` -- who registered the name originally
- `flags = 5` -- TRANSFERABLE (bit 0) + UPDATABLE (bit 2)

The flags value of 5 means: the name can be transferred to a new owner and its
resolver can be updated. It cannot be burned or locked. These flags are
immutable after mint.

---

## Authorization

Before we build the operations, we need the auth pattern. This is Chapter 1
in a function:

```trident
fn verify_auth(auth_hash: Field) {
    let secret: Field = divine()
    let computed: Digest = hash(secret, 0, 0, 0, 0, 0, 0, 0, 0, 0)
    let (h0, _, _, _, _) = computed
    assert_eq(auth_hash, h0)
}
```

The prover supplies a secret. The program hashes it and checks that the first
element matches `auth_hash`. If it does, the prover owns whatever that
auth_hash protects. If not, no proof. Same primitive, same three lines, same
as Chapter 1.

---

## Minting a Name

Registration is minting. You create a new unique asset in the tree.

```trident
fn mint() {
    let old_root: Digest = pub_read5()
    let new_root: Digest = pub_read5()
    let old_count: Field = pub_read()
    let new_count: Field = pub_read()
    let asset_id: Field = pub_read()
    let metadata_hash: Field = pub_read()
    let collection_id: Field = pub_read()
    let config: Digest = pub_read5()

    // Config authority check (who can mint names?)
    let cfg_mint_auth: Field = divine()
    let cfg_other: Field = divine()
    let computed_config: Digest = hash(
        cfg_other, 0, 0, cfg_mint_auth, 0,
        0, 0, 0, 0, 0
    )
    assert_digest(computed_config, config)
    verify_auth(cfg_mint_auth)

    // Supply accounting: exactly one new name
    let expected_count: Field = old_count + 1
    assert_eq(new_count, expected_count)

    // New owner's credentials
    let owner_id: Field = divine()
    let auth_hash: Field = divine()
    let creator_id: Field = divine()
    let flags: Field = 5

    // Create the leaf: nonce = 0, lock_until = 0, royalty = 0
    let new_leaf: Digest = hash_leaf(
        asset_id,
        owner_id,
        0,
        auth_hash,
        0,
        collection_id,
        metadata_hash,
        0,
        creator_id,
        flags
    )

    // Verify the leaf was inserted into the tree
    let new_leaf_expected: Digest = divine5()
    assert_digest(new_leaf, new_leaf_expected)

    reveal NameMint {
        asset_id: asset_id,
        collection_id: collection_id,
        metadata_hash: metadata_hash,
    }

    reveal SupplyChange {
        old_count: old_count,
        new_count: new_count,
    }
}
```

The verifier sees: the old tree root, the new tree root, the name's asset ID,
and the resolver hash. The verifier does not see who owns the name or what
secret protects it. The proof guarantees the leaf was correctly formed and
inserted.

---

## Resolving a Name

Resolution is read-only. It does not require a ZK program.

Given a name's Merkle proof, anyone can verify that name X resolves to key Y:

1. Look up the leaf for `asset_id = hash("cyber")[0]`
2. Read `metadata_hash` from the leaf
3. Verify the Merkle proof against the current root

No proof generation needed. The Merkle tree is publicly committed (the root is
on-chain). The leaf data is available to anyone with the authentication path.
Resolution is cheap -- one Merkle verification, no proving cost.

But *changing* what the name resolves to requires a proof. That is the next
section.

---

## Updating the Resolver

The owner wants "cyber" to point to a new public key. This requires proving
ownership -- then swapping the metadata hash.

```trident
fn update() {
    let old_root: Digest = pub_read5()
    let new_root: Digest = pub_read5()
    let asset_id: Field = pub_read()
    let new_metadata_hash: Field = pub_read()
    let config: Digest = pub_read5()

    // Verify config
    let cfg_admin: Field = divine()
    let computed_config: Digest = hash(
        cfg_admin, 0, 0, 0, 0,
        0, 0, 0, 0, 0
    )
    assert_digest(computed_config, config)

    // Current leaf (secret -- the prover knows the full leaf)
    let leaf_asset_id: Field = divine()
    let leaf_owner_id: Field = divine()
    let leaf_nonce: Field = divine()
    let leaf_auth_hash: Field = divine()
    let leaf_lock_until: Field = divine()
    let leaf_collection_id: Field = divine()
    let leaf_metadata_hash: Field = divine()
    let leaf_royalty_bps: Field = divine()
    let leaf_creator_id: Field = divine()
    let leaf_flags: Field = divine()

    // Verify old leaf exists in the tree
    let old_leaf: Digest = hash_leaf(
        leaf_asset_id,
        leaf_owner_id,
        leaf_nonce,
        leaf_auth_hash,
        leaf_lock_until,
        leaf_collection_id,
        leaf_metadata_hash,
        leaf_royalty_bps,
        leaf_creator_id,
        leaf_flags
    )
    let old_leaf_expected: Digest = divine5()
    assert_digest(old_leaf, old_leaf_expected)

    // Must be the right name
    assert_eq(leaf_asset_id, asset_id)

    // Prove ownership -- Chapter 1 again
    verify_auth(leaf_auth_hash)

    // New leaf: same everything except metadata_hash and nonce
    let new_nonce: Field = leaf_nonce + 1
    let new_leaf: Digest = hash_leaf(
        leaf_asset_id,
        leaf_owner_id,
        new_nonce,
        leaf_auth_hash,
        leaf_lock_until,
        leaf_collection_id,
        new_metadata_hash,
        leaf_royalty_bps,
        leaf_creator_id,
        leaf_flags
    )
    let new_leaf_expected: Digest = divine5()
    assert_digest(new_leaf, new_leaf_expected)

    reveal ResolverUpdate {
        asset_id: leaf_asset_id,
        old_metadata: leaf_metadata_hash,
        new_metadata: new_metadata_hash,
    }
}
```

The old leaf and the new leaf differ in exactly two fields: `metadata_hash`
(the resolver record) and `nonce` (incremented to prevent replay). Everything
else -- owner, flags, collection -- stays the same.

The verifier sees the name, the old root, the new root, and the new resolver
hash. The verifier does not see the owner, the secret, or the old resolver.
The proof guarantees the owner authorized the change.

---

## Transferring a Name

Transfer is Chapter 2's pay pattern applied to a unique asset. Instead of
moving a balance from one account to another, you move ownership of a name
from one key to another.

```trident
fn pay() {
    let old_root: Digest = pub_read5()
    let new_root: Digest = pub_read5()
    let asset_id: Field = pub_read()
    let current_time: Field = pub_read()
    let config: Digest = pub_read5()

    // Verify config
    let cfg_admin: Field = divine()
    let cfg_pay_auth: Field = divine()
    let computed_config: Digest = hash(
        cfg_admin, cfg_pay_auth, 0, 0, 0,
        0, 0, 0, 0, 0
    )
    assert_digest(computed_config, config)

    // Current leaf (secret)
    let leaf_asset_id: Field = divine()
    let leaf_owner_id: Field = divine()
    let leaf_nonce: Field = divine()
    let leaf_auth_hash: Field = divine()
    let leaf_lock_until: Field = divine()
    let leaf_collection_id: Field = divine()
    let leaf_metadata_hash: Field = divine()
    let leaf_royalty_bps: Field = divine()
    let leaf_creator_id: Field = divine()
    let leaf_flags: Field = divine()

    // Verify old leaf
    let old_leaf: Digest = hash_leaf(
        leaf_asset_id,
        leaf_owner_id,
        leaf_nonce,
        leaf_auth_hash,
        leaf_lock_until,
        leaf_collection_id,
        leaf_metadata_hash,
        leaf_royalty_bps,
        leaf_creator_id,
        leaf_flags
    )
    let old_leaf_expected: Digest = divine5()
    assert_digest(old_leaf, old_leaf_expected)

    // Must be the right name
    assert_eq(leaf_asset_id, asset_id)

    // Prove ownership -- Chapter 1 again
    verify_auth(leaf_auth_hash)

    // Config-level pay auth (0 = owner only, else dual auth)
    if cfg_pay_auth == 0 {
    } else {
        verify_auth(cfg_pay_auth)
    }

    // Time-lock check: current_time >= lock_until
    let lock_headroom: Field = sub(current_time, leaf_lock_until)
    let _: U32 = as_u32(lock_headroom)

    // New owner
    let new_owner_id: Field = divine()
    let new_auth_hash: Field = divine()

    // New leaf: owner changes, nonce increments, everything else stays
    let new_nonce: Field = leaf_nonce + 1
    let new_leaf: Digest = hash_leaf(
        leaf_asset_id,
        new_owner_id,
        new_nonce,
        new_auth_hash,
        leaf_lock_until,
        leaf_collection_id,
        leaf_metadata_hash,
        leaf_royalty_bps,
        leaf_creator_id,
        leaf_flags
    )
    let new_leaf_expected: Digest = divine5()
    assert_digest(new_leaf, new_leaf_expected)

    // Nullifier prevents replay (sealed -- verifier sees commitment only)
    seal Nullifier { asset_id: leaf_asset_id, nonce: leaf_nonce }

    reveal NameTransfer {
        asset_id: leaf_asset_id,
        from_owner: leaf_owner_id,
        to_owner: new_owner_id,
    }
}
```

Compare this to Chapter 2's coin pay. The structure is identical:

1. Read the old leaf, verify it exists in the tree
2. Prove ownership with `verify_auth` (Chapter 1)
3. Check time-lock constraints
4. Build the new leaf with the new owner
5. Emit a nullifier to prevent double-spend

The difference: a coin pay changes the balance. A name pay changes the owner.
No balance field exists here -- there is nothing to split or merge. The entire
asset moves as one indivisible unit.

---

## The Full Program

Here is the complete name service. Three operations dispatched by opcode:
pay (0), update (2), mint (3).

```trident
program name

// --- Leaf hashing (10 fields) ---
fn hash_leaf(
    asset_id: Field,
    owner_id: Field,
    nonce: Field,
    auth_hash: Field,
    lock_until: Field,
    collection_id: Field,
    metadata_hash: Field,
    royalty_bps: Field,
    creator_id: Field,
    flags: Field
) -> Digest {
    hash(
        asset_id,
        owner_id,
        nonce,
        auth_hash,
        lock_until,
        collection_id,
        metadata_hash,
        royalty_bps,
        creator_id,
        flags
    )
}

// --- Authorization: divine, hash, assert (Chapter 1) ---
fn verify_auth(auth_hash: Field) {
    let secret: Field = divine()
    let computed: Digest = hash(secret, 0, 0, 0, 0, 0, 0, 0, 0, 0)
    let (h0, _, _, _, _) = computed
    assert_eq(auth_hash, h0)
}

// --- Events ---
event NameMint {
    asset_id: Field,
    collection_id: Field,
    metadata_hash: Field,
}

event NameTransfer {
    asset_id: Field,
    from_owner: Field,
    to_owner: Field,
}

event ResolverUpdate {
    asset_id: Field,
    old_metadata: Field,
    new_metadata: Field,
}

event Nullifier {
    asset_id: Field,
    nonce: Field,
}

event SupplyChange {
    old_count: Field,
    new_count: Field,
}

// --- Op 0: PAY -- transfer name ownership ---
fn pay() {
    let old_root: Digest = pub_read5()
    let new_root: Digest = pub_read5()
    let asset_id: Field = pub_read()
    let current_time: Field = pub_read()
    let config: Digest = pub_read5()
    let cfg_admin: Field = divine()
    let cfg_pay_auth: Field = divine()
    let computed_config: Digest = hash(
        cfg_admin, cfg_pay_auth, 0, 0, 0,
        0, 0, 0, 0, 0
    )
    assert_digest(computed_config, config)
    let leaf_asset_id: Field = divine()
    let leaf_owner_id: Field = divine()
    let leaf_nonce: Field = divine()
    let leaf_auth_hash: Field = divine()
    let leaf_lock_until: Field = divine()
    let leaf_collection_id: Field = divine()
    let leaf_metadata_hash: Field = divine()
    let leaf_royalty_bps: Field = divine()
    let leaf_creator_id: Field = divine()
    let leaf_flags: Field = divine()
    let old_leaf: Digest = hash_leaf(
        leaf_asset_id, leaf_owner_id, leaf_nonce, leaf_auth_hash,
        leaf_lock_until, leaf_collection_id, leaf_metadata_hash,
        leaf_royalty_bps, leaf_creator_id, leaf_flags
    )
    let old_leaf_expected: Digest = divine5()
    assert_digest(old_leaf, old_leaf_expected)
    assert_eq(leaf_asset_id, asset_id)
    verify_auth(leaf_auth_hash)
    if cfg_pay_auth == 0 {
    } else {
        verify_auth(cfg_pay_auth)
    }
    let lock_headroom: Field = sub(current_time, leaf_lock_until)
    let _: U32 = as_u32(lock_headroom)
    let new_owner_id: Field = divine()
    let new_auth_hash: Field = divine()
    let new_nonce: Field = leaf_nonce + 1
    let new_leaf: Digest = hash_leaf(
        leaf_asset_id, new_owner_id, new_nonce, new_auth_hash,
        leaf_lock_until, leaf_collection_id, leaf_metadata_hash,
        leaf_royalty_bps, leaf_creator_id, leaf_flags
    )
    let new_leaf_expected: Digest = divine5()
    assert_digest(new_leaf, new_leaf_expected)
    seal Nullifier { asset_id: leaf_asset_id, nonce: leaf_nonce }
    reveal NameTransfer {
        asset_id: leaf_asset_id,
        from_owner: leaf_owner_id,
        to_owner: new_owner_id,
    }
}

// --- Op 2: UPDATE -- change resolver record ---
fn update() {
    let old_root: Digest = pub_read5()
    let new_root: Digest = pub_read5()
    let asset_id: Field = pub_read()
    let new_metadata_hash: Field = pub_read()
    let config: Digest = pub_read5()
    let cfg_admin: Field = divine()
    let computed_config: Digest = hash(
        cfg_admin, 0, 0, 0, 0,
        0, 0, 0, 0, 0
    )
    assert_digest(computed_config, config)
    let leaf_asset_id: Field = divine()
    let leaf_owner_id: Field = divine()
    let leaf_nonce: Field = divine()
    let leaf_auth_hash: Field = divine()
    let leaf_lock_until: Field = divine()
    let leaf_collection_id: Field = divine()
    let leaf_metadata_hash: Field = divine()
    let leaf_royalty_bps: Field = divine()
    let leaf_creator_id: Field = divine()
    let leaf_flags: Field = divine()
    let old_leaf: Digest = hash_leaf(
        leaf_asset_id, leaf_owner_id, leaf_nonce, leaf_auth_hash,
        leaf_lock_until, leaf_collection_id, leaf_metadata_hash,
        leaf_royalty_bps, leaf_creator_id, leaf_flags
    )
    let old_leaf_expected: Digest = divine5()
    assert_digest(old_leaf, old_leaf_expected)
    assert_eq(leaf_asset_id, asset_id)
    verify_auth(leaf_auth_hash)
    let new_nonce: Field = leaf_nonce + 1
    let new_leaf: Digest = hash_leaf(
        leaf_asset_id, leaf_owner_id, new_nonce, leaf_auth_hash,
        leaf_lock_until, leaf_collection_id, new_metadata_hash,
        leaf_royalty_bps, leaf_creator_id, leaf_flags
    )
    let new_leaf_expected: Digest = divine5()
    assert_digest(new_leaf, new_leaf_expected)
    reveal ResolverUpdate {
        asset_id: leaf_asset_id,
        old_metadata: leaf_metadata_hash,
        new_metadata: new_metadata_hash,
    }
}

// --- Op 3: MINT -- register a new name ---
fn mint() {
    let old_root: Digest = pub_read5()
    let new_root: Digest = pub_read5()
    let old_count: Field = pub_read()
    let new_count: Field = pub_read()
    let asset_id: Field = pub_read()
    let metadata_hash: Field = pub_read()
    let collection_id: Field = pub_read()
    let config: Digest = pub_read5()
    let cfg_mint_auth: Field = divine()
    let cfg_other: Field = divine()
    let computed_config: Digest = hash(
        cfg_other, 0, 0, cfg_mint_auth, 0,
        0, 0, 0, 0, 0
    )
    assert_digest(computed_config, config)
    verify_auth(cfg_mint_auth)
    let expected_count: Field = old_count + 1
    assert_eq(new_count, expected_count)
    let owner_id: Field = divine()
    let auth_hash: Field = divine()
    let creator_id: Field = divine()
    let new_leaf: Digest = hash_leaf(
        asset_id, owner_id, 0, auth_hash, 0,
        collection_id, metadata_hash, 0, creator_id, 5
    )
    let new_leaf_expected: Digest = divine5()
    assert_digest(new_leaf, new_leaf_expected)
    reveal NameMint {
        asset_id: asset_id,
        collection_id: collection_id,
        metadata_hash: metadata_hash,
    }
    reveal SupplyChange {
        old_count: old_count,
        new_count: new_count,
    }
}

// --- Entry point ---
fn main() {
    let op: Field = pub_read()
    if op == 0 {
        pay()
    } else if op == 2 {
        update()
    } else if op == 3 {
        mint()
    }
}
```

Three operations, one dispatch. No lock, no burn -- names are permanent and
always transferable. The flags field (5 = TRANSFERABLE + UPDATABLE) enforces
this at the protocol level.

---

## Build It

```bash
trident build name.tri --target triton -o name.tasm
```

Type-check:

```bash
trident check name.tri
```

See the cost:

```bash
trident build name.tri --costs
```

The dominant cost will be hashing. Each operation does 2-3 hash calls (leaf
verification, auth check, new leaf creation). On Triton VM, each Tip5 hash is
one instruction -- this is among the cheapest hash functions you can use in a
ZK context.

---

## The Connection

The name you just built will be auctioned in Chapter 5 using a Vickrey auction
with sealed bids. Nobody sees anyone else's bid until the auction closes. The
bid commitment is a hash. The bid price is divine. Chapter 1 again.

The coin from Chapter 2 will be the payment currency. The liquidity strategy
from Chapter 4 will make that coin tradeable. And in Chapter 6, the DAO will
govern who can register names -- replacing the single `cfg_mint_auth` with
coin-weighted voting.

It all connects. Every chapter is the same primitive -- divine, hash, assert --
applied to a different problem.

---

## What You Learned

- Unique assets (uniqs / TSP-2) use 10-field leaves where every field matters.
  Coins use 5 fields and pad the rest with zeros.
- A name is a uniq where `asset_id` is the content hash of the name string
  and `metadata_hash` is the resolver record.
- The resolver is readable by anyone with a Merkle proof. No ZK program
  needed for lookups.
- Updating the resolver requires proving ownership: `divine`, `hash`,
  `assert_eq` -- the same pattern from Chapter 1.
- Transferring a name reuses Chapter 2's pay pattern: verify auth, build
  new leaf with new owner, emit nullifier.
- Flags (TRANSFERABLE + UPDATABLE = 5) are set at mint time and never change.
  They define what operations the asset supports at the protocol level.
- For the production uniq implementation with all five PLUMB operations,
  see `examples/uniq/uniq.tri`.

---

## Next

[Chapter 4: Build a Liquidity Strategy](build-a-strategy.md) -- Your coin has
value. Your name has identity. Now you will make the coin tradeable -- a
constant-product AMM where the pricing invariant is proven, not trusted.
