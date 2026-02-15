# 🧬 Trident Programming Model

How programs interact with the outside world. Trident compiles to 20 VMs
and 25 OSes -- each has a different programming model, but they all share
the same universal foundation.

---

## 🧬 The Universal Primitive: `Field`

Every zkVM computes over a finite field. `Field` is the universal type --
the specific prime is an implementation detail of the proof system, not a
semantic property of the program. All Trident values decompose into `Field`
elements: a `Digest` is five, a `u128` is four, and so on.

> Target-dependent detail. The Triton VM default field is the Goldilocks
> prime `p = 2^64 - 2^32 + 1`. Other targets use different primes. Programs
> should never depend on the specific modulus.

## 🌐 I/O Model

Every Trident program communicates through three channels:

| Channel | Instruction | Visible to verifier? | Use for |
|---------|-------------|----------------------|---------|
| Public input | `pub_read()` | Yes | Data the verifier must see |
| Public output | `pub_write()` | Yes | Results and commitments |
| Secret input | `divine()` | No | Witness data, private state |

On provable targets, the verifier sees only the Claim (program hash +
public I/O) and a Proof. On non-provable targets (EVM, WASM, native),
channels map to native I/O: calldata, return data, storage reads.

## 🧮 Arithmetic

`+` and `*` are field addition and multiplication, wrapping modulo the
target's prime. Practical consequences:

- `1 - 2` gives `p - 1`, not `-1`. Use `sub(a, b)`.
- Integer comparison (`<`, `>`) requires explicit `as_u32` conversion.
- For amounts/balances, use u128 encoding (4 `Field` elements).

---

## 🖥️ The Three-Tier Namespace

Programs use three tiers. Each trades portability for OS access:

| Tier | Prefix | Scope | Example |
|------|--------|-------|---------|
| S0 | `vm.*` / `std.*` | All targets | `vm.crypto.hash`, `std.crypto.merkle` |
| S1 | `os.*` | All OSes with the concept | `os.state.read`, `os.neuron.id` |
| S2 | `os.<os>.*` | One OS | `os.neptune.kernel`, `os.ethereum.storage` |

S0 -- Pure computation. Works everywhere. Cannot touch state, identity,
or money.

S1 -- Portable OS abstraction. Names the *intent* (identify neuron, send
signal, read state) -- the compiler picks the *mechanism* for the target OS.

S2 -- OS-native API. Full access to OS-specific features. Importing any
`os.<os>.*` module locks the program to that OS.

### `os.*` Modules

| Module | Purpose | Lowers to |
|--------|---------|-----------|
| `os.neuron` | Identity, authorization | `msg.sender` (EVM), `divine()+hash()` (Neptune) |
| `os.signal` | Value transfer | `CALL(to, amount)` (EVM), emit UTXO (Neptune) |
| `os.token` | Token operations (PLUMB) | ERC-20/721 (EVM), SPL (Solana), TSP-1/2 (Neptune) |
| `os.state` | Persistent storage | `SLOAD/SSTORE` (EVM), account data (Solana) |
| `os.time` | Clock | `block.timestamp` (EVM), kernel timestamp (Neptune) |

The less `os.<os>.*` code in a program, the more portable it is.

---

## 🖥️ The Six Concerns of OS Programming

Every OS must address six concerns. The compiler's job is runtime
binding -- translating each to OS-native primitives.

### 1. Entry Points

| OS family | Entry point |
|-----------|-------------|
| UTXO (Neptune, Nockchain, Nervos) | Script execution per UTXO spent/created |
| Account (Ethereum, Starknet, Near) | Exported functions on a deployed contract |
| Stateless (Solana) | Single instruction handler, accounts passed in |
| Object (Sui, Aptos) | Entry functions on owned/shared objects |
| Journal (SP1, RISC Zero, OpenVM) | `fn main()` -- pure computation |
| Process (Linux, macOS, WASI) | `fn main()` -- argc/argv, stdin/stdout |

