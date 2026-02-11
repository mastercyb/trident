# Error Catalog

A reference of all [Trident](../README.md) compiler errors with explanations and fixes.

## Lexer Errors

### Unexpected character

```
error: unexpected character '@' (U+0040)
  help: this character is not recognized as part of Trident syntax
```

**Cause:** A character that is not part of the Trident grammar was found. Trident source files must be ASCII.

**Fix:** Remove the unexpected character. Check for copy-paste artifacts or encoding issues.

---

### No subtraction operator

```
error: unexpected '-'; Trident has no subtraction operator
  help: use the `sub(a, b)` function instead of `a - b`
```

**Cause:** Trident deliberately omits the `-` operator (see [design rationale](spec.md)). The `->` arrow for return types uses `-`, but standalone `-` is not allowed.

**Fix:** Use `std.core.field.sub(a, b)` or `std.core.field.neg(a)`:

```
use std.core.field
let diff: Field = std.core.field.sub(a, b)
```

---

### No division operator

```
error: unexpected '/'; Trident has no division operator
  help: use the `/% (divmod)` operator instead: `let (quot, rem) = a /% b`
```

**Cause:** Trident has no `/` operator. Division in a [prime field](https://en.wikipedia.org/wiki/Finite_field) is actually multiplication by the modular inverse, which is expensive. The `/% (divmod)` operator makes this cost explicit.

**Fix:** Use the divmod operator:

```
let (quotient, remainder) = a /% b
```

---

### Integer too large

```
error: integer literal '999999999999999999999' is too large
  help: maximum integer value is 18446744073709551615
```

**Cause:** The integer literal exceeds `u64::MAX` (2^64 - 1).

**Fix:** Use a smaller value. For field arithmetic, values are automatically reduced modulo p.

---

### Unterminated asm block

```
error: unterminated asm block: missing closing '}'
  help: every `asm { ... }` block must have a matching closing brace
```

**Cause:** An `asm` block was opened with `{` but never closed.

**Fix:** Add the closing `}`:

```
asm(+1) { push 42 }
```

---

## Parser Errors

### Expected program or module

```
error: expected 'program' or 'module' declaration at the start of file
  help: every .tri file must begin with `program <name>` or `module <name>`
```

**Cause:** The file does not start with a `program` or `module` declaration.

**Fix:** Add a declaration at the top:

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

**Cause:** The code has more than 256 levels of nested blocks (if/else, for, match, etc.).

**Fix:** Extract deeply nested logic into separate functions.

---

## Type Errors

### Type mismatch in binary operation

```
error: binary operator '+' requires matching types, got Field and Bool
```

**Cause:** The left and right sides of a binary operator have incompatible types.

**Fix:** Ensure both operands have the same type:

```
// Wrong
let x: Field = 1
let y: Bool = true
let z = x + y  // ERROR

// Right
let x: Field = 1
let y: Field = 2
let z = x + y  // OK
```

---

### Type mismatch in assignment

```
error: expected Field, got Bool
```

**Cause:** The assigned value does not match the variable's declared type.

**Fix:** Ensure the expression type matches the variable type.

---

### Undefined variable

```
error: undefined variable 'x'
```

**Cause:** The variable has not been declared in the current scope.

**Fix:** Declare the variable with `let` before using it:

```
let x: Field = 42
pub_write(x)
```

---

### Undefined function

```
error: undefined function 'foo'
```

**Cause:** The function has not been declared in the current module or imported modules.

**Fix:** Either define the function or import the module that contains it:

```
use std.crypto.hash
let d: Digest = std.crypto.hash.tip5(a, 0, 0, 0, 0, 0, 0, 0, 0, 0)
```

---

### Function arity mismatch

```
error: function 'foo' expects 2 arguments, got 3
```

**Cause:** The function was called with the wrong number of arguments.

**Fix:** Check the function signature and provide the correct number of arguments.

---

### Return type mismatch

```
error: function 'foo' declared return type Field, but body returns Bool
```

**Cause:** The function body's tail expression or `return` statement has a different type than declared.

**Fix:** Ensure the return expression matches the declared type.

---

### Cannot assign to immutable variable

```
error: cannot assign to immutable variable 'x'
  help: declare with `let mut x` to make it mutable
```

**Cause:** Attempting to assign to a variable declared without `mut`.

**Fix:** Add `mut` to the declaration:

```
let mut x: Field = 0
x = 42  // OK
```

---

### Undefined struct

```
error: undefined struct 'Point'
```

**Cause:** The struct has not been declared in the current scope.

**Fix:** Define the struct or import the module that defines it.

---

### Private field access

```
error: field 'secret' of struct 'Account' is private
```

**Cause:** Attempting to access a field not marked `pub` from outside the defining module.

**Fix:** Either mark the field `pub` or provide a public accessor function.

---

### Tuple destructuring size mismatch

```
error: tuple destructuring: expected 3 elements, got 2
```

**Cause:** The number of variables on the left side of a tuple destructuring does not match the tuple size.

**Fix:** Match the number of variables to the tuple size:

```
let (a, b, c) = function_returning_3_tuple()
```

---

## Control Flow Errors

### Unreachable code after return

```
error: unreachable tail expression after return
  help: remove this expression or move it before the return
```

**Cause:** Code appears after a `return` statement in the same block.

**Fix:** Remove the dead code or restructure the function.

---

### For loop without bounded

```
error: for loop with non-constant range requires `bounded N` annotation
  help: add `bounded <max>` after the range to specify the maximum iteration count
```

**Cause:** A for loop with a dynamic range does not have a `bounded` annotation. Bounded loops are a fundamental requirement of STARK proof systems -- see [For Developers](for-developers.md) Section 3 for why.

**Fix:** Add the `bounded` keyword with a maximum iteration count:

```
for i in 0..n bounded 100 {
    // ...
}
```

---

### Non-exhaustive match

```
error: non-exhaustive match: missing wildcard '_' arm
  help: add a `_ => { ... }` arm to handle all remaining cases
```

**Cause:** A match statement does not cover all possible values and has no wildcard arm.

**Fix:** Add a wildcard arm:

```
match x {
    0 => { handle_zero() }
    _ => { handle_other() }
}
```

---

### Unreachable pattern after wildcard

```
error: unreachable pattern after wildcard '_'
  help: remove this arm or move it before the wildcard
```

**Cause:** A pattern appears after the wildcard `_` arm, which catches everything.

**Fix:** Move the specific pattern before the wildcard, or remove it.

---

## Module Errors

### Cannot find module

```
error: cannot find module 'helpers' (looked at 'path/to/helpers.tri'): No such file
  help: create the file 'path/to/helpers.tri' or check the module name in the `use` statement
```

**Cause:** The referenced module file does not exist at the expected path.

**Fix:** Create the module file or correct the `use` statement.

---

### Circular dependency

```
error: circular dependency detected involving module 'a'
  help: break the cycle by extracting shared definitions into a separate module
```

**Cause:** Two or more modules depend on each other in a cycle.

**Fix:** Extract shared definitions into a third module that both can depend on.

---

### Duplicate function definition

```
error: duplicate function 'main'
```

**Cause:** Two functions with the same name are defined in the same module.

**Fix:** Rename one of the functions.

---

## Event Errors

### Undefined event

```
error: undefined event 'Transfer'
```

**Cause:** The event referenced in an `emit` or `seal` statement has not been declared.

**Fix:** Declare the event:

```
event Transfer {
    from: Digest,
    to: Digest,
    amount: Field,
}
```

---

### Event field mismatch

```
error: event 'Transfer' expects fields: from, to, amount
```

**Cause:** The fields provided in an `emit` or `seal` statement do not match the event declaration.

**Fix:** Provide exactly the fields declared in the event definition, in any order.

---

## Warnings

### Unused import

```
warning: unused import 'std.crypto.hash'
```

**Cause:** A module was imported with `use` but none of its items are used.

**Fix:** Remove the unused `use` statement.

---

### Asm block target mismatch

```
warning: asm block tagged for 'risc_v' will be skipped (current target: 'triton')
```

**Cause:** An inline `asm` block is annotated with a target that does not match the current compilation target. The block will be silently skipped during code generation, producing no instructions.

**Fix:** This warning is informational when you intentionally provide target-specific asm blocks. If the block should run on the current target, update its tag:

```
// Runs only when compiling for triton:
asm(triton, +1) { push 42 }

// Runs on any target (no tag):
asm(+1) { push 42 }
```

---

### Recursion detected

```
error: recursive function call detected: main -> foo -> main
  help: Trident does not allow recursion; use `for` loops instead
```

**Cause:** A function directly or indirectly calls itself. Trident prohibits recursion because [Triton VM](https://triton-vm.org/) requires deterministic trace lengths -- see [For Developers](for-developers.md) Section 4 for the full explanation.

**Fix:** Rewrite the algorithm using `for` loops with `bounded`:

```
// Instead of recursive fibonacci:
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

## Size Generic Errors

### Size argument to non-generic function

```
error: function 'foo' does not accept size arguments
```

**Cause:** Angle bracket size arguments were provided to a function that is not generic.

**Fix:** Remove the size arguments:

```
// Wrong: foo is not generic
let x: Field = foo<3>(a)

// Right
let x: Field = foo(a)
```

---

### Cannot infer size argument

```
error: cannot infer size argument for function 'sum'
```

**Cause:** The compiler cannot determine the size generic parameter from the argument types.

**Fix:** Provide the size argument explicitly:

```
let result: Field = sum<5>(arr)
```

---

## Match Errors

### Non-exhaustive match

```
error: non-exhaustive match: missing wildcard '_' arm
```

**Cause:** A match statement does not cover all possible values and has no wildcard arm.

**Fix:** Add a wildcard arm:

```
match x {
    0 => { handle_zero() }
    _ => { handle_other() }
}
```

---

### Unreachable pattern after wildcard

```
error: unreachable pattern after wildcard '_'
```

**Cause:** A pattern appears after the wildcard `_` arm, which catches everything.

**Fix:** Move the specific pattern before the wildcard, or remove it.

---

## Struct Errors

### Missing struct fields

```
error: missing field 'y' in struct 'Point' initializer
```

**Cause:** A struct literal does not provide all required fields.

**Fix:** Provide all fields declared in the struct:

```
// Wrong: missing 'y'
let p: Point = Point { x: 0 }

// Right
let p: Point = Point { x: 0, y: 0 }
```

---

## See Also

- [Tutorial](tutorial.md) -- Step-by-step guide with working examples
- [Language Reference](reference.md) -- Quick lookup: types, operators, builtins, grammar
- [Language Specification](spec.md) -- Complete language reference
- [Compiling a Program](compiling-a-program.md) -- Build pipeline and compiler stages that produce these errors
- [Programming Model](programming-model.md) -- How programs run in Triton VM
- [Optimization Guide](optimization.md) -- Cost reduction strategies
- [Formal Verification](formal-verification.md) -- Catch errors before runtime via symbolic verification
- [How STARK Proofs Work](stark-proofs.md) -- The proof system behind every Trident program
- [For Developers](for-developers.md) -- Why bounded loops? Why no heap? Concepts explained
- [For Blockchain Devs](for-blockchain-devs.md) -- Where's My Revert? section maps error patterns
- [Vision](vision.md) -- Why Trident exists and what you can build
- [Comparative Analysis](analysis.md) -- Triton VM vs. every other ZK system
