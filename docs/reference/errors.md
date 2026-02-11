# Error Catalog

All Trident compiler diagnostics — errors, warnings, and optimization hints.
Derived from the language specification ([language.md](language.md)), target
constraints ([targets.md](targets.md)), and IR tier rules ([ir.md](ir.md)).

This catalog is the source of truth for diagnostics. If a rule in the reference
can be violated, the error must exist here. Entries marked **(planned)** are
specification-required but not yet implemented in the compiler.

---

## Lexer Errors

### Unexpected character

```
error: unexpected character '@' (U+0040)
  help: this character is not recognized as part of Trident syntax
```

A character outside the Trident grammar was found. Source files must be ASCII.

**Fix:** Remove the character. Check for copy-paste artifacts or encoding issues.

---

### Non-ASCII source **(planned)**

```
error: non-ASCII byte 0xNN at position N
  help: source files must be ASCII
```

Trident source is ASCII-only. Unicode identifiers are not supported.

**Spec:** language.md Section 20 (IDENT production).

---

### No subtraction operator

```
error: unexpected '-'; Trident has no subtraction operator
  help: use the `sub(a, b)` function instead of `a - b`
```

Trident deliberately omits `-` (see [language.md](language.md) Section 4).
Subtraction in a prime field is addition by the additive inverse. Making it
explicit prevents the `(1 - 2) == p - 1` footgun.

**Fix:** `let diff: Field = sub(a, b)`

---

### No division operator

```
error: unexpected '/'; Trident has no division operator
  help: use the `/% (divmod)` operator instead: `let (quot, rem) = a /% b`
```

Field division is multiplication by the modular inverse. The `/%` operator
makes the cost explicit.

**Fix:** `let (quotient, remainder) = a /% b`

---

### No inequality operator **(planned)**

```
error: unexpected '!='; Trident has no inequality operator
  help: use `(a == b) == false`
```

**Spec:** language.md Section 4 (excluded operators).

---

### No greater-than operator **(planned)**

```
error: unexpected '>'; Trident has no '>' operator
  help: use `b < a` (U32 only)
```

**Spec:** language.md Section 4 (excluded operators).

---

### No less-or-equal operator **(planned)**

```
error: unexpected '<='; Trident has no '<=' operator
  help: combine `<` and `==`
```

**Spec:** language.md Section 4 (excluded operators).

---

### No greater-or-equal operator **(planned)**

```
error: unexpected '>='; Trident has no '>=' operator
  help: combine `<` and `==`
```

**Spec:** language.md Section 4 (excluded operators).

---

### No logical AND operator **(planned)**

```
error: unexpected '&&'; Trident has no '&&' operator
  help: use `a * b` for logical AND on Bool values
```

**Spec:** language.md Section 4 (excluded operators).

---

### No logical OR operator **(planned)**

```
error: unexpected '||'; Trident has no '||' operator
  help: use `a + b + (neg(a * b))` or equivalent field logic
```

**Spec:** language.md Section 4 (excluded operators).

---

### No logical NOT operator **(planned)**

```
error: unexpected '!'; Trident has no '!' operator
  help: use `sub(1, a)` for logical NOT on Bool values
```

**Spec:** language.md Section 4 (excluded operators).

---

### No modulo operator **(planned)**

```
error: unexpected '%'; Trident has no '%' operator
  help: use `a /% b` to get both quotient and remainder
```

**Spec:** language.md Section 4 (excluded operators).

---

### No left shift operator **(planned)**

```
error: unexpected '<<'; Trident has no '<<' operator
```

**Spec:** language.md Section 4 (excluded operators).

---

### No right shift operator **(planned)**

```
error: unexpected '>>'; Trident has no '>>' operator
```

**Spec:** language.md Section 4 (excluded operators).

---

### No string literal **(planned)**

```
error: unexpected '"'; Trident has no string type
  help: strings are a permanent exclusion — no target VM supports string operations
```

**Spec:** language.md Section 2, Section 21 (permanent exclusion).

---

### Integer too large

```
error: integer literal '999999999999999999999' is too large
  help: maximum integer value is 18446744073709551615
```

The literal exceeds `u64::MAX` (2^64 - 1).

