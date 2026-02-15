# Warnings

[Back to Error Catalog](../errors.md)

---

### Unused import

```text
warning: unused import 'vm.crypto.hash'
```

Fix: Remove the unused `use` statement.

---

### Asm block target mismatch

```text
warning: asm block tagged for 'risc_v' will be skipped (current target: 'triton')
```

An `asm` block tagged for a different target is silently skipped. This is
informational when using multi-target `asm` blocks intentionally.

---

### Power-of-2 boundary proximity

```text
warning: program is 3 rows below padded height boundary
  help: consider optimizing to stay well below 1024
```

The program is close to a power-of-2 table height boundary. A small code
change could double proving cost.

---

### Unused variable (planned)

```text
warning: unused variable 'x'
  help: prefix with `_` to suppress: `let _x: Field = ...`
```

Spec: general compiler quality.

---

### Unused function (planned)

```text
warning: unused function 'helper'
```

Spec: general compiler quality.

---

### Unused constant (planned)

```text
warning: unused constant 'MAX'
```

Spec: general compiler quality.

---

### Shadowed variable (planned)

```text
warning: variable 'x' shadows previous declaration
```

Spec: general compiler quality.
