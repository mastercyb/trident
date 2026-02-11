# Formal Verification

Trident includes a built-in formal verification pipeline that can prove
properties of programs for all possible inputs -- not by testing, but by
exhaustive symbolic analysis. Because Trident programs are bounded,
first-order, heap-free computations over a finite field, the verification
problem is decidable: the compiler can automatically determine whether an
assertion holds universally or produce a concrete counterexample.

The verification toolchain consists of:

- **Specification annotations** (`#[requires]`, `#[ensures]`, `#[invariant]`,
  `#[pure]`) that declare properties directly in source code.
- **Symbolic execution** (`sym.rs`) that converts programs into constraint
  systems.
- **Algebraic solving** (`solve.rs`) using Schwartz-Zippel random evaluation
  over the Goldilocks field and bounded model checking.
- **SMT checking** (`smt.rs`) that encodes constraints as SMT-LIB2 bitvector
  queries for Z3.
- **Invariant synthesis** (`synthesize.rs`) that automatically infers loop
  invariants and pre/postconditions from code patterns.
- **Semantic equivalence checking** (`equiv.rs`) that proves two functions
  produce identical outputs for all inputs.

All of these are available today through the `trident verify`, `trident equiv`,
and `trident generate` commands.

---

## Specification Annotations

Trident supports four specification annotations on functions. These annotations
are checked at compile time and have zero runtime cost -- they do not appear in
the emitted TASM output.

### `#[requires(predicate)]`

Declares a precondition on function inputs. The function is only obligated to
behave correctly for inputs satisfying the predicate. Multiple `#[requires]`
annotations are conjunctive (all must hold).

```
#[requires(amount > 0)]
#[requires(sender_balance > amount)]
fn transfer(sender_balance: Field, amount: Field) -> Field {
    sub(sender_balance, amount)
}
```

### `#[ensures(predicate)]`

Declares a postcondition on function outputs. The compiler verifies that the
postcondition holds for all inputs satisfying the preconditions. Use `result`
to refer to the return value.

```
#[requires(amount > 0)]
#[ensures(result == sub(balance, amount))]
fn withdraw(balance: Field, amount: Field) -> Field {
    sub(balance, amount)
}
```

For functions returning tuples, named bindings in the ensures clause refer to
the destructured output variables:

```
#[requires(amount > 0)]
#[ensures(new_sender == sub(sender_balance, amount))]
#[ensures(new_receiver == receiver_balance + amount)]
fn transfer(sender_balance: Field, receiver_balance: Field, amount: Field) -> (Field, Field) {
    let new_sender: Field = sub(sender_balance, amount)
    let new_receiver: Field = receiver_balance + amount
    (new_sender, new_receiver)
}
```

### `#[invariant(predicate)]`

Declares a loop invariant. The compiler verifies:

1. The invariant holds at loop entry.
2. If the invariant holds at iteration `i`, it holds at iteration `i + 1`.
3. The invariant at loop exit implies any enclosing postcondition.

```
#[ensures(result == n * (n - 1) / 2)]
fn sum_to(n: U32) -> Field {
    let mut total: Field = 0

    #[invariant(total == i * (i - 1) / 2)]
    for i in 0..n bounded 1000 {
        total = total + as_field(i)
    }

    total
}
```

For loops with small bounds, the verifier can also simply unroll the loop and
check directly without requiring an invariant annotation.

### `#[pure]`

Marks a function as having no I/O side effects (no `pub_read`, `pub_write`,
or `divine` calls). Pure functions enable more aggressive symbolic reasoning
because the verifier can safely inline and rewrite them.

```
#[pure]
fn square(x: Field) -> Field {
    x * x
}
```

### Inline Assertions

Every `assert()` and `assert_eq()` in Trident source is also a specification.
The verifier attempts to prove each assertion holds for all executions that
reach it. No annotation is needed -- assertions are verified automatically.

```
fn safe_transfer(sender_balance: Field, amount: Field, receiver_balance: Field) {
    assert(amount != 0)

    let sender_new = sub(sender_balance, amount)
    let receiver_new = receiver_balance + amount

    // Conservation: the verifier proves this algebraically
    assert_eq(sender_new + receiver_new, sender_balance + receiver_balance)

    pub_write(sender_new)
    pub_write(receiver_new)
}
```

---

## Verification Engine

The `trident verify` command runs the full verification pipeline. It processes
a Trident source file through three stages.

### Stage 1: Symbolic Execution

The symbolic executor (`sym.rs`, ~1,005 lines) walks the type-checked AST and
builds a `ConstraintSystem`. Each variable becomes a symbolic value. Each
operation becomes a constraint. The executor handles:

- **`let` bindings** as symbolic variable assignments (SSA-versioned for
  mutable variables).
