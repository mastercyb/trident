# Size Generic Errors

[Back to Error Catalog](../errors.md)

---

### Size argument to non-generic function

```text
error: function 'foo' is not generic but called with size arguments
```

Fix: Remove the angle bracket arguments.

---

### Size parameter count mismatch

```text
error: function 'foo' expects 2 size parameters, got 1
```

---

### Cannot infer size argument

```text
error: cannot infer size parameter 'N'; provide explicit size argument
```

Fix: Provide the size argument explicitly:

```trident
let result: Field = sum<5>(arr)
```

---

### Expected concrete size

```text
error: expected concrete size, got 'N'
```

A size parameter could not be resolved to a concrete integer.

---

### Array size not compile-time known (planned)

```text
error: array size must be a compile-time known integer
  help: use a literal, const, or size parameter expression
```

Spec: language.md Section 2 (array sizes must be compile-time known).

---

### Zero or negative array size (planned)

```text
error: array size must be a positive integer, got 0
```

Spec: language.md Section 2 (fixed-size arrays, meaningful sizes).
