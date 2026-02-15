# Sui

[← Target Reference](../../reference/targets.md) | VM: [MOVEVM](../../vm/movevm/README.md)

Sui is the object-centric blockchain powered by MOVEVM. Trident compiles
to Move bytecode (`.mv`) and links against `sui.ext.*` for Sui-specific
runtime bindings. Sui's unique contribution is the object model -- state
is organized as objects with explicit ownership, enabling parallel
execution without global locks.

---

## Runtime Parameters

| Parameter | Value |
|---|---|
| VM | MOVEVM |
| Runtime binding | `sui.ext.*` |
| Account model | Object-centric (ownership graph) |
| Storage model | Object store |
| Transaction model | Signed (Ed25519, Secp256k1, zkLogin) |
| Cost model | Gas |
| Cross-chain | -- |

---

## Programming Model

### Entry Points

Sui programs expose entry functions that receive objects as arguments.
The runtime passes objects based on the transaction's specified inputs.

- `init` -- called once when the module is published
- entry functions -- callable by transactions
- public functions -- callable by other modules

```
program my_token

use sui.ext.object
use sui.ext.transfer
use sui.ext.tx
use sui.ext.coin

// Called once at module publication
fn init() {
    let treasury: Field = sui.ext.object.new()
    sui.ext.transfer.send(treasury, sui.ext.tx.sender())
}

// Entry function: mint tokens
pub fn mint(treasury: Field, amount: Field, recipient: Field) {
    // Verify caller owns the treasury capability
    let coin: Field = sui.ext.coin.mint(treasury, amount)
    sui.ext.transfer.public_send(coin, recipient)
}

// Entry function: transfer tokens
pub fn send(coin: Field, recipient: Field) {
    sui.ext.transfer.public_send(coin, recipient)
}
```

### State Access

Sui state is organized as objects -- uniquely identified entities with
an owner. Objects come in three flavors:

| Object type | Access | Consensus | Use for |
|-------------|--------|-----------|---------|
| Owned | Single writer (owner) | No consensus needed | Tokens, capabilities, user data |
| Shared | Multiple writers | Consensus-ordered | AMM pools, auctions, registries |
| Immutable | Read-only, everyone | No consensus needed | Package code, frozen configs |

```
use sui.ext.object
use sui.ext.dynamic_field

// Create a new object
let id: Field = sui.ext.object.new()

// Read object fields (by UID)
let value: Field = sui.ext.object.borrow(id, field_offset)

// Mutate object fields
sui.ext.object.borrow_mut(id, field_offset, new_value)

// Delete an object
sui.ext.object.delete(id)

// Dynamic fields -- attach/read/remove key-value pairs on objects
sui.ext.dynamic_field.add(parent_id, key, value)
let val: Field = sui.ext.dynamic_field.borrow(parent_id, key)
sui.ext.dynamic_field.remove(parent_id, key)
let exists: Bool = sui.ext.dynamic_field.exists(parent_id, key)
```

The object model eliminates Ethereum's global state contention. Transactions
touching different owned objects execute in parallel without ordering.
Only shared objects require consensus.

### Identity and Authorization

Transaction sender identity comes from the signing key:

```
use sui.ext.tx

let sender: Field = sui.ext.tx.sender()
let epoch: Field = sui.ext.tx.epoch()
let epoch_timestamp: Field = sui.ext.tx.epoch_timestamp_ms()
```

Authorization is enforced by object ownership: only the owner of an
object can pass it to a transaction as a mutable reference. Capabilities
are objects that grant specific permissions:

```
// Capability pattern: whoever owns the TreasuryCap can mint
pub fn mint(treasury_cap: Field, amount: Field, recipient: Field) {
    // treasury_cap is an owned object -- only the owner can call this
    let coin: Field = sui.ext.coin.mint(treasury_cap, amount)
    sui.ext.transfer.public_send(coin, recipient)
}
```

No access control lists, no role mappings. If you own the capability
object, you have the permission.

### Value Transfer

Sui has native `Coin<T>` objects. Value moves by transferring object
ownership:

```
use sui.ext.coin
use sui.ext.transfer

// Split a coin (take amount out of existing coin)
let split_coin: Field = sui.ext.coin.split(coin, amount)

// Merge coins (combine into one)
sui.ext.coin.merge(target_coin, source_coin)

// Get coin value
let balance: Field = sui.ext.coin.value(coin)

// Transfer coin to recipient
sui.ext.transfer.public_send(coin, recipient)

// Create a zero-value coin
let empty: Field = sui.ext.coin.zero()
```

### Cross-Contract Interaction

Sui modules can call each other's public functions directly -- there is
no message-passing layer. Shared objects enable multi-module transactions:

```
// Direct function call to another module
// (if the module is imported and the function is public)
use other_package.amm

fn swap(pool: Field, coin_in: Field) -> Field {
    other_package.amm.swap(pool, coin_in)
}
```

Shared objects are the mechanism for composability. An AMM pool is a
shared object that multiple users can interact with concurrently
(consensus-ordered).

### Events

Sui events are typed and emitted via the event module:

```
event Transfer { from: Field, to: Field, amount: Field }

// reveal compiles to event::emit
reveal Transfer { from: sender, to: recipient, amount: value }
```

`reveal` maps to `sui::event::emit`. `seal` emits only the commitment
hash as event data.

---

## Portable Alternative (`os.*`)

Programs that don't need Sui-specific features can use `os.*`
instead of `sui.ext.*` for cross-chain portability:

