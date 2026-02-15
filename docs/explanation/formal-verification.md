# 🔬 Formal Verification

Trident includes a built-in formal verification pipeline that can prove
properties of programs for all possible inputs -- not by testing, but by
exhaustive symbolic analysis. Because Trident programs are bounded,
first-order, heap-free computations over a finite field, the verification
problem is decidable: the compiler can automatically determine whether an
assertion holds universally or produce a concrete counterexample.

The verification toolchain consists of:

- Specification annotations (`#[requires]`, `#[ensures]`, `#[invariant]`,
  `#[pure]`) that declare properties directly in source code.
- Symbolic execution that converts programs into constraint systems.
- Algebraic solving using Schwartz-Zippel random evaluation over the
  Goldilocks field and bounded model checking.
- SMT checking that encodes constraints as SMT-LIB2 bitvector queries
  for Z3.
- Invariant synthesis that automatically infers loop invariants and
  pre/postconditions from code patterns.
- Semantic equivalence checking that proves two functions produce
  identical outputs for all inputs.

All of these are available today through the `trident verify`, `trident equiv`,
and `trident generate` commands.

---

## 🔬 Specification Annotations

Trident supports four specification annotations on functions. These annotations
are checked at compile time and have zero runtime cost -- they do not appear in
the emitted TASM output.

### `#[requires(predicate)]`

Declares a precondition on function inputs. The function is only obligated to
behave correctly for inputs satisfying the predicate. Multiple `#[requires]`
annotations are conjunctive (all must hold).

```trident
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

```trident
#[requires(amount > 0)]
#[ensures(result == sub(balance, amount))]
fn withdraw(balance: Field, amount: Field) -> Field {
    sub(balance, amount)
}
```

For functions returning tuples, named bindings in the ensures clause refer to
the destructured output variables:

```trident
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

```trident
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

```trident
#[pure]
fn square(x: Field) -> Field {
    x * x
}
```

### Inline Assertions

Every `assert()` and `assert_eq()` in Trident source is also a specification.
The verifier attempts to prove each assertion holds for all executions that
reach it. No annotation is needed -- assertions are verified automatically.

```trident
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

## 🏗️ Verification Engine

The `trident verify` command runs the full verification pipeline. It processes
a Trident source file through three stages.

### Stage 1: Symbolic Execution

The symbolic executor walks the type-checked AST and
builds a `ConstraintSystem`. Each variable becomes a symbolic value. Each
operation becomes a constraint. The executor handles:

- `let` bindings as symbolic variable assignments (SSA-versioned for
  mutable variables).
- `assert` / `assert_eq` as equality or truth constraints.
- Control flow (`if/else`, `match`) via path conditions with ITE merging
  of environments from each branch.
- Bounded `for` loops by unrolling up to the declared bound (max 64
  iterations).
- Function calls by inlining (up to depth 64; no recursion means this
  always terminates).
- `divine()` / `pub_read()` / `pub_write()` as fresh symbolic variables
  or recorded symbolic outputs. Hash operations are opaque (uninterpreted).

The resulting `ConstraintSystem` contains all constraints, variable bindings,
public inputs/outputs, and divine inputs.

### Stage 2: Algebraic Solving

The algebraic solver checks the constraint system using two methods:

Schwartz-Zippel random evaluation. Constraints are evaluated at 100 random
points over the Goldilocks field. By the Schwartz-Zippel lemma, a false
polynomial identity is overwhelmingly unlikely to pass all rounds -- the
false-positive probability is negligible for the Goldilocks field size.

Bounded model checking. For systems with few free variables (8 or fewer),
the solver tests a grid of interesting field values exhaustively. For larger
systems, it uses stratified random sampling. When a constraint fails, the
solver reports a concrete counterexample with the variable assignments that
caused the violation.

The solver also detects redundant assertions (constraints that hold for all
tested inputs), which can be removed to reduce proving cost.

### Stage 3: SMT Checking

The SMT encoder translates the constraint system into
an SMT-LIB2 script using the `QF_BV` (quantifier-free bitvector) logic.
Goldilocks field arithmetic is encoded as 128-bit bitvector operations with
modular reduction. Two query modes are supported:

- Safety check: Asserts the negation of the conjunction of all constraints
  and checks satisfiability. SAT means a counterexample was found (a bug).
  UNSAT means all constraints hold for all inputs.
- Witness existence: Asserts all constraints and checks satisfiability.
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

A typical verification report:

```text
Verifying main.tri...
  Static analysis: PASS
  Schwartz-Zippel (100 rounds): ALL PASSED
  Bounded model checking (256 rounds): ALL PASSED
Verdict: SAFE -- no violations found
```

When a violation is found:

```text
Verdict: UNSAFE -- random testing found violations (high confidence)
  Constraint #2: assert((pub_in_0 == 0))
  Counterexample: pub_in_0 = 7164325918402846317
```

---

## 🔄 Invariant Synthesis

The invariant synthesis engine automatically infers specifications from code
patterns. Run it with `trident verify --synthesize`.

