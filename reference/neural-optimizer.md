# Neural TIR-TASM Optimizer

Implementation plan for self-optimizing compilation. A 91K-parameter
neural model at the TIR-TASM boundary, verified by STARK equivalence,
trained by evolutionary search in Goldilocks field arithmetic.

Source document: "Self-Optimizing Compilation for Algebraic Virtual
Machines" (internal design brief).

---

## Why This Works

Three properties make this feasible in trident today:

1. **The cost function is weird.** Triton VM pads all 6 tables to the
   same power-of-2. Cost = `2^ceil(log2(max_table_height))`. Cliff
   discontinuities mean small instruction changes yield 2x speedups.
   No classical heuristic targets this.

2. **TIR-TASM is the only non-deterministic boundary.** Parsing, type
   checking, normalization have unique correct outputs. Only lowering
   TIR to TASM involves genuine optimization choices (stack scheduling,
   table balancing, instruction selection, loop restructuring).

3. **Everything is field arithmetic.** The model (matmul, attention,
   activation) uses only Tier 0+1 ops (add, mul, invert). Compiles to
   every trident target. Runs on GPU via KIR. Proves via STARK.

---

## What Already Exists

Infrastructure audit of the trident codebase:

| Component | Location | Status |
|-----------|----------|--------|
| TIR definition (54 ops, 4 tiers) | `src/ir/tir/mod.rs` | Production |
| TIR-TASM lowering | `src/ir/tir/lower/triton.rs` | Production (TritonLowering) |
| Peephole optimizer (9 passes) | `src/ir/tir/optimize/mod.rs` | Production |
| Stack manager with LRU spilling | `src/ir/tir/stack/mod.rs` | Production |
| 6-table cost model | `src/cost/model/triton.rs` | Production |
| Static cost analyzer | `src/cost/analyzer.rs` | Production |
| Goldilocks field arithmetic | `src/field/goldilocks.rs` | Production |
| Poseidon2 sponge (generic) | `src/field/poseidon2.rs` | Production |
| Padded height + proof estimation | `src/field/proof.rs` | Production |
| Symbolic execution engine | `src/verify/sym/executor.rs` | Production |
| Equivalence checking | `src/verify/equiv/mod.rs` | Production |
| Differential testing | `src/verify/equiv/differential.rs` | Production |
| Polynomial equivalence | `src/verify/equiv/polynomial.rs` | Production |
| Neural net primitives (.tri) | `std/nn/tensor.tri` | Minimal (dot, matmul, relu, dense) |
| Hand-optimized baselines | `benches/std/crypto/*.baseline.tasm` | 13 files |

**What is missing:** Fixed-point arithmetic. Backpropagation. Evolutionary
search. TIR block encoding. Dynamic profiling (actual table heights vs
static estimates). Attention mechanism. The speculative compilation
architecture.

---

## Architecture Decision: Pure Rust

The document proposes implementing the model as Trident programs
(self-referential: the compiler optimizes its own optimizer). The
self-referential loop is the endgame (Phase 6), but the foundation
must be Rust for three reasons:

1. **Training requires mutability.** Weight updates, population
   management, fitness tracking need mutable state that Trident's
   functional model handles poorly.

2. **Integration with the compiler.** The optimizer inserts between
   TIR building and TASM lowering inside `src/ir/tir/lower/`. This
   is a Rust pipeline.

3. **GPU acceleration.** Training and inference on GPU via wgpu/Metal
   (trisha already has this infrastructure) is more practical than
   compiling the model to WGSL via KIR.

The Trident (.tri) implementation becomes relevant in Phase 6 when
the optimizer compiles itself. Until then, Rust is the vehicle.

---

## Implementation Plan

### N0: Fixed-Point Goldilocks Arithmetic

**What:** Scale factor `S = 2^16 = 65536`. Encode real values as field
elements. Multiply with rescale: `(a * b) * inv(S)`.

**Where:** `src/field/fixed.rs` (new file, ~120 LOC)