**Fix:** Use a smaller value. Values are reduced modulo p at runtime.

---

### Unterminated asm block

```
error: unterminated asm block: missing closing '}'
  help: every `asm { ... }` block must have a matching closing brace
```

**Fix:** Add the closing `}`.

---

### Invalid asm annotation

```
error: expected ')' after asm annotation
  help: asm annotations: `asm(+1) { ... }`, `asm(triton) { ... }`, or `asm(triton, +1) { ... }`
```

The `asm` block has a malformed annotation.

**Fix:** Use one of the valid forms:

```
asm { ... }                     // zero effect, default target
asm(+1) { ... }                // effect only
asm(triton) { ... }            // target only
asm(triton, +1) { ... }        // target + effect
```

---

### Expected asm block body

```
error: expected '{' after `asm` keyword
  help: inline assembly syntax is `asm { instructions }` or `asm(triton) { instructions }`
```

**Fix:** Add `{ ... }` after the asm keyword or annotation.

---

## Parser Errors

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

**Spec:** language.md Section 5, Section 21 (permanent exclusion).

---

### No loop keyword **(planned)**

```
error: 'loop' is not supported
  help: use `for` with a bounded range
```

**Spec:** language.md Section 5, Section 21 (permanent exclusion).

---

### No break statement **(planned)**

```
error: 'break' is not supported in Trident
  help: all loops run for their full declared bound
```

**Spec:** language.md Section 5, Section 21 (permanent exclusion).

---

### No continue statement **(planned)**

```
error: 'continue' is not supported in Trident
```

**Spec:** language.md Section 5, Section 21 (permanent exclusion).

---

### No enum declaration **(planned)**

```
error: 'enum' is not supported; Trident has no sum types
  help: use struct + integer tag for variant patterns
```

**Spec:** language.md Section 2, Section 21 (no enums, no sum types).

---

### No trait declaration **(planned)**

```
error: 'trait' is not supported in Trident
```

**Spec:** language.md Section 21 (permanent exclusion).

---

### No impl block **(planned)**

```
error: 'impl' is not supported; use free functions
```

**Spec:** language.md Section 21 (permanent exclusion).

---

### No macro declaration **(planned)**

```
error: macros are not supported in Trident
```

**Spec:** language.md Section 21 (permanent exclusion).

---

### No closure syntax **(planned)**

```
error: closures are not supported in Trident
  help: use named functions instead
```

**Spec:** language.md Section 3, Section 21 (no closures).

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

**Spec:** language.md Section 5, Section 20 (let_stmt grammar includes type).

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

**Spec:** language.md Section 21 (permanent exclusion).

---

### No concurrency keywords **(planned)**

```
error: 'async'/'await'/'spawn' are not supported
  help: Trident execution is sequential; concurrency is handled at the runtime level
```

**Spec:** language.md Section 21 (permanent exclusion).

---

### No pointers or references **(planned)**

```
error: pointers and references ('&', '*') are not supported
  help: all values are passed by copy on the stack
```

**Spec:** language.md Section 2, Section 8, Section 21 (no heap, no pointers).

---

## Type Errors

### Binary operator type mismatch

```
error: operator '+' requires both operands to be Field (or both XField), got Field and Bool
error: operator '==' requires same types, got Field and U32
error: operator '<' requires U32 operands, got Field and Field
error: operator '&' requires U32 operands, got Field and Field
error: operator '/%' requires U32 operands, got Field and Field
error: operator '*.' requires XField and Field, got Field and Field
```

Each operator has specific type requirements. See [language.md](language.md)
Section 4 for the operator table.

---

### Type mismatch in let binding

```
error: type mismatch: declared Field but expression has type Bool
```

The expression type does not match the declared type annotation.

---

### Type mismatch in assignment

```
error: type mismatch in assignment: expected Field but got Bool
```

---

### Cannot assign to immutable variable

```
error: cannot assign to immutable variable
  help: declare the variable with `let mut` to make it mutable
```

**Fix:**

```
let mut x: Field = 0
x = 42
```

---

### Undefined variable

```
error: undefined variable 'x'
  help: check that the variable is declared with `let` before use
```

---

### Undefined function

```
error: undefined function 'foo'
  help: check the function name and ensure the module is imported with `use`
```

