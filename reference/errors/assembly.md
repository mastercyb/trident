# Inline Assembly Errors

[Back to Error Catalog](../errors.md)

---

### Asm effect mismatch (planned)

```text
error: asm block declared effect '+1' but actual stack effect differs
  help: the effect annotation is the contract between assembly and the compiler's stack model
```

The compiler trusts the declared effect but may detect mismatches when the
surrounding code's stack doesn't balance.

Spec: language.md Section 9 (effect annotation is the contract).

---

### Asm in pure function (planned)

```text
error: asm block not allowed in #[pure] function
  help: asm blocks may have unchecked I/O side effects
```

Since the compiler cannot verify what inline assembly does, it's incompatible
with the `#[pure]` guarantee.

Spec: language.md Section 7 (pure = no I/O), Section 9.