### Template-Based Pattern Matching

The synthesizer recognizes common patterns in loop bodies and function
structures:

Additive accumulation. `acc = acc + expr` in a loop yields
`acc >= init_value` and bound-related postconditions.

Counting patterns. Conditional `acc = acc + 1` yields `count <= loop_var`
invariant and `count <= N` postcondition.

Monotonic updates. Variables that only increase get `x >= init_value`.

Identity / constant detection. Functions returning their parameter
unchanged get `result == param`; single-literal bodies get `result == constant`.

Range preservation. U32-to-U32 functions get `result <= 4294967295`.

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

```text
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

## 🔬 Semantic Equivalence

The `trident equiv` command checks whether two functions in the same file
produce identical outputs for all inputs. This is useful for validating
optimizations: verify that a hand-optimized version is semantically equivalent
to the original.

### Equivalence Checking Pipeline

The checker runs three strategies in order, returning as soon as one is
conclusive:

1. Content hash comparison. Function bodies are hashed using a
   normalization that replaces variable names with de Bruijn indices.
   Functions that differ only in variable naming (alpha-equivalent) produce
   the same hash and are immediately declared equivalent.

2. Polynomial normalization. For pure field-arithmetic functions (using
   only `+`, `*`, `-`, constants, and variables), both functions are
   symbolically evaluated and their results are normalized into canonical
   multivariate polynomial form over the Goldilocks field. If the polynomials
   match, the functions are equivalent. This catches algebraic equivalences
   like commutativity (`x + y` vs `y + x`) and distribution
   (`(x + y) * x` vs `x * x + x * y`).

3. Differential testing. The checker builds a synthetic program that
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

## 🤖 LLM-Verified Generation

The `trident generate` command takes a spec file containing function signatures
with `#[requires]` and `#[ensures]` annotations but empty bodies, and generates
implementation scaffolds.

### Spec File Format

A spec file is a valid Trident source file where functions have annotations
but no implementation:

```trident
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

## 🛡️ Decidable Properties

Trident's verification is tractable because the language eliminates the
standard sources of undecidability. No recursion and bounded loops guarantee
termination. No heap, no pointers, no closures, and no dynamic dispatch mean
fixed memory layout and finite state. Arithmetic over the Goldilocks finite
field is decidable. Execution is single-threaded with only pure-data I/O
(`pub_read`, `pub_write`, `divine`). Together these restrictions make the
state space finite and enumerable.

### What Can Be Verified

Algebraic identities. Conservation laws (`sum(outputs) == sum(inputs)`),
commutativity, idempotence, and equivalence of two implementations. Checked
by polynomial identity testing -- typically proves in milliseconds.

Range properties. U32 range checks (`x <= 2^32 - 1`), boolean outputs
(`result in {0, 1}`), array index bounds.

Assertion reachability. Whether an `assert` can ever fail, whether code
is reachable.

Witness existence. For `divine()` values: whether a valid witness exists
for all valid public inputs. Decidable because the field is finite.

Loop invariant checking. Base case + inductive step over finite field
arithmetic. For small bounds, complete unrolling as a fallback.

### What May Time Out

Multiple interacting `divine()` values. Existential quantification over
many witness variables is decidable in theory but may be slow for more than
about 10 divine variables.

Loops with large bounds (> 64 iterations). Unrolling is impractical.
Invariant checking works if an invariant is provided. Without an invariant,
automatic synthesis is attempted but may not find one.

Hash-dependent properties. Hash functions are modeled as opaque
(uninterpreted). Properties requiring reasoning about hash internals cannot
be verified automatically.

---

## ⚠️ Known Limitations

The verification system is functional end-to-end, but several components
have incomplete coverage or known bugs:

Opaque stubs for field access and indexing. Struct field access and array
indexing produce fresh symbolic variables rather than tracking the source
struct/array. The verifier cannot yet reason about read-back properties.

CEGIS is not complete. Template-based synthesis works and produces useful
suggestions, but the CEGIS refinement loop does not yet contribute verified
candidates for functions with parameters.

SMT `Inv` encoding bug. The SMT encoding of multiplicative inverse
declares a variable but omits the `inv_x * x == 1 mod p` constraint, making
it unconstrained. The algebraic solver handles inversion correctly.

Polynomial normalization limited to `+`, `*`, `-`. Functions using
division, hashes, conditionals, or I/O fall through to differential testing
(high confidence, not a proof).

No `#[invariant]` parsing for loops yet. The annotation is part of the
specification design but the parser does not yet recognize it on loop
statements. Loop invariant checking relies on the synthesizer or unrolling.

---

## 🔗 See Also

- [Language Reference](../../reference/language.md) -- syntax, types, and specification annotations
- [Content-Addressed Code](content-addressing.md) -- how verification results are cached via content hashing
- [Tutorial](../tutorials/tutorial.md) -- getting started with Trident programs
- [Compiling a Program](../guides/compiling-a-program.md) -- build pipeline and `--costs` flag