---

### Undefined constant **(planned)**

```
error: undefined constant 'MAX_SIZE'
  help: declare with `const MAX_SIZE: U32 = ...` or import the defining module
```

**Spec:** language.md Section 3 (constants).

---

### Function arity mismatch

```
error: function 'foo' expects 2 arguments, got 3
```

---

### Function argument type mismatch

```
error: argument 1 of 'foo': expected Field but got Bool
```

---

### Return type mismatch

```
error: function 'foo' declared return type Field, but body returns Bool
```

---

### No implicit conversion **(planned)**

```
error: cannot implicitly convert U32 to Field
  help: use `as_field(x)` for U32 -> Field or `as_u32(x)` for Field -> U32
```

No automatic coercion between types. All conversions must be explicit.

**Spec:** language.md Section 2, Section 10 (no implicit conversions).

---

### Undefined struct

```
error: undefined struct 'Point'
  help: check the struct name spelling, or import the module that defines it
```

---

### Struct missing field

```
error: missing field 'y' in struct init
```

All fields must be provided in a struct literal.

---

### Struct unknown field

```
error: unknown field 'z' in struct 'Point'
```

---

### Struct field type mismatch

```
error: field 'x': expected Field but got Bool
```

---

### Field access on non-struct

```
error: field access on non-struct type Field
```

---

### Private field access

```
error: field 'secret' of struct 'Account' is private
```

**Fix:** Mark the field `pub` or provide a public accessor function.

---

### Private function access **(planned)**

```
error: function 'helper' of module 'wallet' is private
  help: mark the function `pub` to make it accessible from other modules
```

**Spec:** language.md Section 1 (visibility: pub or default private).

---

### Private struct access **(planned)**

```
error: struct 'Internal' of module 'wallet' is private
  help: mark the struct `pub` to make it accessible from other modules
```

**Spec:** language.md Section 1 (visibility).

---

### Index on non-array

```
error: index access on non-array type Field
```

---

### Array index type mismatch **(planned)**

```
error: array index must be U32 or compile-time integer, got Bool
```

**Spec:** language.md Section 4 (array indexing).

---

### Array index out of bounds **(planned)**

```
error: array index 5 is out of bounds for array of size 3
```

Compile-time constant indices are bounds-checked statically.

**Spec:** language.md Section 4 (array indexing with compile-time sizes).

---

### Array element type mismatch

```
error: array element type mismatch: expected Field got Bool
```

All elements of an array literal must have the same type.

---

### Tuple element count limit **(planned)**

```
error: tuple has 20 elements, maximum is 16
```

**Spec:** language.md Section 2 (max 16 tuple elements).

---

### Parameter count limit **(planned)**

```
error: function 'foo' has 20 parameters, maximum is 16
  help: group related parameters into a struct
```

**Spec:** language.md Section 3 (maximum 16 parameters).

---

### Tuple destructuring mismatch

```
error: tuple destructuring: expected 3 elements, got 2 names
```

---

### Digest destructuring mismatch

```
error: digest destructuring requires exactly D names, got N
```

The number of names in a digest destructuring must match the target's
digest width.

---

### Cannot destructure non-tuple

```
error: cannot destructure non-tuple type Field
```

---

### Tuple assignment mismatch

```
error: tuple assignment: expected 3 elements, got 2 names
```

---

### If condition type

```
error: if condition must be Bool or Field, got Digest
```

---

### Recursion detected

```
error: recursive call cycle detected: main -> foo -> main
  help: stack-machine targets do not support recursion; use loops (`for`) or iterative algorithms instead
```

Trident prohibits recursion because all target VMs require deterministic
trace lengths. Rewrite using `for` loops with `bounded`:

```
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

```
error: unreachable code after return statement
  help: remove this code or move it before the return
```

---

### Unreachable code after halt **(planned)**

```
error: unreachable code after unconditional halt
  help: code after `assert(false)` or `halt` can never execute
```

**Spec:** language.md Section 10 (dead code after halt/assert rejected).

---

### No function overloading **(planned)**

```
error: function 'foo' is already defined
  help: Trident does not support function overloading; use distinct names