**Interface:**
```rust
/// Fixed-point value in Goldilocks field (scale factor 2^16).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fixed(pub Goldilocks);

impl Fixed {
    pub const SCALE: u64 = 65536;  // 2^16
    pub const ZERO: Self;
    pub const ONE: Self;           // = Fixed(Goldilocks(65536))

    pub fn from_f64(v: f64) -> Self;
    pub fn to_f64(self) -> f64;
    pub fn add(self, rhs: Self) -> Self;   // field add (no rescale)
    pub fn sub(self, rhs: Self) -> Self;   // field sub (no rescale)
    pub fn mul(self, rhs: Self) -> Self;   // field mul + inv(S) rescale
    pub fn neg(self) -> Self;              // field neg
    pub fn inv(self) -> Self;              // field inv + S rescale
    pub fn relu(self) -> Self;             // if < p/2 then self else ZERO
}
```

**Why 2^16:** 16-bit fractional precision. Quantization literature shows
models under 1M params tolerate 8-bit weights with <1% accuracy loss.
16 bits gives ample headroom. The Goldilocks modulus (2^64 - 2^32 + 1)
has ~48 bits of headroom above the fixed-point range, preventing overflow
in multiply-accumulate chains of up to ~65K terms.

**Key insight:** `inv(S)` is a constant. Precompute once:
`INV_SCALE = Goldilocks(65536).inv()`. Every fixed-point multiply is
two field multiplies (one for the product, one for the rescale).

**Verification:** Field law tests (commutativity, associativity,
distributivity). Round-trip tests: `from_f64(x).to_f64() ≈ x` within
precision bounds. Overflow tests at accumulation limits.

---

### N1: TIR Block Encoding

**What:** Encode a TIR basic block as a fixed-size tensor for neural
input. Each node = 4 field elements. Max 32 nodes. Plus 16-element
stack context. Total: 144 field elements.

**Where:** `src/ir/tir/encode.rs` (new file, ~200 LOC)

**Encoding per node:**
```
word 0: opcode (6 bits) | type (3 bits) | input0_ref (5 bits) | input1_ref (5 bits)
word 1: live_start (liveness interval begin)
word 2: live_end (liveness interval end)
word 3: reserved (loop bound / immediate value / 0)
```

**Opcode mapping:** TIROp's 54 variants -> 0..53 (6 bits). Type tag from
the TIR builder's type tracking. Input references are indices into the
block's node array (0..31). Liveness from the stack manager's LRU
timestamps.

**Block extraction:** Walk `Vec<TIROp>` and split at control flow
boundaries (Call, Return, IfElse, Loop). Each straight-line segment
becomes a basic block. Blocks > 32 nodes are split.

**Stack context:** At block entry, encode `st0..st15` occupancy from the
stack manager state: (type_tag, liveness) per slot = 16 elements.

**Interface:**
```rust
pub struct TIRBlock {
    pub nodes: [u64; 128],   // 32 nodes * 4 words, zero-padded
    pub context: [u64; 16],  // stack state at entry
}

pub fn encode_blocks(ops: &[TIROp]) -> Vec<TIRBlock>;
```

---

### N2: TASM Scoring (Dynamic Table Heights)

**What:** Execute a TASM sequence and measure actual table heights across
all 6 tracked tables. Return the cliff-aware cost.

**Where:** `src/cost/scorer.rs` (new file, ~150 LOC)

**Current state:** `src/cost/model/triton.rs` provides *static* per-op
costs (e.g., `Hash = [1, 6, 0, 1, 0, 0]`). `src/cost/analyzer.rs` sums
these over the AST. This is an approximation — actual heights depend on
runtime behavior (loop iteration counts, branch choices).

**What we add:** A lightweight TASM interpreter that counts actual table
row increments per instruction. Not a full VM — no memory model, no
hash computation. Just a counter per table.

```rust
pub struct TableProfile {
    pub heights: [u64; 6],  // proc, hash, u32, opstack, ram, jumpstack
}

impl TableProfile {
    pub fn max_height(&self) -> u64;
    pub fn padded_height(&self) -> u64;  // 2^ceil(log2(max))
    pub fn cost(&self) -> u64;           // = padded_height
}

/// Count table rows for a TASM instruction sequence.
pub fn profile_tasm(instructions: &[&str]) -> TableProfile;
```

**The cliff function:** `cost = 2^ceil(log2(max(heights)))`. This is
what the neural model optimizes. A sequence at max height 1025 costs
2048. At 1024 it costs 1024. The 2x cliff.