- **`assert` / `assert_eq`** as equality or truth constraints.
- **`if/else`** as path conditions, with ITE (if-then-else) merging of
  environments from both branches.
- **Bounded `for` loops** by unrolling up to the declared bound (default
  maximum unroll: 64 iterations). Loops with dynamic bounds get path
  conditions on each iteration.
- **`match` arms** with per-arm path conditions and environment merging.
- **Function calls** by inlining (up to depth 64; no recursion in Trident
  means this always terminates).
- **`divine()` / `pub_read()`** as fresh symbolic variables.
- **`pub_write()`** by recording symbolic output values.
- **Hash operations** as opaque symbolic values (uninterpreted).

The resulting `ConstraintSystem` contains all constraints, variable bindings,
public inputs/outputs, and divine inputs.

### Stage 2: Algebraic Solving

The algebraic solver (`solve.rs`, ~1,032 lines) checks the constraint system
using two methods:

**Schwartz-Zippel random evaluation.** Constraints are evaluated at 100 random
points over the Goldilocks field (p = 2^64 - 2^32 + 1). If a polynomial
identity holds at k random points, the probability it is false is at most d/p
where d is the polynomial degree. For typical degrees (< 2^16) and Goldilocks
p (approximately 2^64), the false-positive probability is negligible. Early
rounds use special values (0, 1, p-1, small primes, powers of 2, U32 boundary
values) for better coverage.

**Bounded model checking.** For systems with few free variables (8 or fewer),
the solver tests a grid of interesting field values exhaustively. For larger
systems, it uses stratified random sampling. When a constraint fails, the
solver reports a concrete counterexample with the variable assignments that
caused the violation.

The solver also detects redundant assertions (constraints that hold for all
tested inputs), which can be removed to reduce proving cost.

### Stage 3: SMT Checking

The SMT encoder (`smt.rs`, ~534 lines) translates the constraint system into
an SMT-LIB2 script using the `QF_BV` (quantifier-free bitvector) logic.
Goldilocks field arithmetic is encoded as 128-bit bitvector operations with
modular reduction. Two query modes are supported:

- **Safety check**: Asserts the negation of the conjunction of all constraints
  and checks satisfiability. SAT means a counterexample was found (a bug).
  UNSAT means all constraints hold for all inputs.
- **Witness existence**: Asserts all constraints and checks satisfiability.
  SAT means a valid `divine()` witness exists. UNSAT means no valid witness
  can be constructed.

### CLI Usage

```bash
# Standard verification: symbolic execution + algebraic solver + BMC
trident verify main.tri

# Verbose output: show the constraint system summary
trident verify main.tri --verbose

# Export SMT-LIB2 encoding for external solvers
trident verify main.tri --smt output.smt2

# Run Z3 directly (requires Z3 installed and in PATH)
trident verify main.tri --z3

# Machine-readable JSON report (for CI or LLM consumption)
trident verify main.tri --json

# Run automatic invariant synthesis alongside verification
trident verify main.tri --synthesize
```

### Verification Output

A typical verification report looks like:

```
Verifying main.tri...

Static analysis: PASS (no trivially violated assertions)

Random testing (Schwartz-Zippel):
Solver: 5 constraints, 100 rounds
  Result: ALL PASSED

Bounded model checking:
Solver: 5 constraints, 256 rounds
  Result: ALL PASSED
  Redundant assertions (always true): 1

Optimization: 1 assertion(s) appear redundant (always true)
  These could be removed to reduce proving cost.

Verdict: SAFE -- no violations found
```

When a violation is found:

```
Verdict: UNSAFE -- random testing found violations (high confidence)
  Constraint #2: assert((pub_in_0 == 0))
  Counterexample:
    pub_in_0 = 7164325918402846317
```

---

## Invariant Synthesis

The invariant synthesis engine (`synthesize.rs`, ~1,307 lines) automatically
infers specifications from code patterns. Run it with `trident verify --synthesize`.

### Template-Based Pattern Matching

The synthesizer recognizes common patterns in loop bodies and function
structures:

**Additive accumulation.** When a mutable variable is updated as
`acc = acc + expr` inside a loop, the synthesizer infers invariants like
`acc >= init_value` and suggests postconditions relating the final
accumulator value to the loop bound.

**Counting patterns.** When `acc = acc + 1` appears inside a conditional
within a loop, the synthesizer infers `count <= loop_var` as a loop
invariant and `count <= N` as a postcondition.

**Monotonic updates.** When a variable only increases (e.g., `x = x + 3`
with a non-negative literal), the synthesizer infers `x >= init_value`.

**Identity preservation.** Single-parameter functions that return their
parameter unchanged get `result == param` as a postcondition with full
confidence.

**Range preservation.** Functions with U32 inputs and U32 outputs get
`result <= 4294967295` as a suggested postcondition.

