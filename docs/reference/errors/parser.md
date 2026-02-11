# Parser Errors

[Back to Error Catalog](../errors.md)

---

### Expected program or module

```
error: expected 'program' or 'module' declaration at the start of file
  help: every .tri file must begin with `program <name>` or `module <name>`
```

**Fix:**

```
program my_app

fn main() { }
```

---

### Nesting depth exceeded

```
error: nesting depth exceeded (maximum 256 levels)
  help: simplify your program by extracting deeply nested code into functions
```

More than 256 levels of nested blocks. Extract inner logic into functions.

---

### Expected item

```
error: expected item (fn, struct, event, or const)
  help: top-level items must be function, struct, event, or const definitions
```

A top-level construct is not a valid item.

**Fix:** Only `fn`, `struct`, `event`, and `const` are valid at module scope.

---

### Expected type

```
error: expected type
  help: valid types are: Field, XField, Bool, U32, Digest, [T; N], (T, U), or a struct name
```

A type annotation contains something that is not a recognized type.

---

### Expected array size

```
error: expected array size (integer literal or size parameter name)
  help: array sizes are written as `N`, `3`, `M + N`, or `N * 2`
```

The array size expression is invalid.

---

### Expected expression

```
error: expected expression, found <token>
  help: expressions include literals (42, true), variables, function calls, and operators
```

---

### Invalid field pattern

```
error: expected field pattern (identifier, literal, or _)
  help: use `field: var` to bind, `field: 0` to match, or `field: _` to ignore
```

A struct pattern field has an invalid pattern.

---

### Attribute validation

```
error: #[intrinsic] can only be applied to functions
error: #[test] can only be applied to functions
error: #[pure] can only be applied to functions
error: #[requires] can only be applied to functions
error: #[ensures] can only be applied to functions
```

Attributes are only valid on function definitions.

---

### No wildcard import **(planned)**

```
error: wildcard import 'use merkle.*' is forbidden
  help: import the module name directly: `use merkle`
```

**Spec:** language.md Section 1 (no wildcard imports).

---

### No import renaming **(planned)**

```
error: import renaming 'use merkle as m' is forbidden
  help: use the original module name: `use merkle`
```

**Spec:** language.md Section 1 (no renaming).

---

### No else-if **(planned)**

```
error: 'else if' is not supported
  help: nest 'if' inside 'else': `else { if cond { ... } }`
```

**Spec:** language.md Section 5 (if/else, no else-if).

---

### No while loop **(planned)**

```
error: 'while' is not supported
  help: use `for i in 0..n bounded N { }` with a declared bound
```

**Spec:** language.md Section 5, patterns.md Permanent Exclusions.

---

### No loop keyword **(planned)**

```
error: 'loop' is not supported
  help: use `for` with a bounded range
```

**Spec:** language.md Section 5, patterns.md Permanent Exclusions.

---

### No break statement **(planned)**

```
error: 'break' is not supported in Trident
  help: all loops run for their full declared bound
```

**Spec:** language.md Section 5, patterns.md Permanent Exclusions.

---

### No continue statement **(planned)**

```
error: 'continue' is not supported in Trident
```

**Spec:** language.md Section 5, patterns.md Permanent Exclusions.

---

### No enum declaration **(planned)**

```
error: 'enum' is not supported; Trident has no sum types
  help: use struct + integer tag for variant patterns
```

**Spec:** language.md Section 2, patterns.md Permanent Exclusions (no enums, no sum types).

---

### No trait declaration **(planned)**

```
error: 'trait' is not supported in Trident
```

**Spec:** patterns.md Permanent Exclusions.

---

### No impl block **(planned)**

```
error: 'impl' is not supported; use free functions
```

**Spec:** patterns.md Permanent Exclusions.

---

### No macro declaration **(planned)**

```
error: macros are not supported in Trident
```

**Spec:** patterns.md Permanent Exclusions.

---

### No closure syntax **(planned)**

```
error: closures are not supported in Trident
  help: use named functions instead
```

**Spec:** language.md Section 3, patterns.md Permanent Exclusions (no closures).

---

### No method syntax **(planned)**

```
error: method syntax 'x.foo()' is not supported
  help: use `foo(x)` instead
```

Field access is `x.field`. Function calls must be free-standing.

**Spec:** language.md Section 3 (no method syntax).

---

### Missing type annotation on let **(planned)**

```
error: let binding requires a type annotation
  help: write `let x: Field = ...` not `let x = ...`
```

**Spec:** language.md Section 5, grammar.md (let_stmt grammar includes type).

---

### I/O declaration in module **(planned)**

```
error: I/O declarations ('pub input', 'sec input') are only allowed in program files
  help: move I/O declarations to a `program` file, not a `module`
```

**Spec:** language.md Section 3 (I/O declarations: program modules only).

---

### No re-export **(planned)**

```
error: re-exports are not supported
  help: if A uses B, C cannot access B through A; import B directly
```

**Spec:** language.md Section 1 (no re-exports).

---

### No exceptions **(planned)**

```
error: 'try'/'catch'/'throw' are not supported
  help: use `assert` for failure — proof generation becomes impossible on assert failure
```

**Spec:** patterns.md Permanent Exclusions.

---

### No concurrency keywords **(planned)**

```
error: 'async'/'await'/'spawn' are not supported
  help: Trident execution is sequential; concurrency is handled at the runtime level
```

**Spec:** patterns.md Permanent Exclusions.

---

### No pointers or references **(planned)**

```
error: pointers and references ('&', '*') are not supported
  help: all values are passed by copy on the stack
```

**Spec:** language.md Section 2, Section 8, patterns.md Permanent Exclusions (no heap, no pointers).

---

### Unsupported visibility modifier **(planned)**

```
error: visibility modifier 'pub(crate)' is not supported
  help: Trident has only `pub` (public) or default (private)
```

No `pub(crate)`, `friend`, or `internal` modifiers. Visibility is binary:
`pub` or private.

**Spec:** language.md Section 1 (no pub(crate), no friend, no internal).

---

### No heap allocation **(planned)**

```
error: 'alloc' is not supported; Trident has no heap
  help: use stack variables or RAM (ram_read/ram_write)
```

No `alloc`, `free`, `new`, or garbage collection. All memory is either
stack (16 elements, LRU spill) or word-addressed RAM.

**Spec:** language.md Section 8, patterns.md Permanent Exclusions (no heap, no GC).
