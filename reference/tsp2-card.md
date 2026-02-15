# TSP-2 — Card Standard

PLUMB implementation for unique assets.

Conservation law: `owner_count(id) = 1`.

See the [Gold Standard](../docs/explanation/gold-standard.md) for the PLUMB
framework, skill library, and design rationale.

---

## What Differs from TSP-1

1. Leaf represents an asset (unique item), not an account balance
2. Invariant: uniqueness, not divisible supply
3. No divisible arithmetic — no `balance`, no range checks, no splitting
4. Per-asset state — metadata, royalty, creator, flags live in the leaf
5. Creator immutability — `creator_id` is set at mint and can never change
6. Flag-gated operations — transferable, burnable, updatable bits control
   which PLUMB operations are allowed per asset

Operations are still Pay, Lock, Update, Mint, Burn. What changes is what
the circuit enforces inside each.

---

## Asset Leaf — 10 field elements

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
| `flags` | Field | Bitfield controlling allowed operations |

First 5 fields occupy the same positions as TSP-1. Last 5 — reserved
zeros in TSP-1 — carry per-asset state in TSP-2.

### Flags Bitfield

| Bit | Name | When set | When clear |
|-----|------|----------|------------|
| 0 | `TRANSFERABLE` | Pay (transfer) allowed | Pay rejected |
| 1 | `BURNABLE` | Burn allowed | Burn rejected |
| 2 | `UPDATABLE` | Metadata update allowed | Metadata frozen forever |
| 3 | `LOCKABLE` | Lock (time-lock) allowed | Lock rejected |
| 4 | `MINTABLE` | Re-mint into collection allowed | Collection closed |

Flags are set at mint time and cannot be changed after creation.
A soulbound credential: `flags = 0`. A game item: `flags = 31`
(all operations). A standard collectible: `flags = 11` (transferable +
burnable + lockable).

### Collection Binding

When `collection_id != 0`, the asset belongs to a collection identified
by its config hash. Collection membership is immutable after mint.

### Creator Immutability

`creator_id` is set at mint and preserved by every subsequent operation.
Provides an unforgeable provenance chain. The Royalties skill reads
`royalty_bps` from the leaf and `royalty_receiver` from collection
metadata.

---

## Token Config — 10 field elements

Same as TSP-1. See [TSP-1 Config](tsp1-coin.md#token-config--10-field-elements).

---

## Collection Metadata — 10 field elements

```
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
| *reserved* | Field x3 | Extension space |

---

## Merkle Tree

Same structure as TSP-1: binary tree of depth `TREE_DEPTH`, internal
node `hash(left[0..5], right[0..5])`, root is the public state
commitment.

---

## Global Public State

| Field | Type | Description |
|---|---|---|
| `state_root` | Digest | Merkle root of all assets |
| `asset_count` | Field | Total number of assets in tree |
| `config_hash` | Digest | Token configuration commitment |
| `metadata_hash` | Digest | Collection metadata commitment |
| `current_time` | Field | Block timestamp for time-lock checks |

---

## Operations

All 5 operations follow the PLUMB proof envelope.

### Op 0: Pay (Transfer Ownership)

**Constraints:**
1. Config verified, `pay_auth` and `pay_hook` extracted
2. Asset leaf verifies against `old_root`
3. `hash(secret) == leaf.auth_hash`
4. If `pay_auth != 0`, dual auth required
5. `current_time >= leaf.lock_until`
6. `leaf.flags & TRANSFERABLE`
7. `collection_id`, `creator_id`, `royalty_bps`, `metadata_hash`, `flags` unchanged
8. New leaf: `owner_id = new_owner`, `auth_hash = new_auth`, `nonce += 1`
9. New leaf produces `new_root`
10. Nullifier emitted: `hash(asset_id, nonce)`

### Op 1: Lock (Time-Lock Asset)

**Constraints:**
1. Config verified, `lock_auth` and `lock_hook` extracted
2. Owner auth required
3. If `lock_auth != 0`, dual auth required
4. `leaf.flags & LOCKABLE`
5. `lock_until_time >= leaf.lock_until` (extend only)
6. All immutable fields unchanged
7. Leaf: `lock_until = lock_until_time`, `nonce += 1`

### Op 2: Update (Config or Metadata)

**Config update:**
1. `old_root == new_root` (state unchanged)
2. Admin auth, `admin_auth != 0`
3. New config fields hash to `new_config`

**Metadata update:**
1. Owner auth required
2. `flags & UPDATABLE`
3. Only `metadata_hash` changes
4. `nonce += 1`

### Op 3: Mint (Originate)

**Constraints:**
1. Config verified, `mint_auth` and `mint_hook` extracted
2. `mint_auth != 0` (minting enabled)
3. Mint authorization verified
4. `asset_id` not in tree (non-membership proof)
5. `creator_id = minter_id` (immutable forever)
6. `collection_id`, `flags`, `royalty_bps` set (immutable after mint)
7. `flags & MINTABLE`
8. `nonce = 0`, `lock_until = 0`
9. New leaf produces `new_root`
10. `new_asset_count == old_asset_count + 1`
11. If `max_supply != 0`: `new_asset_count <= max_supply`

### Op 4: Burn (Release)

**Constraints:**
1. Config verified, `burn_auth` and `burn_hook` extracted
2. Owner auth required
3. If `burn_auth != 0`, dual auth required
4. `current_time >= leaf.lock_until`
5. `leaf.flags & BURNABLE`
6. Leaf removed (Merkle deletion)
7. `new_asset_count == old_asset_count - 1`
8. Nullifier emitted: `hash(asset_id, nonce)`

---

## Hooks

Same hook system as TSP-1. See [TSP-1 Hooks](tsp1-coin.md#hooks).

---

## Security Properties

Properties 1-4 from TSP-1 apply (replay prevention, time-lock
enforcement, lock monotonicity, config binding, irreversible renounce,
config-state separation, hook composability, symmetric authority, safe
defaults, no approvals).

Additional TSP-2 properties:

1. **Uniqueness** — non-membership proof at mint, exactly one leaf per `asset_id`
2. **Creator immutability** — `creator_id` set at mint, preserved by all operations
3. **Flag enforcement** — operations rejected if the corresponding flag bit is clear
4. **Supply cap** — if `max_supply != 0`, minting rejected when `asset_count >= max_supply`
5. **Immutable fields** — `collection_id`, `creator_id`, `royalty_bps`, `flags` never change after mint
