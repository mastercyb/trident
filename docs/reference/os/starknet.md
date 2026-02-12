# Starknet

[← Target Reference](../targets.md) | VM: [Cairo VM](../vm/cairo.md)

Starknet is the STARK-based Ethereum L2 powered by the Cairo VM. Trident
compiles to Sierra (`.sierra`) and links against `ext.starknet.*` for
Starknet-specific runtime bindings. Starknet features native account
abstraction -- every account is a smart contract.

---

## Runtime Parameters

| Parameter | Value |
|---|---|
| VM | Cairo VM |
| Runtime binding | `ext.starknet.*` |
| Account model | Account (native account abstraction) |
| Storage model | Key-value |
| Transaction model | Signed (Stark curve) |
| Cost model | Steps + builtins |
| Cross-chain | Ethereum L2 (L1 messaging) |

---

## Programming Model

### Entry Points

Starknet contracts expose typed entry points categorized by mutability:

- **External** -- mutates state, callable by transactions and other contracts
- **View** -- read-only, no state mutation
- **Constructor** -- runs once at deployment
- **L1 handler** -- triggered by messages from Ethereum L1

```
program my_token

use ext.starknet.storage
use ext.starknet.account

// Constructor
#[constructor]
fn init(name: Field, supply: Field) {
    let deployer: Field = ext.starknet.account.caller()
    ext.starknet.storage.write(storage_var("total_supply"), supply)
    ext.starknet.storage.write_map(storage_var("balances"), deployer, supply)
}

// External function
pub fn transfer(to: Field, amount: Field) {
    let sender: Field = ext.starknet.account.caller()
    let sender_bal: Field = ext.starknet.storage.read_map(
        storage_var("balances"), sender
    )
    assert(sender_bal >= amount)

    ext.starknet.storage.write_map(
        storage_var("balances"), sender, sub(sender_bal, amount)
    )
    ext.starknet.storage.write_map(
        storage_var("balances"), to, sender_bal + amount
    )

    reveal Transfer { from: sender, to: to, amount: amount }
}

// View function
#[view]
pub fn balance_of(owner: Field) -> Field {
    ext.starknet.storage.read_map(storage_var("balances"), owner)
}

// L1 handler -- triggered by Ethereum L1 message
#[l1_handler]
fn handle_deposit(from_l1: Field, amount: Field) {
    let recipient: Field = ext.starknet.account.caller()
    // ... credit the deposit ...
}
```

### State Access

Starknet contracts have persistent key-value storage. Storage variables
are addressed by a Pedersen hash of the variable name. Mappings use
`h(h(var_name, key1), key2)` for nested keys.

```
use ext.starknet.storage

// Storage variable access
let var_addr: Field = storage_var("total_supply")
let supply: Field = ext.starknet.storage.read(var_addr)
ext.starknet.storage.write(var_addr, new_supply)

// Mapping access
let bal: Field = ext.starknet.storage.read_map(
    storage_var("balances"), owner
)
ext.starknet.storage.write_map(
    storage_var("balances"), owner, new_balance
)

// Nested mapping
let allowance: Field = ext.starknet.storage.read_map2(
    storage_var("allowances"), owner, spender
)
```

`storage_var("name")` computes `sn_keccak(name)` -- the Starknet
storage address convention.

### Identity and Authorization

Starknet has **native account abstraction** -- every account is a smart
contract that validates its own transactions. There is no privileged
signature scheme.

```
use ext.starknet.account

let caller: Field = ext.starknet.account.caller()           // get_caller_address
let contract: Field = ext.starknet.account.self_address()    // get_contract_address
let tx_info: Field = ext.starknet.account.tx_info()          // transaction info hash
```

Account contracts implement `__validate__` and `__execute__` entry points.
The protocol calls `__validate__` first (must succeed or transaction is
rejected), then `__execute__` runs the actual logic.

### Value Transfer

Starknet has no native transfer opcode. Token transfers are contract calls
to the ERC-20 contract:

```
use ext.starknet.call

// Transfer STRK tokens via ERC-20 contract call
ext.starknet.call.invoke(
    STRK_CONTRACT_ADDRESS,
    selector("transfer"),
    [recipient, amount_low, amount_high]   // u256 as two felts
)
```

### Cross-Contract Interaction

Starknet supports contract calls and library calls:

```
use ext.starknet.call

// Regular contract call
let result: [Field; N] = ext.starknet.call.invoke(
    contract_address,
    selector("function_name"),
    calldata
)

// Library call (runs code in caller's context, like delegatecall)
let result: [Field; N] = ext.starknet.call.library_call(
    class_hash,
    selector("function_name"),
    calldata
)

// Deploy a new contract
let deployed_address: Field = ext.starknet.call.deploy(
    class_hash,
    constructor_calldata,
    salt
)
```

**L1/L2 messaging** -- send messages to Ethereum L1:

```
use ext.starknet.messaging

// Send message to L1 contract
ext.starknet.messaging.send_to_l1(l1_address, payload)
```

### Events

Starknet events use indexed keys and unindexed data:

```
event Transfer { from: Field, to: Field, amount: Field }

// reveal compiles to emit_event with indexed keys
reveal Transfer { from: sender, to: receiver, amount: value }
```

