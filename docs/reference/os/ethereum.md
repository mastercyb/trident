# Ethereum

[← Target Reference](../targets.md) | VM: [EVM](../vm/evm.md)

Ethereum is the canonical EVM chain -- L1 settlement layer. Trident compiles
to EVM bytecode (`.evm`) and links against `ext.ethereum.*` for Ethereum-
specific runtime bindings. Same bytecode runs on all EVM-compatible chains
with different `ext.*` bindings.

---

## Runtime Parameters

| Parameter | Value |
|---|---|
| VM | EVM |
| Runtime binding | `ext.ethereum.*` |
| Account model | Account |
| Storage model | Key-value (SLOAD/SSTORE) |
| Transaction model | Signed (ECDSA) |
| Cost model | Gas |
| Cross-chain | -- (canonical L1) |

---

## Programming Model

### Entry Points

Ethereum programs are **contracts** -- deployed bytecode with exported
functions. A contract has:

- **Constructor** -- runs once at deployment, initializes storage
- **External functions** -- callable by transactions or other contracts
- **View functions** -- read-only, no state mutation

```
program my_token

use ext.ethereum.storage
use ext.ethereum.account

// Constructor: called once at deployment
#[constructor]
fn init(supply: Field) {
    let deployer: Field = ext.ethereum.account.caller()
    ext.ethereum.storage.write(0, supply)           // slot 0 = total supply
    ext.ethereum.storage.write_map(1, deployer, supply)  // slot 1 = balances mapping
}

// External function: transfer tokens
pub fn transfer(to: Field, amount: Field) {
    let sender: Field = ext.ethereum.account.caller()
    let sender_bal: Field = ext.ethereum.storage.read_map(1, sender)
    let to_bal: Field = ext.ethereum.storage.read_map(1, to)

    assert(sender_bal >= amount)
    ext.ethereum.storage.write_map(1, sender, sub(sender_bal, amount))
    ext.ethereum.storage.write_map(1, to, to_bal + amount)

    reveal Transfer { from: sender, to: to, amount: amount }
}

// View function: read balance
#[view]
pub fn balance_of(owner: Field) -> Field {
    ext.ethereum.storage.read_map(1, owner)
}
```

### State Access

Ethereum contracts have persistent key-value storage. Each contract has
2^256 storage slots, accessed by SLOAD/SSTORE.

```
use ext.ethereum.storage

// Direct slot access
let value: Field = ext.ethereum.storage.read(slot)
ext.ethereum.storage.write(slot, value)

// Mapping access (Solidity-style: keccak256(key . slot))
let bal: Field = ext.ethereum.storage.read_map(slot, key)
ext.ethereum.storage.write_map(slot, key, value)

// Nested mapping (keccak256(key2 . keccak256(key1 . slot)))
let allowance: Field = ext.ethereum.storage.read_map2(slot, owner, spender)
ext.ethereum.storage.write_map2(slot, owner, spender, value)
```

Storage layout follows Solidity conventions: slot 0 is the first declared
variable, mappings use `keccak256(key . slot)` for index computation.
The compiler handles the encoding automatically.

### Identity and Authorization

Ethereum provides protocol-level identity via transaction signatures.
The EVM injects the caller's address before the program runs.

```
use ext.ethereum.account

let caller: Field = ext.ethereum.account.caller()      // msg.sender
let origin: Field = ext.ethereum.account.origin()       // tx.origin
let self_addr: Field = ext.ethereum.account.self_address()  // address(this)

// Ownership check
assert(caller == owner)
```

**No reentrancy.** Trident programs are sequential with bounded loops.
There is no callback mechanism, no fallback function, no way for an
external call to re-enter the current contract mid-execution.

### Value Transfer

ETH transfers use the native transfer mechanism:

```
use ext.ethereum.transfer
use ext.ethereum.account

let my_balance: Field = ext.ethereum.account.balance(self_addr)
ext.ethereum.transfer.send(recipient, amount)
```

ERC-20 token operations are implemented as contract calls (see
Cross-Contract Interaction below).

### Cross-Contract Interaction

The EVM supports several call types:

```
use ext.ethereum.call

// Regular call (can transfer ETH + call function)
let result: [Field; N] = ext.ethereum.call.call(
    target_address, value, calldata
)

// Static call (read-only, reverts on state change)
let result: [Field; N] = ext.ethereum.call.static_call(
    target_address, calldata
)

// Delegate call (runs target code in caller's storage context)
let result: [Field; N] = ext.ethereum.call.delegate_call(
    target_address, calldata
)

// Return data from last call
let data: [Field; N] = ext.ethereum.call.return_data()
```

### Events

EVM events use LOG0-LOG4 opcodes with indexed topics:

```
event Transfer { from: Field, to: Field, amount: Field }
event Approval { owner: Field, spender: Field, amount: Field }

// reveal compiles to LOG with indexed topics
reveal Transfer { from: sender, to: receiver, amount: value }
```

`reveal` maps to LOG opcodes. `seal` has no native EVM equivalent --
the compiler emits only the commitment hash as a LOG topic.

---

## Portable Alternative (`std.os.*`)

Programs that don't need Ethereum-specific features can use `std.os.*`
instead of `ext.ethereum.*` for cross-chain portability:

| `ext.ethereum.*` (this OS only) | `std.os.*` (any OS) |
|---------------------------------|---------------------|
| `ext.ethereum.storage.read(slot)` | `std.os.state.read(key)` → SLOAD |
| `ext.ethereum.account.caller()` | `std.os.neuron.id()` → msg.sender (padded to Digest) |
| `ext.ethereum.transfer.send(to, amt)` | `std.os.signal.send(from, to, amt)` → CALL with value (self) / transferFrom (delegated) |
| `ext.ethereum.block.timestamp()` | `std.os.time.now()` → block.timestamp |

Use `ext.ethereum.*` when you need: precompiles, delegatecall, specific
LOG topics, storage maps, or other EVM-specific features. See
[stdlib.md](../stdlib.md) for the full `std.os.*` API.

---

## Ecosystem Mapping

| Solidity concept | Trident equivalent |
|---|---|
| `contract MyToken { }` | `program my_token` with `use ext.ethereum.*` |
| `constructor(uint supply)` | `#[constructor] fn init(supply: Field)` |
| `function transfer() external` | `pub fn transfer()` |
| `function balanceOf() view` | `#[view] pub fn balance_of()` |
| `msg.sender` | `ext.ethereum.account.caller()` |
| `tx.origin` | `ext.ethereum.account.origin()` |
| `address(this)` | `ext.ethereum.account.self_address()` |
| `address(this).balance` | `ext.ethereum.account.balance(self_addr)` |
| `mapping(address => uint)` | `ext.ethereum.storage.read_map(slot, key)` |
| `SLOAD(slot)` | `ext.ethereum.storage.read(slot)` |
| `SSTORE(slot, val)` | `ext.ethereum.storage.write(slot, value)` |
| `payable.transfer(amount)` | `ext.ethereum.transfer.send(to, amount)` |
| `target.call(data)` | `ext.ethereum.call.call(target, value, data)` |
| `target.staticcall(data)` | `ext.ethereum.call.static_call(target, data)` |
| `target.delegatecall(data)` | `ext.ethereum.call.delegate_call(target, data)` |
| `emit Transfer(from, to, amount)` | `reveal Transfer { from, to, amount }` |
| `block.number` | `ext.ethereum.block.number()` |
| `block.timestamp` | `ext.ethereum.block.timestamp()` |
| `block.coinbase` | `ext.ethereum.block.coinbase()` |
| `block.basefee` | `ext.ethereum.block.base_fee()` |
| `block.chainid` | `ext.ethereum.block.chain_id()` |
| `tx.gasprice` | `ext.ethereum.tx.gas_price()` |
| `gasleft()` | `ext.ethereum.tx.gas_remaining()` |
| `require(cond, "msg")` | `assert(cond)` (no error messages -- revert or succeed) |
| `revert("msg")` | `assert(false)` |
| `ecrecover(hash, v, r, s)` | `ext.ethereum.precompile.ecrecover(hash, v, r, s)` |
| `keccak256(data)` | `hash(...)` (uses VM-native hash on Triton; Keccak on EVM) |

---

## `ext.ethereum.*` API Reference

| Module | Function | Signature | Description |
|--------|----------|-----------|-------------|
| **storage** | `read(slot)` | `Field -> Field` | SLOAD |
| | `write(slot, val)` | `(Field, Field) -> ()` | SSTORE |
| | `read_map(slot, key)` | `(Field, Field) -> Field` | Mapping read |
| | `write_map(slot, key, val)` | `(Field, Field, Field) -> ()` | Mapping write |
| | `read_map2(slot, k1, k2)` | `(Field, Field, Field) -> Field` | Nested mapping read |
| | `write_map2(slot, k1, k2, val)` | `(Field, Field, Field, Field) -> ()` | Nested mapping write |
| **account** | `caller()` | `-> Field` | msg.sender |
| | `origin()` | `-> Field` | tx.origin |
| | `self_address()` | `-> Field` | address(this) |
| | `balance(addr)` | `Field -> Field` | Address balance |
| **transfer** | `send(to, amount)` | `(Field, Field) -> ()` | ETH transfer |
| **call** | `call(addr, value, data)` | `(Field, Field, [Field]) -> [Field]` | External call |
| | `static_call(addr, data)` | `(Field, [Field]) -> [Field]` | Read-only call |
| | `delegate_call(addr, data)` | `(Field, [Field]) -> [Field]` | Delegated call |
| | `return_data()` | `-> [Field]` | Last call return data |
| **event** | `log(topics, data)` | `([Field], [Field]) -> ()` | Raw LOG |
| **block** | `number()` | `-> Field` | block.number |
| | `timestamp()` | `-> Field` | block.timestamp |
| | `coinbase()` | `-> Field` | block.coinbase |
| | `base_fee()` | `-> Field` | block.basefee |
| | `chain_id()` | `-> Field` | block.chainid |
| **tx** | `gas_price()` | `-> Field` | tx.gasprice |
| | `gas_remaining()` | `-> Field` | gasleft() |
| **precompile** | `ecrecover(hash, v, r, s)` | `(Field, Field, Field, Field) -> Field` | ECDSA recovery |
| | `sha256(data)` | `[Field] -> Digest` | SHA-256 precompile |

---

## Notes

The EVM is a 256-bit stack machine. Trident's field elements are mapped to
u256 values. `Field` arithmetic becomes modular arithmetic over the EVM's
native 256-bit word. Storage layout follows Solidity conventions for
compatibility with existing tooling (etherscan, foundry, hardhat).

For VM details, see [evm.md](../vm/evm.md).
For mental model migration from Solidity, see
[For Blockchain Devs](../../tutorials/for-blockchain-devs.md).
