# Module Errors

[Back to Error Catalog](../errors.md)

---

### Cannot find module

```text
error: cannot find module 'helpers' (looked at 'path/to/helpers.tri'): No such file
  help: create the file 'path/to/helpers.tri' or check the module name in the `use` statement
```

---

### Circular dependency

```text
error: circular dependency detected involving module 'a'
  help: break the cycle by extracting shared definitions into a separate module
```

---

### Duplicate function

```text
error: duplicate function 'main'
```

---

### Cannot read entry file

```text
error: cannot read 'main.tri': No such file or directory
  help: check that the file exists and is readable
```

---

### Program without main (planned)

```text
error: program 'my_program' must have a `fn main()` entry point
  help: add `fn main() { ... }` or change to `module` if this is a library
```

Spec: language.md Section 1 (program must have fn main).

---

### Module with main (planned)

```text
error: module 'my_module' must not define `fn main()`
  help: modules are libraries; change to `program` if this is an entry point
```

Spec: language.md Section 1 (module must NOT have fn main).

---

### Duplicate struct (planned)

```text
error: duplicate struct definition 'Point'
```

Spec: language.md Section 1 (items are unique within a module).

---

### Duplicate constant (planned)

```text
error: duplicate constant definition 'MAX'
```

Spec: language.md Section 1 (items are unique within a module).

---

### Duplicate import (planned)

```text
error: duplicate import 'use merkle'
```

Spec: language.md Section 1 (import rules).

---

### Self import (planned)

```text
error: module cannot import itself
```

Spec: language.md Section 1 (DAG requirement).