```

**Spec:** language.md Section 3 (no function overloading).

---

### No type generics **(planned)**

```
error: type-level generics are not supported
  help: only size parameters (integers) are allowed: `fn foo<N>(...)`
```

**Spec:** language.md Section 3, Section 21 (only integer size parameters).

---

### No default arguments **(planned)**

```
error: default parameter values are not supported
  help: define separate functions for different argument combinations
```

**Spec:** language.md Section 3 (no default arguments).

---

### No variadic arguments **(planned)**

```
error: variadic arguments are not supported
  help: use a fixed-size array parameter instead
```

**Spec:** language.md Section 3 (no variadic arguments).

---

### Transitive import access **(planned)**

```
error: cannot access 'B.foo' through module 'A'
  help: import module 'B' directly with `use B`
```

If A imports B, C cannot reach B's items through A. No re-exports.

**Spec:** language.md Section 1 (no re-exports).

---

### No floats **(planned)**

```
error: floating-point types are not supported
  help: use Field for arithmetic — all computation is over finite fields
```

**Spec:** language.md Section 2, Section 21 (permanent exclusion).

---

### No Option or Result **(planned)**

```
error: 'Option' and 'Result' types are not supported
  help: use assert for validation; failure = no proof
```

**Spec:** language.md Section 2 (no Option, no Result).

---

## Control Flow Errors

### For loop without bounded

```
error: loop end must be a compile-time constant, or annotated with a bound
  help: use a literal like `for i in 0..10 { }` or add a bound: `for i in 0..n bounded 100 { }`
```

All loops must have compile-time-known or declared upper bounds for
deterministic trace length computation.

---

### Non-exhaustive match

```
error: non-exhaustive match: not all possible values are covered
  help: add a wildcard `_ => { ... }` arm to handle all remaining values
```

---

### Unreachable pattern after wildcard

```
error: unreachable pattern after wildcard '_'
  help: the wildcard `_` already matches all values; remove this arm or move it before `_`
```

---

### Match pattern type mismatch

```
error: integer pattern on Bool scrutinee; use `true` or `false`
error: Bool pattern on non-Bool scrutinee
```

---

### Struct pattern type mismatch

```
error: struct pattern `Point` does not match scrutinee type `Config`
```

---

### Unknown struct field in pattern

```
error: struct `Point` has no field `z`
```

---

### Missing field in struct pattern **(planned)**

```
error: match on struct 'Point' is missing field 'y' in pattern
  help: bind or ignore all fields: `Point { x, y: _ }`
```

Struct patterns must account for every field.

**Spec:** language.md Section 5 (exhaustive match, struct patterns).

---

### Duplicate match arm **(planned)**

```
error: duplicate match arm for value '0'
  help: remove the duplicate arm
```

**Spec:** language.md Section 5 (match semantics).

---

## Size Generic Errors

### Size argument to non-generic function

```
error: function 'foo' is not generic but called with size arguments
```

**Fix:** Remove the angle bracket arguments.

---

### Size parameter count mismatch

```
error: function 'foo' expects 2 size parameters, got 1
```

---

### Cannot infer size argument

```
error: cannot infer size parameter 'N'; provide explicit size argument
```

**Fix:** Provide the size argument explicitly:

```
let result: Field = sum<5>(arr)
```

---

### Expected concrete size

```
error: expected concrete size, got 'N'
```

A size parameter could not be resolved to a concrete integer.

---

### Array size not compile-time known **(planned)**

```
error: array size must be a compile-time known integer
  help: use a literal, const, or size parameter expression
```

**Spec:** language.md Section 2 (array sizes must be compile-time known).

---

### Zero or negative array size **(planned)**

```
error: array size must be a positive integer, got 0
```

**Spec:** language.md Section 2 (fixed-size arrays, meaningful sizes).

---

## Event Errors

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

**Spec:** language.md Section 15, Section 10 (reveal/seal must match event
with matching field types).

---

### Duplicate event declaration **(planned)**

```
error: event 'Transfer' is already defined
```

**Spec:** language.md Section 20 (items are unique within a module).

---

## Annotation Errors

### #[intrinsic] restriction

```
error: #[intrinsic] is only allowed in std.*/ext.* modules, not in 'my_module'
```

The `#[intrinsic]` attribute is reserved for standard library and extension
modules shipped with the compiler. User code cannot use it.

