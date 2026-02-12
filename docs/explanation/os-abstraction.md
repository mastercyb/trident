# 🖥️ OS Abstraction

Trident is designed to compile to 25 operating systems — blockchains,
zkVMs, and traditional runtimes. Each OS has a different programming model, but
they all share the same six concerns. The compiler's job is **runtime
binding** — translating these concerns to OS-native primitives.

For the portable `os.*` API that abstracts these patterns, see the
[OS Reference](../reference/os.md). For how each `os.*` call lowers to
target-native code, see [Multi-Target Compilation](multi-target.md).

---

## 🖥️ The Six Concerns of OS Programming

Every OS, regardless of model, must address six concerns. The compiler's
job is **runtime binding** -- translating these concerns to OS-native
primitives via `os.<os>.*` modules.

The tables below describe **OS-native patterns** (the `os.<os>.*` layer — S2).
For the portable `os.*` layer (S1) that abstracts these patterns, see
[Standard Library — Portable OS Layer](../reference/stdlib.md).

### 1. Entry Points -- How Programs Start

| OS family | Entry point | Example |
|-----------|-------------|---------|
| UTXO (Neptune, Nockchain, Nervos) | Script execution per UTXO spent/created | Lock scripts, type scripts |
| Account (Ethereum, Starknet, Near, Cosmos) | Exported functions on a deployed contract | `transfer()`, `approve()`, `query()` |
| Stateless (Solana) | Single instruction handler, accounts passed in | `process_instruction(accounts)` |
| Object (Sui, Aptos) | Entry functions operating on owned/shared objects | `public entry fn transfer(obj, recipient)` |
| Journal (SP1, RISC Zero, OpenVM) | `fn main()` -- pure computation, no persistent state | Read journal, compute, write journal |
| Process (Linux, macOS, WASI) | `fn main()` -- argc/argv, stdin/stdout | Standard process entry |

### 2. State Access -- How State Is Stored and Read

| OS family | State model | Trident pattern |
|-----------|-------------|-----------------|
| UTXO | Merkle tree of UTXOs | Divine leaf data, authenticate against root via `merkle_step` |
| Account | Key-value storage slots | `os.<os>.storage.read(key)` / `write(key, value)` |
| Stateless | Account data buffers | `os.solana.account.data(index)` (accounts passed by caller) |
| Object | Object store (ownership graph) | `os.sui.object.borrow(id)` / `transfer.send(obj, recipient)` |
| Journal | No persistent state | Public I/O only (`pub_read` / `pub_write`) |
| Process | Filesystem, environment | `os.<os>.fs.read()` / `write()` |

The divine-and-authenticate pattern is specific to UTXO chains. Account-based
chains provide direct storage access. The same Trident program structure
(read state, compute, write state) applies everywhere -- only the access
mechanism differs.

### 3. Identity -- Who Is Calling

| OS family | Identity mechanism | Trident pattern |
|-----------|-------------------|-----------------|
| UTXO | Hash preimage (no sender concept) | `divine()` secret, `hash()`, `assert_eq()` |
| Account (EVM) | Protocol-level signature verification | `os.ethereum.account.caller()` (= msg.sender) |
| Account (Starknet) | Native account abstraction | `os.starknet.account.caller()` |
| Stateless (Solana) | Signer accounts in transaction | `os.solana.account.is_signer(index)` |
| Object (Sui) | Transaction sender | `os.sui.tx.sender()` |
| Journal | No identity (pure computation) | N/A |
| Process | UID/PID | `os.<os>.process.uid()` |

### 4. Signals -- How Neurons Communicate

| OS family | Signal mechanism | Trident pattern |
|-----------|-----------------|-----------------|
| UTXO | Create new UTXOs, destroy old ones | Kernel outputs (new UTXOs) in transaction |
| Account (EVM) | Transfer opcode | `os.ethereum.transfer.send(from, to, amount)` |
| Stateless (Solana) | Lamport transfer via system program | `os.solana.transfer.lamports(from, to, amount)` |
| Object (Sui) | Object transfer (ownership change) | `os.sui.coin.split()`, `os.sui.transfer.send()` |
| Journal | No value (off-chain computation) | N/A |
| Process | N/A | N/A |

