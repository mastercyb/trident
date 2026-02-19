# 4-Dimensional Verification & Benchmark Framework

## Context

Neural training optimizes cost without correctness checks — model learned to emit empty TASM scoring cost=1 (fake 95% reduction). But the problem is deeper: trident generates TASM text and never verifies it runs correctly. `trident bench` only counts instructions. No execution, no correctness, no proving time comparison.

The fix is a comprehensive benchmark framework that serves as ground truth for the entire compiler. Four dimensions of code generation (Rust reference, classic compiler, manual TASM, neural TASM) compared on four metrics (correctness, execution speed, proving time, verification time).

## Architecture

### The four dimensions

| Dimension | Source | What it is |
|-----------|--------|------------|
| **reference** | `benches/<path>/reference.rs` | Rust implementation — generates synthetic inputs, computes expected output. Ground truth. |
| **classic** | `trident build <path>.tri` | Standard compiler output. The default pipeline. |
| **manual** | `benches/<path>/*.baseline.tasm` | Hand-written TASM. Already exists. Expert-level floor. |
| **neural** | Neural optimizer output | ML-generated TASM. Must prove it's correct before counting. |

### The four metrics

| Metric | How | Tool |
|--------|-----|------|
| **Correctness** | Same output as Rust reference on same inputs | `trisha run` |
| **Execution speed** | Cycle count from Triton VM | `trisha run` (cycle_count) |
| **Proving time** | Wall-clock STARK proof generation | `trisha prove` (proving_time_ms) |
| **Verification time** | Wall-clock proof verification | `trisha verify` (wall-clock) |

Proving and verification scale differently: proving = O(n log n), verification = O(log² n). Code that's cheaper in execution may be more expensive in proving if tables are unbalanced. Verification time is what the end user (on-chain verifier) sees. All four metrics can diverge — measure all.

### Directory structure

```
benches/
  std/
    crypto/
      poseidon/
        reference.rs           # NEW: Rust impl + input generator
        poseidon.baseline.tasm  # EXISTS (rename from flat layout)
      auth/
        reference.rs
        auth.baseline.tasm
    nn/
      tensor/
        reference.rs
        tensor.baseline.tasm
  ...
```

Each bench directory contains:
- `reference.rs` — Rust code compiled as a standalone binary. CLI: `./reference generate N` outputs N test cases (input, expected_output) as JSON lines. `./reference verify <input> <output>` exits 0 if correct.
- `*.baseline.tasm` — hand-optimized TASM (already exists)
- Classic TASM — generated on-the-fly by `trident build`
- Neural TASM — generated on-the-fly by neural optimizer

### How `trident bench` evolves

Current: counts instructions, compares ratios.