`reveal` maps to `emit_event`. `seal` emits only the Pedersen hash
of the event data as a key.

---

## Portable Alternative (`std.os.*`)

Programs that don't need Starknet-specific features can use `std.os.*`
instead of `ext.starknet.*` for cross-chain portability:

| `ext.starknet.*` (this OS only) | `std.os.*` (any OS) |
|---------------------------------|---------------------|
| `ext.starknet.storage.read(addr)` | `std.os.state.read(key)` → storage_var read |
| `ext.starknet.account.caller()` | `std.os.neuron.id()` → get_caller_address |
| `ext.starknet.call.invoke(addr, sel, args)` | No portable equivalent (cross-contract is OS-specific) |
| `ext.starknet.messaging.send_to_l1(to, data)` | No portable equivalent (L1/L2 messaging is OS-specific) |

Use `ext.starknet.*` when you need: L1/L2 messaging, library calls,
Pedersen-addressed storage vars, or other Starknet-specific features. See
[stdlib.md](../stdlib.md) for the full `std.os.*` API.

---

## Ecosystem Mapping

| Cairo/Starknet concept | Trident equivalent |
|---|---|
| `#[starknet::contract] mod MyToken` | `program my_token` with `use ext.starknet.*` |
| `#[constructor]` | `#[constructor] fn init(...)` |
| `#[external(v0)]` | `pub fn function_name(...)` |
| `#[view]` | `#[view] pub fn function_name(...)` |
| `#[l1_handler]` | `#[l1_handler] fn handle_*(...)` |
| `get_caller_address()` | `ext.starknet.account.caller()` |
| `get_contract_address()` | `ext.starknet.account.self_address()` |
| `get_tx_info()` | `ext.starknet.account.tx_info()` |
| `get_block_number()` | `ext.starknet.account.block_number()` |
| `get_block_timestamp()` | `ext.starknet.account.block_timestamp()` |
| `@storage_var fn balance(addr) -> felt252` | `ext.starknet.storage.read_map(storage_var("balance"), addr)` |
| `self.balance.read(addr)` | `ext.starknet.storage.read_map(var, addr)` |
| `self.balance.write(addr, val)` | `ext.starknet.storage.write_map(var, addr, val)` |
| `IMyContract::transfer(addr, args)` | `ext.starknet.call.invoke(addr, selector, args)` |
| `library_call(class_hash, ...)` | `ext.starknet.call.library_call(hash, selector, args)` |
| `deploy_syscall(...)` | `ext.starknet.call.deploy(hash, calldata, salt)` |
| `send_message_to_l1(to, payload)` | `ext.starknet.messaging.send_to_l1(addr, payload)` |
| `self.emit(Transfer { ... })` | `reveal Transfer { ... }` |
| `selector!("function_name")` | `selector("function_name")` (sn_keccak) |
| `assert(condition, 'error')` | `assert(condition)` |

---

## `ext.starknet.*` API Reference

| Module | Function | Signature | Description |
|--------|----------|-----------|-------------|
| **storage** | `read(addr)` | `Field -> Field` | Read storage variable |
| | `write(addr, val)` | `(Field, Field) -> ()` | Write storage variable |
| | `read_map(addr, key)` | `(Field, Field) -> Field` | Read mapping entry |
| | `write_map(addr, key, val)` | `(Field, Field, Field) -> ()` | Write mapping entry |
| | `read_map2(addr, k1, k2)` | `(Field, Field, Field) -> Field` | Read nested mapping |
| | `write_map2(addr, k1, k2, val)` | `(Field, Field, Field, Field) -> ()` | Write nested mapping |
| **account** | `caller()` | `-> Field` | get_caller_address |
| | `self_address()` | `-> Field` | get_contract_address |
| | `tx_info()` | `-> Field` | Transaction info hash |
| | `block_number()` | `-> Field` | Current block number |
| | `block_timestamp()` | `-> Field` | Current block timestamp |
| **call** | `invoke(addr, selector, args)` | `(Field, Field, [Field]) -> [Field]` | Contract call |
| | `library_call(hash, selector, args)` | `(Field, Field, [Field]) -> [Field]` | Library call |
| | `deploy(hash, args, salt)` | `(Field, [Field], Field) -> Field` | Deploy contract |
| **event** | `emit(keys, data)` | `([Field], [Field]) -> ()` | Raw event emission |
| **messaging** | `send_to_l1(addr, payload)` | `(Field, [Field]) -> ()` | L1 message |
| **crypto** | `pedersen(a, b)` | `(Field, Field) -> Field` | Pedersen hash |
| | `poseidon(data)` | `[Field] -> Field` | Poseidon hash |

---

## Notes

Starknet is the only chain with native account abstraction -- every account
is a contract, and any signature scheme can be used for transaction
validation. This aligns naturally with Trident's hash-preimage
authorization pattern.

Proving happens on Starknet's sequencer (not client-side). The compiler
outputs Sierra (Safe Intermediate Representation) which the sequencer
JIT-compiles to Cairo assembly for execution.

For VM details, see [cairo.md](../vm/cairo.md).
For mental model migration from smart contracts, see
[For Blockchain Devs](../../tutorials/for-blockchain-devs.md).