| `sui.ext.*` (this OS only) | `os.*` (any OS) |
|----------------------------|---------------------|
| `sui.ext.dynamic_field.borrow(id, key)` | `os.state.read(key)` → dynamic_field.borrow |
| `sui.ext.tx.sender()` | `os.neuron.id()` → tx_context::sender |
| `sui.ext.coin.split()` + `sui.ext.transfer.public_send()` | `os.signal.send(from, to, amt)` → split + public_transfer |
| `sui.ext.tx.epoch_timestamp_ms()` | `os.time.now()` → epoch_timestamp_ms |

Use `sui.ext.*` when you need: object ownership (owned/shared/frozen),
dynamic fields, capability pattern, or other Sui-specific features. See
[os.md](../../reference/os.md) for the full `os.*` API.

---

## Ecosystem Mapping

| Move/Sui concept | Trident equivalent |
|---|---|
| `module my_package::my_token` | `program my_token` with `use sui.ext.*` |
| `fun init(ctx: &mut TxContext)` | `fn init()` |
| `public entry fun transfer(...)` | `pub fn transfer(...)` |
| `public fun balance(coin): u64` | `pub fn balance(coin: Field) -> Field` |
| `tx_context::sender(ctx)` | `sui.ext.tx.sender()` |
| `tx_context::epoch(ctx)` | `sui.ext.tx.epoch()` |
| `object::new(ctx)` | `sui.ext.object.new()` |
| `object::delete(id)` | `sui.ext.object.delete(id)` |
| `transfer::transfer(obj, recipient)` | `sui.ext.transfer.send(obj, recipient)` |
| `transfer::public_transfer(obj, recipient)` | `sui.ext.transfer.public_send(obj, recipient)` |
| `transfer::share_object(obj)` | `sui.ext.transfer.share(obj)` |
| `transfer::freeze_object(obj)` | `sui.ext.transfer.freeze(obj)` |
| `dynamic_field::add(parent, key, val)` | `sui.ext.dynamic_field.add(parent, key, val)` |
| `dynamic_field::borrow(parent, key)` | `sui.ext.dynamic_field.borrow(parent, key)` |
| `dynamic_field::remove(parent, key)` | `sui.ext.dynamic_field.remove(parent, key)` |
| `coin::value(coin)` | `sui.ext.coin.value(coin)` |
| `coin::split(coin, amount, ctx)` | `sui.ext.coin.split(coin, amount)` |
| `coin::join(target, source)` | `sui.ext.coin.merge(target, source)` |
| `coin::zero(ctx)` | `sui.ext.coin.zero()` |
| `event::emit(MyEvent { ... })` | `reveal MyEvent { ... }` |
| `assert!(condition, ERROR_CODE)` | `assert(condition)` |

---

## `sui.ext.*` API Reference

| Module | Function | Signature | Description |
|--------|----------|-----------|-------------|
| object | `new()` | `-> Field` | Create new object UID |
| | `delete(id)` | `Field -> ()` | Delete object |
| | `borrow(id, offset)` | `(Field, U32) -> Field` | Read object field |
| | `borrow_mut(id, offset, val)` | `(Field, U32, Field) -> ()` | Write object field |
| | `id(obj)` | `Field -> Digest` | Get object ID |
| transfer | `send(obj, recipient)` | `(Field, Field) -> ()` | Transfer owned object |
| | `public_send(obj, recipient)` | `(Field, Field) -> ()` | Transfer with store ability |
| | `share(obj)` | `Field -> ()` | Make object shared |
| | `freeze(obj)` | `Field -> ()` | Make object immutable |
| dynamic_field | `add(parent, key, val)` | `(Field, Field, Field) -> ()` | Add dynamic field |
| | `borrow(parent, key)` | `(Field, Field) -> Field` | Read dynamic field |
| | `borrow_mut(parent, key, val)` | `(Field, Field, Field) -> ()` | Write dynamic field |
| | `remove(parent, key)` | `(Field, Field) -> Field` | Remove dynamic field |
| | `exists(parent, key)` | `(Field, Field) -> Bool` | Check field existence |
| tx | `sender()` | `-> Field` | Transaction sender |
| | `epoch()` | `-> Field` | Current epoch |
| | `epoch_timestamp_ms()` | `-> Field` | Epoch timestamp (ms) |
| coin | `value(coin)` | `Field -> Field` | Coin balance |
| | `split(coin, amount)` | `(Field, Field) -> Field` | Split coin |
| | `merge(target, source)` | `(Field, Field) -> ()` | Merge coins |
| | `zero()` | `-> Field` | Zero-value coin |
| | `mint(cap, amount)` | `(Field, Field) -> Field` | Mint new coins |
| | `burn(cap, coin)` | `(Field, Field) -> ()` | Burn coins |
| event | `emit(type, data)` | `(Field, [Field]) -> ()` | Emit typed event |

---

## Notes

Sui's object model maps naturally to Trident's value semantics -- all
values are copied, not referenced. Move's linear type system (resources
cannot be copied or dropped) is enforced by the Sui runtime, not by
Trident's type checker. The compiler emits the correct Move bytecode
with the appropriate abilities (copy, drop, store, key).

Parallel execution: transactions on owned objects bypass consensus entirely.
This is why Sui achieves high throughput for transfers and simple operations.
Only shared-object transactions require ordering.

For VM details, see [movevm.md](../../vm/movevm/README.md).