### 2. State Access

| OS family | State model |
|-----------|-------------|
| UTXO | Merkle tree of UTXOs. Divine leaf data, authenticate against root. |
| Account | Key-value storage slots. Direct read/write. |
| Stateless | Account data buffers. Passed in by caller. |
| Object | Object store with ownership graph. |
| Journal | No persistent state. Public I/O only. |
| Process | Filesystem, environment. |

### 3. Identity

| OS family | Identity mechanism |
|-----------|-------------------|
| UTXO | Hash preimage (no sender concept) |
| Account (EVM) | Protocol-level signature: `msg.sender` |
| Stateless (Solana) | Signer accounts in transaction |
| Object (Sui) | Transaction sender |
| Journal | No identity (pure computation) |
| Process | UID/PID |

### 4. Signals (Value Transfer)

| OS family | Mechanism |
|-----------|-----------|
| UTXO | Create new UTXOs, destroy old ones |
| Account (EVM) | Transfer opcode |
| Stateless (Solana) | Lamport transfer via system program |
| Object (Sui) | Object transfer (ownership change) |
| Journal / Process | N/A |

### 5. Cross-Contract Interaction

| OS family | Mechanism |
|-----------|-----------|
| UTXO (Neptune) | Recursive proof verification |
| Account (EVM) | CALL/STATICCALL/DELEGATECALL |
| Stateless (Solana) | CPI (cross-program invocation) |
| Object (Sui) | Direct function calls on shared objects |
| Cosmos | IBC messages |
| Journal | Proof composition |

### 6. Events

| OS family | Mechanism |
|-----------|-----------|
| UTXO (Neptune) | `reveal` (public) / `seal` (hashed commitment) |
| Account (EVM) | LOG0-LOG4 opcodes |
| Stateless (Solana) | Program logs / events |
| Journal | Journal output (`pub_write`) |
| Process | stdout / structured logging |

---

## 🌐 OS Families

### UTXO Model (Neptune, Nockchain, Nervos, Aleo)

Programs are scripts attached to transaction outputs. The program never
sees "the blockchain" -- it receives a commitment (Merkle root) as public
input and authenticates everything against it.

Key pattern: divine-and-authenticate. The prover supplies private data
via `divine()`, then proves it belongs to the committed state via Merkle
proofs.

### Account Model (Ethereum, Starknet, Near, Cosmos, Ton, Polkadot)

Programs are contracts with persistent storage. The OS provides direct
read/write access to storage slots. Identity comes from the protocol layer.

### Stateless Model (Solana)

Programs are stateless instruction handlers. State lives in separate
accounts passed in by the caller.

### Object Model (Sui, Aptos)

Programs operate on objects with explicit ownership. The type system
enforces resource safety -- objects cannot be copied or dropped unless
explicitly allowed.

### Journal Model (SP1, RISC Zero, OpenVM, Boundless, Succinct)

Programs are pure computations with no persistent state. Input comes
from a journal (public) and host communication (private).

### Process Model (Linux, macOS, WASI, Browser, Android)

Programs are processes with standard OS primitives. No proofs, no
blockchain state. For testing, debugging, and conventional execution.

---

## 🔄 Data Flow

```text
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
       prove() ------------> Proof ---------> true / false
```

On non-provable targets, the prover/verifier split collapses: execution
is direct, no proof. The program still uses the same I/O channels.

---

## 🔗 See Also

- [OS Reference](../../reference/os.md) -- Full `os.*` API and per-OS lowering tables
- [Multi-Target Compilation](multi-target.md) -- Compiler architecture and backend traits
- [Gold Standard](gold-standard.md) -- PLUMB token standards and capability library
- [For Onchain Devs](for-onchain-devs.md) -- Migration from Solidity, Anchor, CosmWasm
- [Language Reference](../../reference/language.md) -- Types, operators, builtins, grammar