---

### N3: The 91K-Parameter Model

**What:** Encoder-decoder neural network operating on TIR blocks,
producing TASM instruction sequences. All arithmetic in Fixed-point
Goldilocks.

**Where:** `src/ir/tir/neural/mod.rs` (new module, ~500 LOC total)

**Subfiles:**
- `model.rs` — Network struct, forward pass (~250 LOC)
- `attention.rs` — DAG-aware self-attention (~100 LOC)
- `weights.rs` — Weight initialization, serialization (~80 LOC)
- `decode.rs` — TASM instruction decoding from output logits (~70 LOC)

**Architecture (from document section 4.3):**

```
ENCODER: 2 layers, 2 heads, dim 64
  Per layer:
    Q, K, V projections: 3 * 64 * 64 = 12,288 params
    Output projection:   64 * 64      =  4,096 params
    FFN: 64 -> 128 -> 64              = 16,512 params
    LayerNorm: 2 * 64                 =    128 params
    Layer total:                        33,024 params
  2 layers:                             66,048 params

DECODER: Autoregressive MLP
  Input: 64 (latent) + 64 (prev instruction) = 128
  Hidden: 128 -> 128                  = 16,512 params
  Output: 128 -> 64 (instruction)     =  8,256 params
  Decoder total:                        24,768 params

TOTAL:                                  ~91,000 params
                                        ~728 KB (91K * 8 bytes)
```

**Why DAG-aware attention:** Standard self-attention attends to all
positions. DAG-aware attention restricts: node i attends to node j only
if j is a data dependency of i or shares a liveness interval. The mask
is derived from the TIR block's data-flow graph. This halves the
effective attention cost and forces the model to learn structural
relationships rather than positional heuristics.

**Activation function:** Polynomial GeLU approximation via Pade
approximant. All field operations — no transcendentals needed.

```rust
/// GeLU approximation: x * sigmoid(1.702 * x)
/// sigmoid(x) ≈ 1/2 + x/4 - x^3/48 (third-order Taylor)
fn gelu_approx(x: Fixed) -> Fixed;
```

**Output decoding:** Each output element is a field element encoding
(7-bit opcode + 4-bit argument). Decode via `opcode = val >> 4`,
`arg = val & 0xF`. Map to TASM instruction strings.

---

### N4: Evolutionary Training

**What:** Population-based optimization. 16 individuals, each a complete
weight vector. Tournament selection, uniform crossover, per-weight
mutation. No gradients.

**Where:** `src/ir/tir/neural/evolve.rs` (~200 LOC)

**Why evolutionary over gradient:**
- No fixed-point gradient approximation error accumulation
- Pure field arithmetic (crossover = conditional copy, mutation = random
  field element)
- Better at cliff-jumping (discrete optimization)
- Simpler implementation (no backprop graph)
- The document recommends hybrid, but evolution alone is the compact
  path

**Algorithm:**
```rust
pub struct Population {
    individuals: Vec<WeightVec>,  // 16 weight vectors
    fitness: Vec<i64>,            // negative padded height (higher = better)
}

impl Population {
    pub fn new_random(rng: &mut impl Rng) -> Self;
    pub fn evaluate(&mut self, blocks: &[TIRBlock], verifier: &Verifier);
    pub fn evolve(&mut self, rng: &mut impl Rng);
}
```

**Evaluation:** For each individual, run inference on a batch of TIR
blocks. Score each output TASM via `profile_tasm()`. Only count verified
outputs (semantic equivalence). Fitness = negative sum of padded heights
across the batch.

**Selection:** Sort by fitness. Top 4 survive. 12 new children via
uniform crossover of random survivor pairs + 1% per-weight mutation.

**Memory:** 16 * 91K * 8 bytes = 11.6 MB. Fits in L3 cache.

**Cost per generation:** 16 * inference(~50K ops) + 16 * scoring(~100K
ops) + sort/crossover/mutation(~100K ops) = ~2.5M field ops. At 50 GOPS
on Metal: ~50 us per generation.

---

### N5: Speculative Compilation Architecture

**What:** The neural path is strictly speculative. Classical lowering
always runs. Neural output accepted only if verified equivalent AND
lower cost.

