# 🖥️ Operating System Reference

[← Target Reference](targets.md) | [Standard Library](stdlib.md)

An OS defines the runtime environment: storage, accounts, syscalls, and
I/O conventions. The compiler's job is runtime binding — link against
OS-specific modules (`os.<os>.*`). Everything in this document is about
the OS, not the instruction set. For VMs, see [vm.md](vm.md).

---

## The Model: Neurons, Signals, Tokens

The entire blockchain design space reduces to three primitives:

- Neuron — an actor. Accounts, UTXOs, objects, cells, notes, resources,
  contracts, wallets — all are neurons. A neuron has identity, can hold
  state, and can send signals.
- Signal — a transaction. A bundle of directed weighted edges (cyberlinks)
  from neuron to neuron. The weight is the amount. The signal is the act of
  communication itself.
- Token — a neuron viewed as an asset. Neurons ARE tokens. A fungible
  token (ETH, SOL, CKByte) is a fungible neuron — many identical
  interchangeable units, like shares of a company. A uniq
  (unique asset, smart contract, unique UTXO) is a non-fungible neuron — unique
  identity, one-of-one, like a person.

The model: neurons send signals carrying tokens to other neurons.
Neurons are the tokens. Everything else — accounts, UTXOs, objects,
cells — is how a specific OS represents neurons internally. The
compiler's job is to map neuron/signal operations down to those internals.

---

## The Three-Tier Namespace

```trident
std.*          Standard library      Pure computation (all 20 VMs, all 25 OSes)
os.*           OS standard           Universal runtime contract (all OSes)
os.<os>.*      OS extensions         OS-native API (one specific OS)
```

Programs can mix all three tiers. `std.*` for math and crypto. `os.*`
for portable neuron identity, signals, state, and events. `os.<os>.*`
when OS-native features are needed (PDAs, object ownership, L1/L2
messaging, CPI, etc.).

---

## `os.*` — The Gold Standard

Available on all blockchain and traditional OSes. The compiler lowers each
function to the OS-native mechanism based on `--target`. Programs using
only `std.*` + `os.*` are portable across all OSes that support the
required operations. If an OS doesn't support a concept (e.g.,
`os.neuron.id()` on UTXO chains, `os.signal.send()` on journal targets),
the compiler emits a clear error.

### `os.neuron` — Identity and Authorization

| Function | Signature | Description |
|----------|-----------|-------------|
| `id()` | `() -> Digest` | Identity of the current neuron (caller) |
| `verify(expected)` | `(expected: Digest) -> Bool` | Check caller matches expected |
| `auth(credential)` | `(credential: Digest) -> ()` | Assert authorized; crash if not |

A neuron is identified by a `Digest` — the universal identity container.
A 20-byte EVM address, a 32-byte Solana pubkey, and a 251-bit Starknet
felt all fit in a Digest.

`neuron.auth(credential)` is an assertion — it succeeds silently or crashes
the VM. On account chains, it checks the caller address. On UTXO chains,
it checks a hash preimage (divine the secret, hash it, assert the digest
matches). Same source code, different mechanism. This is the only auth
mechanism that works on every OS with identity.

Supported: Account, Stateless, Object, Process.
`id()`/`verify()` compile error: UTXO (no caller — use `auth()`), Journal (no identity).
`auth()` compile error: Journal (no identity).

#### Per-OS Lowering

| OS family | OSes | `neuron.id()` lowers to |
|-----------|------|-------------------------|
| Account (EVM) | Ethereum | `msg.sender` (padded to Digest) |
| Account (Cairo) | Starknet | `get_caller_address` |
| Account (WASM) | Near, Cosmos | `predecessor_account_id` / `info.sender` |
| Account (other) | Ton, Polkadot, Miden | OS-native caller address |
| Stateless | Solana | `account.key(0)` (first signer) |
| Object | Sui, Aptos | `tx_context::sender` |
| Process | Linux, macOS, WASI, Android | `getuid()` (padded to Digest) |
| UTXO | Neptune, Nockchain, Nervos, Aleo, Aztec | Compile error — no caller; use `neuron.auth()` |
| Journal | Boundless, Succinct, OpenVM Network | Compile error — no identity |

