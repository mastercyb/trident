# TSP-1 — Coin Standard

PLUMB implementation for divisible assets.

Conservation law: `sum(balances) = supply`.

See the [Gold Standard](../explanation/gold-standard.md) for the PLUMB
framework, skill library, and design rationale.

---

## Account Leaf — 10 field elements

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
| `controller` | Field | Program ID that must co-authorize operations (0 = owner only) |
| `locked_by` | Field | Program ID that locked this account (0 = not program-locked) |
| `lock_data` | Field | Program-specific lock data (position ID, collateral ratio, etc.) |
| *reserved* | Field x2 | Extension space |

### Controller Field

When `controller != 0`, every operation requires a composed proof from
the controller program in addition to normal auth. Enables
program-controlled accounts (fund collateral, escrow, protocol
treasuries).

### Locked-by Field

When `locked_by != 0`, tokens are committed to a specific program. The
`lock_data` field carries program-specific state. Only a proof from the
`locked_by` program can unlock the account. Unlike `lock_until`
(time-based), this is program-based locking.

---

## Token Config — 10 field elements

Shared by all PLUMB standards. Every operation verifies the full config
hash and extracts its dedicated authority and hook.

```
config = hash(admin_auth, pay_auth, lock_auth, mint_auth, burn_auth,
              pay_hook, lock_hook, update_hook, mint_hook, burn_hook)
```

| Field | Type | Description |
|---|---|---|
| `admin_auth` | Field | Admin secret hash. 0 = renounced (permanently immutable) |
| `pay_auth` | Field | Config-level pay authority. 0 = account auth only |
| `lock_auth` | Field | Config-level lock authority. 0 = account auth only |
| `mint_auth` | Field | Config-level mint authority. 0 = minting disabled |
| `burn_auth` | Field | Config-level burn authority. 0 = account auth only |
| `pay_hook` | Field | External program ID for pay logic (0 = none) |
| `lock_hook` | Field | External program ID for lock logic (0 = none) |
| `update_hook` | Field | External program ID for update logic (0 = none) |
| `mint_hook` | Field | External program ID for mint logic (0 = none) |
| `burn_hook` | Field | External program ID for burn logic (0 = none) |

### Authority Semantics

| Operation type | Auth = 0 | Auth != 0 |
|---|---|---|
| Account ops (pay, lock, burn) | Account auth only (permissionless) | Dual auth: account + config authority |
| Config ops (mint) | Operation disabled | Config authority required |
| Config ops (update) | Renounced (permanently frozen) | Admin authority required |

---

## Token Metadata — 10 field elements

Standalone on-chain commitment, independent of config. Does not affect
circuit business logic directly.

```
metadata = hash(name_hash, ticker_hash, teaser_hash, site_hash, custom_hash,
                price_oracle, volume_oracle, 0, 0, 0)
```

| Field | Type | Description |
|---|---|---|
| `name_hash` | Field | Hash of token name string |
| `ticker_hash` | Field | Hash of ticker symbol |
| `teaser_hash` | Field | Hash of token description/teaser |
| `site_hash` | Field | Content hash of token website/frontend |
| `custom_hash` | Field | Hash of arbitrary custom metadata |
| `price_oracle` | Field | Reference to external price oracle program (0 = none) |
| `volume_oracle` | Field | Reference to external volume oracle program (0 = none) |

---

## Merkle Tree

- Binary tree of depth `TREE_DEPTH` (e.g. 20)
- Internal node: `hash(left[0..5], right[0..5])`
- Root is the public state commitment

---

## Global Public State

| Field | Type | Description |
|---|---|---|
| `state_root` | Digest | Merkle root of all accounts |
| `supply` | Field | Sum of all balances |
| `config_hash` | Digest | Token configuration commitment |
| `metadata_hash` | Digest | Token metadata commitment |
| `current_time` | Field | Block timestamp for time-lock checks |

---

## Operations

All 5 operations follow the PLUMB proof envelope: divine 10 config
fields, hash, assert match, extract authority and hook, verify auth,
apply transition, update root, emit public I/O.

### Op 0: Pay

Transfer `amount` tokens from sender to receiver.

**Public I/O:** `op, old_root(5), new_root(5), supply, current_time, amount, config(5)`