**Where:** Modify `src/ir/tir/lower/triton.rs` (~50 LOC change)

**Current flow:**
```
TIR -> peephole optimize -> TritonLowering::lower() -> TASM
```

**New flow:**
```
TIR -> peephole optimize -> TritonLowering::lower() -> baseline TASM
                         -> NeuralOptimizer::lower() -> candidate TASM
                         -> verify(TIR, candidate)
                         -> if verified && cost(candidate) < cost(baseline):
                              use candidate
                            else:
                              use baseline
```

**Interface:**
```rust
pub struct SpeculativeLowering {
    classical: TritonLowering,
    neural: Option<NeuralOptimizer>,
}

impl StackLowering for SpeculativeLowering {
    fn lower(&self, ops: &[TIROp]) -> Vec<String> {
        let baseline = self.classical.lower(ops);
        if let Some(neural) = &self.neural {
            if let Some(candidate) = neural.try_lower(ops) {
                if verify_equivalent(ops, &candidate)
                    && cost(&candidate) < cost(&baseline)
                {
                    return candidate;
                }
            }
        }
        baseline
    }
}
```

**Invariants (from document section 7.1):**
1. Output is ALWAYS semantically correct.
2. Output is ALWAYS <= classical baseline cost.
3. Monotonic improvement — neural path never makes things worse.

**Verification leverages existing infrastructure:**
- `src/verify/equiv/` — semantic equivalence (hash, polynomial,
  differential testing)
- `src/verify/sym/` — symbolic execution for straight-line blocks
- `src/verify/solve/` — bounded model checking

---

### N6: Training Integration

**What:** Wire evolutionary training into the compilation workflow.
Background training during `trident build`. Weight persistence.

**Where:** `src/ir/tir/neural/trainer.rs` (~150 LOC)

**Workflow:**
1. During normal compilation, collect (TIR block, baseline TASM, score)
   tuples into a training buffer.
2. After compilation completes, run N generations of evolutionary search
   (non-blocking, background thread).
3. If a new weight vector improves average score on the training buffer,
   promote it to the active weights.
4. Persist weights to `~/.trident/neural/weights.bin` (728 KB).

**Weight versioning:** Content-addressed. Hash the weight vector with
Poseidon2. The hash identifies the optimizer version.

```rust
pub struct TrainingSession {
    population: Population,
    training_buffer: Vec<(TIRBlock, Vec<String>, u64)>,
    active_weights: WeightVec,
    active_hash: [u8; 32],
}
```

---

## Execution Order

| Phase | What | LOC | Depends On | Deliverable |
|-------|------|-----|------------|-------------|
| N0 | Fixed-point arithmetic | ~120 | Nothing | `src/field/fixed.rs` |
| N1 | TIR block encoding | ~200 | N0 | `src/ir/tir/encode.rs` |
| N2 | TASM scoring | ~150 | Nothing | `src/cost/scorer.rs` |
| N3 | 91K model | ~500 | N0, N1 | `src/ir/tir/neural/` |
| N4 | Evolutionary training | ~200 | N0, N2, N3 | `src/ir/tir/neural/evolve.rs` |
| N5 | Speculative lowering | ~50 | N2, N3 | `src/ir/tir/lower/triton.rs` |
| N6 | Training integration | ~150 | N4, N5 | `src/ir/tir/neural/trainer.rs` |

**Total new code:** ~1,370 LOC Rust.

N0 and N2 are independent — can be built in parallel.
N1 and N2 are independent — can be built in parallel.
N3 depends on N0 + N1.
N4 depends on N0 + N2 + N3.
N5 depends on N2 + N3.
N6 depends on N4 + N5.

---

## Files Modified

**New files:**
- `src/field/fixed.rs` — Fixed-point Goldilocks arithmetic
- `src/ir/tir/encode.rs` — TIR block encoding for neural input
- `src/cost/scorer.rs` — Dynamic TASM table profiling
- `src/ir/tir/neural/mod.rs` — Module root
- `src/ir/tir/neural/model.rs` — Network forward pass
- `src/ir/tir/neural/attention.rs` — DAG-aware self-attention
- `src/ir/tir/neural/weights.rs` — Weight init + serialization
- `src/ir/tir/neural/decode.rs` — Output to TASM decoding
- `src/ir/tir/neural/evolve.rs` — Evolutionary search
- `src/ir/tir/neural/trainer.rs` — Training integration