| OS family | OSes | `neuron.auth(cred)` lowers to |
|-----------|------|-------------------------------|
| Account (EVM) | Ethereum | `assert(msg.sender == cred)` |
| Account (Cairo) | Starknet | `assert(get_caller_address() == cred)` |
| Account (WASM) | Near, Cosmos | `assert(predecessor == cred)` / `assert(sender == cred)` |
| Account (other) | Ton, Polkadot, Miden | `assert(caller == cred)` |
| Stateless | Solana | `assert(is_signer(find_account(cred)))` |
| Object | Sui, Aptos | `assert(tx.sender() == cred)` |
| UTXO | Neptune, Nockchain, Nervos, Aleo, Aztec | `divine()` + `hash()` + `assert_eq(hash, cred)` |
| Process | Linux, macOS, WASI, Android | `assert(getuid() == cred)` |
| Journal | Boundless, Succinct, OpenVM Network | Compile error — no identity |

### `os.signal` — Communication Between Neurons

| Function | Signature | Description |
|----------|-----------|-------------|
| `send(from, to, amount)` | `(from: Digest, to: Digest, amount: Field) -> ()` | Emit a weighted directed edge from one neuron to another |
| `balance(neuron)` | `(neuron: Digest) -> Field` | Query neuron balance |

`send(from, to, amount)` is the universal primitive: a directed weighted
edge — a signal — from one neuron to another. In most cases `from` is the
current neuron, but delegation/proxy/allowance patterns pass a different
`from` (e.g., ERC-20 `transferFrom`, spending another neuron's UTXO with
their authorization).

Supported: Account, Stateless, Object, UTXO.
Compile error: Journal (no value), Process (no native value).

#### Per-OS Lowering

| OS family | OSes | `signal.send(from, to, amount)` lowers to |
|-----------|------|----------------------------------------------|
| Account (EVM) | Ethereum | `CALL(to, amount, "")` (self) / `transferFrom(from, to, amount)` (delegated) |
| Account (Cairo) | Starknet | `transfer(from, to, amount)` syscall |
| Account (WASM) | Near, Cosmos | `Promise::transfer` / `BankMsg::Send` |
| Account (other) | Ton, Polkadot | OS-native transfer message |
| Stateless | Solana | `system_program::transfer(from, to, amount)` |
| Object | Sui, Aptos | `coin::split` + `transfer::public_transfer` |
| UTXO | Neptune, Nockchain, Nervos, Aleo, Aztec | Emit output UTXO/note (from = consumed input, to = recipient) |
| Process | Linux, macOS, WASI, Browser, Android | Compile error — no native value |
| Journal | Boundless, Succinct, OpenVM Network | Compile error — no native value |

### `os.state` — Persistent State

| Function | Signature | Description |
|----------|-----------|-------------|
| `read(key)` | `(key: Field) -> Field` | Read one field element at key |
| `write(key, value)` | `(key: Field, value: Field) -> ()` | Write one field element at key |
| `read_n(key, width)` | `(key: Field, width: U32) -> [Field; N]` | Read N elements starting at key |
| `write_n(key, values)` | `(key: Field, values: [Field; N]) -> ()` | Write N elements starting at key |
| `exists(key)` | `(key: Field) -> Bool` | Check if key has been written |

Supported: Account, Stateless, Object, UTXO, Process.
Compile error: Journal (no persistent state).

On UTXO chains, the compiler auto-generates the divine-and-authenticate
pattern: divine the value, hash it, Merkle-prove against the state root.
The developer writes `state.read(key)` — the proof machinery is invisible.

#### Per-OS Lowering

| OS family | OSes | `state.read(key)` lowers to |
|-----------|------|-----------------------------|
| Account | Ethereum, Starknet, Near, Cosmos, Ton, Polkadot, Miden | `SLOAD(key)` / storage read syscall |
| Stateless | Solana | `account.data(derived_index, offset)` |
| Object | Sui, Aptos | `dynamic_field.borrow(context_object, key)` |
| UTXO | Neptune, Nockchain, Nervos, Aleo, Aztec | `divine()` + `merkle_authenticate(key, root)` |
| Process | Linux, macOS, WASI, Browser, Android | File / environment read |
| Journal | Boundless, Succinct, OpenVM Network | Compile error — no persistent state |

### `os.token` — Token Operations (PLUMB)

Tokens are neurons viewed as assets. `os.signal.send()` moves native
currency between neurons. `os.token` provides the full PLUMB lifecycle —
Pay, Lock, Update, Mint, Burn — plus read queries,
for both coins and uniqs.

