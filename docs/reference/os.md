# Operating System Reference

[← Target Reference](targets.md) | [Standard Library](stdlib.md)

An OS defines the runtime environment: storage, accounts, syscalls, and
I/O conventions. The compiler's job is runtime binding — link against
OS-specific modules (`<os>.ext.*`). Everything in this document is about
the OS, not the instruction set. For VMs, see [vm.md](vm.md).

---

## The Model: Neurons, Signals, Tokens

The entire blockchain design space reduces to three primitives:

- **Neuron** — an actor. Accounts, UTXOs, objects, cells, notes, resources,
  contracts, wallets — all are neurons. A neuron has identity, can hold
  state, and can send signals.
- **Signal** — a transaction. A bundle of directed weighted edges (cyberlinks)
  from neuron to neuron. The weight is the amount. The signal is the act of
  communication itself.
- **Token** — a neuron viewed as an asset. Neurons ARE tokens. A fungible
  token (ETH, SOL, CKByte) is a fungible neuron — many identical
  interchangeable units, like shares of a company. A non-fungible token
  (NFT, smart contract, unique UTXO) is a non-fungible neuron — unique
  identity, one-of-one, like a person.

The model: **neurons send signals carrying tokens to other neurons.**
Neurons are the tokens. Everything else — accounts, UTXOs, objects,
cells — is how a specific OS represents neurons internally. The
compiler's job is to map neuron/signal operations down to those internals.

---

## The Three-Tier Namespace

```
std.*          Standard library      Pure computation (all 20 VMs, all 25 OSes)
os.*           OS standard           Universal runtime contract (all OSes)
<os>.ext.*     OS extensions         OS-native API (one specific OS)
```

Programs can mix all three tiers. `std.*` for math and crypto. `os.*`
for portable neuron identity, signals, state, and events. `<os>.ext.*`
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

**Supported:** Account, Stateless, Object, Process.
**`id()`/`verify()` compile error:** UTXO (no caller — use `auth()`), Journal (no identity).
**`auth()` compile error:** Journal (no identity).

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
| UTXO | Neptune, Nockchain, Nervos, Aleo, Aztec | **Compile error** — no caller; use `neuron.auth()` |
| Journal | Boundless, Succinct, OpenVM Network | **Compile error** — no identity |

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
| Journal | Boundless, Succinct, OpenVM Network | **Compile error** — no identity |

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

**Supported:** Account, Stateless, Object, UTXO.
**Compile error:** Journal (no value), Process (no native value).

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
| Process | Linux, macOS, WASI, Browser, Android | **Compile error** — no native value |
| Journal | Boundless, Succinct, OpenVM Network | **Compile error** — no native value |

### `os.state` — Persistent State

| Function | Signature | Description |
|----------|-----------|-------------|
| `read(key)` | `(key: Field) -> Field` | Read one field element at key |
| `write(key, value)` | `(key: Field, value: Field) -> ()` | Write one field element at key |
| `read_n(key, width)` | `(key: Field, width: U32) -> [Field; N]` | Read N elements starting at key |
| `write_n(key, values)` | `(key: Field, values: [Field; N]) -> ()` | Write N elements starting at key |
| `exists(key)` | `(key: Field) -> Bool` | Check if key has been written |

**Supported:** Account, Stateless, Object, UTXO, Process.
**Compile error:** Journal (no persistent state).

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
| Journal | Boundless, Succinct, OpenVM Network | **Compile error** — no persistent state |

### `os.time` — Clock

| Function | Signature | Description |
|----------|-----------|-------------|
| `now()` | `() -> Field` | Current timestamp |
| `block_height()` | `() -> Field` | Current block/slot number |

**Supported:** All OS families.

On blockchain OSes, `now()` returns block/slot timestamp. On traditional
OSes, it returns wall-clock time. On journal targets, it returns the
timestamp provided as public input.

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

### `os.event` — Events

`reveal` and `seal` are the event mechanism. They compile to the TIR ops
`Reveal` and `Seal`, which each backend lowers to its native event
mechanism (LOG on EVM, sol_log on Solana, announcements on Neptune).
No additional `os.event` module needed — events use language-level
`reveal`/`seal` statements directly.

---

## OS Registry

25 OSes across provable, blockchain, and traditional runtimes:

| OS | VM | Runtime binding | Account / process model | Interop | Details |
|----|-----|----------------|------------------------|---------|---------|
| **Provable** | | | | | |
| Neptune | [TRITON](vm/triton.md) | `neptune.ext.*` | UTXO | -- | [neptune.md](os/neptune.md) |
| Polygon Miden | [MIDEN](vm/miden.md) | `miden.ext.*` | Account | -- | [miden.md](os/miden.md) |
| Nockchain | [NOCK](vm/nock.md) | `nockchain.ext.*` | UTXO (Notes) | -- | [nockchain.md](os/nockchain.md) |
| Starknet | [CAIRO](vm/cairo.md) | `starknet.ext.*` | Account | Ethereum L2 | [starknet.md](os/starknet.md) |
| Boundless | [RISCZERO](vm/risczero.md) | `boundless.ext.*` | -- | Ethereum verification | [boundless.md](os/boundless.md) |
| Succinct | [SP1](vm/sp1.md) | `succinct.ext.*` | -- | Ethereum verification | [succinct.md](os/succinct.md) |
| OpenVM Network | [OPENVM](vm/openvm.md) | `openvm.ext.*` | -- | -- | [openvm-network.md](os/openvm-network.md) |
| Aleo | [AVM](vm/avm.md) | `aleo.ext.*` | Record (UTXO) | -- | [aleo.md](os/aleo.md) |
| Aztec | [AZTEC](vm/aztec.md) | `aztec.ext.*` | Note (UTXO) + public | Ethereum L2 | [aztec.md](os/aztec.md) |
| **Blockchain** | | | | | |
| Ethereum | [EVM](vm/evm.md) | `ethereum.ext.*` | Account | -- | [ethereum.md](os/ethereum.md) |
| Solana | [SBPF](vm/sbpf.md) | `solana.ext.*` | Account (stateless programs) | -- | [solana.md](os/solana.md) |
| Near Protocol | [WASM](vm/wasm.md) | `near.ext.*` | Account (1 contract each) | -- | [near.md](os/near.md) |
| Cosmos (100+ chains) | [WASM](vm/wasm.md) | `cosmwasm.ext.*` | Account | IBC | [cosmwasm.md](os/cosmwasm.md) |
| Arbitrum | [WASM](vm/wasm.md) + [EVM](vm/evm.md) | `arbitrum.ext.*` | Account (EVM-compatible) | Ethereum L2 | [arbitrum.md](os/arbitrum.md) |
| Internet Computer | [WASM](vm/wasm.md) | `icp.ext.*` | Canister | -- | [icp.md](os/icp.md) |
| Sui | [MOVEVM](vm/movevm.md) | `sui.ext.*` | Object-centric | -- | [sui.md](os/sui.md) |
| Aptos | [MOVEVM](vm/movevm.md) | `aptos.ext.*` | Account (resources) | -- | [aptos.md](os/aptos.md) |
| Ton | [TVM](vm/tvm.md) | `ton.ext.*` | Account (cells) | -- | [ton.md](os/ton.md) |
| Nervos CKB | [CKB](vm/ckb.md) | `nervos.ext.*` | Cell (UTXO-like) | -- | [nervos.md](os/nervos.md) |
| Polkadot | [POLKAVM](vm/polkavm.md) | `polkadot.ext.*` | Account | XCM | [polkadot.md](os/polkadot.md) |
| **Traditional** | | | | | |
| Linux | [X86-64](vm/x86-64.md) / [ARM64](vm/arm64.md) / [RISCV](vm/riscv.md) | `linux.ext.*` | Process | POSIX syscalls | [linux.md](os/linux.md) |
| macOS | [ARM64](vm/arm64.md) / [X86-64](vm/x86-64.md) | `macos.ext.*` | Process | POSIX + Mach | [macos.md](os/macos.md) |
| Android | [ARM64](vm/arm64.md) / [X86-64](vm/x86-64.md) | `android.ext.*` | Process (sandboxed) | NDK, JNI | [android.md](os/android.md) |
| WASI | [WASM](vm/wasm.md) | `wasi.ext.*` | Process (capability) | WASI preview 2 | [wasi.md](os/wasi.md) |
| Browser | [WASM](vm/wasm.md) | `browser.ext.*` | Event loop | JavaScript, Web APIs | [browser.md](os/browser.md) |

Key observations:

- **One VM, many OSes.** WASM powers 6+ OSes (Near, Cosmos, ICP, Arbitrum,
  WASI, Browser). x86-64 and ARM64 power Linux, macOS, Android. MOVEVM
  powers Sui and Aptos. Same bytecode output, different `<os>.ext.*` bindings.
- **RISC-V lowering is shared** across SP1, OPENVM, RISCZERO, JOLT, CKB,
  POLKAVM, and native RISCV — 7 targets from one `RiscVLowering`.
- **Arbitrum** supports both WASM (Stylus) and EVM.

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

Each OS provides its own `<os>.ext.*` modules with runtime-specific
bindings: storage, accounts, syscalls, transaction models. Importing any
`<os>.ext.*` module binds the program to that OS — the compiler rejects
cross-OS imports.

### Implemented

| Module | Description | OS doc |
|--------|-------------|--------|
| `neptune.ext.kernel` | Transaction kernel MAST authentication | [neptune.md](os/neptune.md) |
| `neptune.ext.utxo` | UTXO structure authentication | [neptune.md](os/neptune.md) |
| `neptune.ext.xfield` | Extension field arithmetic intrinsics | [neptune.md](os/neptune.md) |
| `neptune.ext.proof` | Recursive STARK verification | [neptune.md](os/neptune.md) |
| `neptune.ext.recursive` | Low-level recursive proof primitives | [neptune.md](os/neptune.md) |
| `neptune.ext.registry` | On-chain definition registry (5 ops) | [neptune.md](os/neptune.md) |

### Designed (not yet implemented)

| OS | Modules | OS doc |
|----|---------|--------|
| Ethereum | `ethereum.ext.` storage, account, transfer, call, event, block, tx, precompile | [ethereum.md](os/ethereum.md) |
| Solana | `solana.ext.` account, pda, cpi, transfer, system, log, clock, rent | [solana.md](os/solana.md) |
| Starknet | `starknet.ext.` storage, account, call, event, messaging, crypto | [starknet.md](os/starknet.md) |
| Sui | `sui.ext.` object, transfer, dynamic_field, tx, coin, event | [sui.md](os/sui.md) |

See each OS doc for the full API reference.

---

## See Also

- [Target Reference](targets.md) — OS model, integration tracking, how-to-add checklists
- [VM Reference](vm.md) — VM registry, lowering paths, tier/type/builtin tables, cost models
- [Standard Library](stdlib.md) — `std.*` modules
- [Language Reference](language.md) — Types, operators, builtins, grammar
- [Provable Computation](provable.md) — Hash, sponge, Merkle, extension field (Tier 2-3)
- Per-OS docs: `os/<os>.md`

---

*Trident v0.5 — Write once. Run anywhere.*