**Constraints:**
1. Config verified, `pay_auth` and `pay_hook` extracted
2. Sender leaf verifies against `old_root`
3. `hash(secret) == sender.auth_hash`
4. If `pay_auth != 0`, dual auth required
5. `current_time >= sender.lock_until`
6. `sender.balance >= amount` (range check via `as_u32`)
7. Sender: `balance -= amount`, `nonce += 1`
8. Receiver: `balance += amount`
9. New leaves produce `new_root`
10. Supply unchanged

### Op 1: Lock(time)

Lock an account's tokens until a future timestamp.

**Public I/O:** `op, old_root(5), new_root(5), supply, lock_until_time, config(5)`

**Constraints:**
1. Config verified, `lock_auth` and `lock_hook` extracted
2. Account auth required
3. If `lock_auth != 0`, dual auth required
4. `lock_until_time >= leaf.lock_until` (extend only)
5. Leaf: `lock_until = lock_until_time`, `nonce += 1`
6. Merkle root updated, supply unchanged

### Op 2: Update

Update token configuration (authorities, hooks). Setting `admin_auth = 0`
in the new config permanently renounces authority.

**Public I/O:** `op, old_root(5), new_root(5), supply, old_config(5), new_config(5)`

**Constraints:**
1. `old_root == new_root` (state unchanged)
2. Old config verified, `update_hook` extracted
3. `hash(admin_secret) == old_config.admin_auth`
4. `admin_auth != 0` (not renounced)
5. New config fields hash to `new_config`
6. Supply unchanged

### Op 3: Mint

Create `amount` new tokens for a recipient.

**Public I/O:** `op, old_root(5), new_root(5), old_supply, new_supply, amount, config(5)`

**Constraints:**
1. Config verified, `mint_auth` and `mint_hook` extracted
2. `hash(mint_secret) == config.mint_auth` (always required, 0 = disabled)
3. `new_supply == old_supply + amount`
4. Recipient: `balance += amount`
5. Merkle root updated

### Op 4: Burn

Destroy `amount` tokens from an account.

**Public I/O:** `op, old_root(5), new_root(5), old_supply, new_supply, current_time, amount, config(5)`

**Constraints:**
1. Config verified, `burn_auth` and `burn_hook` extracted
2. Account auth required
3. If `burn_auth != 0`, dual auth required
4. `current_time >= leaf.lock_until`
5. `leaf.balance >= amount` (range check)
6. `new_supply == old_supply - amount`
7. Leaf: `balance -= amount`, `nonce += 1`
8. Merkle root updated

---

## Hooks

| Hook | Triggered by | Example use case |
|---|---|---|
| `pay_hook` | Every pay | Whitelist/blacklist, transfer limits, compliance |
| `lock_hook` | Every lock | Maximum lock duration, lock rewards |
| `update_hook` | Every config update | Multi-sig requirement, timelock on upgrades |
| `mint_hook` | Every mint | Cap enforcement, vesting schedule, KYC gate |
| `burn_hook` | Every burn | Minimum burn amount, burn tax, audit trail |

The token circuit proves the state transition is valid and that the
config (including hook references) is authentic. The verifier composes
the token proof with the hook program's proof. If `hook == 0`, no
external proof is required.

---

## Security Properties

1. **No negative balances** — `as_u32()` range check
2. **Replay prevention** — monotonic nonce + nullifiers `hash(account_id, old_nonce)`
3. **Time-lock enforcement** — `current_time` from block timestamp
4. **Lock monotonicity** — locks can only extend, not shorten
5. **Supply conservation** — `supply` is public, enforced per operation
6. **Account abstraction** — `auth_hash` accepts any preimage scheme
7. **Config binding** — every op verifies full config hash
8. **Irreversible renounce** — `admin_auth = 0` permanently freezes config
9. **Config-state separation** — config updates cannot modify the Merkle tree
10. **Hook composability** — hooks bound to config hash, composed at verification
11. **Symmetric authority** — every op has a dedicated authority and hook
12. **Safe defaults** — `mint_auth = 0` = disabled, others `= 0` = permissionless
13. **No approvals** — no allowances, no `transferFrom`, no approval phishing