See [TSP-1 — Coin](tsp1-coin.md) and [TSP-2 — Card](tsp2-card.md) for
leaf formats, config model, and circuit constraints. See the
[Gold Standard](../docs/explanation/gold-standard.md) for the PLUMB framework
design rationale and skill library.

#### PLUMB Operations — Coins

| Function | Signature | Description |
|----------|-----------|-------------|
| `pay(to, amount)` | `(to: Digest, amount: Field) -> ()` | Transfer value to recipient |
| `lock(until)` | `(until: Field) -> ()` | Time-lock caller's account |
| `update(new_config)` | `(new_config: Digest) -> ()` | Update token config (admin only) |
| `mint(to, amount)` | `(to: Digest, amount: Field) -> ()` | Create new tokens for recipient |
| `burn(amount)` | `(amount: Field) -> ()` | Destroy tokens from caller |

#### Read Queries — Coins

| Function | Signature | Description |
|----------|-----------|-------------|
| `balance(account)` | `(account: Digest) -> Field` | Query token balance |
| `supply()` | `() -> Field` | Query total supply |

#### PLUMB Operations — Uniqs

| Function | Signature | Description |
|----------|-----------|-------------|
| `pay(asset_id, to)` | `(asset_id: Digest, to: Digest) -> ()` | Transfer ownership of unique asset |
| `lock(asset_id, until)` | `(asset_id: Digest, until: Field) -> ()` | Time-lock asset |
| `update(new_config)` | `(new_config: Digest) -> ()` | Update token config (admin only) |
| `mint(asset_id, to, metadata)` | `(asset_id: Digest, to: Digest, metadata: Digest) -> ()` | Create unique asset |
| `burn(asset_id)` | `(asset_id: Digest) -> ()` | Destroy unique asset |

#### Read Queries — Uniqs

| Function | Signature | Description |
|----------|-----------|-------------|
| `owner(asset_id)` | `(asset_id: Digest) -> Digest` | Query current owner |
| `metadata(asset_id)` | `(asset_id: Digest) -> Digest` | Query metadata commitment |
| `exists(asset_id)` | `(asset_id: Digest) -> Bool` | Check if asset exists in tree |

All 5 PLUMB operations require authorization — the compiler enforces this
via the OS-native mechanism. `pay` and `burn` require account auth (plus
config-level dual auth if configured). `mint` requires config-level mint
authority. `update` requires admin authority. `lock` extends the time-lock
on an account or asset (extend only — cannot shorten).

Supported: Account, Stateless, Object, UTXO.
Compile error: Journal (no persistent state), Process (no native token concept).

#### Per-OS Lowering — Coins

| OS family | OSes | `token.pay(to, amount)` lowers to |
|-----------|------|------------------------------------|
| Account (EVM) | Ethereum | `transfer(to, amount)` (ERC-20) |
| Account (Cairo) | Starknet | `transfer(to, amount)` |
| Account (WASM) | Near, Cosmos | `ft_transfer` / `SendMsg` |
| Stateless | Solana | `spl_token::transfer(from, to, amount)` |
| Object | Sui, Aptos | `coin::split` + `transfer::public_transfer` |
| UTXO | Neptune | Consume sender leaf, emit two leaves: sender (balance - amount), receiver (balance + amount) (TSP-1 Pay op) |
| UTXO | Nervos, Aleo, Aztec | Consume cell/record/note, emit with updated balances |
| Process | Linux, macOS, WASI, Browser, Android | Compile error — no native token |
| Journal | Boundless, Succinct, OpenVM Network | Compile error — no persistent state |

| OS family | OSes | `token.mint(to, amount)` lowers to |
|-----------|------|------------------------------------|
| Account (EVM) | Ethereum | `_mint(to, amount)` (ERC-20 internal) |
| Account (Cairo) | Starknet | `mint(to, amount)` syscall |
| Account (WASM) | Near, Cosmos | `ft_transfer` / `MintMsg` |
| Stateless | Solana | `spl_token::mint_to(mint, to, amount)` |
| Object | Sui, Aptos | `coin::mint(treasury, amount)` + `transfer` |
| UTXO | Neptune | Emit output UTXO with amount (TSP-1 Mint op) |
| UTXO | Nervos, Aleo, Aztec | Emit output cell/record/note with amount |
| Process | Linux, macOS, WASI, Browser, Android | Compile error — no native token |
| Journal | Boundless, Succinct, OpenVM Network | Compile error — no persistent state |