### 5. Cross-Contract Interaction

| OS family | Mechanism | Trident pattern |
|-----------|-----------|-----------------|
| UTXO (Neptune) | Recursive proof verification | `os.neptune.proof.verify_inner_proof()` |
| Account (EVM) | CALL/STATICCALL/DELEGATECALL | `os.ethereum.call.call(address, data)` |
| Account (Starknet) | Contract calls, library calls | `os.starknet.call.invoke(address, selector, args)` |
| Stateless (Solana) | CPI (cross-program invocation) | `os.solana.cpi.invoke(program, accounts, data)` |
| Object (Sui) | Direct function calls on shared objects | Call functions from other modules directly |
| Cosmos | IBC messages | `os.cosmwasm.ibc.send(channel, data)` |
| Journal | Proof composition | Recursive verification in the same journal |
| Process | Subprocess, IPC | `os.<os>.process.exec()` |

### 6. Events -- Observable Side Effects

| OS family | Native mechanism | Trident pattern |
|-----------|-----------------|-----------------|
| UTXO (Neptune) | Announcements (kernel leaf 2) | `reveal` (public) / `seal` (hashed) |
| Account (EVM) | LOG0-LOG4 opcodes | `reveal` compiles to LOG; `seal` has no EVM equivalent |
| Account (Starknet) | Events (indexed) | `reveal` compiles to emit_event |
| Stateless (Solana) | Program logs / events | `reveal` compiles to sol_log_data |
| Object (Sui) | Events (Move) | `reveal` compiles to event::emit |
| Journal | Journal output | `pub_write()` is the event |
| Process | stdout / structured logging | `reveal` compiles to structured log output |

Trident's `reveal` and `seal` are the universal event mechanism. `reveal`
emits data in the clear. `seal` hashes the data via sponge construction --
only the commitment digest is visible. On OSes without native privacy support,
`seal` emits only the hash digest.

---

## 🌐 OS Families

### UTXO Model (Neptune, Nockchain, Nervos, Aleo)

Programs are **scripts** attached to transaction outputs. A lock script proves
the right to spend a UTXO. A type script validates conservation rules. The
program never sees "the blockchain" -- it receives a commitment (Merkle root)
as public input and authenticates everything against it.

**Key pattern: divine-and-authenticate.** The prover supplies private data
via `divine()`, then proves it belongs to the committed state via Merkle proofs.
This is the fundamental state access pattern for all UTXO chains.

For the complete Neptune programming model -- transaction kernels, UTXO
structure, address types, block structure, and `os.neptune.*` API -- see
[Neptune OS Reference](../../os/neptune/README.md).

### Account Model (Ethereum, Starknet, Near, Cosmos, Ton, Polkadot)

Programs are **contracts** with persistent storage. The OS provides direct
read/write access to storage slots. Identity comes from the protocol layer
(msg.sender, caller address). The program is deployed once and called
repeatedly with different inputs.

For programming models:
[Ethereum](../../os/ethereum/README.md) |
[Starknet](../../os/starknet/README.md) |
[Near](../../os/near/README.md) |
[Cosmos](../../os/cosmwasm/README.md) |
[Ton](../../os/ton/README.md) |
[Polkadot](../../os/polkadot/README.md)

### Stateless Model (Solana)

Programs are **stateless instruction handlers**. State lives in separate
accounts that are passed into the program by the caller. The program reads
and writes account data but does not own storage. Identity comes from
signer accounts in the transaction.

For the complete Solana programming model -- accounts, PDAs, CPI, and
`os.solana.*` API -- see [Solana OS Reference](../../os/solana/README.md).

