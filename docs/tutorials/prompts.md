# Trident Prompt Templates

Structured prompts for LLM-assisted Trident development. Use these templates
with `trident generate` or directly with AI assistants.

---

## Template 1: Contract from Description

```
Write a Trident program that implements: {DESCRIPTION}

Requirements:
- All inputs come from pub_read() or divine()
- All outputs go through pub_write()
- Use bounded for-loops (declare `bounded N` for runtime bounds)
- No subtraction operator: use sub(a, b)
- No division: use a * inv(b)
- Type annotations required on all let bindings
- Reference: docs/llm-reference.md

Target: Triton VM (Goldilocks field, p = 2^64 - 2^32 + 1)
```

## Template 2: Spec-Driven Implementation

```
Given this Trident specification:

{SPEC_FILE_CONTENT}

Generate a complete implementation where:
1. Every #[requires] precondition is assumed on entry
2. Every #[ensures] postcondition is verified with assert/assert_eq
3. The implementation satisfies all postconditions
4. Use only Trident builtins (sub, neg, inv for arithmetic)
5. All loops are bounded
6. No recursion

Run `trident verify` to check correctness after implementation.
```

## Template 3: Optimize for Cost

```
This Trident function has high proving cost. The dominant table is {TABLE_NAME}
with height {HEIGHT}.

Current implementation:
{CODE}

Cost report:
{COST_REPORT}

Optimize to reduce the dominant table height. Strategies:
- Replace hash operations with arithmetic where possible
- Replace u32 operations with field arithmetic where safe
- Reduce loop iterations
- Combine multiple hash calls into sponge operations
- Eliminate redundant assertions (marked by trident verify --json)
```

## Template 4: Fix Verification Failure

```
This Trident program failed verification:

Source:
{SOURCE_CODE}

Verification report (JSON):
{JSON_REPORT}

The counterexample shows:
{COUNTEREXAMPLE}

Fix the program so that:
1. All assertions pass for all valid inputs
2. The specification annotations are satisfied
3. The fix is minimal (don't restructure working code)
```

## Template 5: Migrate from Solidity

```
Convert this Solidity function to Trident:

{SOLIDITY_CODE}

Key differences:
- uint256 -> Field (mod p arithmetic) or U32 (range-checked 32-bit)
- No subtraction operator: use sub(a, b)
- No division: use a * inv(b)
- No dynamic arrays: use [T; N] with compile-time N
- No mappings: use RAM (ram_read/ram_write) with address conventions
- require() -> assert()
- msg.sender -> pub_read() (caller provides identity)
- Events: event + emit/seal syntax
- No reentrancy (single execution, no external calls)
```

## Template 6: Write Test Functions

```
Write #[test] functions for this Trident code:

{CODE}

Each test should:
- Be annotated with #[test]
- Use assert() or assert_eq() to verify behavior
- Test edge cases: 0, 1, max u32, field element boundaries
- Test the happy path and error conditions
- Keep tests focused (one behavior per test)

Example:
#[test]
fn test_add_zero() {
    assert_eq(add(42, 0), 42)
}
```

## Template 7: Neptune Transaction Validation

```
Write a Neptune-style UTXO validation program in Trident.

The program validates a {TRANSACTION_TYPE} transaction where:
{TRANSACTION_RULES}

Pattern:
1. Read public inputs (transaction data commitments)
2. Read divine inputs (witness data: preimages, proofs)
3. Verify Merkle authentication paths for UTXOs
4. Check balance conservation (inputs == outputs)
5. Verify authorization (signature or hash preimage)
6. Write public outputs (new commitments, nullifiers)
7. Seal private data (hash commitments for privacy)

Use: std.crypto.hash, std.crypto.merkle, std.crypto.auth
Events: emit for public data, seal for private commitments
```

---

## JSON Report Fields for LLM Consumption

When using `trident verify --json`, the output contains:

```json
{
  "version": 1,
  "verdict": "safe|unsafe|unknown",
  "summary": {
    "total_constraints": N,
    "active_constraints": N,
    "variables": N,
    "static_violations": N,
    "random_violations": N,
    "bmc_violations": N
  },
  "counterexamples": [
    {
      "constraint_index": N,
      "constraint_desc": "human-readable constraint",
      "source": "random|bmc|static",
      "assignments": [["var_name", value], ...]
    }
  ],
  "suggestions": [
    {
      "kind": "fix_violation|remove_redundant|add_assertion",
      "message": "human-readable suggestion"
    }
  ]
}
```

Use `verdict` to determine pass/fail. Use `counterexamples` to understand
failures. Use `suggestions` for automated fix generation.
