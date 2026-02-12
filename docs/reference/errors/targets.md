# Target Errors

[Back to Error Catalog](../errors.md)

---

### Unknown target

```
error: unknown target 'wasm' (looked for 'vm/wasm.toml')
  help: available targets: triton, miden, openvm, sp1, cairo
```

---

### Cannot read target config

```
error: cannot read target config 'vm/foo.toml': No such file
```

---

### Invalid target name

```
error: invalid target name '../../../etc/passwd'
```

Target names cannot contain path traversal characters.

---

### Tier capability exceeded **(planned)**

```
error: program uses Tier 2 operations but target 'sp1' only supports up to Tier 1
  help: remove hash/sponge/merkle operations or choose a Tier 2 target (triton, miden, nock)
```

The program's tier (highest-tier op used) exceeds the target's maximum
supported tier. See [targets.md](../targets.md) for tier compatibility.

**Spec:** ir.md (compiler rejects programs using ops above target capability).

---

### XField on unsupported target **(planned)**

```
error: type 'XField' is not available on target 'miden' (xfield_width = 0)
  help: XField requires a target with extension field support (currently: triton, nock)
```

**Spec:** provable.md Extension Field, targets.md (XField = Tier 2, extension field
targets only).

---

### Scalar multiply on unsupported target **(planned)**

```
error: operator '*.' (scalar multiply) is not available on target 'miden'
  help: '*.' requires XField support (currently: triton, nock)
```

**Spec:** provable.md Extension Field (Tier 2 operator), targets.md.

---

### Hash builtins on unsupported target **(planned)**

```
error: builtin 'hash' is not available on target 'sp1' (Tier 2 required)
  help: hash/sponge operations require a target with native hash coprocessor (triton, miden, nock)
```

**Spec:** language.md Section 6 Hash, targets.md (hash = Tier 1).

---

### Sponge builtins on unsupported target **(planned)**

```
error: builtin 'sponge_init' is not available on target 'sp1'
  help: sponge operations require a Tier 2 target (triton, miden, nock)
```

**Spec:** provable.md Sponge, targets.md (sponge = Tier 2).

---

### Seal on unsupported target **(planned)**

```
error: 'seal' requires sponge support (Tier 2)
  help: seal hashes fields via sponge; use 'reveal' for public output on Tier 1 targets
```

`seal` internally uses the sponge construction to hash event fields before
writing the commitment digest to public output. Targets without sponge
support cannot execute `seal`.

**Spec:** provable.md Sponge (seal requires sponge = Tier 2).

---

### Merkle builtins on unsupported target **(planned)**

```
error: builtin 'merkle_step' is not available on target 'sp1'
  help: Merkle operations require a Tier 2 target (triton, miden, nock)
```

**Spec:** provable.md Merkle Authentication, targets.md (merkle = Tier 2).

---

### XField builtins on unsupported target **(planned)**

```
error: builtin 'xfield' is not available on target 'miden'
  help: extension field builtins require XField support (currently: triton, nock)
```

**Spec:** provable.md Extension Field, targets.md (XField builtins = TRITON, NOCK).

---

### Cross-target import **(planned)**

```
error: cannot import 'neptune.ext.xfield' when compiling for target 'miden'
  help: <os>.ext.* modules bind to a specific OS
```

Importing `<os>.ext.*` binds the program to that OS. Compiling for a
different target is a hard error.

**Spec:** os.md Extension Tracking, targets.md (cross-OS imports rejected).

---

### Tier 3 on non-Triton target **(planned)**

```
error: recursive proof verification (Tier 3) is only available on TRITON and NOCK
  help: ProofBlock, FriVerify, and extension field folding require TRITON or NOCK
```

**Spec:** ir.md (Tier 3 = TRITON, NOCK), targets.md tier compatibility.

---

### Invalid proof_block program hash **(planned)**

```
error: proof_block() requires Digest argument, got Field
```

The `proof_block` construct takes a program hash of type `Digest` to
identify which program's proof is being verified recursively.

**Spec:** provable.md Proof Composition (proof_block(program_hash: Digest)).

---

### Hash rate argument mismatch **(planned)**

```
error: hash() requires 10 field arguments on target 'triton', got 8
  help: hash rate R = 10 for TRITON; see targets.md for per-target rates
```

The number of arguments to `hash()` must match the target's hash rate R.

**Spec:** language.md Section 6 Hash (hash takes R elements, R is target-dependent).

---

### Sponge absorb argument mismatch **(planned)**

```
error: sponge_absorb() requires 10 field arguments on target 'triton', got 5
  help: sponge rate R = 10 for TRITON; see targets.md for per-target rates
```

**Spec:** provable.md Sponge (sponge_absorb takes R elements).