New (additive, doesn't break existing):

```
trident bench                    # existing: instruction count comparison
trident bench --verify           # NEW: correctness check (all 4 dims vs reference)
trident bench --exec             # NEW: cycle count comparison via trisha run
trident bench --prove            # NEW: proving time comparison via trisha prove
trident bench --check            # NEW: verification time comparison via trisha verify
trident bench --full             # NEW: all of the above
```

#### `--verify` flow per benchmark:

1. Build Rust reference: `cargo build` the `reference.rs`
2. Generate test inputs: `./reference generate 10` → 10 test cases
3. For each test case, for each dimension (classic, manual, neural):
   a. Wrap TASM as ProgramBundle (inject test input as public_input)
   b. `trisha run <bundle>` → capture output + cycle count
   c. Compare output against reference expected_output
4. Report: pass/fail per dimension, cycle counts, ratios

#### Output format:

```
poseidon::hash2
  reference   ✓  (Rust, 0.001ms)
  classic     ✓  142 cyc  prove 1.2s  verify 45ms  1.38x baseline
  manual      ✓  103 cyc  prove 0.9s  verify 42ms  1.00x baseline
  neural      ✗  INCORRECT (output mismatch on input #3)

poseidon::sbox
  reference   ✓  (Rust, 0.001ms)
  classic     ✓  8 cyc    prove 0.1s  verify 12ms  1.33x baseline
  manual      ✓  6 cyc    prove 0.1s  verify 11ms  1.00x baseline
  neural      ✓  5 cyc    prove 0.1s  verify 10ms  0.83x baseline  ← genuine!
```

### Integration with trisha

trisha is a separate binary. Call via subprocess:
- `trisha run --tasm <file.tasm> --input-values 1,2,3` (needs new `--tasm` flag in trisha to accept raw TASM instead of .tri)
- Returns: output values on stdout, cycle count on stderr
- Alternatively: write a minimal wrapper that constructs ProgramBundle JSON and pipes it

**For training (fast path):** subprocess is too slow (100ms per call × thousands per epoch). Training keeps its own block-level stack verifier (~150 LOC inline in trident). This is NOT reimplementing triton-vm — it's a 25-instruction dispatch table for straight-line blocks only. trisha is the oracle for final verification; the inline verifier is the fast approximation for training feedback.

**For bench (slow path, correctness matters):** always use trisha. One subprocess call per test case. Acceptable: ~10 test cases × 4 dimensions × ~100ms = ~4 seconds per benchmark function.

### Block-level training verifier

Stays in `src/cost/stack_verifier.rs` (~150 LOC). Executes straight-line TASM blocks on concrete u64 values. Uses Goldilocks field arithmetic from `src/field/goldilocks.rs`. Supports the ~25 stack/arithmetic instructions that appear in blocks. Crypto/IO/memory ops modeled by stack effects only (push/pop correct number of elements, dummy values).

Training loop: generate test stack → run baseline TASM → run candidate TASM → compare stacks. Reject if mismatch.

## Files to create

| File | Purpose | ~LOC |
|------|---------|------|
| `src/cost/stack_verifier.rs` | Block-level TASM stack calculator for training | ~200 |
| `benches/std/crypto/poseidon/reference.rs` | First Rust reference impl (poseidon hash) | ~100 |

## Files to modify

| File | Change |
|------|--------|
| `src/cost/mod.rs` | Add `pub mod stack_verifier;` |
| `src/cli/train.rs` | Add `per_block_tasm` to CompiledFile, correctness check in scoring |
| `src/cli/build.rs` | Same correctness check in `score_neural_output` |
| `src/cli/bench.rs` | Add `--verify`, `--exec`, `--prove`, `--full` flags (scaffolding — full impl when trisha --tasm ready) |
| `reference/quality.md` | Document 4-dimensional verification as quality requirement |
| `CLAUDE.md` | Add verification framework section |

## Documentation updates

### `reference/quality.md` — add section:

```markdown
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
```

### `CLAUDE.md` — add section:

```markdown
## Verification Framework

Four ways to produce TASM: Rust reference, classic compiler, manual
baseline, neural optimizer. All must agree on correctness.
`trident bench --full` is the scoreboard.

- `benches/*/reference.rs` — Rust ground truth (generates inputs, expected outputs)
- `benches/*/*.baseline.tasm` — hand-optimized TASM (expert floor)
- Classic TASM — `trident build` output
- Neural TASM — neural optimizer output

Four metrics: correctness (`trisha run` vs reference), execution speed
(cycle count), proving time (`trisha prove`), verification time
(`trisha verify`). Block-level training uses inline stack verifier
(`src/cost/stack_verifier.rs`) for fast feedback.
```

## Implementation order

1. **Stack verifier** (`stack_verifier.rs`) + training integration — unblocks meaningful neural training immediately
2. **Documentation** (quality.md, CLAUDE.md) — codify the 4D framework
3. **First reference.rs** (poseidon) — prove the pattern works
4. **Bench --verify scaffolding** — wire up trisha subprocess calls
5. **trisha --tasm flag** (in trisha repo) — accept raw TASM files

Steps 1-2 now, steps 3-5 follow.

## Verification

1. `cargo check` — zero warnings
2. `cargo test` — all pass (including stack_verifier tests)
3. `trident train --epochs 3` — scores are realistic (model can't cheat with empty output)
4. Manual test: craft known-correct TASM → stack_verifier passes. Craft incorrect TASM → stack_verifier rejects.
5. `grep -r "reference.rs\|stack_verifier\|4.*dimension\|four.*dimension" reference/ CLAUDE.md` — documentation in place
