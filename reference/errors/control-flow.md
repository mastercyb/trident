# Control Flow Errors

[Back to Error Catalog](../errors.md)

---

### For loop without bounded

```text
error: loop end must be a compile-time constant, or annotated with a bound
  help: use a literal like `for i in 0..10 { }` or add a bound: `for i in 0..n bounded 100 { }`
```

All loops must have compile-time-known or declared upper bounds for
deterministic trace length computation.

---

### Non-exhaustive match

```text
error: non-exhaustive match: not all possible values are covered
  help: add a wildcard `_ => { ... }` arm to handle all remaining values
```

---

### Unreachable pattern after wildcard

```text
error: unreachable pattern after wildcard '_'
  help: the wildcard `_` already matches all values; remove this arm or move it before `_`
```

---

### Match pattern type mismatch

```text
error: integer pattern on Bool scrutinee; use `true` or `false`
error: Bool pattern on non-Bool scrutinee
```

---

### Struct pattern type mismatch

```text
error: struct pattern `Point` does not match scrutinee type `Config`
```

---

### Unknown struct field in pattern

```text
error: struct `Point` has no field `z`
```

---

### Missing field in struct pattern (planned)

```text
error: match on struct 'Point' is missing field 'y' in pattern
  help: bind or ignore all fields: `Point { x, y: _ }`
```

Struct patterns must account for every field.

Spec: language.md Section 5 (exhaustive match, struct patterns).

---

### Duplicate match arm (planned)

```text
error: duplicate match arm for value '0'
  help: remove the duplicate arm
```

Spec: language.md Section 5 (match semantics).
