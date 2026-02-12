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
> [Multi-Target Compilation](multi-target.md) for the multi-target story.

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

For how each OS family handles entry points, state, identity, signals,
cross-contract interaction, and events, see [OS Abstraction](os-abstraction.md).

---

## The Portable OS Layer: `std.*` → `os.*` → `os.<os>.*`

The stdlib has three tiers. Each trades portability for OS access:

```
std.*          S0 — Proof primitives      All 20 VMs, all 25 OSes
os.*           S1 — Portable OS           All blockchain + traditional OSes
os.<os>.*      S2 — OS-native             One specific OS
```

| Tier | Layer | Scope | Example |
|------|-------|-------|---------|
| S0 | **`std.*`** | All targets | `vm.crypto.hash`, `std.crypto.merkle`, `vm.io.io` |
| S1 | **`os.*`** | All OSes with the concept | `os.state.read`, `os.neuron.id`, `os.neuron.auth` |
| S2 | **`os.<os>.*`** | One OS | `os.neptune.kernel`, `os.ethereum.storage`, `os.solana.account` |

**S0 — `std.*`**: Pure computation. Hash, Merkle, field arithmetic, I/O
channels. Works everywhere but cannot touch state, identity, or money.

**S1 — `os.*`**: Portable OS abstraction. Names the *intent* (identify
neuron, send signal, read state) — the compiler picks the *mechanism* based
on the target OS. A program using `os.state.read(key)` compiles to SLOAD
on Ethereum, `account.data` on Solana, `dynamic_field.borrow` on Sui, and
`divine()` + `merkle_authenticate` on Neptune. Same source, different lowering.

**S2 — `os.<os>.*`**: OS-native API. Full access to OS-specific features
(PDAs, object ownership, CPI, kernel MAST, IBC). Required when the portable
layer cannot express what you need.

### `os.*` Modules

| Module | Intent | Compile error when... |
|--------|--------|-----------------------|
| `os.neuron` | Identity and authorization | UTXO (no caller for `id()`), Journal (no identity) |
| `os.signal` | Send weighted edges between neurons | Journal + process targets (no value) |
| `os.state` | Read/write persistent state | Journal targets (no state) |
| `os.time` | Current time and step | -- (all OSes have time) |
| `os.event` | Observable side effects | -- (uses `reveal`/`seal` directly) |

The compiler emits a clear error when an `os.*` function targets an OS
that doesn't support the concept. For example, `os.neuron.id()` on
Neptune produces: *"UTXO chains have no caller — use `os.neuron.auth()`
or `os.neptune.*` for hash-preimage identity."*

### Choosing a Tier

```
// S0 — pure math, any target
use std.crypto.merkle
fn verify(root: Digest, leaf: Digest, index: U32, depth: U32) {
    std.crypto.merkle.verify(root, leaf, index, depth)
}

// S1 — portable OS, any blockchain
use os.state
use os.neuron
fn guarded_write(key: Field, value: Field, credential: Digest) {
    os.neuron.auth(credential)
    os.state.write(key, value)
}

// S2 — OS-native, Ethereum only
use os.ethereum.storage
fn read_balance(slot: Field) -> Field {
    os.ethereum.storage.read(slot)
}
```

A program can mix all three tiers. Use `std.*` for portable math, `os.*`
for portable OS interaction, and `os.<os>.*` when you need OS-specific
features. The compiler rejects `os.<os>.*` imports when targeting a different OS:
`use os.ethereum.storage` is a compile error with `--target solana`.

For full `os.*` API specifications and per-OS lowering tables, see
[Standard Library Reference](../reference/stdlib.md).

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
- [Multi-Target Compilation](multi-target.md) -- Compiler architecture and backend traits
- [OS Abstraction](os-abstraction.md) -- OS families, six concerns, portable API
- [Tutorial](../tutorials/tutorial.md) -- Step-by-step guide to writing Trident programs
- [Language Reference](../reference/language.md) -- Types, operators, builtins, grammar
- [Target Reference](../reference/targets.md) -- OS model, integration tracking, how-to-add checklists
- [How STARK Proofs Work](stark-proofs.md) -- The proof system underlying provable execution
- [Optimization Guide](../guides/optimization.md) -- Cost reduction strategies
