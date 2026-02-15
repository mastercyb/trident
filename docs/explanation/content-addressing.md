# 🔗 Content-Addressed Code

*Every Trident function is identified by a cryptographic hash of what it computes,
not what it is called. Names are metadata. The hash is the identity.*

---

## 🔗 1. What Is Content-Addressed Code

In a traditional programming environment, code is identified by its file path, module
name, or package version. These are all mutable, ambiguous, and external to the code
itself. Two identical functions in different files have different identities; a renamed
function appears to be a new function; a minor version bump may or may not change
behavior.

Content-addressed code inverts this model. Each function's identity is a cryptographic
hash of its normalized abstract syntax tree (AST). Two functions that compute the same
thing produce the same hash, regardless of variable names, formatting, or file location.
A renamed function keeps its hash. A changed function gets a new hash automatically.

This model was pioneered by [Unison](https://www.unison-lang.org/). Trident applies it
to a domain where content addressing is not merely convenient but cryptographically
essential: provable computation.

### Why It Matters for Provable Computation

A zkVM verifier already checks proofs against a program hash -- the program *is* its
hash at the verification layer. Content addressing pushes this identity up to the
source level, creating a single identity that spans from writing code to deploying
verified proofs:

- Verification certificates prove properties of a specific computation, identified
  by hash. Change the code, get a new hash; the old certificate no longer applies.
- Proving cost is deterministic for a given computation. The hash indexes into the
  cost cache -- same hash, same cost, always.
- Cross-chain equivalence reduces to hash comparison. If two deployments share a
  source hash, they run the same computation.
- Audit results attach to hashes, not names. An audit of `#a7f3b2c1` is an audit
  of that exact computation forever.

---

## 🏗️ 2. How It Works

Trident's content hashing pipeline normalizes the AST, serializes it deterministically,
and hashes the result with Poseidon2 over the Goldilocks field.

### 2.1 AST Normalization

Before hashing, the AST is normalized so that semantically identical functions produce
identical byte sequences. The normalization steps are:

#### Step 1: Replace variable names with de Bruijn indices

```trident
// Before normalization:
fn transfer(sender_balance: Field, amount: Field) -> Field {
    let new_balance = sender_balance - amount
    new_balance
}

// After normalization (de Bruijn indices):
fn (#0: Field, #1: Field) -> Field {
    let #2 = #0 - #1
    #2
}
```

Variable names are metadata, not identity. The function `transfer(a, b)` and
`transfer(x, y)` with identical bodies produce identical hashes.

#### Step 2: Replace dependency references with their content hashes

```trident
// Before:
let d = hash(input)

// After:
let d = #f8a2b1c3(input)    // #f8a2b1c3 is the content hash of the hash function
```

Dependencies are pinned by hash. If a called function changes, its hash changes,
and all callers get new hashes too. Propagation is automatic and exact.

#### Step 3: Canonicalize struct field ordering

```trident
// These produce the same hash:
let p = Point { x: 1, y: 2 }
let p = Point { y: 2, x: 1 }     // fields sorted alphabetically before hashing
```

#### Step 4: Strip metadata

Comments, documentation, source location, formatting, and specification annotations
(`#[requires]`, `#[ensures]`) are all stripped before computing the computational hash.
Only the executable content contributes.

### 2.2 Deterministic Serialization

The normalized AST is serialized to a deterministic byte sequence using a tagged binary
encoding. Each node is prefixed with a one-byte type tag (e.g., `0x01` for function
definitions, `0x03` for variables by de Bruijn index, `0x07` for addition). Types
have their own tag range (e.g., `0x80` for `Field`, `0x86` for `Digest`).

A version byte (`0x01` for the current format) prefixes every serialized output. The
format is frozen per version -- once released, it never changes. This guarantees that
the same source code always produces the same hash within a given version.

The full tag table is defined in `src/hash.rs`.

### 2.3 Poseidon2 Hashing

The serialized bytes are hashed with Poseidon2 over the Goldilocks field
(p = 2^64 - 2^32 + 1). This is a SNARK-friendly algebraic hash:

- State width 8, rate 4, capacity 4
- S-box x^7, 8 full rounds + 22 partial rounds
- 256-bit output (4 Goldilocks field elements)

Poseidon2 was chosen over a conventional hash (SHA3, BLAKE3) because content hashes
are cheaply provable inside ZK proofs. This enables trustless compilation verification
and on-chain registries where the hash itself can be verified in-circuit. Round
constants are derived deterministically from BLAKE3 for reproducibility.

### 2.4 Hash Display

Hashes are displayed in two forms:

| Form | Example | Use |
|------|---------|-----|
| Short (40-bit base-32) | `#a7f3b2c1` | CLI output, human reference |
| Full (256-bit hex) | `a7f3b2c1d4e5...` | Internal storage, registry keys |

The short form is for human convenience only. The full hash is always used internally.

### 2.5 Hash Composition

A function's hash includes the hashes of all functions it calls. This creates a
Merkle-like structure: if `verify_merkle` changes, its hash changes, which changes the
hash of every function that calls it. Only truly affected functions get new hashes.

Circular dependencies are impossible because Trident enforces a module DAG. The hash
computation always terminates.

### 2.6 What Does Not Affect the Hash

Since hashing operates on the normalized AST, the hash is invariant to variable names, comments, whitespace, formatting, argument order in commutative expressions, and import paths. Only the computation's structure and the hashes of called functions matter.

---

## 📦 3. Using the Definitions Store

The definitions store (`trident store`) is a hash-keyed storage
inspired by Unison's codebase model. Every function is stored by its content hash,
with names as mutable pointers into the hash-keyed store.

### 3.1 Codebase Location

The codebase is stored at `~/.trident/codebase/` by default. Override with the
`$TRIDENT_CODEBASE_DIR` environment variable.

```text
~/.trident/codebase/
  defs/
    <2-char-prefix>/
      <full-hex-hash>.def       # serialized definition
  names.txt                     # name -> hash mappings
  history.txt                   # name binding history
```

### 3.2 Adding Definitions

Parse a `.tri` file and store all its function definitions in the codebase:

```bash
trident store add myfile.tri
```

Output:

```text
Added 3 new definitions, updated 1, unchanged 2
  #a7f3b2c1  verify_merkle      (new)
  #c4e9d1a8  transfer_token     (new)
  #b4c5d6e7  main               (updated)
```

Each function is hashed independently. If a function's hash already exists in the
codebase (even from a different file or author), the existing definition is reused
and the name pointer is updated.

### 3.3 Listing Definitions

```bash
trident store list
```

Shows all named definitions, sorted alphabetically:

```text
  #b4c5d6e7  main
  #c4e9d1a8  transfer_token
  #a7f3b2c1  verify_merkle
```

### 3.4 Viewing Definitions

View a definition by name or by hash prefix:

```bash
trident store view verify_merkle
trident store view #a7f3b2
```

Output includes the function source, its hash, spec annotations, and dependency list:

```trident
-- verify_merkle #a7f3b2c1
pub fn verify_merkle(root: Digest, leaf: Digest, index: U32, depth: U32) {
    ...
}

-- Dependencies:
--   hash #f8a2b1c3
--   divine_digest #e2f1b3a9
```

### 3.5 Renaming Definitions

Renaming is instant and non-breaking because it only updates the name pointer.
The hash (and therefore all cached compilation and verification results) is unchanged:

```bash
trident store rename old_name new_name
```

### 3.6 Viewing Dependencies

Show what a definition depends on and what depends on it:

```bash
trident store deps transfer_token
```

Output:

```text
Dependencies:
  #a7f3b2c1  verify_merkle
  #d4e5f6a7  check_balance

Dependents:
  #f7a2b1c3  batch_transfer
```

### 3.7 Name History

Show all hashes a name has pointed to over time:

```bash
trident store history verify_merkle
```

This is useful for tracking how a function has evolved. Old definitions remain in the
codebase (append-only semantics) -- they are never deleted.

### 3.8 Codebase Statistics

```bash
trident store stats
```

Shows the number of unique definitions, name bindings, and total source bytes.

---

## 🔐 4. Content Hashing

The `trident hash` command computes content hashes for all functions in a file without
storing them in the codebase.

### 4.1 Basic Usage

```bash
trident hash myfile.tri
```

Output:

```text
File: #e1d2c3b4 myfile.tri
  #a7f3b2c1 verify_merkle
  #c4e9d1a8 transfer_token
  #b4c5d6e7 main
```

Use `trident hash --full` to display full 256-bit hashes instead of 7-character abbreviations.

### 4.2 Project Hashing

Point `trident hash` at a project directory (with `trident.toml`) to hash the entry
file:

```bash
trident hash .
```

### 4.3 Verifying Alpha-Equivalence

Two functions with different variable names but identical computation produce the same
hash. This is a quick way to check if a refactored function is alpha-equivalent to the
original:

```bash
# In file_a.tri: fn add(a: Field, b: Field) -> Field { a + b }
# In file_b.tri: fn add(x: Field, y: Field) -> Field { x + y }

trident hash file_a.tri
#   #c9d5e3f6 add

trident hash file_b.tri
#   #c9d5e3f6 add     <-- same hash
```

---

## ⚡ 5. Verification Caching

Verification results are cached by content hash. Because a hash uniquely identifies an
exact computation, and definitions are immutable once hashed, verification results are
valid for as long as the cache exists.

### 5.1 Cache Location

```trident
~/.trident/cache/
  compile/
    <source_hash_hex>.<target>.tasm     # compiled output
    <source_hash_hex>.<target>.meta     # padded height metadata
  verify/
    <source_hash_hex>.verify            # verification result
```

Override with `$TRIDENT_CACHE_DIR`.

### 5.2 How It Works

When `trident verify` runs on a file:

1. Each function is hashed via the content-addressing pipeline.
2. The verification cache is checked for each hash.
3. If a cached result exists, it is returned immediately -- no re-verification.
4. If not cached, the function is verified (symbolic execution, algebraic solving,
   random testing, bounded model checking).
5. The result is stored in the cache, keyed by the content hash.

```text
Verification Cache Entry:
  safe=true
  constraints=42
  variables=10
  verdict=Safe
  timestamp=1707580800
```

### 5.3 Cache Semantics

- Append-only: once written, a cache entry is never modified. First write wins.
- Keyed by content hash: same computation always maps to the same result.
- Shared across projects: any function with the same hash reuses the same cached
  result, regardless of which project or file it came from.

### 5.4 Compilation Caching

Compilation results use the same content-hash-keyed cache.

### 5.5 Cache Invalidation

There is no manual cache invalidation. The content hash *is* the cache key.
If the code changes, the hash changes, and the old cache entry is simply unused
(a new entry is created for the new hash). If the code is unchanged, the old
cache entry is still valid.

This eliminates an entire class of build-system bugs where stale caches produce
incorrect results.

---

## 🔍 6. Semantic Equivalence

Two definitions with the same content hash are trivially equivalent. For definitions with different hashes, `trident equiv` checks semantic equivalence through three strategies: content hash comparison, polynomial normalization, and differential testing. See [Formal Verification](formal-verification.md) for the full equivalence pipeline.

---

## 🔮 7. Future: On-Chain Registry (0.2)

The 0.1 release provides local-first store and an HTTP registry. Version 0.2
will add an on-chain Merkle registry — content-addressed definitions
anchored on-chain with provable registration and verification. This will
enable trustless, blockchain-backed code discovery and certification.

---

## 🔗 8. See Also

- [Language Reference](../../reference/language.md) -- Complete language syntax, types, and built-in functions
- [Formal Verification](formal-verification.md) -- How verification works, including how results are cached by content hash
- [Deploying a Program](../guides/deploying-a-program.md) -- Deployment by content hash
