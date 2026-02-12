# 🔱 Neptune

[← Target Reference](../../docs/reference/targets.md) | VM: [TRITON](../../vm/triton/README.md)

Neptune is the provable blockchain powered by TRITON. Programs produce
STARK proofs of correct execution. Same bytecode output as bare TRITON
(`.tasm`), but with OS-level runtime bindings for UTXOs, transaction
kernels, and recursive proof composition.

---

## Runtime Parameters

| Parameter | Value |
|---|---|
| VM | TRITON |
| Runtime binding | `os.neptune.*` |
| Account model | UTXO |
| Storage model | Merkle-authenticated |
| Transaction model | Proof-based |
| Cost model | Table rows (proving cost) |
| Cross-chain | -- |

---

## Programming Model

### Entry Points

Neptune has two kinds of scripts that Trident programs implement:

**Lock scripts** guard a UTXO -- they prove the right to spend.

Public input: kernel MAST hash (1 Digest = 5 field elements).

```
program my_lock_script

use vm.io.io
use std.crypto.auth

fn main() {
    let kernel_hash: Digest = divine5()
    auth.verify_preimage(EXPECTED_POSTIMAGE)
}
```

**Type scripts** validate coin rules (e.g., "amounts balance," "timelock
expired").

Public input: 3 Digests (kernel hash, input UTXOs hash, output UTXOs hash).

```
program my_type_script

use vm.io.io

fn main() {
    let kernel_hash: Digest = divine5()
    let input_utxos_hash: Digest = divine5()
    let output_utxos_hash: Digest = divine5()
    // ... validate conservation rules ...
}
```

### State Access (Divine-and-Authenticate)

Programs cannot directly access blockchain state. Neptune uses a universal
pattern:

1. Public input contains a **MAST hash** (Merkle root) of a known structure
2. The program **divines** the actual value (secret input)
3. The program **authenticates** the divined value against the MAST hash
   using Merkle proofs (`merkle_step`)
4. If authentication fails, the VM crashes -- no proof is generated

```
use vm.io.io
use os.neptune.kernel

fn main() {
    let kernel_hash: Digest = divine5()

    // Authenticate individual kernel fields against the root
    let fee: Field = kernel.authenticate_fee(kernel_hash)
    let ts: Field = kernel.authenticate_timestamp(kernel_hash)
}
```

Internally, `kernel.authenticate_fee()`:
1. Divines the fee value from secret input
2. Hashes the BFieldCodec-encoded value
3. Uses `merkle_step` to walk up to the MAST root (3 steps, height-3 tree)
4. Asserts the computed root matches `kernel_hash`

This pattern applies to every piece of state: kernel fields, UTXO data,
block headers, mutator set membership.

### Identity and Authorization

There is no `msg.sender`. Authorization is explicit: the prover divines
a secret and proves knowledge of it by hashing and asserting the hash
matches an expected value.

```
use vm.io.io
use vm.crypto.hash
use vm.core.assert

fn verify_auth(expected: Digest) {
    let preimage: Digest = divine5()
    let computed: Digest = hash(preimage[0], preimage[1], preimage[2],
                                preimage[3], preimage[4], 0, 0, 0, 0, 0)
    assert_digest(computed, expected)
}
```

This is **account abstraction by default**. The "secret" can be anything:
a private key, a Shamir share, a biometric hash, a hardware attestation,
or the output of another ZK proof.

Neptune supports two address types:
- **Generation addresses** (`nolga` prefix) -- lattice-based KEM (post-quantum),
  AES-256-GCM encrypted UTXO notifications on-chain
- **Symmetric addresses** (`nolsa` prefix) -- shared symmetric key,
  AES-256-GCM, off-chain or on-chain notifications

Both use hash-lock scripts: `hash(divine_preimage) == expected_postimage`.

### Value Transfer

Value moves by creating and destroying UTXOs in a transaction. The
transaction kernel specifies inputs (UTXOs being spent) and outputs
(new UTXOs being created). Type scripts enforce conservation rules.

```
Utxo {
    lock_script_hash: Digest,     // hash of the ownership program
    coins: Vec<Coin>,             // values inside
}

Coin {
    type_script_hash: Digest,     // hash of the validation program
    state: Vec<Field>,            // arbitrary data (amount, timelock, etc.)
}
```

Known type scripts:

| Type Script | State | Validation |
|-------------|-------|------------|
| NativeCurrency | `state[0..4]` = amount (u128) | sum(inputs) + coinbase = sum(outputs) + fee |
| TimeLock | `state[0]` = release timestamp | `release_date < tx_timestamp` |

### Cross-Contract Interaction

Programs are isolated -- no external calls. Composition happens through
**recursive proof verification**: a program can verify that another STARK
proof is valid inside its own execution.

```
use os.neptune.proof

fn main() {
    // Verify an inner proof inside this program's execution
    proof.verify_inner_proof(NUM_FRI_ROUNDS)

    // Or aggregate multiple proofs
    proof.aggregate_proofs(NUM_PROOFS, NUM_FRI_ROUNDS)
}
```

This is how Neptune achieves composability without shared mutable state.
Each script produces its own proof. A merge transaction can combine
multiple transaction proofs into one.

### Events

Neptune uses **announcements** -- public messages embedded in transactions
at leaf index 2 of the kernel MAST tree.

In Trident, events map to announcements:

```
event Transfer { from: Digest, to: Digest, amount: Field }

// reveal -- all fields visible to verifier
reveal Transfer { from: sender, to: receiver, amount: value }

// seal -- only commitment digest visible
seal Transfer { from: sender, to: receiver, amount: value }
```

`seal` requires Tier 2 sponge support (native on TRITON).

Announcements are used for UTXO notifications:
- `message[0]` = key type flag (79 = Generation, 80 = Symmetric)
- `message[1]` = receiver identifier (for efficient scanning)
- `message[2..]` = encrypted payload (UTXO + sender randomness)

---

## Transaction Kernel

Every Neptune transaction has a **TransactionKernel** with 8 fields,
organized as a Merkle tree of height 3:

| Leaf | Field | Type | Description |
|------|-------|------|-------------|
| 0 | `inputs` | `Vec<RemovalRecord>` | UTXOs being spent |
| 1 | `outputs` | `Vec<AdditionRecord>` | New UTXOs being created |
| 2 | `announcements` | `Vec<Announcement>` | Public messages |
| 3 | `fee` | `NativeCurrencyAmount` | Transaction fee (u128) |
| 4 | `coinbase` | `Option<NativeCurrencyAmount>` | Block reward (mining only) |
| 5 | `timestamp` | `Timestamp` | Transaction timestamp |
| 6 | `mutator_set_hash` | `Digest` | Current UTXO set state |
| 7 | `merge_bit` | `bool` | Merged transaction flag |

The **kernel MAST hash** is the root of this tree and serves as the
primary public input for all scripts.

---

## Block Structure

### Block Kernel MAST (3 leaves)

| Leaf | Field |
|------|-------|
| 0 | header MAST hash |
| 1 | body MAST hash |
| 2 | appendix |

### Block Header MAST (8 leaves, height 3)

| Leaf | Field | Type |
|------|-------|------|
| 0 | `version` | u32 |
| 1 | `height` | BlockHeight |
| 2 | `prev_block_digest` | Digest |
| 3 | `timestamp` | Timestamp |
| 4 | `pow` | ProofOfWork |
| 5 | `cumulative_proof_of_work` | ProofOfWork |
| 6 | `difficulty` | U32s<5> |
| 7 | `guesser_receiver_data` | encrypted data |

### Block Body MAST (4 leaves)

| Leaf | Field |
|------|-------|
| 0 | transaction_kernel MAST hash |
| 1 | mutator_set_accumulator |
| 2 | lock_free_mmr_accumulator |
| 3 | block_mmr_accumulator |

---

## Portable Alternative (`os.*`)

Programs that don't need Neptune-specific features can use `os.*`
instead of `os.neptune.*` for cross-chain portability:

| `os.neptune.*` (this OS only) | `os.*` (any OS) |
|--------------------------------|---------------------|
| `os.neptune.kernel.authenticate_*` + divine/merkle | `os.state.read(key)` → auto-generates divine + merkle_authenticate |
| Hash preimage via `std.crypto.auth` | `os.neuron.auth(cred)` → divine + hash + assert_eq |
| Manual UTXO output construction | `os.signal.send(from, to, amt)` → emit output UTXO |

**Note:** `os.neuron.id()` is a **compile error** on Neptune — UTXO chains
have no caller concept. Use `os.neuron.auth(credential)` for authorization.

Use `os.neptune.*` when you need: kernel MAST authentication, recursive proof
verification, UTXO structure access, or other Neptune-specific features. See
[os.md](../../docs/reference/os.md) for the full `os.*` API.

---

## Ecosystem Mapping

| Neptune concept | Trident equivalent |
|---|---|
| Lock script | `program` with `fn main()`, public input = kernel MAST hash |
| Type script | `program` with `fn main()`, public input = 3 Digests |
| UTXO | Struct of lock_script_hash + coins, authenticated via Merkle |
| Coin | Struct of type_script_hash + state, validated by type script |
| Kernel field access | `os.neptune.kernel.authenticate_*(kernel_hash)` |
| Spending authorization | Hash preimage via `divine5()` + `hash()` + `assert_digest()` |
| Token balance | NativeCurrency type script, `state[0..4]` = u128 amount |
| Timelock | TimeLock type script, `state[0]` = release timestamp |
| Announcements | `reveal` / `seal` events |
| UTXO notification | Encrypted announcement with key type flag |
| Proof composition | `os.neptune.proof.verify_inner_proof()` |
| Program identity | Tip5 hash of the compiled program |

---

## `os.neptune.*` API Reference

| Module | Function | Description |
|--------|----------|-------------|
| `os.neptune.kernel` | `read_lock_script_hash()` | Read kernel MAST hash (lock script entry) |
| | `read_type_script_hashes()` | Read 3 Digests (type script entry) |
| | `leaf_inputs()` .. `leaf_merge_bit()` | Leaf index constants (0-7) |
| | `authenticate_field(hash, leaf_idx)` | Merkle-authenticate any kernel field |
| | `authenticate_fee(hash)` | Authenticate and return fee |
| | `authenticate_timestamp(hash)` | Authenticate and return timestamp |
| `os.neptune.utxo` | `authenticate(divined, expected)` | Verify divined digest matches expected |
| `os.neptune.xfield` | `new(a, b, c)` | Construct XField from 3 base fields |
| | `inv(a)` | Extension field inverse |
| | `xx_dot_step(acc, ptr_a, ptr_b)` | XField * XField dot product step |
| | `xb_dot_step(acc, ptr_a, ptr_b)` | XField * BField dot product step |
| `os.neptune.proof` | `parse_claim()` | Read Claim from public input |
| | `hash_public_io(claim)` | Hash all public I/O into binding digest |
| | `fri_verify(commitment, seed, rounds)` | Full FRI verification chain |
| | `verify_inner_proof(num_fri_rounds)` | End-to-end inner proof verification |
| | `aggregate_proofs(num_proofs, rounds)` | Batch N proofs into 1 outer proof |
| `os.neptune.recursive` | `xfe_inner_product(ptr_a, ptr_b, count)` | XField inner product accumulation |
| | `xb_inner_product(ptr_a, ptr_b, count)` | XField * BField inner product |
| | `read_claim()` | Read (program_digest, num_inputs, num_outputs) |
| | `verify_commitment(expected)` | Authenticate FRI commitment roots |
| `os.neptune.registry` | Op 0: `REGISTER` | Add definition to on-chain registry |
| | Op 1: `VERIFY` | Prove definition is registered + verified |
| | Op 2: `UPDATE` | Update verification certificate |
| | Op 3: `LOOKUP` | Authenticate definition against registry |
| | Op 4: `EQUIV` | Register equivalence claim between definitions |

---

## Notes

Neptune is the reference implementation of the Trident OS model. It is the
only OS with fully implemented `os.neptune.*` bindings (6 modules, ~500 lines of
Trident code in `ext/neptune/`). All other OS bindings are designed but not
yet implemented.

For VM details, see [triton.md](../../vm/triton/README.md).
For the divine-and-authenticate pattern in depth, see
[Programming Model](../../docs/explanation/programming-model.md).
For Solidity-to-Trident mental model migration, see
[For Onchain Devs](../../docs/explanation/for-onchain-devs.md).
