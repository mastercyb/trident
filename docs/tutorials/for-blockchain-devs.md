# From Smart Contracts to Zero-Knowledge Programs

You know how to write smart contracts. Trident programs look similar but work
fundamentally differently. This guide maps your existing mental model -- whether
it comes from Solidity, Vyper, Anchor, CosmWasm, or Substrate -- to the
zero-knowledge paradigm.

Trident compiles to multiple STARK-based zero-knowledge virtual machines.
The primary target is [Triton VM](https://triton-vm.org/) via
[TASM](https://triton-vm.org/spec/), but the same source code can target other
backends without modification. The result is not a contract deployed on-chain.
It is a program that runs locally, produces a cryptographic proof, and lets
anyone verify that proof in milliseconds without re-executing anything.

---

## The Paradigm Shift

| Smart Contract (EVM, SVM, CosmWasm) | ZK Program (Trident / Triton VM) |
|--------------------------------------|----------------------------------|
| Runs on a blockchain VM | Runs locally on the prover's machine |
| State persists between calls | Each proof is standalone -- no persistent state |
| Everyone sees execution (all calldata, storage, logs) | Only the prover sees execution (zero-knowledge) |
| Gas metering at runtime | Proving cost computed at compile time |
| Revert on failure, gas consumed | Assertion failure = no proof generated, nothing consumed |
| Verifier re-executes the transaction | Verifier checks a STARK proof (milliseconds, constant cost) |
| Deployed bytecode lives on-chain | Program is identified by its Tip5 hash |
| Upgradeable via proxy patterns | Config commitments with admin auth; code hash is identity |
| Locked to one VM (EVM, SVM, CairoVM) | Multi-target: same source compiles to Triton VM, Miden VM, etc. |
| Security from elliptic curves (secp256k1, ed25519) | Security from hash functions only (quantum-safe) |

The deepest shift: a smart contract is *imperative middleware* that mutates
shared state. A ZK program is a *claim about computation* -- "I ran this
program on these inputs and got these outputs, and here is the proof."

---

## Write Once, Prove Anywhere

Smart contracts are locked to a single VM. A Solidity contract runs on the EVM.
A CosmWasm contract runs on Cosmos chains. If you want to target a different
chain, you rewrite the program from scratch in a different language.

Trident separates the **program** from the **proving backend**. The same source
compiles to multiple STARK VMs:

| Layer | What It Contains | Example |
|-------|-----------------|---------|
| **Universal core** | Types, control flow, I/O, hashing, Merkle proofs | `std.crypto.hash`, `std.crypto.merkle`, `std.io.io` |
| **Backend extensions** | Target-specific intrinsics | `ext.triton.xfield`, `ext.triton.kernel` |

Programs that use only `std.*` modules are **fully portable** -- they compile
and generate valid proofs on any supported backend. Programs that import
`ext.*` modules are backend-specific and will only compile for that target.

```
// Portable program -- compiles to any backend
use std.crypto.merkle
use std.io.io

fn verify_membership(root: Digest, leaf: Digest, index: U32, depth: U32) {
    std.crypto.merkle.verify(root, leaf, index, depth)
}
```

```
// Triton-specific program -- uses extension field arithmetic
use std.crypto.merkle
use ext.triton.xfield           // binds to Triton VM backend

fn verify_with_xfield(root: Digest) {
    // ext.triton.xfield provides extension field operations
    // that map directly to Triton VM instructions
}
```

```bash
# Compile for the default target (Triton VM)
trident build program.tri -o program.tasm

# Compile for a different target
trident build program.tri --target miden -o program.masm
```

**Why this matters for blockchain developers**: You write your proof logic once.
If you later need to generate proofs on a different VM (because of cost,
performance, or ecosystem requirements), you recompile -- you do not rewrite.
The cryptographic guarantees are identical across targets because the same
source code defines the same computation.

For the full architecture, see the [Universal Design](universal-design.md)
document. For backend extension authoring, see the `ext/` directory.

---

## Where's My State?

### EVM / Solidity

State lives in 256-bit storage slots. Mappings are `keccak256(key . slot)`.
Every node stores the full state trie. Anyone can read any slot.

```solidity
// Solidity
mapping(address => uint256) public balances;

function getBalance(address who) view returns (uint256) {
    return balances[who];
}
```

### SVM / Anchor

State lives in accounts -- byte buffers owned by programs. You pass accounts
into instructions and deserialize them.

### CosmWasm

State lives in a key-value store (`deps.storage`). You read/write with
`load`/`save` on typed `Item` and `Map` wrappers.

### Trident

There are no storage slots, no accounts, no key-value store. State is a
**Merkle tree commitment** -- a single hash (the root) that represents the
entire state. The prover knows the full tree; the verifier only sees the root.

To read state, the prover **divines** (secretly inputs) the leaf data and then
**authenticates** it against the root using a Merkle proof. This is the
divine-and-authenticate pattern (see [Programming Model](programming-model.md) for the full treatment):

```
// Trident — read an account from the state tree
use std.io.io
use std.crypto.hash

let state_root: Digest = std.io.io.pub_read5()    // verifier provides the root

let account_id: Field = std.io.io.divine()         // prover secretly inputs the data
let balance: Field = std.io.io.divine()
let nonce: Field = std.io.io.divine()
let auth_hash: Field = std.io.io.divine()
let lock_until: Field = std.io.io.divine()

// Hash the leaf and prove it belongs to the tree
let leaf: Digest = std.crypto.hash.tip5(account_id, balance, nonce,
                                        auth_hash, lock_until,
                                        0, 0, 0, 0, 0)
// Merkle proof authenticates leaf against root
// (the sibling hashes are also divined and verified internally)
```

The verifier never sees `account_id`, `balance`, or any leaf data. It only
sees `state_root` and the proof that the program executed correctly.

**Key insight**: In Solidity, state is *read from on-chain storage*. In
Trident, state is *claimed by the prover and cryptographically verified*.

### Side-by-Side: Token Balance Lookup

```solidity
// Solidity
function balanceOf(address who) view returns (uint256) {
    return balances[who];  // storage read
}
```

```
// Trident
use std.io.io
use std.crypto.hash

fn load_account() -> (Field, Field, Field, Field, Field) {
    let id: Field = std.io.io.divine()
    let bal: Field = std.io.io.divine()
    let nonce: Field = std.io.io.divine()
    let auth: Field = std.io.io.divine()
    let lock: Field = std.io.io.divine()
    // prove this data is in the state tree (Merkle proof)
    let leaf: Digest = std.crypto.hash.tip5(id, bal, nonce, auth, lock,
                                            0, 0, 0, 0, 0)
    (id, bal, nonce, auth, lock)
}
```

---

## Where's My msg.sender?

### EVM / Solidity

`msg.sender` is implicit -- injected by the EVM based on the transaction
signature. ECDSA verification happens at the protocol level.

```solidity
// Solidity
function withdraw() external {
    require(msg.sender == owner, "not owner");
    // ...
}
```

### SVM / Anchor

The `Signer` account constraint checks that the transaction was signed by the
corresponding private key. The runtime enforces it before your program runs.

### CosmWasm

`info.sender` is provided by the runtime in the `MessageInfo` struct.

### Trident

There is no `msg.sender`. There is no implicit identity. Authorization is
explicit: the prover **divines** a secret and proves knowledge of it by hashing
it and asserting the hash matches an expected value.

```
// Trident — authorization via hash preimage
use std.io.io
use std.crypto.hash
use std.core.assert

fn verify_auth(auth_hash: Field) {
    let secret: Field = std.io.io.divine()             // prover inputs secret
    let computed: Digest = std.crypto.hash.tip5(secret, 0, 0, 0, 0,
                                                0, 0, 0, 0, 0)
    let (h0, _, _, _, _) = computed
    std.core.assert.assert_eq(auth_hash, h0)           // must match stored hash
}
```

The verifier never sees `secret`. It only knows the proof is valid, which means
*someone who knew the preimage of `auth_hash`* ran this program.

**This is account abstraction by default.** The "secret" can be anything:
- A private key
- A Shamir secret share (threshold multisig)
- A biometric hash
- The output of another ZK proof (recursive verification)
- A hardware security module attestation

There is no privileged key type. No secp256k1, no ed25519. Just hash preimages.

### Side-by-Side: Access Control

```solidity
// Solidity — Ownable pattern
address public owner;

modifier onlyOwner() {
    require(msg.sender == owner, "not owner");
    _;
}

function setConfig(uint256 val) external onlyOwner {
    config = val;
}
```

```
// Trident — admin auth pattern
use std.io.io

fn update() {
    let old_config: Digest = std.io.io.pub_read5()
    let new_config: Digest = std.io.io.pub_read5()

    let admin_auth: Field = std.io.io.divine()    // divine the admin auth hash
    // ... divine and verify full config ...

    verify_auth(admin_auth)                       // prove knowledge of admin secret

    // ... verify new config commitment ...
}
```

---

## Where's My Gas?

### EVM / Solidity

Gas is metered per opcode at runtime. You estimate it before sending. Unused gas
is refunded. A function that *might* loop 1000 times costs 1000-iterations of
gas even if you are just estimating.

### SVM / Anchor

Compute units, similar model. Metered at runtime, capped per transaction.

### CosmWasm / Substrate

Gas (CosmWasm) or weight (Substrate). Runtime metering with per-call limits.

### Trident

There is no runtime metering. Proving cost is determined by six execution
tables in Triton VM:

| Table | What It Measures |
|-------|-----------------|
| **Processor** | Clock cycles (instructions executed) |
| **Hash** | Hash coprocessor rows (6 per `hash` / `tip5` call) |
| **U32** | Range checks, bitwise operations (`as_u32`, `split`, `&`) |
| **Op Stack** | Operand stack underflow handling |
| **RAM** | Memory read/write operations |
| **Jump Stack** | Function call/return, branching overhead |

The **tallest table** determines the actual STARK proving cost. All tables are
padded to the next power of 2. This means:

**The power-of-2 cliff**: If your tallest table has 1025 rows, it pads to 2048.
If it had 1024 rows, it pads to 1024. That one extra instruction *doubled*
your proving cost. This is the single most important cost concept in ZK
programming.

Cost is known at **compile time** because all loops have bounded iteration
counts and there is no dynamic dispatch. See [How STARK Proofs Work](stark-proofs.md) Section 4 for why there are exactly six tables, and the [Optimization Guide](optimization.md) for cost reduction strategies.

```bash
# See the cost breakdown
trident build token.tri --costs

# See which functions are most expensive
trident build token.tri --hotspots

# See per-line cost annotations
trident build token.tri --annotate

# Save and compare costs across changes
trident build token.tri --save-costs before.json
# ... make changes ...
trident build token.tri --compare before.json
```

### Side-by-Side: Cost Estimation

```solidity
// Solidity — estimate gas at runtime
uint256 gasStart = gasleft();
doWork();
uint256 gasUsed = gasStart - gasleft();
// You don't know until you run it
```

```bash
# Trident — cost known before execution
$ trident build token.tri --costs
# Processor:  3,847 rows (padded: 4,096)
# Hash:       2,418 rows (padded: 4,096)  <-- dominant
# U32:          312 rows (padded: 4,096)
# Op Stack:     891 rows (padded: 4,096)
# RAM:          604 rows (padded: 4,096)
# Jump Stack:   203 rows (padded: 4,096)
# Padded height: 4,096
```

---

## Where's My Revert?

### EVM / Solidity

```solidity
require(balance >= amount, "insufficient balance");
revert("something went wrong");
// try/catch for external calls
```

Revert unwinds state changes but consumes gas up to that point.

### SVM / Anchor

`require!()` macro, or return `Err(ErrorCode::...)`. State changes are rolled
back but compute units are consumed.

### CosmWasm

Return `Err(ContractError::...)`. Atomic rollback.

### Trident

```
assert(balance >= amount)
```

If the assertion fails, the VM halts. No proof is generated. There is no
partial execution, no state to roll back (because state was never mutated --
it was proven). There is no gas cost for failure (there is no gas).

**No partial failure.** Either the entire proof succeeds and every assertion
holds, or nothing happens. There is no try/catch because there is nothing to
catch -- a failed assertion means the computation is invalid and no proof
exists.

```
// Trident — range check pattern (balance >= amount)
use std.core.convert
use std.core.field

fn assert_non_negative(val: Field) {
    let checked: U32 = std.core.convert.as_u32(val)   // fails if val > 2^32 or negative in field
}

let new_balance: Field = std.core.field.sub(balance, amount)
assert_non_negative(new_balance)                       // no proof if balance < amount
```

The `as_u32()` conversion is how Trident checks that a field element is in a
safe range. If `sub(balance, amount)` wraps around in the prime field (because
`amount > balance`), the result is a huge number that fails the U32 range
check.

---

## Where's My Event?

### EVM / Solidity

```solidity
event Transfer(address indexed from, address indexed to, uint256 amount);

function transfer(address to, uint256 amount) external {
    // ...
    emit Transfer(msg.sender, to, amount);
}
```

Events are logged on-chain. Anyone can read them. Indexers watch for them.

### Trident

Two kinds of events:

**`emit` -- open events** (like Solidity events). All fields visible to the verifier:

```
event Transfer {
    from: Digest,
    to: Digest,
    amount: Field,
}

fn pay() {
    // ...
    emit Transfer { from: sender, to: receiver, amount: value }
}
```

**`seal` -- sealed events** (no EVM equivalent). Fields are hashed; only the
digest is visible to the verifier. The verifier knows *an event happened* but
cannot read its contents:

```
event Nullifier {
    account_id: Field,
    nonce: Field,
}

fn pay() {
    // ...
    seal Nullifier { account_id: s_id, nonce: s_nonce }
}
```

Sealed events are uniquely ZK. They enable privacy-preserving audit trails:
the verifier can confirm that a nullifier was emitted (preventing double-spend)
without learning which account was involved.

---

## Pattern Translation Table

### 1. ERC-20 Transfer --> Token Pay Operation

```solidity
// Solidity
function transfer(address to, uint256 amount) external returns (bool) {
    require(balances[msg.sender] >= amount, "insufficient");
    balances[msg.sender] -= amount;
    balances[to] += amount;
    emit Transfer(msg.sender, to, amount);
    return true;
}
```

```
// Trident (simplified from fungible_token/token.tri)
use std.io.io
use std.core.field
use std.crypto.auth

fn pay() {
    let old_root: Digest = std.io.io.pub_read5()
    let new_root: Digest = std.io.io.pub_read5()
    let amount: Field = std.io.io.pub_read()

    // Divine and verify sender account from Merkle tree
    let s_bal: Field = std.io.io.divine()
    // ... authenticate against old_root ...

    verify_auth(s_auth)                               // prove ownership
    let new_s_bal: Field = std.core.field.sub(s_bal, amount)
    assert_non_negative(new_s_bal)                     // balance check

    // Divine and verify receiver, compute new leaves
    let new_r_bal: Field = r_bal + amount
    // ... verify new leaves produce new_root ...

    seal Nullifier { account_id: s_id, nonce: s_nonce }
    emit SupplyCheck { supply: supply }
}
```

### 2. Access Control (Ownable) --> Auth Hash Verification

```solidity
// Solidity
modifier onlyOwner() {
    require(msg.sender == owner, "not owner");
    _;
}
```

```
// Trident
use std.io.io
use std.crypto.hash
use std.core.assert

fn verify_auth(auth_hash: Field) {
    let secret: Field = std.io.io.divine()
    let computed: Digest = std.crypto.hash.tip5(secret, 0, 0, 0, 0,
                                                0, 0, 0, 0, 0)
    let (h0, _, _, _, _) = computed
    std.core.assert.assert_eq(auth_hash, h0)
}
```

### 3. Timelock --> lock_until Field Comparison

```solidity
// Solidity
require(block.timestamp >= unlockTime, "locked");
```

```
// Trident
use std.io.io
use std.core.field

let current_time: Field = std.io.io.pub_read()          // verifier provides timestamp
let time_diff: Field = std.core.field.sub(current_time, lock_until)
assert_non_negative(time_diff)                           // current_time >= lock_until
```

### 4. Mappings --> Merkle Tree Leaves

```solidity
// Solidity
mapping(address => uint256) public balances;
balances[user] = 100;
uint256 bal = balances[user];
```

```
// Trident — state is a Merkle tree, each "mapping entry" is a leaf
use std.crypto.hash

let leaf: Digest = std.crypto.hash.tip5(account_id, balance, nonce, auth, lock,
                                        0, 0, 0, 0, 0)
// Leaf membership proven via Merkle proof against state root
```

### 5. Constructor --> Program Constants / Config Commitment

```solidity
// Solidity
constructor(string memory name_, uint256 supply_) {
    name = name_;
    totalSupply = supply_;
}
```

```
// Trident — config is a hash commitment, provided as public input
use std.io.io

let config: Digest = std.io.io.pub_read5()
// Divine and verify config fields
let admin_auth: Field = std.io.io.divine()
let mint_auth: Field = std.io.io.divine()
// ... hash all fields and assert match ...
```

### 6. View Functions --> pub_write Outputs

```solidity
// Solidity
function balanceOf(address who) view returns (uint256) {
    return balances[who];
}
```

```
// Trident — prove a value and output it publicly
use std.io.io

fn balance_proof() {
    let root: Digest = std.io.io.pub_read5()
    let bal: Field = std.io.io.divine()
    // ... authenticate bal against root ...
    std.io.io.pub_write(bal)           // verifier sees the balance
}
```

### 7. require / revert --> assert

```solidity
// Solidity
require(amount > 0, "zero amount");
require(sender != receiver, "self-transfer");
```

```
// Trident
assert(amount > 0)
// Note: no error messages. Either proof exists or it does not.
```

### 8. block.timestamp --> pub_read (Public Input from Verifier)

```solidity
// Solidity
uint256 ts = block.timestamp;     // injected by EVM
```

```
// Trident
use std.io.io

let current_time: Field = std.io.io.pub_read()   // verifier provides the timestamp
// The verifier is responsible for providing the correct value.
// The program can authenticate it against a kernel MAST hash
// if running inside Neptune's transaction model.
```

### 9. Upgradeable Proxy --> Config Update with Admin Auth

```solidity
// Solidity (ERC-1967 proxy pattern)
function upgradeTo(address newImpl) external onlyOwner {
    _setImplementation(newImpl);
}
```

```
// Trident — config update operation (Op 2 in fungible token)
use std.io.io

fn update() {
    let old_config: Digest = std.io.io.pub_read5()
    let new_config: Digest = std.io.io.pub_read5()

    // Verify old config and authenticate admin
    let old_admin: Field = std.io.io.divine()
    // ... verify old config hash ...
    verify_auth(old_admin)             // prove admin knowledge

    // Verify new config is well-formed
    // ... verify new config hash ...

    // Setting admin_auth = 0 renounces forever (irreversible)
}
```

### 10. Token Mint / Burn --> Supply Accounting with Merkle Tree

```solidity
// Solidity
function mint(address to, uint256 amount) external onlyMinter {
    totalSupply += amount;
    balances[to] += amount;
}
```

```
// Trident
use std.io.io
use std.core.assert

fn mint() {
    let old_supply: Field = std.io.io.pub_read()
    let new_supply: Field = std.io.io.pub_read()
    let amount: Field = std.io.io.pub_read()

    verify_auth(cfg_mint_auth)                        // mint authority required

    let expected: Field = old_supply + amount
    std.core.assert.assert_eq(new_supply, expected)   // supply accounting

    // Update recipient leaf in Merkle tree
    let new_r_bal: Field = r_bal + amount
    // ... verify new Merkle root ...

    emit SupplyChange { old_supply: old_supply, new_supply: new_supply }
}
```

---

## What's New (No EVM Equivalent)

These concepts have no direct parallel in smart contract development:

### `divine()` -- Secret Witness Input

The prover can input arbitrary data that the verifier never sees. This is how
you feed private data into a proof. The program must verify any divined value
is legitimate (via hashing, Merkle proofs, or range checks).

```
use std.io.io

let secret: Field = std.io.io.divine()       // one field element, invisible to verifier
let preimage: Digest = std.io.io.divine5()   // five field elements (a Digest)
```

In EVM, all calldata is public. In Trident, `divine` is the default way to
input data, and `pub_read` is the exception for data the verifier must see.

### `seal` -- Privacy-Preserving Events

Emit an event where the verifier can confirm it happened but cannot read its
contents. Used for nullifiers, private audit trails, and compliance proofs.

```
seal Nullifier { account_id: s_id, nonce: s_nonce }
// Verifier sees: hash(account_id, nonce) -- not the actual values
```

### Bounded Loops

All iteration in Trident must have a compile-time upper bound. There is no
`while(true)`, no unbounded recursion, no dynamic dispatch. This guarantees
the execution trace has a known maximum size, which is what makes compile-time
cost analysis possible.

```
for i in 0..n bounded 100 {
    // Runs at most 100 iterations.
    // The compiler costs this as exactly 100 iterations,
    // even if n < 100 at runtime.
    process(i)
}
```

### Cost Annotations

Every Trident function has a deterministic proving cost. The compiler gives you
complete visibility:

```bash
trident build main.tri --costs       # full table breakdown
trident build main.tri --hotspots    # top 5 most expensive functions
trident build main.tri --annotate    # per-line cost annotations
trident build main.tri --hints       # optimization suggestions
```

No EVM toolchain gives you this level of cost certainty. In Solidity, gas
depends on storage state, calldata, and runtime conditions. In Trident, cost
is a pure function of the source code.

### Recursive Proof Verification

A Trident program can verify that another STARK proof is valid *inside* its
own execution. This enables proof composition: prove that transaction A is valid
and transaction B is valid in a single combined proof. Triton VM's native hash
instructions make this practical -- verifying a STARK proof costs ~600K cycles,
compared to millions in RISC-V based zkVMs.

### Quantum Safety

All cryptographic security in Triton VM comes from
[Tip5](https://eprint.iacr.org/2023/107) hash functions and
[FRI](https://eccc.weizmann.ac.il/report/2017/134/) commitments.
No elliptic curves anywhere. No secp256k1, no BN254, no BLS12-381. This
means proofs are resistant to quantum attacks without any migration needed.
See [How STARK Proofs Work](stark-proofs.md) Section 10 for the full
quantum safety argument.

In contrast, every EVM chain's security (transaction signatures, precompiles,
validator keys) depends on elliptic curves that a sufficiently powerful quantum
computer could break. See [Comparative Analysis](analysis.md) for how every
major ZK system scores on quantum safety.

---

## Mental Model Cheat Sheet

| You're used to... | In Trident, think... |
|---|---|
| "Deploy a contract" | "Publish the program hash" |
| "Call a function" | "Generate a proof" |
| "Read storage" | "Divine a value and prove it's in the Merkle tree" |
| "msg.sender" | "Divine a secret, hash it, assert it matches" |
| "Gas limit" | "Padded table height (power of 2)" |
| "Revert" | "Assertion failure -- no proof exists" |
| "Event log" | "`emit` (public) or `seal` (private)" |
| "Block.timestamp" | "`pub_read()` -- verifier provides it" |
| "Contract storage" | "Merkle root commitment (one Digest)" |
| "Function visibility" | "`pub` keyword on module functions" |
| "Target one chain" | "Write once, compile to any STARK backend" |
| "ABI encoding" | "Field elements (everything is field elements)" |
| "uint256" | "`Field` (mod 2^64 - 2^32 + 1) or `U32` (range-checked)" |
| "bytes32" | "`Digest` (5 field elements, 320 bits)" |
| "Proxy upgrade" | "Config update with admin auth hash" |
| "Constructor args" | "Public inputs or config commitments" |
| "View function" | "`pub_write()` outputs in the proof claim" |

---

## Platform-Specific Notes

### Coming from EVM (Solidity / Vyper)

- **No reentrancy.** There are no external calls. Programs are isolated.
- **No overflow/underflow.** Arithmetic wraps in the prime field. Use
  `std.core.convert.as_u32()` for explicit range checks when you need bounded
  integers.
- **No `address` type.** Identity is a `Digest` (hash of an auth secret) or
  a `Field` (single element of a hash). There are no 20-byte addresses.
- **No ABI.** Public I/O is a sequence of field elements. No encoding/decoding.
- **No inheritance.** Use modules and `use` imports. Composition over inheritance.

### Coming from SVM (Anchor / Rust)

- **No accounts model.** State is a Merkle tree, not separate account buffers.
- **No PDAs (Program Derived Addresses).** Identity is a hash preimage.
- **No CPI (Cross-Program Invocation).** Programs are standalone. Composition
  happens through recursive proof verification.
- **No `Signer` constraint.** Authorization is in-circuit via `divine` + `hash`
  + `assert`.
- **Simpler type system.** No lifetimes, no borrows, no `Option<T>`. Every type
  has a fixed width known at compile time.

### Coming from CosmWasm

- **No `deps.storage`.** No key-value store. State is a Merkle root.
- **No `info.sender`.** Auth is explicit hash preimage verification.
- **No `Response` with messages.** No inter-contract messages. Programs produce
  proofs, not responses.
- **No JSON schema.** I/O is field elements, not JSON.

### Coming from Substrate

- **No runtime pallets.** Each program is self-contained.
- **No weight system.** Cost is the padded table height, not weight classes.
- **No on-chain governance hooks.** Admin auth is a hash preimage; governance
  would be a separate proof that composes with the program proof.
- **No storage tries.** State is a Merkle tree you manage explicitly.

---

## Quick Start for Solidity Devs

### 1. Install Trident

```bash
git clone https://github.com/nicktriton/trident
cd trident
cargo build --release
# Add target/release/trident to your PATH
```

### 2. Create a Project and Read the Hello World

```bash
trident init my_first_zk
cd my_first_zk
cat main.tri
```

The default `main.tri` reads two public inputs, adds them, and writes the
result. Build it:

```bash
trident build main.tri -o hello.tasm
trident build main.tri --costs
```

### 3. Read the Fungible Token Example

The `examples/fungible_token/` directory contains a complete ZK-native token
with pay, lock, update, mint, and burn operations. Start with
`examples/fungible_token/SPEC.md` for the design, then read `token.tri` for
the implementation.

### 4. Build and Check Costs

```bash
trident build examples/fungible_token/token.tri --costs
trident build examples/fungible_token/token.tri --hotspots
```

### 5. Full Walkthrough

Read the [Tutorial](tutorial.md) for a complete step-by-step guide covering
types, functions, modules, I/O, hashing, events, testing, and cost analysis.

---

## Further Reading

### Trident documentation

- [Tutorial](tutorial.md) -- Step-by-step Trident developer guide
- [Language Reference](reference.md) -- Quick lookup: types, operators, builtins, grammar
- [Language Specification](spec.md) -- Complete language reference
- [Deploying a Program](deploying-a-program.md) -- Neptune UTXO scripts, lock/type scripts, multi-target deployment
- [Gold Standard Libraries](gold-standard.md) -- Token standards (TSP-1/TSP-2), bridge clients, crypto gadgets
- [Programming Model](programming-model.md) -- How programs run in Triton VM
- [Optimization Guide](optimization.md) -- Cost reduction strategies
- [Universal Design](universal-design.md) -- Multi-target architecture: universal core, backend extensions
- [Formal Verification](formal-verification.md) -- Prove properties of your contracts, find bugs before deployment
- [How STARK Proofs Work](stark-proofs.md) -- From execution traces to quantum-safe proofs
- [For Developers](for-developers.md) -- Zero-knowledge from scratch (if you also need the ZK primer)
- [Error Catalog](errors.md) -- All compiler error messages explained
- [Vision](vision.md) -- Why Trident exists and what you can build
- [Comparative Analysis](analysis.md) -- Triton VM vs. every other ZK system

### External resources

- [Triton VM](https://triton-vm.org/) -- The target zero-knowledge virtual machine
- [Triton VM Specification](https://triton-vm.org/spec/) -- TASM instruction set
- [Neptune Cash](https://neptune.cash/) -- Production blockchain built on Triton VM
- [Tip5 Hash Function](https://eprint.iacr.org/2023/107) -- The algebraic hash used everywhere