**Constant result detection.** Functions whose body is a single literal
expression get `result == constant` with full confidence.

### Precondition Inference

The synthesizer examines `assert()` calls in function bodies. If an assertion
references function parameters, it surfaces the condition as a suggested
`#[requires]` precondition. Similarly, `as_u32(param)` calls generate
`param <= 4294967295` precondition suggestions.

### Postcondition Inference

For functions with a tail expression that is a binary operation on parameters,
the synthesizer suggests `result == lhs op rhs` as a postcondition. The
symbolic constraint system is also inspected: constant outputs and
input-passthrough outputs are detected automatically.

### CEGIS (Counterexample-Guided Inductive Synthesis)

The synthesizer includes a CEGIS loop that proposes candidate invariants,
verifies them against the algebraic solver, and attempts to refine failures
by weakening candidates (e.g., widening `<= K` to `<= K+1` or lowering
`>= K` to `>= K-1`). The maximum number of refinement rounds is 5.

### Synthesis Output

```
Synthesized 4 specification(s):

  [medium] sum_loop loop invariant (over i): acc >= 0
    Accumulation pattern: acc is additively updated in loop over i

  [low] sum_loop postcondition (#[ensures]): acc == sum of additions over 0..10
    After the loop, acc holds the accumulated sum

  [high] identity postcondition (#[ensures]): result == x
    Function returns its parameter x unchanged (identity)

  [high] range_check precondition (#[requires]): val <= 4294967295
    as_u32(val) requires the value fits in U32 range
```

---

## Semantic Equivalence

The `trident equiv` command checks whether two functions in the same file
produce identical outputs for all inputs. This is useful for validating
optimizations: verify that a hand-optimized version is semantically equivalent
to the original.

### Equivalence Checking Pipeline

The checker runs three strategies in order, returning as soon as one is
conclusive:

1. **Content hash comparison.** Function bodies are hashed using a
   normalization that replaces variable names with de Bruijn indices.
   Functions that differ only in variable naming (alpha-equivalent) produce
   the same hash and are immediately declared equivalent.

2. **Polynomial normalization.** For pure field-arithmetic functions (using
   only `+`, `*`, `-`, constants, and variables), both functions are
   symbolically evaluated and their results are normalized into canonical
   multivariate polynomial form over the Goldilocks field. If the polynomials
   match, the functions are equivalent. This catches algebraic equivalences
   like commutativity (`x + y` vs `y + x`) and distribution
   (`(x + y) * x` vs `x * x + x * y`).

3. **Differential testing.** The checker builds a synthetic program that
   reads shared inputs, calls both functions, and asserts their outputs are
   equal. This synthetic program is then run through the full verification
   pipeline (symbolic execution + algebraic solver + BMC). If no violation
   is found, the functions are declared equivalent with high confidence. If
   a violation is found, the checker reports a counterexample with the
   differing inputs and outputs.

### CLI Usage

```bash
# Check equivalence of two functions in the same file
trident equiv program.tri fn_original fn_optimized

# With verbose symbolic analysis
trident equiv program.tri fn_original fn_optimized --verbose
```

### Exit Codes

- `0` -- functions are equivalent.
- `1` -- functions are not equivalent (counterexample found).
- `2` -- equivalence could not be determined (e.g., signature mismatch,
  function not found).

---

## LLM-Verified Generation

The `trident generate` command takes a spec file containing function signatures
with `#[requires]` and `#[ensures]` annotations but empty bodies, and generates
implementation scaffolds.

### Spec File Format

A spec file is a valid Trident source file where functions have annotations
but no implementation:

```
program transfer_spec

#[requires(amount > 0)]
#[requires(sender_balance > amount)]
#[ensures(new_sender == sub(sender_balance, amount))]
#[ensures(new_receiver == receiver_balance + amount)]
fn transfer(sender_balance: Field, receiver_balance: Field, amount: Field) -> (Field, Field) {
}

#[requires(amount > 0)]
#[ensures(result == balance + amount)]
fn deposit(balance: Field, amount: Field) -> Field {
}
```

### Scaffold Generation

The generator reads the spec file, preserves all annotations, and produces an
implementation scaffold with:

- `#[requires]` conditions turned into runtime assertions.
- `#[ensures]` conditions with `result ==` turned into computed return
  expressions.
- TODO comments for any logic that cannot be directly inferred from the spec.

### Verification Loop

The intended workflow is:

1. Write a spec file with `#[requires]` and `#[ensures]` annotations.
2. Run `trident generate spec.tri -o impl.tri` to produce a scaffold.
3. Fill in any remaining logic (or have an LLM complete it).
4. Run `trident verify impl.tri` to verify the implementation satisfies all
   assertions and annotations.