| OS family | OSes | `token.balance(account)` lowers to |
|-----------|------|------------------------------------|
| Account (EVM) | Ethereum | `balanceOf(account)` (ERC-20) |
| Account (Cairo) | Starknet | `balance_of(account)` |
| Account (WASM) | Near, Cosmos | `ft_balance_of` / `QueryBalanceRequest` |
| Stateless | Solana | `spl_token::get_account(account).amount` |
| Object | Sui, Aptos | `coin::balance(account)` |
| UTXO | Neptune | Sum UTXO values for account (Merkle-authenticated) |
| UTXO | Nervos, Aleo, Aztec | Sum cell/record/note values |
| Process | Linux, macOS, WASI, Browser, Android | Compile error — no native token |
| Journal | Boundless, Succinct, OpenVM Network | Compile error — no persistent state |

#### Per-OS Lowering — Uniqs

| OS family | OSes | `token.pay(asset_id, to)` lowers to |
|-----------|------|------------------------------------------|
| Account (EVM) | Ethereum | `transferFrom(owner, to, tokenId)` (ERC-721) |
| Account (Cairo) | Starknet | `transfer_from(owner, to, token_id)` |
| Account (WASM) | Near, Cosmos | `uniq_transfer` / `SendUniq` |
| Stateless | Solana | `mpl_token::transfer(asset, to)` (Metaplex) |
| Object | Sui, Aptos | `transfer::public_transfer(object, to)` |
| UTXO | Neptune | Consume owner's asset leaf, emit new leaf with `owner_id = to` (TSP-2 Pay op) |
| UTXO | Nervos, Aleo, Aztec | Consume cell/record/note, emit with new owner |
| Process | Linux, macOS, WASI, Browser, Android | Compile error — no native token |
| Journal | Boundless, Succinct, OpenVM Network | Compile error — no persistent state |

| OS family | OSes | `token.owner(asset_id)` lowers to |
|-----------|------|-----------------------------------|
| Account (EVM) | Ethereum | `ownerOf(tokenId)` (ERC-721) |
| Account (Cairo) | Starknet | `owner_of(token_id)` |
| Account (WASM) | Near, Cosmos | `uniq_token` / `OwnerOf` query |
| Stateless | Solana | `mpl_token::get_metadata(asset).owner` |
| Object | Sui, Aptos | `object::owner(object_id)` |
| UTXO | Neptune | Merkle inclusion proof for asset leaf, read `owner_id` |
| UTXO | Nervos, Aleo, Aztec | Merkle inclusion proof, read owner field |
| Process | Linux, macOS, WASI, Browser, Android | Compile error — no native token |
| Journal | Boundless, Succinct, OpenVM Network | Compile error — no persistent state |

### `os.time` — Clock

| Function | Signature | Description |
|----------|-----------|-------------|
| `now()` | `() -> Field` | Current timestamp |
| `step()` | `() -> Field` | Current step number (block height, slot, epoch, etc.) |

Supported: All OS families.

On blockchain OSes, `now()` returns block/slot timestamp. On traditional
OSes, it returns wall-clock time. On journal targets, it returns the
timestamp provided as public input. `step()` returns the discrete
progression counter — block height on most chains, slot on Solana,
tick count on process OSes.

#### Per-OS Lowering

| OS family | OSes | `time.now()` lowers to |
|-----------|------|------------------------|
| Account (EVM) | Ethereum | `block.timestamp` |
| Account (Cairo) | Starknet | `get_block_timestamp` |
| Account (WASM) | Near, Cosmos | `env.block.time` |
| Account (other) | Ton, Polkadot | OS-native block time |
| Stateless | Solana | `Clock::unix_timestamp` |
| Object | Sui, Aptos | `tx_context::epoch_timestamp_ms` |
| UTXO | Neptune, Nockchain | `kernel.authenticate_timestamp(root)` |
| Process | Linux, macOS, Android | `clock_gettime(CLOCK_REALTIME)` |
| WASI/Browser | WASI, Browser | `wall_clock.now()` / `Date.now()` |
| Journal | Boundless, Succinct, OpenVM Network | Timestamp from public input |

| OS family | OSes | `time.step()` lowers to |
|-----------|------|------------------------|
| Account (EVM) | Ethereum | `block.number` |
| Account (Cairo) | Starknet | `get_block_number` |
| Account (WASM) | Near, Cosmos | `env.block.height` |
| Account (other) | Ton, Polkadot | OS-native block number |
| Stateless | Solana | `Clock::slot` |
| Object | Sui, Aptos | `tx_context::epoch()` |
| UTXO | Neptune, Nockchain | `kernel.authenticate_block_height(root)` |
| Process | Linux, macOS, Android | Monotonic tick counter |
| WASI/Browser | WASI, Browser | `monotonic_clock.now()` / `performance.now()` |
| Journal | Boundless, Succinct, OpenVM Network | Step number from public input |

