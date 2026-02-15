# Type Errors

[Back to Error Catalog](../errors.md)

---

### Binary operator type mismatch

```text
error: operator '+' requires both operands to be Field (or both XField), got Field and Bool
error: operator '==' requires same types, got Field and U32
error: operator '<' requires U32 operands, got Field and Field
error: operator '&' requires U32 operands, got Field and Field
error: operator '/%' requires U32 operands, got Field and Field
error: operator '*.' requires XField and Field, got Field and Field
```

Each operator has specific type requirements. See [language.md](../language.md)
Section 4 for the operator table.

---

### Type mismatch in let binding

```text
error: type mismatch: declared Field but expression has type Bool
```

The expression type does not match the declared type annotation.

---

### Type mismatch in assignment

```text
error: type mismatch in assignment: expected Field but got Bool
```

---

### Cannot assign to immutable variable

```text
error: cannot assign to immutable variable
  help: declare the variable with `let mut` to make it mutable
```

#### Fix

```trident
let mut x: Field = 0
x = 42
```

---

### Undefined variable

```text
error: undefined variable 'x'
  help: check that the variable is declared with `let` before use
```

---

### Undefined function

```text
error: undefined function 'foo'
  help: check the function name and ensure the module is imported with `use`
```

---

### Undefined constant (planned)

```text
error: undefined constant 'MAX_SIZE'
  help: declare with `const MAX_SIZE: U32 = ...` or import the defining module
```

Spec: language.md Section 3 (constants).

---

### Function arity mismatch

```text
error: function 'foo' expects 2 arguments, got 3
```

---

### Function argument type mismatch

```text
error: argument 1 of 'foo': expected Field but got Bool
```

---

### Return type mismatch

```text
error: function 'foo' declared return type Field, but body returns Bool
```

---

### No implicit conversion (planned)

```text
error: cannot implicitly convert U32 to Field
  help: use `as_field(x)` for U32 -> Field or `as_u32(x)` for Field -> U32
```

No automatic coercion between types. All conversions must be explicit.

Spec: language.md Section 2, Section 10 (no implicit conversions).

---

### Undefined struct

```text
error: undefined struct 'Point'
  help: check the struct name spelling, or import the module that defines it
```

---

### Struct missing field

```text
error: missing field 'y' in struct init
```

All fields must be provided in a struct literal.

---

### Struct unknown field

```text
error: unknown field 'z' in struct 'Point'
```

---

### Struct field type mismatch

```text
error: field 'x': expected Field but got Bool
```

---

### Field access on non-struct

```text
error: field access on non-struct type Field
```

---

### Private field access

```text
error: field 'secret' of struct 'Account' is private
```

Fix: Mark the field `pub` or provide a public accessor function.

---

### Private function access (planned)

```text
error: function 'helper' of module 'wallet' is private
  help: mark the function `pub` to make it accessible from other modules
```

Spec: language.md Section 1 (visibility: pub or default private).

---

### Private struct access (planned)

```text
error: struct 'Internal' of module 'wallet' is private
  help: mark the struct `pub` to make it accessible from other modules
```

Spec: language.md Section 1 (visibility).

---

### Index on non-array

```text
error: index access on non-array type Field
```

---

### Array index type mismatch (planned)

```text
error: array index must be U32 or compile-time integer, got Bool
```

Spec: language.md Section 4 (array indexing).

---

### Array index out of bounds (planned)

```text
error: array index 5 is out of bounds for array of size 3
```

Compile-time constant indices are bounds-checked statically.

Spec: language.md Section 4 (array indexing with compile-time sizes).

---

### Array element type mismatch

```text
error: array element type mismatch: expected Field got Bool
```

All elements of an array literal must have the same type.

---

### Tuple element count limit (planned)

```text
error: tuple has 20 elements, maximum is 16
```

Spec: language.md Section 2 (max 16 tuple elements).

---

### Parameter count limit (planned)

```text
error: function 'foo' has 20 parameters, maximum is 16
  help: group related parameters into a struct
```

Spec: language.md Section 3 (maximum 16 parameters).

---

### Tuple destructuring mismatch

```text
error: tuple destructuring: expected 3 elements, got 2 names
```

---

### Digest destructuring mismatch

```text
error: digest destructuring requires exactly D names, got N
```

The number of names in a digest destructuring must match the target's
digest width.

---

### Cannot destructure non-tuple

```text
error: cannot destructure non-tuple type Field
```

---

### Tuple assignment mismatch

```text
error: tuple assignment: expected 3 elements, got 2 names
```

---

### If condition type

```text
error: if condition must be Bool or Field, got Digest
```

---

### Recursion detected

```text
error: recursive call cycle detected: main -> foo -> main
  help: stack-machine targets do not support recursion; use loops (`for`) or iterative algorithms instead
```

Trident prohibits recursion because all target VMs require deterministic
trace lengths. Rewrite using `for` loops with `bounded`:

```trident
fn fib(n: Field) -> Field {
    let mut a: Field = 0
    let mut b: Field = 1
    for i in 0..n bounded 100 {
        let tmp: Field = b
        b = a + b
        a = tmp
    }
    a
}
```

---

### Unreachable code after return

```text
error: unreachable code after return statement
  help: remove this code or move it before the return
```

---

### Unreachable code after halt (planned)

```text
error: unreachable code after unconditional halt
  help: code after `assert(false)` or `halt` can never execute
```

Spec: language.md Section 10 (dead code after halt/assert rejected).

---

### No function overloading (planned)

```text
error: function 'foo' is already defined
  help: Trident does not support function overloading; use distinct names
```

Spec: language.md Section 3 (no function overloading).

---

### No type generics (planned)

```text
error: type-level generics are not supported
  help: only size parameters (integers) are allowed: `fn foo<N>(...)`
```

Spec: language.md Section 3, Section 12 (only integer size parameters).

---

### No default arguments (planned)

```text
error: default parameter values are not supported
  help: define separate functions for different argument combinations
```

Spec: language.md Section 3 (no default arguments).

---

### No variadic arguments (planned)

```text
error: variadic arguments are not supported
  help: use a fixed-size array parameter instead
```

Spec: language.md Section 3 (no variadic arguments).

---

### Transitive import access (planned)

```text
error: cannot access 'B.foo' through module 'A'
  help: import module 'B' directly with `use B`
```

If A imports B, C cannot reach B's items through A. No re-exports.

Spec: language.md Section 1 (no re-exports).

---

### No floats (planned)

```text
error: floating-point types are not supported
  help: use Field for arithmetic — all computation is over finite fields
```

Spec: language.md Section 2, Section 12.

---

### No Option or Result (planned)

```text
error: 'Option' and 'Result' types are not supported
  help: use assert for validation; failure = no proof
```

Spec: language.md Section 2 (no Option, no Result).
