# Annotation Errors

[Back to Error Catalog](../errors.md)

---

### #[intrinsic] restriction

```text
error: #[intrinsic] is only allowed in std.*/ext.* modules, not in 'my_module'
```

The `#[intrinsic]` attribute is reserved for standard library and extension
modules shipped with the compiler. User code cannot use it.

---

### #[test] validation

```text
error: #[test] function 'test_add' must have no parameters
error: #[test] function 'test_add' must not have a return type
```

Test functions take no arguments and return nothing.

---

### #[pure] I/O restriction

```text
error: #[pure] function cannot call 'pub_read' (I/O side effect)
error: #[pure] function cannot use 'reveal' (I/O side effect)
error: #[pure] function cannot use 'seal' (I/O side effect)
```

Functions annotated `#[pure]` cannot perform any I/O operations.

---

### Unknown attribute (planned)

```text
error: unknown attribute '#[foo]'
  help: valid attributes are: cfg, test, pure, intrinsic, requires, ensures
```

Spec: language.md Section 7 (closed set of attributes).

---

### Duplicate attribute (planned)

```text
error: duplicate attribute '#[pure]' on function 'foo'
```

Spec: language.md Section 7.

---

### Unknown cfg flag (planned)

```text
error: unknown cfg flag 'unknown_flag'
  help: valid cfg flags are target-specific and project-defined
```

Spec: language.md Section 7 (cfg conditional compilation).

---

### Invalid requires/ensures predicate (planned)

```text
error: invalid predicate in #[requires]: undefined variable 'x'
  help: predicates may only reference parameter names and constants
```

The expression inside `#[requires(...)]` or `#[ensures(...)]` must be a
valid boolean expression over the function's parameters (for requires)
or parameters and `result` (for ensures).

Spec: language.md Section 7 (#[requires]/#[ensures] predicates).

---

### Result in requires predicate (planned)

```text
error: 'result' is not available in #[requires] predicates
  help: 'result' refers to the return value and is only valid in #[ensures]
```

The `result` keyword represents the function's return value, which does
not exist at the point of precondition checking.

Spec: language.md Section 7 (result = return value, ensures only).
