# Trident Programming Model

This document describes the [Neptune](https://neptune.cash/) blockchain programming model as it applies to
Trident programs. It covers how programs run inside [Triton VM](https://triton-vm.org/), what blockchain
state they can access, and how the standard library exposes these capabilities.

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

## Triton VM Execution Model *(Triton VM default)*

Trident compiles to [TASM](https://triton-vm.org/spec/) (Triton Assembly), which runs inside [Triton VM](https://triton-vm.org/) -- a
[STARK](stark-proofs.md)-based [zero-knowledge](https://en.wikipedia.org/wiki/Zero-knowledge_proof) virtual machine. Programs are **isolated**: they have
no syscalls, no environment variables, no network access. A program's entire
world consists of:

| Resource              | Instruction          | Visible to Verifier? |
|-----------------------|----------------------|----------------------|
| **Public input**      | `read_io n`          | Yes (in Claim.input) |
| **Public output**     | `write_io n`         | Yes (in Claim.output)|
| **Secret input**      | `divine n`           | No                   |
| **Secret digests**    | `merkle_step`        | No                   |
| **RAM**               | `read_mem`/`write_mem`| No                  |
| Stack, sponge, jumps  | various              | No                   |

The verifier only ever sees the **Claim** and the **Proof** (a [STARK proof](stark-proofs.md)):

```
Claim {
    program_digest: Digest,   // hash of the program (Triton VM default: Tip5)
    version: u32,
    input: Vec<Field>,        // public input consumed by read_io
    output: Vec<Field>,       // public output produced by write_io
}
```

> **Target-dependent detail.** The hash function used for `program_digest` and
> the digest width (5 `Field` elements on Triton VM) vary by target. Use
> `std.target.DIGEST_WIDTH` rather than hardcoding `5`.

Everything else (secret input, RAM, stack states, execution trace) remains
hidden. This is the zero-knowledge property.

### Public Input vs Secret Input

- **`io.read()` / `read_io`**: Reads from public input. The verifier sees these
  values as part of the Claim. Use for data that must be publicly verifiable.

- **`io.divine()` / `divine`**: Reads from secret (nondeterministic) input. The
  verifier never sees these values. Use for witness data (preimages, proofs,
  authentication paths).

- **`merkle.step()` / `merkle_step`**: Reads a sibling digest from the secret
  digest queue and computes one [Merkle tree](https://en.wikipedia.org/wiki/Merkle_tree) step. Used to authenticate data
  against a known root hash. *(Triton VM default: Tip5-based Merkle; other
  targets use their native hash.)*

### The Divine-and-Authenticate Pattern

Since programs cannot directly access blockchain state, [Neptune](https://neptune.cash/) uses a universal
pattern:

1. The public input contains a **MAST hash** (Merkle root) of a known structure
2. The program **divines** the actual value it needs (secret input)
3. The program **authenticates** the divined value against the MAST hash using
   Merkle proofs (`merkle_step`)
4. If the proof checks out, the divined value is cryptographically trustworthy.
   If authentication fails, the assertion crashes the VM -- no proof is generated
   and the transaction is rejected

This pattern is used everywhere: accessing transaction fields, block headers,
UTXO data, timestamps, fees, etc.

## [Neptune](https://neptune.cash/) Transaction Model

### Transaction Kernel

Every Neptune transaction has a **TransactionKernel** with 8 fields, organized
as a [Merkle tree](https://en.wikipedia.org/wiki/Merkle_tree) of height 3:

| Leaf | Field              | Type                        | Description                          |
|------|--------------------|-----------------------------|--------------------------------------|
| 0    | `inputs`           | `Vec<RemovalRecord>`        | UTXOs being spent (Bloom filter idx) |
| 1    | `outputs`          | `Vec<AdditionRecord>`       | New UTXOs being created              |
| 2    | `announcements`    | `Vec<Announcement>`         | Public messages for coordination     |
| 3    | `fee`              | `NativeCurrencyAmount`      | Transaction fee (u128)               |
| 4    | `coinbase`         | `Option<NativeCurrencyAmount>` | Block reward (mining tx only)     |
| 5    | `timestamp`        | `Timestamp`                 | Transaction timestamp                |
| 6    | `mutator_set_hash` | `Digest`                    | Hash of current UTXO set state       |
| 7    | `merge_bit`        | `bool`                      | Whether this is a merged transaction |

The **kernel MAST hash** is the root of this Merkle tree. It is the primary
public input for both lock scripts and type scripts.

### Script Types

[Neptune](https://neptune.cash/) has two kinds of scripts that Trident programs implement:

#### Lock Scripts (ownership)

A lock script guards a UTXO. It proves the right to spend.

**Public input**: kernel MAST hash (5 field elements = 1 Digest)

```trident
// Read the kernel MAST hash from public input
let kernel_hash: Digest = io.read5()
```

The lock script can then divine any kernel field and authenticate it against
`kernel_hash` using Merkle proofs.

#### Type Scripts (validation)

A type script validates coin rules (e.g. "amounts balance", "timelock expired").

**Public input**: 15 field elements (3 Digests)

```trident
// Read type script inputs
let kernel_hash: Digest = io.read5()
let input_utxos_hash: Digest = io.read5()
let output_utxos_hash: Digest = io.read5()
```

Type scripts can authenticate kernel fields AND the actual UTXO/coin data.

### Announcements

Announcements are public messages embedded in transactions. They are stored at
leaf index 2 of the kernel MAST tree.

```
Announcement {
    message: Vec<Field>   // arbitrary data
}
```

In Neptune, announcements are used for:
- **UTXO notifications**: encrypted data telling a recipient about incoming funds
- **Coordination**: any public data the sender wants to attach to a transaction

The announcement message layout for UTXO notifications:
- `message[0]` = key type flag (79 = Generation, 80 = Symmetric)
- `message[1]` = receiver identifier (for efficient scanning)
- `message[2..]` = encrypted payload (UTXO + sender randomness)

### UTXO Structure

```
Utxo {
    lock_script_hash: Digest,     // hash of the lock script program
    coins: Vec<Coin>,             // the values inside
}

Coin {
    type_script_hash: Digest,     // hash of the validation program
    state: Vec<Field>,            // arbitrary data (amount, timelock, etc.)
}
```

A UTXO stores only the **hash** of its lock script, not the script itself.
The actual lock script is provided by the spender as part of the witness.

### Known Type Scripts

| Type Script      | State Interpretation              | Validation Rule                    |
|------------------|-----------------------------------|------------------------------------|
| NativeCurrency   | `state[0..4]` = amount (u128)    | sum(inputs) + coinbase = sum(outputs) + fee |
| TimeLock         | `state[0]` = release timestamp   | `release_date < tx_timestamp`      |

## [Neptune](https://neptune.cash/) Address Types

Neptune supports two address types, which differ in their cryptographic scheme:

### Generation Addresses (lattice-based)

- **[Bech32m](https://github.com/bitcoin/bips/blob/master/bip-0350.mediawiki) prefix**: `nolga` (Neptune Lattice-based Generation Address)
- **Flag value**: 79
- **Encryption**: Lattice KEM (post-quantum) wrapping AES-256-GCM
- **Lock script**: hash-lock on `lock_postimage`
  - Spending key derives `unlock_key_preimage = Tip5::hash_varlen(seed || 1)`
  - Lock checks: `hash(divine_preimage) == lock_postimage`
- **UTXO notification**: via on-chain announcements (encrypted with lattice KEM)

### Symmetric Addresses (shared-secret)

- **[Bech32m](https://github.com/bitcoin/bips/blob/master/bip-0350.mediawiki) prefix**: `nolsa` (Neptune Symmetric Address)
- **Flag value**: 80
- **Encryption**: AES-256-GCM with shared symmetric key
- **Lock script**: hash-lock on `lock_after_image`
  - Spending key derives `unlock_key = Tip5::hash_varlen(seed || "unlock_key")`
  - Lock checks: `hash(divine_preimage) == lock_after_image`
- **UTXO notification**: via off-chain private channel OR on-chain announcements

### Lock Script Pattern (both types)

Both address types use the same fundamental pattern -- a **hash lock**:

```trident
// Standard Neptune lock script pattern:
// 1. Divine the secret preimage (unlock key)
// 2. Hash it (target-dependent hash function; Triton VM default: Tip5)
// 3. Compare against the expected postimage (hardcoded in the script)
// 4. Also read the kernel hash (to bind proof to this transaction)

let preimage: Digest = io.divine5()
let computed: Digest = hash.tip5(preimage[0], preimage[1], preimage[2],
                                  preimage[3], preimage[4], 0, 0, 0, 0, 0)
let kernel_hash: Digest = io.read5()
assert.digest(computed, EXPECTED_POSTIMAGE)
```

> **Target-dependent detail.** The `hash.tip5()` call and its 10-element rate
> are specific to Triton VM. Portable code should use `std.crypto.hash.compress()`
> and `std.target.HASH_RATE`.

The key difference between Generation and Symmetric is **how the preimage is
derived from the seed** and **how UTXO notifications are encrypted**, not the
lock script structure itself.

## Block Structure

### Block Kernel MAST (3 leaves)

| Leaf | Field       |
|------|-------------|
| 0    | header MAST hash |
| 1    | body MAST hash   |
| 2    | appendix         |

### Block Header MAST (8 leaves, height 3)

| Leaf | Field                      | Type           |
|------|----------------------------|----------------|
| 0    | `version`                  | u32            |
| 1    | `height`                   | BlockHeight    |
| 2    | `prev_block_digest`        | Digest         |
| 3    | `timestamp`                | Timestamp      |
| 4    | `pow`                      | ProofOfWork    |
| 5    | `cumulative_proof_of_work` | ProofOfWork    |
| 6    | `difficulty`               | U32s<5>        |
| 7    | `guesser_receiver_data`    | encrypted data |

### Block Body MAST (4 leaves)

| Leaf | Field                      |
|------|----------------------------|
| 0    | transaction_kernel MAST hash |
| 1    | mutator_set_accumulator    |
| 2    | lock_free_mmr_accumulator  |
| 3    | block_mmr_accumulator      |

## Standard Library Reference

### Low-Level Intrinsics (`std.core`, `std.io`)

These modules wrap individual VM instructions:

| Module              | Key Functions                                    |
|---------------------|--------------------------------------------------|
| `std.io.io`         | `read`, `read5`, `write`, `write5`, `divine`, `divine5` |
| `std.crypto.hash`   | `tip5`, `sponge_init`, `sponge_absorb`, `sponge_squeeze` |
| `std.core.field`    | `add`, `mul`, `sub`, `neg`, `inv`                |
| `std.core.u32`      | `log2`, `pow`, `popcount`                        |
| `std.core.convert`  | `as_u32`, `as_field`, `split`                    |
| `std.io.mem`        | `read`, `write`, `read_block`, `write_block`     |
| `std.core.assert`   | `is_true`, `eq`, `digest`                        |
| `std.core.xfield`   | `new`, `inv`, `xx_dot_step`, `xb_dot_step`       |

### High-Level Blockchain Modules (`std.crypto`, `std.io`)

These modules compose intrinsics into Neptune-specific patterns:

| Module              | Purpose                                          |
|---------------------|--------------------------------------------------|
| `std.crypto.merkle` | Merkle tree step + verify inclusion proofs        |
| `std.crypto.auth`   | Hash-lock authentication (lock script patterns)   |
| `std.io.storage`    | Key-value RAM storage patterns                    |

### std.neptune.kernel -- Transaction Kernel Access

The kernel module provides functions to authenticate transaction kernel fields
against the kernel MAST hash received as public input. This module is
Neptune-specific.

```trident
use std.neptune.kernel

// Read the kernel MAST hash from public input
let kh: Digest = io.read5()

// Authenticate and retrieve individual fields
let ts: Field = kernel.timestamp(kh)
let fee: Field = kernel.fee(kh)
```

Internally, each function:
1. Divines the field value from secret input
2. Hashes the BFieldCodec-encoded value
3. Uses `merkle_step` to walk up to the MAST root
4. Asserts the computed root matches the provided kernel hash

### std.crypto.merkle -- Merkle Tree Operations

```trident
use std.crypto.merkle

// Single step up the tree (intrinsic, uses divine sibling)
let (parent_idx, parent): (U32, Digest) = merkle.step(idx, d0, d1, d2, d3, d4)

// Verify full inclusion proof (depth steps from leaf to root)
merkle.verify(leaf, root, leaf_index, depth)
```

### std.crypto.auth -- Lock Script Authentication

```trident
use std.crypto.auth

// Simple hash-preimage lock (standard Neptune pattern)
auth.verify_preimage(expected_hash)
```

## Arithmetic

All arithmetic in Trident operates on `Field` elements. The `+` and `*`
operators map to field addition and multiplication, which wrap modulo the
target's prime automatically -- there is no overflow error. This is true on
every compilation target.

> **Triton VM default.** The native field is the [Goldilocks prime](https://xn--2-umb.com/22/goldilocks/)
> `p = 2^64 - 2^32 + 1`. Other targets use different primes (see
> [Universal Design](universal-design.md)).

Practical consequences:

- `Field` elements range from 0 to p-1
- `1 - 2` in field arithmetic gives `p - 1`, not `-1`. Use `std.core.field.sub()` to make this explicit
- Integer comparison (`<`, `>`) requires explicit `as_u32` conversion
- For amounts/balances, use u128 encoding (4 `Field` elements)

## Data Flow Summary

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

## See Also

- [Tutorial](tutorial.md) -- Step-by-step guide to writing Trident programs
- [Language Reference](reference.md) -- Quick lookup: types, operators, builtins, grammar
- [Language Specification](spec.md) -- Complete language reference
- [Optimization Guide](optimization.md) -- Cost reduction strategies for all six tables
- [How STARK Proofs Work](stark-proofs.md) -- The proof system underlying Triton VM execution
- [Error Catalog](errors.md) -- All compiler error messages explained
- [For Developers](for-developers.md) -- Zero-knowledge concepts explained for conventional programmers
- [For Blockchain Devs](for-blockchain-devs.md) -- Mental model migration from smart contracts
- [Vision](vision.md) -- Why Trident exists and what you can build
- [Universal Design](universal-design.md) -- Multi-target compilation and the Field abstraction
- [Comparative Analysis](analysis.md) -- Triton VM vs. every other ZK system
- [Neptune Cash](https://neptune.cash/) -- Reference blockchain using Triton VM
- [Triton VM specification](https://triton-vm.org/spec/) -- Target VM instruction set
