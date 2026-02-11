# Trident Programming Model

How programs interact with the outside world. Trident compiles to 20 VMs
and 25 OSes -- each OS has a different programming model, but they all
share the same universal foundation.

## The Universal Primitive: `Field`

Every zkVM computes over a finite field. `Field` is the **universal primitive
type** of provable computation -- the specific prime is an implementation detail
of the proof system, not a semantic property of the program. A Trident program
that multiplies two field elements and asserts the result means the same thing
on every target. The developer reasons about field arithmetic abstractly; the
backend implements it concretely.

All Trident values decompose into `Field` elements: a `Digest` is five `Field`
elements, a `u128` is four, and so on. I/O channels, memory, and the stack all
traffic in `Field`. This is true regardless of the compilation target.

> **Target-dependent detail.** The Triton VM default field is the Goldilocks
> prime `p = 2^64 - 2^32 + 1`. Other targets use different primes. Programs
> should never depend on the specific modulus -- see
> [Universal Design](universal-design.md) for the multi-target story.

## Universal I/O Model

Regardless of the target, every Trident program communicates through
the same three channels:

| Channel | Instruction | Visible to verifier? | Use for |
|---------|-------------|----------------------|---------|
| **Public input** | `pub_read()` | Yes | Data the verifier must see |
| **Public output** | `pub_write()` | Yes | Results and commitments |
| **Secret input** | `divine()` | No | Witness data, private state |

On provable targets, the verifier only sees the **Claim** (program hash +
public I/O) and a **Proof**. Everything else -- secret input, RAM, stack
states, execution trace -- remains hidden. This is the zero-knowledge property.

On non-provable targets (EVM, WASM, native), these channels map to the OS's
native I/O: calldata, return data, storage reads. The privacy guarantee
disappears, but the program logic is identical.

## Arithmetic

All arithmetic operates on `Field` elements. `+` and `*` are field addition
and multiplication, wrapping modulo the target's prime automatically.

Practical consequences:

- `Field` elements range from 0 to p-1
- `1 - 2` in field arithmetic gives `p - 1`, not `-1`. Use `sub(a, b)`
- Integer comparison (`<`, `>`) requires explicit `as_u32` conversion
- For amounts/balances, use u128 encoding (4 `Field` elements)

---

## The Six Concerns of OS Programming

Every OS, regardless of model, must address six concerns. The compiler's
job is **runtime binding** -- translating these concerns to OS-native
primitives via `ext.<os>.*` modules.

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
| Account | Key-value storage slots | `ext.<os>.storage.read(key)` / `write(key, value)` |
| Stateless | Account data buffers | `ext.solana.account.data(index)` (accounts passed by caller) |
| Object | Object store (ownership graph) | `ext.sui.object.borrow(id)` / `transfer.send(obj, recipient)` |
| Journal | No persistent state | Public I/O only (`pub_read` / `pub_write`) |
| Process | Filesystem, environment | `ext.<os>.fs.read()` / `write()` |

The divine-and-authenticate pattern is specific to UTXO chains. Account-based
chains provide direct storage access. The same Trident program structure
(read state, compute, write state) applies everywhere -- only the access
mechanism differs.

### 3. Identity -- Who Is Calling

| OS family | Identity mechanism | Trident pattern |
|-----------|-------------------|-----------------|
| UTXO | Hash preimage (no sender concept) | `divine()` secret, `hash()`, `assert_eq()` |
| Account (EVM) | Protocol-level signature verification | `ext.ethereum.account.caller()` (= msg.sender) |
| Account (Starknet) | Native account abstraction | `ext.starknet.account.caller()` |
| Stateless (Solana) | Signer accounts in transaction | `ext.solana.account.is_signer(index)` |
| Object (Sui) | Transaction sender | `ext.sui.tx.sender()` |
| Journal | No identity (pure computation) | N/A |
| Process | UID/PID | `ext.<os>.process.uid()` |

### 4. Value Transfer -- How Money Moves

| OS family | Transfer mechanism | Trident pattern |
|-----------|-------------------|-----------------|
| UTXO | Create new UTXOs, destroy old ones | Kernel outputs (new UTXOs) in transaction |
| Account (EVM) | Transfer opcode | `ext.ethereum.transfer.send(to, amount)` |
| Stateless (Solana) | Lamport transfer via system program | `ext.solana.transfer.lamports(from, to, amount)` |
| Object (Sui) | Object transfer (ownership change) | `ext.sui.coin.split()`, `ext.sui.transfer.send()` |
| Journal | No value (off-chain computation) | N/A |
| Process | N/A | N/A |

### 5. Cross-Contract Interaction

| OS family | Mechanism | Trident pattern |
|-----------|-----------|-----------------|
| UTXO (Neptune) | Recursive proof verification | `ext.neptune.proof.verify_inner_proof()` |
| Account (EVM) | CALL/STATICCALL/DELEGATECALL | `ext.ethereum.call.call(address, data)` |
| Account (Starknet) | Contract calls, library calls | `ext.starknet.call.invoke(address, selector, args)` |
| Stateless (Solana) | CPI (cross-program invocation) | `ext.solana.cpi.invoke(program, accounts, data)` |
| Object (Sui) | Direct function calls on shared objects | Call functions from other modules directly |
| Cosmos | IBC messages | `ext.cosmwasm.ibc.send(channel, data)` |
| Journal | Proof composition | Recursive verification in the same journal |
| Process | Subprocess, IPC | `ext.<os>.process.exec()` |

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

## OS Families

### UTXO Model (Neptune, Nockchain, Nervos, Aleo)

