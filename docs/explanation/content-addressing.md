# Content-Addressed Code

*Every Trident function is identified by a cryptographic hash of what it computes,
not what it is called. Names are metadata. The hash is the identity.*

---

## 1. What Is Content-Addressed Code

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
essential: **provable computation**.

### Why It Matters for Provable Computation

A zkVM verifier already checks proofs against a program hash -- the program *is* its
hash at the verification layer. Content addressing pushes this identity up to the
source level, creating a single identity that spans from writing code to deploying
verified proofs:

- **Verification certificates** prove properties of a specific computation, identified
  by hash. Change the code, get a new hash; the old certificate no longer applies.
- **Proving cost** is deterministic for a given computation. The hash indexes into the
  cost cache -- same hash, same cost, always.
- **Cross-chain equivalence** reduces to hash comparison. If two deployments share a
  source hash, they run the same computation.
- **Audit results** attach to hashes, not names. An audit of `#a7f3b2c1` is an audit
  of that exact computation forever.

---

## 2. How It Works

Trident's content hashing pipeline normalizes the AST, serializes it deterministically,
and hashes the result with Poseidon2 over the Goldilocks field.

### 2.1 AST Normalization

Before hashing, the AST is normalized so that semantically identical functions produce
identical byte sequences. The normalization steps are:

**Step 1: Replace variable names with de Bruijn indices.**

```
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

**Step 2: Replace dependency references with their content hashes.**

```
// Before:
let d = hash(input)

// After:
let d = #f8a2b1c3(input)    // #f8a2b1c3 is the content hash of the hash function
```

Dependencies are pinned by hash. If a called function changes, its hash changes,
and all callers get new hashes too. Propagation is automatic and exact.

**Step 3: Canonicalize struct field ordering.**

```
// These produce the same hash:
let p = Point { x: 1, y: 2 }
let p = Point { y: 2, x: 1 }     // fields sorted alphabetically before hashing
```

**Step 4: Strip metadata.**

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

The serialized bytes are hashed with **Poseidon2** over the Goldilocks field
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

| Element | Affects hash? |
|---------|:------------:|
| Variable names | No |
| Function name | No |
| Comments | No |
| Formatting / whitespace | No |
| `#[requires]` / `#[ensures]` annotations | No |
| Source file path | No |
| Struct field order in initializers | No |
| Function body (computation) | **Yes** |
| Parameter types | **Yes** |
| Return type | **Yes** |
| Called functions (by their hash) | **Yes** |

---

## 3. Using the Codebase Manager (UCM)

The Universal Codebase Manager (`trident ucm`) is a hash-keyed definitions store
inspired by Unison's codebase model. Every function is stored by its content hash,
with names as mutable pointers into the hash-keyed store.

### 3.1 Codebase Location

The codebase is stored at `~/.trident/codebase/` by default. Override with the
`$TRIDENT_CODEBASE_DIR` environment variable.

```
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
trident ucm add myfile.tri
```

Output:

```
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
trident ucm list
```

Shows all named definitions, sorted alphabetically:

```
  #b4c5d6e7  main
  #c4e9d1a8  transfer_token
  #a7f3b2c1  verify_merkle
```

### 3.4 Viewing Definitions

View a definition by name or by hash prefix:

```bash
trident ucm view verify_merkle
trident ucm view #a7f3b2
```

Output includes the function source, its hash, spec annotations, and dependency list:

```
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
trident ucm rename old_name new_name
```

### 3.6 Viewing Dependencies

Show what a definition depends on and what depends on it:

```bash
trident ucm deps transfer_token
```

Output:

```
Dependencies:
  #a7f3b2c1  verify_merkle
  #d4e5f6a7  check_balance

Dependents:
  #f7a2b1c3  batch_transfer
```

### 3.7 Name History

Show all hashes a name has pointed to over time:

```bash
trident ucm history verify_merkle
```

This is useful for tracking how a function has evolved. Old definitions remain in the
codebase (append-only semantics) -- they are never deleted.

### 3.8 Codebase Statistics

```bash
trident ucm stats
```

Shows the number of unique definitions, name bindings, and total source bytes.

---

## 4. Content Hashing

The `trident hash` command computes content hashes for all functions in a file without
storing them in the codebase.

### 4.1 Basic Usage

```bash
trident hash myfile.tri
```

Output:

```
File: #e1d2c3b4 myfile.tri
  #a7f3b2c1 verify_merkle
  #c4e9d1a8 transfer_token
  #b4c5d6e7 main
```

### 4.2 Full Hashes

By default, hashes are shown in short (40-bit) form. Use `--full` for the complete
256-bit hex:

```bash
trident hash myfile.tri --full
```

```
File: a7f3b2c1d4e5f6a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8d9e0f1a2 myfile.tri
  a7f3b2c1d4e5f6a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8d9e0f1a2 verify_merkle
  ...
```

### 4.3 Project Hashing

Point `trident hash` at a project directory (with `trident.toml`) to hash the entry
file:

```bash
trident hash .
```

### 4.4 What Gets Hashed

For each function, `trident hash` performs the full normalization pipeline:

1. Parse the source file
2. For each function: normalize the AST (de Bruijn indices, dependency substitution,
   field-order canonicalization, metadata stripping)
3. Serialize the normalized AST with version prefix and type tags
4. Hash the serialized bytes with Poseidon2

The file-level hash combines all function hashes (sorted by name for determinism)
with the module name.

### 4.5 Verifying Alpha-Equivalence

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

## 5. Verification Caching

Verification results are cached by content hash. Because a hash uniquely identifies an
exact computation, and definitions are immutable once hashed, verification results are
valid for as long as the cache exists.

