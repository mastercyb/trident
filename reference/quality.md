# Quality Control

Trident targets provable compilation: the compiler will self-host on
Triton VM and produce a STARK proof that compilation was correct. Every
line of code may end up inside a proof circuit. Quality here means
soundness — a bug isn't just a bug, it's a potential soundness hole.

## Forbidden Patterns

- No `HashMap` in deterministic paths — use `BTreeMap` or indexed vec
- No `println!` in library code — use the diagnostic system
- No `std::process::exit` outside `main.rs`
- No `.unwrap()` outside tests
- No floating point anywhere
- No `async` in the compilation pipeline (only in LSP and CLI)

## File Size Limit

No single `.rs` file should exceed 500 lines. If it does, split it
into submodules. `lib.rs` is the only exception (re-exports).

When auditing files > 500 lines, split the audit into sections:
read the file in chunks (offset/limit), report per-section.

## Review Passes

Invoke passes by number: "Run PASS 3 and PASS 7 on this module."
On full audit — run all passes in parallel using agents, persist
results to `.cortex/`, prepare a fix plan before applying.

### PASS 1: DETERMINISM
- No floating point — all arithmetic over Goldilocks p = 2^64 - 2^32 + 1
- No HashMap iteration (non-deterministic order)
- No system clock, no randomness without explicit seed
- Serialization is canonical — single valid encoding per value
- Cross-platform: same input → same state root, always

*"Find any source of non-determinism in this code."*

### PASS 2: BOUNDED LOCALITY
- Every function's read-set is O(k)-bounded — trace it
- No hidden global state (singletons, lazy_static with mutation)
- Graph walks have explicit depth/hop limits
- State updates touch only declared write-set
- Local change cannot trigger unbounded cascade

*"What is the maximum read-set and write-set? Can a local change cascade globally?"*

### PASS 3: FIELD ARITHMETIC CORRECTNESS
- All reductions correct mod p — no overflow before reduce
- Multiplication uses widening (u64 → u128 → reduce)
- Inverse/division handles zero explicitly (panic or Option)
- Batch operations: individual vs batch results match
- Edge values correct: 0, 1, p-1, p

*"Check edge cases: 0, 1, p-1, and values near 2^64. Does reduction overflow?"*

### PASS 4: CRYPTO HYGIENE
- No secret-dependent branching (constant-time)
- No secret data in error messages, logs, or Debug impls
- Zeroize sensitive memory on drop
- Hash domain separation — unique prefix/tag per use
- Proof constraints: neither under-constrained nor over-constrained

*"Is there any path where secret material leaks through timing, errors, or logs?"*

### PASS 5: TYPE SAFETY & INVARIANTS
- Newtypes for distinct domains (ParticleId != NeuronId)
- States encoded in types (`Unverified<Proof>` vs `Verified<Proof>`)
- `unsafe` blocks have safety comments
- No `.unwrap()` on fallible paths
- Invalid state construction prevented by type system

*"Can a caller construct an invalid state? Can types from different domains mix?"*

### PASS 6: ERROR HANDLING & DEGRADATION
- Every error type is meaningful — no `anyhow` in library code
- Errors propagate with context (error chains)
- No panic in library code
- Resource cleanup on all error paths (RAII, Drop)
- Partial failure doesn't corrupt shared state

*"What happens when this fails halfway through? Is state still consistent?"*

### PASS 7: ADVERSARIAL INPUT
- All external inputs validated before processing
- Sizes, lengths, indices bounds-checked
- No allocation proportional to untrusted input without cap
- Malformed proofs/signatures rejected before expensive computation

*"What's the cheapest input an attacker can craft for maximum damage?"*

### PASS 8: ARCHITECTURE & COMPOSABILITY
- Single responsibility per module
- Dependencies point inward (domain ← application ← infra)
- Traits define boundaries — implementations swappable
- No circular dependencies
- Public API is minimal

*"Can I replace this implementation without touching callers?"*

### PASS 9: READABILITY & NAMING
- Names match the whitepaper terminology
- Functions do what their name says — no hidden side effects
- Comments explain *why*, not *what*
- Magic numbers are named constants with units
- Code reads top-down

*"Can someone reading only this file understand what it does and why?"*

### PASS 10: COMPACTNESS & ELIMINATION
- No dead code, no commented-out blocks
- No premature abstraction — one impl doesn't need a trait
- No duplicate logic
- No unnecessary allocations (clone, to_vec, collect where iter suffices)
- "What can I delete?" before "what should I add?"

*"What can be removed without changing behavior?"*

### PASS 11: PERFORMANCE & SCALABILITY
- Hot path is allocation-free
- No O(n^2) without justification and n-bound
- Batch operations for anything called in loops
- Cache-friendly access patterns
- Profiled, not guessed

*"What is the complexity at 10^9 nodes? Where does it break first?"*

### PASS 12: TESTABILITY
- Pure functions where possible
- Side effects injected (trait objects, closures)
- Property-based tests for invariants
- Edge case tests: empty, one, max, overflow, malicious
- Test names describe the property, not the method

*"What property should always hold? Write a proptest for it."*

## Severity Tiers

| Tier | Passes | When |
|------|--------|------|
| Every commit | 1, 5, 6, 9 | Determinism, types, errors, readability |
| Every PR | + 2, 7, 8, 10 | Locality, adversarial, architecture, compactness |
| Every release | + 3, 4, 11, 12 | Crypto, field math, performance, full test coverage |

## Audit Protocol

1. Launch parallel agents partitioned by module scope (no overlapping files).
2. Each agent runs assigned passes, writes findings to `.cortex/`.
3. Main session reads `.cortex/`, summarizes, and prepares a fix plan.
4. User confirms the fix plan before any changes are applied.
5. Fixes applied as atomic commits. `.cortex/` cleaned of stale entries.

## Four-Dimensional Verification

Every function that compiles to TASM is verified across four dimensions:

| Dimension | Source | Role |
|-----------|--------|------|
| Reference | `benches/*/reference.rs` (Rust) | Ground truth: generates inputs, computes expected outputs |
| Classic | `trident build` | Default compiler pipeline |
| Manual | `benches/*/*.baseline.tasm` | Hand-optimized expert TASM |
| Neural | Neural optimizer | ML-optimized TASM |

Four metrics compared across all dimensions:

1. **Correctness** — output must match Rust reference on all test inputs
2. **Execution speed** — Triton VM cycle count (via `trisha run`)
3. **Proving time** — STARK proof generation wall-clock (via `trisha prove`)
4. **Verification time** — STARK proof verification wall-clock (via `trisha verify`)

Slow code is a bug. Incorrect code is a soundness hole.
`trident bench --full` is the scoreboard.