---

### #[test] validation

```
error: #[test] function 'test_add' must have no parameters
error: #[test] function 'test_add' must not have a return type
```

Test functions take no arguments and return nothing.

---

### #[pure] I/O restriction

```
error: #[pure] function cannot call 'pub_read' (I/O side effect)
error: #[pure] function cannot use 'reveal' (I/O side effect)
error: #[pure] function cannot use 'seal' (I/O side effect)
```

Functions annotated `#[pure]` cannot perform any I/O operations.

---

### Unknown attribute **(planned)**

```
error: unknown attribute '#[foo]'
  help: valid attributes are: cfg, test, pure, intrinsic, requires, ensures
```

**Spec:** language.md Section 7 (closed set of attributes).

---

### Duplicate attribute **(planned)**

```
error: duplicate attribute '#[pure]' on function 'foo'
```

**Spec:** language.md Section 7.

---

### Unknown cfg flag **(planned)**

```
error: unknown cfg flag 'unknown_flag'
  help: valid cfg flags are target-specific and project-defined
```

**Spec:** language.md Section 7 (cfg conditional compilation).

---

## Module Errors

### Cannot find module

```
error: cannot find module 'helpers' (looked at 'path/to/helpers.tri'): No such file
  help: create the file 'path/to/helpers.tri' or check the module name in the `use` statement
```

---

### Circular dependency

```
error: circular dependency detected involving module 'a'
  help: break the cycle by extracting shared definitions into a separate module
```

---

### Duplicate function

```
error: duplicate function 'main'
```

---

### Cannot read entry file

```
error: cannot read 'main.tri': No such file or directory
  help: check that the file exists and is readable
```

---

### Program without main **(planned)**

```
error: program 'my_program' must have a `fn main()` entry point
  help: add `fn main() { ... }` or change to `module` if this is a library
```

**Spec:** language.md Section 1 (program must have fn main).

---

### Module with main **(planned)**

```
error: module 'my_module' must not define `fn main()`
  help: modules are libraries; change to `program` if this is an entry point
```

**Spec:** language.md Section 1 (module must NOT have fn main).

---

### Duplicate struct **(planned)**

```
error: duplicate struct definition 'Point'
```

**Spec:** language.md Section 20 (items are unique within a module).

---

### Duplicate constant **(planned)**

```
error: duplicate constant definition 'MAX'
```

**Spec:** language.md Section 20 (items are unique within a module).

---

### Duplicate import **(planned)**

```
error: duplicate import 'use merkle'
```

**Spec:** language.md Section 1 (import rules).

---

### Self import **(planned)**

```
error: module cannot import itself
```

**Spec:** language.md Section 1 (DAG requirement).

---

## Target Errors

### Unknown target

```
error: unknown target 'wasm' (looked for 'targets/wasm.toml')
  help: available targets: triton, miden, openvm, sp1, cairo
```

---

### Cannot read target config

```
error: cannot read target config 'targets/foo.toml': No such file
```

---

### Invalid target name

```
error: invalid target name '../../../etc/passwd'
```

Target names cannot contain path traversal characters.

---

### Tier capability exceeded **(planned)**

```
error: program uses Tier 2 operations but target 'sp1' only supports up to Tier 1
  help: remove hash/sponge/merkle operations or choose a Tier 2 target (triton, miden)
```

The program's tier (highest-tier op used) exceeds the target's maximum
supported tier. See [targets.md](targets.md) for tier compatibility.

**Spec:** ir.md (compiler rejects programs using ops above target capability).

---

### XField on unsupported target **(planned)**

```
error: type 'XField' is not available on target 'miden' (xfield_width = 0)
  help: XField requires a target with extension field support (currently: triton)
```

**Spec:** language.md Section 11, targets.md (XField = Tier 2, extension field
targets only).

---

### Scalar multiply on unsupported target **(planned)**

```
error: operator '*.' (scalar multiply) is not available on target 'miden'
  help: '*.' requires XField support (currently: triton only)
```

**Spec:** language.md Section 12 (Tier 2 operator), targets.md.

---

### Hash builtins on unsupported target **(planned)**