### 5.1 Cache Location

```
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

```
Verification Cache Entry:
  safe=true
  constraints=42
  variables=10
  verdict=Safe
  timestamp=1707580800
```

### 5.3 Cache Semantics

- **Append-only**: once written, a cache entry is never modified. First write wins.
- **Keyed by content hash**: same computation always maps to the same result.
- **Shared across projects**: any function with the same hash reuses the same cached
  result, regardless of which project or file it came from.

### 5.4 Compilation Caching

Compilation results follow the same model. Each entry is keyed by
`(source_hash, target)`:

```bash
# First compile: full compilation
trident build myfile.tri --target triton

# Second compile of the same code (even from a different file):
# instant cache hit, no recompilation
trident build other_file.tri --target triton
```

Compilation caches store the TASM output and padded height metadata.

### 5.5 Cache Invalidation

There is no manual cache invalidation. The content hash *is* the cache key.
If the code changes, the hash changes, and the old cache entry is simply unused
(a new entry is created for the new hash). If the code is unchanged, the old
cache entry is still valid.

This eliminates an entire class of build-system bugs where stale caches produce
incorrect results.

---

## 6. Semantic Equivalence

The `trident equiv` command checks whether two functions are semantically equivalent --
they produce the same output for all inputs, even if their ASTs differ.

### 6.1 Basic Usage

Both functions must be in the same `.tri` file:

```bash
trident equiv myfile.tri double_a double_b
```

Output for equivalent functions:

```
Equivalence check: double_a vs double_b
  Method: polynomial normalization
  Verdict: EQUIVALENT
```

Output for non-equivalent functions:

```
Equivalence check: f vs g
  Method: differential testing (counterexample found)
  Verdict: NOT EQUIVALENT
  Counterexample:
    __input_0 = 3
    __input_1 = 5
    f(...) = 8
    g(...) = 15
```

### 6.2 Equivalence Methods

The checker uses a layered strategy, from cheapest to most expensive:

**Level 1: Content hash comparison (alpha-equivalence)**

If both functions produce the same content hash, they are structurally identical
(up to variable renaming). This is instant.

**Level 2: Polynomial normalization**

For pure field-arithmetic functions (using only `+`, `*`, constants, and variables),
the checker normalizes both functions to multivariate polynomial normal form over the
Goldilocks field. If the polynomials match, the functions are equivalent.

This catches cases like `x + x` vs `x * 2`, or `(a + b) * c` vs `a * c + b * c`.

**Level 3: Differential testing**

For functions that cannot be reduced to polynomials, the checker builds a synthetic
"differential test program" that calls both functions with the same inputs and asserts
their outputs are equal. It then runs the full verification pipeline (symbolic
execution, random testing, bounded model checking) on this synthetic program.

If verification passes, the functions are equivalent. If it finds a counterexample,
the functions are not equivalent, and the counterexample is reported.

### 6.3 Verbose Mode

Use `--verbose` to see content hashes and detailed analysis:

```bash
trident equiv myfile.tri f g --verbose
```

### 6.4 Signature Requirements

Both functions must have compatible signatures (same parameter types and return type).
If signatures do not match, the checker reports `UNKNOWN` with a diagnostic message.

---

## 7. On-Chain Registry

The file `ext/triton/registry.tri` implements an on-chain registry as a Merkle tree
of content-addressed definitions, written in Trident itself. It runs as a Triton VM
program, providing trustless, on-chain code registration and verification.

### 7.1 Registry Operations

The registry dispatches on an operation code:

| Op | Name | Description |
|:--:|------|-------------|
| 0 | `register` | Add a new definition to the registry Merkle tree |
| 1 | `verify_membership` | Prove a definition is registered and verified |
| 2 | `update_certificate` | Attach or update a verification certificate |
| 3 | `lookup` | Authenticate a definition against the registry root |
| 4 | `register_equivalence` | Record an equivalence claim between two definitions |

### 7.2 Registry Entries

Each leaf in the registry Merkle tree is a Tip5 hash of:

- **Content hash** -- the Poseidon2 content hash of the definition
- **Type signature hash** -- hash of the function's type signature
- **Dependencies hash** -- hash of the dependency list
- **Certificate hash** -- hash of the verification certificate (0 if unverified)
- **Metadata hash** -- hash of tags, publisher, timestamp, bound name

### 7.3 CLI Integration

The on-chain registry is accessed through `trident registry` subcommands:

```bash
# Register a UCM definition on-chain
trident registry onchain-register verify_merkle

# Verify a definition is registered
trident registry onchain-verify verify_merkle

# Attach a verification certificate
trident registry onchain-certify verify_merkle --input myfile.tri

# Show registry status
trident registry onchain-status
```

Registration requires that the definition first exists in the local UCM codebase
(via `trident ucm add`).

### 7.4 Authorization

Registry mutations (register, update, equivalence claims) require publisher
authorization. The registry verifies authorization by hashing a divined secret and
checking it against a public authorization hash. This is enforced inside the ZK proof
itself.

---

## 8. Links

- [Tutorial](../tutorials/tutorial.md) -- Getting started with Trident, including first use of `trident hash` and `trident ucm`
- [Formal Verification](formal-verification.md) -- How verification works, including how results are cached by content hash
- [Language Reference](../reference/reference.md) -- Complete language syntax, types, and built-in functions
- [Deploying a Program](../guides/deploying-a-program.md) -- Deployment by content hash, on-chain registry
- [Universal Design](universal-design.md) -- Multi-target architecture and how hashes relate to backends
- [Language Specification](../reference/spec.md) -- Formal hash computation rules
- [Vision](vision.md) -- The broader vision for content-addressed provable computation