**Modified files:**
- `src/field/mod.rs` — Add `pub mod fixed;`
- `src/cost/mod.rs` — Add `pub mod scorer;`
- `src/ir/tir/mod.rs` — Add `pub mod encode; pub mod neural;`
- `src/ir/tir/lower/triton.rs` — Speculative lowering wrapper
- `src/ir/tir/lower/mod.rs` — Wire SpeculativeLowering into factory

---

## Key Technical Decisions

### Why 6 tables, not 9?

The document references 9 Triton VM AETs. Trident's cost model tracks 6
(processor, hash, u32, op_stack, ram, jump_stack). The other 3 (Cascade,
Lookup, Degree Lowering) are internal to the STARK prover and grow
proportionally to the tracked tables. Optimizing the 6 visible tables
implicitly optimizes all 9.

### Why evolutionary, not gradient?

The compact path. Backpropagation in fixed-point Goldilocks requires:
- Automatic differentiation graph (~300 LOC)
- Gradient accumulation with overflow management
- Learning rate scheduling
- Approximation error tracking

Evolutionary search requires:
- Random number generation (already have `divine` / Rust rng)
- Array copy (crossover)
- Comparison + sort (selection)

The document recommends hybrid (gradient cold start + evolutionary
refinement). The pure evolutionary path ships in ~200 LOC. Gradient
training can be added later if evolution converges too slowly.

### Why speculative, not replacement?

The classical lowering (`TritonLowering`) is proven correct by
construction — each TIR op maps to a known-correct TASM pattern. The
neural model is trained, not proven. The speculative architecture means:
- No regression is possible (classical fallback always available)
- The neural path is pure upside
- Verification cost (~10K field ops per block) is negligible vs proving
  cost (seconds)

### Block size limit: 32 nodes

Most TIR basic blocks in real programs are 5-20 nodes. The 32-node limit
covers 95%+ of blocks without padding waste. Larger blocks are split at
natural boundaries (function calls, control flow). The 128-element input
tensor fits in a single GPU workgroup's shared memory.

---

## The Self-Referential Endgame

Not in scope for initial implementation, but the architecture enables it:

1. Implement the model as a Trident program (`std/nn/optimizer.tri`)
   using the existing `std/nn/tensor.tri` primitives extended with
   fixed-point support.

2. Compile the optimizer with itself. The optimizer's TASM is now
   optimized by the optimizer.

3. Iterate until convergence: recompile, measure score, repeat until
   `|score(k+1) - score(k)| < epsilon`.

4. At the fixed point, the compiler hash is self-consistent: compiling
   the compiler with the compiler produces the same binary.

This is the content-addressed self-verification from the document's
section 8 — the compiler that proves its own optimization.

---

## Verification Plan

After each phase:
- `cargo check` — zero warnings
- `cargo test` — all tests pass
- No existing benchmark regressions (`trident bench`)

Phase-specific verification:

| Phase | Verification |
|-------|-------------|
| N0 | Field law property tests. Round-trip f64 precision tests. Overflow at 65K accumulation. |
| N1 | Encode-decode round-trip. Known TIR blocks produce expected encodings. |
| N2 | Profile known TASM sequences against hand-computed table heights. Cliff boundary tests. |
| N3 | Forward pass determinism. Output dimensions correct. Known-weight inference matches expected output. |
| N4 | Population fitness improves over 100 generations on a toy problem. |
| N5 | Speculative lowering never produces worse output than classical. Semantic equivalence holds for all accepted candidates. |
| N6 | Weights persist and reload correctly. Training session improves scores on held-out blocks. |

---

## Estimated Effort

| Phase | Pomodoros | Notes |
|-------|-----------|-------|
| N0 | 2 | Straightforward field wrapper |
| N1 | 3 | TIR walking + encoding logic |
| N2 | 2 | Instruction-level table counting |
| N3 | 6 | Core model, most complex phase |
| N4 | 3 | Selection + crossover + mutation |
| N5 | 2 | Integration point, relies on existing equiv |
| N6 | 3 | Persistence, background threading |
| **Total** | **~21 pomodoros (~3.5 sessions)** | |
