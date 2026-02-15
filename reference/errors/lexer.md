# Lexer Errors

[Back to Error Catalog](../errors.md)

---

### Unexpected character

```text
error: unexpected character '@' (U+0040)
  help: this character is not recognized as part of Trident syntax
```

A character outside the Trident grammar was found. Source files must be ASCII.

Fix: Remove the character. Check for copy-paste artifacts or encoding issues.

---

### Non-ASCII source (planned)

```text
error: non-ASCII byte 0xNN at position N
  help: source files must be ASCII
```

Trident source is ASCII-only. Unicode identifiers are not supported.

Spec: grammar.md (IDENT production).

---

### No subtraction operator

```text
error: unexpected '-'; Trident has no subtraction operator
  help: use the `sub(a, b)` function instead of `a - b`
```

Trident deliberately omits `-` (see [language.md](../language.md) Section 4).
Subtraction in a prime field is addition by the additive inverse. Making it
explicit prevents the `(1 - 2) == p - 1` footgun.

Fix: `let diff: Field = sub(a, b)`

---

### No division operator

```text
error: unexpected '/'; Trident has no division operator
  help: use the `/% (divmod)` operator instead: `let (quot, rem) = a /% b`
```

Field division is multiplication by the modular inverse. The `/%` operator
makes the cost explicit.

Fix: `let (quotient, remainder) = a /% b`

---

### No inequality operator (planned)

```text
error: unexpected '!='; Trident has no inequality operator
  help: use `(a == b) == false`
```

Spec: language.md Section 4 (excluded operators).

---

### No greater-than operator (planned)

```text
error: unexpected '>'; Trident has no '>' operator
  help: use `b < a` (U32 only)
```

Spec: language.md Section 4 (excluded operators).

---

### No less-or-equal operator (planned)

```text
error: unexpected '<='; Trident has no '<=' operator
  help: combine `<` and `==`
```

Spec: language.md Section 4 (excluded operators).

---

### No greater-or-equal operator (planned)

```text
error: unexpected '>='; Trident has no '>=' operator
  help: combine `<` and `==`
```

Spec: language.md Section 4 (excluded operators).

---

### No logical AND operator (planned)

```text
error: unexpected '&&'; Trident has no '&&' operator
  help: use `a * b` for logical AND on Bool values
```

Spec: language.md Section 4 (excluded operators).

---

### No logical OR operator (planned)

```text
error: unexpected '||'; Trident has no '||' operator
  help: use `a + b + (neg(a * b))` or equivalent field logic
```

Spec: language.md Section 4 (excluded operators).

---

### No logical NOT operator (planned)

```text
error: unexpected '!'; Trident has no '!' operator
  help: use `sub(1, a)` for logical NOT on Bool values
```

Spec: language.md Section 4 (excluded operators).

---

### No modulo operator (planned)

```text
error: unexpected '%'; Trident has no '%' operator
  help: use `a /% b` to get both quotient and remainder
```

Spec: language.md Section 4 (excluded operators).

---

### No left shift operator (planned)

```text
error: unexpected '<<'; Trident has no '<<' operator
```

Spec: language.md Section 4 (excluded operators).

---

### No right shift operator (planned)

```text
error: unexpected '>>'; Trident has no '>>' operator
```

Spec: language.md Section 4 (excluded operators).

---

### No string literal (planned)

```text
error: unexpected '"'; Trident has no string type
  help: strings are a permanent exclusion — no target VM supports string operations
```

Spec: language.md Section 2, Section 12.

---

### No block comment (planned)

```text
error: block comments '/* */' are not supported
  help: use line comments: `// comment`
```

Trident only supports line comments (`//`). Block comments are not part of
the grammar.

Spec: grammar.md (`comment = "//" .* NEWLINE`).

---

### Integer too large

```text
error: integer literal '999999999999999999999' is too large
  help: maximum integer value is 18446744073709551615
```

The literal exceeds `u64::MAX` (2^64 - 1).

Fix: Use a smaller value. Values are reduced modulo p at runtime.

---

### Unterminated asm block

```text
error: unterminated asm block: missing closing '}'
  help: every `asm { ... }` block must have a matching closing brace
```

Fix: Add the closing `}`.

---

### Invalid asm annotation

```text
error: expected ')' after asm annotation
  help: asm annotations: `asm(+1) { ... }`, `asm(triton) { ... }`, or `asm(triton, +1) { ... }`
```

The `asm` block has a malformed annotation.

Fix: Use one of the valid forms:

```trident
asm { ... }                     // zero effect, default target
asm(+1) { ... }                // effect only
asm(triton) { ... }            // target only
asm(triton, +1) { ... }        // target + effect
```

---

### Expected asm block body

```text
error: expected '{' after `asm` keyword
  help: inline assembly syntax is `asm { instructions }` or `asm(triton) { instructions }`
```

Fix: Add `{ ... }` after the asm keyword or annotation.