### `os.event` — Events

`reveal` and `seal` are the event mechanism. They compile to the TIR ops
`Reveal` and `Seal`, which each backend lowers to its native event
mechanism (LOG on EVM, sol_log on Solana, announcements on Neptune).
No additional `os.event` module needed — events use language-level
`reveal`/`seal` statements directly.

---

## OS Registry

Designed for 25 OSes across provable, blockchain, and traditional runtimes (today: Neptune):

| OS | VM | Runtime binding | Account / process model | Interop | Details |
|----|-----|----------------|------------------------|---------|---------|
| **Provable** | | | | | |
| Neptune | [TRITON](../../vm/triton/README.md) | `os.neptune.*` | UTXO | -- | [neptune.md](../../os/neptune/README.md) |
| Polygon Miden | [MIDEN](../../vm/miden/README.md) | `os.miden.*` | Account | -- | [miden.md](../../os/miden/README.md) |
| Nockchain | [NOCK](../../vm/nock/README.md) | `os.nockchain.*` | UTXO (Notes) | -- | [nockchain.md](../../os/nockchain/README.md) |
| Starknet | [CAIRO](../../vm/cairo/README.md) | `os.starknet.*` | Account | Ethereum L2 | [starknet.md](../../os/starknet/README.md) |
| Boundless | [RISCZERO](../../vm/risczero/README.md) | `os.boundless.*` | -- | Ethereum verification | [boundless.md](../../os/boundless/README.md) |
| Succinct | [SP1](../../vm/sp1/README.md) | `os.succinct.*` | -- | Ethereum verification | [succinct.md](../../os/succinct/README.md) |
| OpenVM Network | [OPENVM](../../vm/openvm/README.md) | `os.openvm.*` | -- | -- | [openvm-network.md](../../os/openvm-network/README.md) |
| Aleo | [AVM](../../vm/avm/README.md) | `os.aleo.*` | Record (UTXO) | -- | [aleo.md](../../os/aleo/README.md) |
| Aztec | [AZTEC](../../vm/aztec/README.md) | `os.aztec.*` | Note (UTXO) + public | Ethereum L2 | [aztec.md](../../os/aztec/README.md) |
| **Blockchain** | | | | | |
| Ethereum | [EVM](../../vm/evm/README.md) | `os.ethereum.*` | Account | -- | [ethereum.md](../../os/ethereum/README.md) |
| Solana | [SBPF](../../vm/sbpf/README.md) | `os.solana.*` | Account (stateless programs) | -- | [solana.md](../../os/solana/README.md) |
| Near Protocol | [WASM](../../vm/wasm/README.md) | `os.near.*` | Account (1 contract each) | -- | [near.md](../../os/near/README.md) |
| Cosmos (100+ chains) | [WASM](../../vm/wasm/README.md) | `os.cosmwasm.*` | Account | IBC | [cosmwasm.md](../../os/cosmwasm/README.md) |
| Arbitrum | [WASM](../../vm/wasm/README.md) + [EVM](../../vm/evm/README.md) | `os.arbitrum.*` | Account (EVM-compatible) | Ethereum L2 | [arbitrum.md](../../os/arbitrum/README.md) |
| Internet Computer | [WASM](../../vm/wasm/README.md) | `os.icp.*` | Canister | -- | [icp.md](../../os/icp/README.md) |
| Sui | [MOVEVM](../../vm/movevm/README.md) | `os.sui.*` | Object-centric | -- | [sui.md](../../os/sui/README.md) |
| Aptos | [MOVEVM](../../vm/movevm/README.md) | `os.aptos.*` | Account (resources) | -- | [aptos.md](../../os/aptos/README.md) |
| Ton | [TVM](../../vm/tvm/README.md) | `os.ton.*` | Account (cells) | -- | [ton.md](../../os/ton/README.md) |
| Nervos CKB | [CKB](../../vm/ckb/README.md) | `os.nervos.*` | Cell (UTXO-like) | -- | [nervos.md](../../os/nervos/README.md) |
| Polkadot | [POLKAVM](../../vm/polkavm/README.md) | `os.polkadot.*` | Account | XCM | [polkadot.md](../../os/polkadot/README.md) |
| **Traditional** | | | | | |
| Linux | [X86-64](../../vm/x86-64/README.md) / [ARM64](../../vm/arm64/README.md) / [RISCV](../../vm/riscv/README.md) | `os.linux.*` | Process | POSIX syscalls | [linux.md](../../os/linux/README.md) |
| macOS | [ARM64](../../vm/arm64/README.md) / [X86-64](../../vm/x86-64/README.md) | `os.macos.*` | Process | POSIX + Mach | [macos.md](../../os/macos/README.md) |
| Android | [ARM64](../../vm/arm64/README.md) / [X86-64](../../vm/x86-64/README.md) | `os.android.*` | Process (sandboxed) | NDK, JNI | [android.md](../../os/android/README.md) |
| WASI | [WASM](../../vm/wasm/README.md) | `os.wasi.*` | Process (capability) | WASI preview 2 | [wasi.md](../../os/wasi/README.md) |
| Browser | [WASM](../../vm/wasm/README.md) | `os.browser.*` | Event loop | JavaScript, Web APIs | [browser.md](../../os/browser/README.md) |