Programs are **scripts** attached to transaction outputs. A lock script proves
the right to spend a UTXO. A type script validates conservation rules. The
program never sees "the blockchain" -- it receives a commitment (Merkle root)
as public input and authenticates everything against it.

**Key pattern: divine-and-authenticate.** The prover supplies private data
via `divine()`, then proves it belongs to the committed state via Merkle proofs.
This is the fundamental state access pattern for all UTXO chains.

For the complete Neptune programming model -- transaction kernels, UTXO
structure, address types, block structure, and `ext.neptune.*` API -- see
[Neptune OS Reference](../reference/os/neptune.md).

### Account Model (Ethereum, Starknet, Near, Cosmos, Ton, Polkadot)

Programs are **contracts** with persistent storage. The OS provides direct
read/write access to storage slots. Identity comes from the protocol layer
(msg.sender, caller address). The program is deployed once and called
repeatedly with different inputs.

For programming models:
[Ethereum](../reference/os/ethereum.md) |
[Starknet](../reference/os/starknet.md) |
[Near](../reference/os/near.md) |
[Cosmos](../reference/os/cosmwasm.md) |
[Ton](../reference/os/ton.md) |
[Polkadot](../reference/os/polkadot.md)

### Stateless Model (Solana)

Programs are **stateless instruction handlers**. State lives in separate
accounts that are passed into the program by the caller. The program reads
and writes account data but does not own storage. Identity comes from
signer accounts in the transaction.

For the complete Solana programming model -- accounts, PDAs, CPI, and
`ext.solana.*` API -- see [Solana OS Reference](../reference/os/solana.md).

### Object Model (Sui, Aptos)

Programs operate on **objects** (Sui) or **resources** (Aptos) with explicit
ownership. Objects can be owned (single-writer), shared (consensus-ordered),
or immutable. The type system enforces resource safety -- objects cannot be
copied or dropped unless explicitly allowed.

For programming models:
[Sui](../reference/os/sui.md) |
[Aptos](../reference/os/aptos.md)

### Journal Model (SP1, RISC Zero, OpenVM, Boundless, Succinct)

Programs are **pure computations** with no persistent state. Input comes
from a journal (public) and host communication (private). Output goes to
a journal. The proof attests that the computation was performed correctly.
No accounts, no storage, no identity.

For programming models:
[Boundless](../reference/os/boundless.md) |
[Succinct](../reference/os/succinct.md) |
[OpenVM](../reference/os/openvm-network.md)

### Process Model (Linux, macOS, WASI, Browser, Android)

Programs are **processes** with standard OS primitives: files, sockets,
stdin/stdout, environment variables. No proofs, no blockchain state.
These targets exist for testing, debugging, and running Trident programs
as conventional software.

For programming models:
[Linux](../reference/os/linux.md) |
[macOS](../reference/os/macos.md) |
[WASI](../reference/os/wasi.md) |
[Browser](../reference/os/browser.md) |
[Android](../reference/os/android.md)

---

## `std.*` vs `ext.*`

Programs that use only `std.*` modules are **fully portable** -- they compile
to any target. Programs that import `ext.<os>.*` modules are OS-specific.

| Layer | Scope | Example |
|-------|-------|---------|
| **`std.*`** (universal) | All targets | `std.crypto.hash`, `std.crypto.merkle`, `std.io.io`, `std.core.field` |
| **`ext.<os>.*`** (OS-specific) | One OS | `ext.neptune.kernel`, `ext.ethereum.storage`, `ext.solana.account` |

```
// Portable -- compiles to any backend
use std.crypto.merkle

fn verify(root: Digest, leaf: Digest, index: U32, depth: U32) {
    std.crypto.merkle.verify(root, leaf, index, depth)
}
```

```
// Ethereum-specific -- requires --target ethereum
use ext.ethereum.storage

fn read_balance(slot: Field) -> Field {
    ext.ethereum.storage.read(slot)
}
```

The compiler rejects `ext.*` imports when targeting a different OS:
`use ext.ethereum.storage` is a compile error with `--target solana`.

---

## Data Flow

```
PROVER SIDE:                              VERIFIER SIDE:

Program  ----hash---->  program_digest
                              |
PublicInput  ---------->  claim.input ---------> Claim {
                              |                    program_digest,
NonDeterminism {              |                    version,
  individual_tokens,     [execution]               input,
  digests,                    |                    output,
  ram,                        v                  }
}                        claim.output --------->    +
                              |                  Proof
      VM::trace_execution()   |                     |
             |                |                     v
             v                |              verify()
  AlgebraicExecutionTrace     |                     |
             |                |                     v
             v                |                true / false
       prove() ------------> Proof ------------->
```

On non-provable targets (EVM, WASM, native), the prover/verifier split
collapses: execution is direct, and there is no proof. The program still
uses the same I/O channels -- `pub_read` becomes calldata or stdin,
`pub_write` becomes return data or stdout.

---

## See Also

- [For Blockchain Devs](../tutorials/for-blockchain-devs.md) -- Mental model migration from Solidity, Anchor, CosmWasm, Substrate
- [Universal Design](universal-design.md) -- Multi-target compilation architecture
- [Tutorial](../tutorials/tutorial.md) -- Step-by-step guide to writing Trident programs
- [Language Reference](../reference/language.md) -- Types, operators, builtins, grammar
- [Target Reference](../reference/targets.md) -- OS model, target profiles, cost models
- [How STARK Proofs Work](stark-proofs.md) -- The proof system underlying provable execution
- [Optimization Guide](../guides/optimization.md) -- Cost reduction strategies