5. If verification fails, use the counterexample to fix the implementation
   and re-verify.

The `--json` flag on `trident verify` produces machine-readable output that
an LLM can parse to understand exactly which assertion failed and why,
enabling automated generate-verify loops.

### CLI Usage

```bash
# Generate scaffold to stdout
trident generate spec.tri

# Generate scaffold to a file
trident generate spec.tri -o implementation.tri
```

---

## Decidable Properties

Trident's verification is tractable because the language eliminates the
standard sources of undecidability:

| Source of difficulty | Trident's answer |
|---------------------|-----------------|
| Termination | No recursion. All loops have compile-time bounds. |
| Aliasing | No heap. No pointers. |
| Dynamic dispatch | First-order only. No closures, no vtables. |
| Unbounded state | Fixed memory layout at compile time. |
| Side effects | Only `pub_read`, `pub_write`, `divine` -- all pure data. |
| Integer undecidability | Arithmetic over Goldilocks finite field is decidable. |
| Concurrency | Single-threaded, sequential execution. |

### What Can Be Verified

**Algebraic identities.** Conservation laws (`sum(outputs) == sum(inputs)`),
commutativity, idempotence, and equivalence of two implementations. Checked
by polynomial identity testing -- typically proves in milliseconds.

**Range properties.** U32 range checks (`x <= 2^32 - 1`), boolean outputs
(`result in {0, 1}`), array index bounds.

**Assertion reachability.** Whether an `assert` can ever fail, whether code
is reachable.

**Witness existence.** For `divine()` values: whether a valid witness exists
for all valid public inputs. Decidable because the field is finite.

**Loop invariant checking.** Base case + inductive step over finite field
arithmetic. For small bounds, complete unrolling as a fallback.

### What May Time Out

**Multiple interacting `divine()` values.** Existential quantification over
many witness variables is decidable in theory but may be slow for more than
about 10 divine variables.

**Loops with large bounds (> 64 iterations).** Unrolling is impractical.
Invariant checking works if an invariant is provided. Without an invariant,
automatic synthesis is attempted but may not find one.

**Hash-dependent properties.** Hash functions are modeled as opaque
(uninterpreted). Properties requiring reasoning about hash internals cannot
be verified automatically.

---

## Known Limitations

The verification system is functional end-to-end, but several components
have incomplete coverage or known bugs:

**Symbolic execution: opaque stubs for field access and indexing.** In
`sym.rs`, struct field access (`expr.field`) and array indexing
(`expr[index]`) produce fresh opaque symbolic variables rather than tracking
the relationship to the source struct or array. This means the verifier
cannot reason about properties that depend on reading back a struct field
or array element that was previously written. Approximately 68% of the
symbolic executor handles real cases; field access and indexing are the
primary gaps.

**CEGIS is not complete.** The counterexample-guided invariant synthesis
loop (`synthesize.rs`) has the refinement infrastructure in place but the
source-construction step (`build_verification_source`) currently returns
`None` for functions with parameters, which is the common case. Template-
based synthesis works and produces useful suggestions; the CEGIS loop
itself does not yet contribute verified candidates for most functions.

**SMT `Inv` encoding has a bug.** In `smt.rs`, the encoding of
`SymValue::Inv` (multiplicative inverse) declares a fresh variable but
does not emit the constraint `inv_x * x == 1 mod p` into the SMT script.
The variable is declared but left unconstrained, which means the solver
can assign it any value. Programs that rely on field inversion for
verification correctness may produce unsound results through the SMT
backend. The algebraic solver (Schwartz-Zippel) handles inversion
correctly via Fermat's little theorem.

**Polynomial normalization limited to `+`, `*`, `-`.** The equivalence
checker's polynomial normalization only handles addition, multiplication,
subtraction, negation, constants, and variables. Functions using division,
hashes, conditionals, or I/O fall through to differential testing, which
provides high confidence but not a proof.

**No `#[invariant]` parsing for loops yet.** The `#[invariant]` annotation
is described in this document as part of the specification language design,
but the parser does not yet recognize it on loop statements. Loop invariant
checking currently requires manual unrolling or relies on the synthesizer.

---

## Links

- [Tutorial](tutorial.md) -- getting started with Trident programs
- [Language Reference](reference.md) -- complete syntax and type reference, including `#[pure]` and specification annotations
- [Language Specification](spec.md) -- formal grammar and semantics
- [Content-Addressed Code](content-addressed.md) -- how verification results are cached and shared via content hashing
- [Optimization Guide](optimization.md) -- cost reduction strategies (verification can identify redundant assertions)
- [Compiling a Program](compiling-a-program.md) -- build pipeline and `--costs` flag
- [For Developers](for-developers.md) -- zero-knowledge concepts for general developers
- [Vision](vision.md) -- project goals and design philosophy
