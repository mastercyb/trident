# Event Errors

[Back to Error Catalog](../errors.md)

---

### Undefined event

```
error: undefined event 'Transfer'
```

**Fix:** Declare the event before using `reveal` or `seal`:

```
event Transfer { from: Digest, to: Digest, amount: Field }
```

---

### Event field count limit

```
error: event 'BigEvent' has 12 fields, max is 9
```

Events are limited to 9 Field-width fields.

---

### Event field type restriction

```
error: event field 'data' must be Field type, got [Field; 3]
```

All event fields must be `Field` type.

---

### Missing event field

```
error: missing field 'amount' in event 'Transfer'
```

---

### Unknown event field

```
error: unknown field 'extra' in event 'Transfer'
```

---

### Event field type mismatch in reveal/seal **(planned)**

```
error: reveal field 'amount': expected Field but got Bool
```

The expression type does not match the event field's declared type.

**Spec:** language.md Section 10 (reveal/seal must match event
with matching field types).

---

### Duplicate event declaration **(planned)**

```
error: event 'Transfer' is already defined
```

**Spec:** language.md Section 1 (items are unique within a module).