```
error: builtin 'hash' is not available on target 'sp1' (Tier 2 required)
  help: hash/sponge operations require a target with native hash coprocessor (triton, miden)
```

**Spec:** language.md Section 13, targets.md (hash = Tier 2).

---

### Sponge builtins on unsupported target **(planned)**

```
error: builtin 'sponge_init' is not available on target 'sp1'
  help: sponge operations require a Tier 2 target (triton, miden)
```

**Spec:** language.md Section 13, targets.md (sponge = Tier 2).

---

### Merkle builtins on unsupported target **(planned)**

```
error: builtin 'merkle_step' is not available on target 'sp1'
  help: Merkle operations require a Tier 2 target (triton, miden)
```

**Spec:** language.md Section 14, targets.md (merkle = Tier 2).

---

### XField builtins on unsupported target **(planned)**

```
error: builtin 'xfield' is not available on target 'miden'
  help: extension field builtins require XField support (currently: triton only)
```

**Spec:** language.md Section 16, targets.md (XField builtins = Triton only).

---

### Cross-target import **(planned)**

```
error: cannot import 'ext.triton.xfield' when compiling for target 'miden'
  help: ext.<target>.* modules bind to a specific target
```

Importing `ext.<target>.*` binds the program to that target. Compiling
for a different target is a hard error.

**Spec:** language.md Section 18, targets.md (cross-target imports rejected).

---

### Tier 3 on non-Triton target **(planned)**

```
error: recursive proof verification (Tier 3) is only available on Triton VM
  help: ProofBlock, FriVerify, and extension field folding require Triton VM
```

**Spec:** ir.md (Tier 3 = Triton only), targets.md tier compatibility.

---

### Hash rate argument mismatch **(planned)**

```
error: hash() requires 10 field arguments on target 'triton', got 8
  help: hash rate R = 10 for Triton VM; see targets.md for per-target rates
```

The number of arguments to `hash()` must match the target's hash rate R.

**Spec:** language.md Section 13 (hash takes R elements, R is target-dependent).

---

### Sponge absorb argument mismatch **(planned)**

```
error: sponge_absorb() requires 10 field arguments on target 'triton', got 5
  help: sponge rate R = 10 for Triton VM; see targets.md for per-target rates
```

**Spec:** language.md Section 13 (sponge_absorb takes R elements).

---

## Builtin Type Errors

These errors enforce the type signatures of builtin functions. Some may be
caught by generic function type checking (T07/T08), but builtins have
target-dependent signatures that deserve explicit diagnostics.

### Builtin argument type mismatch **(planned)**

```
error: builtin 'sub' expects (Field, Field), got (U32, U32)
  help: sub() operates on Field values; convert with as_field() first
```

**Spec:** language.md Section 6 (each builtin has specific argument types).

---

### Builtin argument count mismatch **(planned)**

```
error: builtin 'split' expects 1 argument, got 2
```

**Spec:** language.md Section 6.

---

### Assert argument type **(planned)**

```
error: assert() requires Bool argument, got Digest
```

**Spec:** language.md Section 6 (assert(cond: Bool)).

---

### Assert_eq argument type **(planned)**

```
error: assert_eq() requires (Field, Field), got (Bool, Bool)
  help: use `assert(a == b)` for Bool equality
```

**Spec:** language.md Section 6 (assert_eq takes Field, Field).

---

### Assert_digest argument type **(planned)**

```
error: assert_digest() requires (Digest, Digest), got (Field, Field)
```

**Spec:** language.md Section 6.

---

### RAM address type **(planned)**

```
error: ram_read() address must be Field, got Bool
```

**Spec:** language.md Section 6, Section 8 (RAM: word-addressed by Field).

---

## Inline Assembly Errors

### Asm effect mismatch **(planned)**

```
error: asm block declared effect '+1' but actual stack effect differs
  help: the effect annotation is the contract between assembly and the compiler's stack model
```

The compiler trusts the declared effect but may detect mismatches when the
surrounding code's stack doesn't balance.

**Spec:** language.md Section 9 (effect annotation is the contract).

---

### Asm in pure function **(planned)**

```
error: asm block not allowed in #[pure] function
  help: asm blocks may have unchecked I/O side effects
```