### Object Model (Sui, Aptos)

Programs operate on **objects** (Sui) or **resources** (Aptos) with explicit
ownership. Objects can be owned (single-writer), shared (consensus-ordered),
or immutable. The type system enforces resource safety -- objects cannot be
copied or dropped unless explicitly allowed.

For programming models:
[Sui](../../os/sui/README.md) |
[Aptos](../../os/aptos/README.md)

### Journal Model (SP1, RISC Zero, OpenVM, Boundless, Succinct)

Programs are **pure computations** with no persistent state. Input comes
from a journal (public) and host communication (private). Output goes to
a journal. The proof attests that the computation was performed correctly.
No accounts, no storage, no identity.

For programming models:
[Boundless](../../os/boundless/README.md) |
[Succinct](../../os/succinct/README.md) |
[OpenVM](../../os/openvm-network/README.md)

### Process Model (Linux, macOS, WASI, Browser, Android)

Programs are **processes** with standard OS primitives: files, sockets,
stdin/stdout, environment variables. No proofs, no blockchain state.
These targets exist for testing, debugging, and running Trident programs
as conventional software.

For programming models:
[Linux](../../os/linux/README.md) |
[macOS](../../os/macos/README.md) |
[WASI](../../os/wasi/README.md) |
[Browser](../../os/browser/README.md) |
[Android](../../os/android/README.md)

---

## 🧩 The Portable OS API

Level 3 connects business logic to the runtime environment. It has two
tiers:

**`os.*`** — the portable runtime. Designed for all 25 OSes. Programs
using only Level 1 + `os.*` are designed to be portable across every OS
that supports the required operations.

| Module | Purpose | Lowers to |
|--------|---------|-----------|
| `os.neuron` | Identity, authorization | `msg.sender` (EVM), `predecessor_account_id` (Near), `divine()+hash()` (Neptune) |
| `os.signal` | Value transfer | `CALL(to, amount)` (EVM), `system_program::transfer` (Solana), emit UTXO (Neptune) |
| `os.token` | Token operations (PLUMB) | ERC-20/721 (EVM), SPL (Solana), TSP-1/2 (Neptune) |
| `os.state` | Persistent storage | `SLOAD/SSTORE` (EVM), account data (Solana), Merkle-authenticated RAM (Neptune) |
| `os.time` | Clock | `block.timestamp` (EVM), `Clock::unix_timestamp` (Solana), kernel timestamp (Neptune) |

**`os.<os>.*`** — OS-specific extensions. Importing any `os.<os>.*`
module locks the program to that OS. Used when you need capabilities
that don't have a portable abstraction.

| OS | Extensions | Use case |
|----|-----------|----------|
| Neptune | `os.neptune.kernel`, `os.neptune.utxo`, `os.neptune.xfield` | UTXO authentication, kernel MAST, extension field arithmetic |
| Ethereum | `os.ethereum.call`, `os.ethereum.precompile` | Raw CALL/DELEGATECALL, precompile access |
| Solana | `os.solana.pda`, `os.solana.cpi` | PDA derivation, cross-program invocation |
| Cosmos | `os.cosmwasm.ibc`, `os.cosmwasm.bank` | IBC packets, bank module |
| Sui | `os.sui.object`, `os.sui.transfer` | Object-centric model |

The less `os.<os>.*` code in a program, the more portable it is. Good
Trident programs have thick Level 1 and thin `os.<os>.*`.

---

## 🔗 See Also

- [OS Reference](../reference/os.md) — Full os.* API specifications and per-OS lowering tables
- [Programming Model](programming-model.md) — Field arithmetic, I/O model, data flow
- [Multi-Target Compilation](multi-target.md) — Compiler architecture and backend traits
- [Gold Standard](gold-standard.md) — PLUMB token standards and capability library
- [For Blockchain Devs](for-blockchain-devs.md) — Migration from Solidity, Anchor, CosmWasm