Key observations:

- One VM, many OSes. WASM powers 6+ OSes (Near, Cosmos, ICP, Arbitrum,
  WASI, Browser). x86-64 and ARM64 power Linux, macOS, Android. MOVEVM
  powers Sui and Aptos. Same bytecode output, different `os.<os>.*` bindings.
- RISC-V lowering is shared across SP1, OPENVM, RISCZERO, JOLT, CKB,
  POLKAVM, and native RISCV — 7 targets from one `RiscVLowering`.
- Arbitrum supports both WASM (Stylus) and EVM.

---

## OS TOML `[runtime]` Fields

The compiler selects lowering strategy from three fields in `os/*.toml`:

| Field | Values | Effect on `os.*` |
|-------|--------|---------------------|
| `account_model` | `account`, `stateless`, `object`, `utxo`, `journal`, `process` | Selects neuron/signal lowering |
| `storage_model` | `key-value`, `account-data`, `object-store`, `merkle-authenticated`, `filesystem`, `none` | Selects state lowering |
| `transaction_model` | `signed`, `proof-based`, `none` | Selects neuron.auth/signal lowering |

---

## Extension Tracking

Each OS provides its own `os.<os>.*` modules with runtime-specific
bindings: storage, accounts, syscalls, transaction models. Importing any
`os.<os>.*` module binds the program to that OS — the compiler rejects
cross-OS imports.

### Implemented

| Module | Description | OS doc |
|--------|-------------|--------|
| `os.neptune.kernel` | Transaction kernel MAST authentication | [neptune.md](../../os/neptune/README.md) |
| `os.neptune.utxo` | UTXO structure authentication | [neptune.md](../../os/neptune/README.md) |
| `os.neptune.xfield` | Extension field arithmetic intrinsics | [neptune.md](../../os/neptune/README.md) |
| `os.neptune.proof` | Recursive STARK verification | [neptune.md](../../os/neptune/README.md) |
| `os.neptune.recursive` | Low-level recursive proof primitives | [neptune.md](../../os/neptune/README.md) |


### Designed (not yet implemented)

| OS | Modules | OS doc |
|----|---------|--------|
| Ethereum | `os.ethereum.` storage, account, transfer, call, event, block, tx, precompile | [ethereum.md](../../os/ethereum/README.md) |
| Solana | `os.solana.` account, pda, cpi, transfer, system, log, clock, rent | [solana.md](../../os/solana/README.md) |
| Starknet | `os.starknet.` storage, account, call, event, messaging, crypto | [starknet.md](../../os/starknet/README.md) |
| Sui | `os.sui.` object, transfer, dynamic_field, tx, coin, event | [sui.md](../../os/sui/README.md) |

See each OS doc for the full API reference.

---

## 🔗 See Also

- [Target Reference](targets.md) — OS model, integration tracking, how-to-add checklists
- [VM Reference](vm.md) — VM registry, lowering paths, tier/type/builtin tables, cost models
- [Standard Library](stdlib.md) — `std.*` modules
- [Language Reference](language.md) — Types, operators, builtins, grammar, sponge, Merkle, extension field, proof composition
- Per-OS docs: `os/<os>/README.md`

---

*Trident v0.5 — Write once. Run anywhere.*