Since the compiler cannot verify what inline assembly does, it's incompatible
with the `#[pure]` guarantee.

**Spec:** language.md Section 7 (pure = no I/O), Section 9.

---

## Warnings

### Unused import

```
warning: unused import 'std.crypto.hash'
```

**Fix:** Remove the unused `use` statement.

---

### Asm block target mismatch

```
warning: asm block tagged for 'risc_v' will be skipped (current target: 'triton')
```

An `asm` block tagged for a different target is silently skipped. This is
informational when using multi-target `asm` blocks intentionally.

---

### Power-of-2 boundary proximity

```
warning: program is 3 rows below padded height boundary
  help: consider optimizing to stay well below 1024
```

The program is close to a power-of-2 table height boundary. A small code
change could double proving cost.

---

### Unused variable **(planned)**

```
warning: unused variable 'x'
  help: prefix with `_` to suppress: `let _x: Field = ...`
```

**Spec:** general compiler quality.

---

### Unused function **(planned)**

```
warning: unused function 'helper'
```

**Spec:** general compiler quality.

---

### Unused constant **(planned)**

```
warning: unused constant 'MAX'
```

**Spec:** general compiler quality.

---

### Shadowed variable **(planned)**

```
warning: variable 'x' shadows previous declaration
```

**Spec:** general compiler quality.

---

## Optimization Hints

The compiler produces hints (not errors) when it detects cost antipatterns.
These appear with `trident build --hints`.

### H0001: Hash table dominance

```
hint[H0001]: hash table is 3.2x taller than processor table
```

The hash table dominates proving cost. Processor-level optimizations will
not reduce proving time.

**Action:** Batch data before hashing, reduce Merkle depth, use
`sponge_absorb_mem` instead of repeated `sponge_absorb`.

---

### H0002: Power-of-2 headroom

```
hint[H0002]: padded height is 1024, but max table height is only 519
```

Significant headroom below the next power-of-2 boundary. The program could
be more complex at zero additional proving cost.

---

### H0003: Redundant range check

```
hint[H0003]: as_u32(x) is redundant — value is already proven U32
```

A value that was already range-checked is being checked again.

**Action:** Remove the redundant `as_u32()` call.

---

### H0004: Loop bound waste

```
hint[H0004]: loop in 'process' bounded 128 but iterates only 10 times
```

The declared loop bound is much larger than the actual constant iteration
count. This inflates worst-case cost analysis.

**Action:** Tighten the `bounded` declaration to match actual usage.

---

### H0005: Unnecessary spill **(planned)**

```
hint[H0005]: variable 'x' spilled to RAM but used immediately after
  help: reorder declarations to keep frequently-used variables in the top 16 stack positions
```

The compiler's LRU spill policy pushed a variable to RAM unnecessarily.

**Action:** Reorder variable declarations or split large blocks into functions.

**Spec:** language.md Section 8 (stack: 16 elements, LRU spill to RAM).

---

## Summary

| Category | Total | Implemented | Planned |
|----------|------:|------------:|--------:|
| Lexer | 19 | 7 | 12 |
| Parser | 24 | 8 | 16 |
| Type | 34 | 24 | 10 |
| Control flow | 8 | 6 | 2 |
| Size generics | 6 | 4 | 2 |
| Events | 7 | 5 | 2 |
| Annotations | 6 | 3 | 3 |
| Module | 10 | 4 | 6 |
| Target | 14 | 3 | 11 |
| Builtin type | 6 | 0 | 6 |
| Inline assembly | 2 | 0 | 2 |
| Warnings | 7 | 3 | 4 |
| Hints | 5 | 4 | 1 |
| **Total** | **148** | **71** | **77** |

---

## See Also

- [Language Reference](language.md) — Types, operators, builtins, grammar
- [Target Reference](targets.md) — Target profiles, cost models, and OS model
- [IR Reference](ir.md) — 54 operations, 4 tiers, lowering paths
- [Tutorial](../tutorials/tutorial.md) — Step-by-step guide with working examples
- [For Developers](../tutorials/for-developers.md) — Why bounded loops? Why no heap?
- [Optimization Guide](../guides/optimization.md) — Cost reduction strategies
