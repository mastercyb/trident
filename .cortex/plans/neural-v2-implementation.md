# Neural Compiler v2: GNN+Transformer with GFlowNets

## Context

The v1 neural optimizer (10K-param MLP, evolutionary training, 64-token VOCAB, max 16 output tokens) has hit its ceiling. The architecture can't learn — too small, no graph structure awareness, and evolution without gradients can't navigate a 10K-dimensional landscape effectively.

v2 replaces the entire neural model with a ~10-15M param GNN encoder + Transformer decoder trained via supervised learning + GFlowNets, per the design doc at `docs/explanation/neural-tir-tasm-compiler-v2.md`.

**Not a gradual migration.** v1 neural model code gets replaced directly. The existing GPU infrastructure (`src/gpu/mod.rs`, device init) and verification pipeline (stack verifier + trisha proven tier) remain intact.

## Architecture Overview

```
TIR ops → TirGraph (nodes + typed edges)
    → GNN Encoder (GATv2, 3-4 layers, ~3M params)
        CPU for single-graph inference, GPU for batched training
    → Node embeddings (N × 256) + global context (256)
    → Stack-Aware Transformer Decoder (6 layers, 8 heads, d=256, ~10M params)
    → Grammar Mask (WGSL compute shader, GPU-resident, no CPU sync)
    → Beam Search (K=32, GPU-resident)
    → [first CPU↔GPU sync]
    → Parallel Triton VM simulation (K candidates, CPU/rayon)
    → Rank by clock_cycles → best valid (or fallback to compiler)
```

Training: Stage 1 (supervised CE) → Stage 2 (GFlowNet TB loss) → Stage 3 (online replay)

---

## Phase 0: Dependencies + wgpu Resolution

**Complexity: S | Blocks everything**

Add to `Cargo.toml`:
```toml
burn = { version = "0.20", features = ["wgpu", "autodiff"] }
# NOTE: do NOT add "dataset" or "sqlite" features — they pull in rusqlite (C FFI).
# Our data pipeline uses rkyv directly. Pure Rust only.
rayon = "1.10"
petgraph = "0.7"
serde = { version = "1", features = ["derive"] }
rkyv = { version = "0.8", features = ["validation"] }
statrs = "0.18"
```

Add to `[dev-dependencies]`:
```toml
criterion = { version = "0.5", features = ["async_tokio"] }
```

**wgpu conflict resolution:** burn 0.20 uses wgpu internally. The existing `wgpu = "24"` may conflict.
1. Try `cargo check` — if it resolves, done.
2. If conflict: remove top-level `wgpu = "24"`, re-export from burn-wgpu or pin compatible version. Update `src/gpu/mod.rs` imports accordingly.
3. Ensure existing `src/gpu/mod.rs` device init still works (it's used by grammar mask shader and potentially future v2 shaders).

**Note:** `triton-vm` is NOT a direct dependency — we use trisha subprocess. Omitted intentionally.

**Why rkyv over bincode:** bincode is abandoned (maintainer doxxed, GitHub repo archived, git history wiped). rkyv 0.8 is actively maintained, pure Rust, zero-copy deserialization (replay buffer scans without deserialize cost), 70M+ downloads. The `validation` feature lets us safely mmap archived data.

**No C dependencies rule:** burn `["wgpu", "autodiff"]` is pure Rust. Do NOT enable `dataset` or `sqlite` features — those pull in `rusqlite` which vendors C sqlite3. Our data pipeline (seed pairs, replay buffer, checkpoints) uses rkyv + serde directly. burn's native record format for model weights is also pure Rust.

**File:** `Cargo.toml`

---

## Phase 1: Data Layer — TirGraph

**Complexity: M | Depends on: Phase 0 | Parallel with: Phase 2**

### New files

**`src/neural/mod.rs`:**
```rust
pub mod data;
pub mod model;
pub mod training;
pub mod inference;
```

**`src/neural/data/mod.rs`**
**`src/neural/data/tir_graph.rs`:**

```rust
pub struct TirGraph {
    pub nodes: Vec<TirNode>,
    pub edges: Vec<(usize, usize, EdgeKind)>,
}

pub struct TirNode {
    pub op: OpKind,
    pub field_type: FieldType,
    pub immediate: Option<u64>,
}

pub enum EdgeKind { DataDep, ControlFlow, MemOrder }
pub enum FieldType { BFE, XFE, Unknown }

pub enum OpKind { /* 54 variants matching TIROp */ }

impl TirGraph {
    pub fn from_tir_ops(ops: &[TIROp]) -> Self;
}
```

**Edge extraction algorithm:**
- **DataDep:** Simulate abstract stack. When op B pops a value produced by op A → edge `(A→B, DataDep)`.
- **ControlFlow:** Sequential `(i→i+1, ControlFlow)`. Branches: edges to both arms.
- **MemOrder:** All memory ops (`ReadMem`, `WriteMem`, `RamRead`, `RamWrite`) get pairwise `MemOrder` edges (conservative ordering).

**Node feature vector** (§2.3): `[op_onehot(54) ‖ field_type_onehot(3) ‖ has_immediate(1) ‖ immediate_normalized(1)]` = 59 dims.

**Q5 resolution (XFE immediates):** Only encode BFE immediates. XFE ops (`ExtMul`, `ExtInvert`) have `has_immediate=0`. The 3-element extension field encoding adds complexity for marginal gain at 50 seed examples.

**Modify:** `src/lib.rs` — add `pub mod neural;`

**Tests:** Build TirGraph from known TIR sequences (reuse opcode test cases from `src/ir/tir/encode.rs`), verify node/edge counts, feature vector dimensions.

---

## Phase 2: VOCAB Expansion (64 → ~120)

**Complexity: M | Depends on: Phase 0 | Parallel with: Phase 1**

**`src/neural/model/vocab.rs`:**

```rust
pub const VOCAB_V2_SIZE: usize = 120;

pub struct Vocab {
    encode_map: HashMap<String, u32>,
    decode_map: Vec<String>,
}

impl Vocab {
    pub fn new() -> Self;  // Full TASM instruction set
    pub fn encode(&self, line: &str) -> Option<u32>;
    pub fn decode(&self, code: u32) -> Option<&str>;
    pub fn size(&self) -> usize;
}
```

Full TASM vocab: all `dup 0..15`, `swap 0..15`, `pick 0..15`, `place 0..15`, `push N` for N=-1..10, `pop 1..5`, `read_io 1..5`, `write_io 1..5`, `divine 1..5`, `read_mem 1..5`, `write_mem 1..5`, `invert`, `hash`, `sponge_init/absorb/squeeze/absorb_mem`, `merkle_step`, `merkle_step_mem`, `skiz`, `call`, `return`, `recurse`, `assert`, `assert_vector`, `nop`, `halt`, `split`, `eq`, `lt`, `add`, `mul`, `and`, `xor`, `div_mod`, `log_2_floor`, `pop_count`, `pow`, `x_invert`, `xb_mul`, `xx_dot_step`, `xb_dot_step`, EOS=0.

Enumerate exact count from Triton VM ISA spec. The `~120` is approximate — exact count determined during implementation.

**`src/neural/model/mod.rs`** — module declarations

---

## Phase 3: GNN Encoder

**Complexity: XL | Depends on: Phase 0, 1 | Parallel with: Phase 4, 5**

The hardest component. burn has no native GNN layers.

**`src/neural/model/encoder.rs`:**

```rust
#[derive(Module)]
pub struct GnnEncoder<B: Backend> {
    node_embed: Linear<B>,              // 59 → d=256
    edge_embed: Embedding<B>,           // 3 edge types → d_edge=32
    gat_layers: Vec<GatV2Layer<B>>,     // 3-4 layers
    global_proj: Linear<B>,             // 2*d → d (mean+max pool)
}

#[derive(Module)]
pub struct GatV2Layer<B: Backend> {
    w_src: Linear<B>,
    w_dst: Linear<B>,
    w_edge: Linear<B>,
    attn: Param<Tensor<B, 1>>,
    ffn: Linear<B>,
    norm: LayerNorm<B>,
}
```

**Custom ops:** `src/neural/model/gnn_ops.rs`
- `scatter_add(src, indices, num_nodes)` — aggregate messages by destination
- `batch_graphs(graphs) → (large_graph, offsets)` — pack for GPU training (§3.2)
- GATv2 attention: `a^T · LeakyReLU(W·[h_i ‖ h_j ‖ e_ij])` with softmax per neighborhood

**CPU vs GPU split (§5):**
- **Inference (single graph):** CPU/NEON. GPU submission overhead > matmul for ≤200 nodes.
- **Training (batch ≥16):** GPU. Pack all graphs into one large disconnected graph with offset indices.
- **Crossover:** ~8 graphs. Parameterize as config value, not compile-time constant.

**Pre-implementation measurement (Q2):** Before committing to GATv2, benchmark GPU command submission latency on Apple Silicon. If GATv2 is too complex in burn initially, implement GraphSAGE (mean-aggregation only, ~1 week simpler).

**~3M params** at d=256, 4 layers.

---

## Phase 4: Transformer Decoder

**Complexity: L | Depends on: Phase 0, 2 | Parallel with: Phase 3, 5**

**`src/neural/model/decoder.rs`:**

```rust
#[derive(Module)]
pub struct StackAwareDecoder<B: Backend> {
    token_embed: Embedding<B>,        // VOCAB_V2_SIZE → d=256
    depth_embed: Embedding<B>,        // 65 (MAX_STACK+1) → 32
    type_proj: Linear<B>,             // 24 (3×W=8) → 32
    pos_embed: Embedding<B>,          // 256 (MAX_SEQ) → d

    layers: Vec<DecoderLayer<B>>,     // 6 layers
    final_norm: LayerNorm<B>,
    output_proj: Linear<B>,           // d → VOCAB_V2_SIZE
}

#[derive(Module)]
pub struct DecoderLayer<B: Backend> {
    self_attn: MultiHeadAttention<B>, // 8 heads, d=256
    cross_attn: MultiHeadAttention<B>,// attends to GNN node embeddings
    ffn: FeedForward<B>,              // d → 4d → d
    norm1, norm2, norm3: LayerNorm<B>,
}
```

**Stack-aware inputs (§3.3):** At each step: `input_t = [token_emb(prev) ‖ depth_emb(depth_t) ‖ type_proj(types_t)] + pos_emb(t)`. Stack depth and type state come from grammar state machine (Phase 5).

**Cross-attention:** Decoder attends to full set of GNN node embeddings — primary mechanism for reading TIR.

**MAX_SEQ:** 256 (up from 16 in v1).

**~10M params.** burn provides `MultiHeadAttention`, `LayerNorm`, `Linear`, `Embedding` natively.

---

## Phase 5: Grammar Mask

**Complexity: L | Depends on: Phase 0, 2 | Parallel with: Phase 3, 4**

### Grammar tables

**`src/neural/model/grammar_tables.rs`:** Per-instruction static tables from TASM spec.
```rust
pub struct GrammarTables {
    pub pop_arity: Vec<i32>,      // [VOCAB_V2_SIZE]
    pub push_arity: Vec<i32>,     // [VOCAB_V2_SIZE]
    pub input_types: Vec<u32>,    // [VOCAB_V2_SIZE * MAX_POP_SLOTS]
    pub output_types: Vec<u32>,   // [VOCAB_V2_SIZE * MAX_PUSH_SLOTS]
}
```

Reuse cost knowledge from `src/cost/scorer.rs` (table heights) and `src/cost/stack_verifier.rs` (StackState execution, ALLOWED ops).

### CPU implementation (for training)

**`src/neural/model/grammar.rs`:**
```rust
pub struct StackStateMachine {
    depth: i32,
    types: Vec<u32>,  // top W=8 slots: BFE=0, XFE=1, EMPTY=2
}

impl StackStateMachine {
    pub fn step(&mut self, token: u32);
    pub fn valid_mask(&self) -> Vec<f32>;  // VOCAB_V2_SIZE: -1e9 or 0.0
    pub fn stack_depth(&self) -> i32;
    pub fn type_encoding(&self) -> Vec<f32>; // 3*W=24 dims for decoder
}

/// Precompute masks for full sequence (teacher forcing)
pub fn precompute_masks(target_tokens: &[u32]) -> Vec<Vec<f32>>;
```

During training (§3.4): ground truth tokens → precompute all masks CPU-side before forward pass. `masked_logits = logits + mask_sequence`. No GPU state machine needed.

### GPU shader (for inference beam search)

**`src/gpu/shaders/grammar_mask.wgsl`:** From §3.4 — K=32 beams, VOCAB_SIZE=120, one workgroup of 32 threads. Per-step: update stack state from sampled token, compute valid mask. No CPU↔GPU sync during decode.

**Pre-implementation measurement (Q1):** Profile shader with K=32, VOCAB=120, 200 steps. Compare GPU time vs CPU masking + sync. If GPU not faster, fallback to chunked CPU masking every N=8 tokens.

**Modify:** `src/gpu/shaders.rs` — add `pub const GRAMMAR_MASK: &str` constant.

---

## Phase 6: Beam Search + Validation

**Complexity: L | Depends on: Phase 4, 5**

**`src/neural/inference/beam.rs`:**
```rust
pub struct BeamSearch {
    pub k: usize,         // 32
    pub max_steps: usize, // 256
}

pub struct BeamResult {
    pub sequences: Vec<Vec<u32>>,
    pub log_probs: Vec<f32>,
}
```

**Inference flow (§3.1, §5):**
1. CPU: TIR → TirGraph
2. CPU: GNN encode → node embeddings + global (single graph = CPU, §5)
3. Transfer to GPU (zero-copy on Apple Silicon)
4. GPU: decoder loop (K beams batched, grammar mask same dispatch, top-K argsort)
5. Transfer K sequences to CPU
6. CPU/rayon: parallel Triton VM simulation × K candidates

**`src/neural/inference/execute.rs`:**
```rust
pub fn validate_and_rank(
    candidates: &[Vec<u32>],
    vocab: &Vocab,
    baseline_tasm: &str,
) -> Option<(Vec<String>, u64)>  // (best_tasm, cycles) or None
```
Uses rayon `par_iter`. For each candidate: decode vocab codes → TASM strings → `stack_verifier::verify_equivalent()` + `scorer::profile_tasm()`. Return cheapest valid or None.

**Fallback (§3.5):** If all K invalid → return compiler output. Log failure with TIR hash. Record as `BuildResult { valid: false, fallback_used: true }` into replay buffer. Never block compilation.

**`src/neural/inference/mod.rs`**

---

## Phase 7: Training Stage 1 — Supervised Pre-training

**Complexity: L | Depends on: Phase 1-6 (all model + inference)**

### Seed data extraction

**`src/neural/data/pairs.rs`:**
- Compile corpus (41 .tri files → TIR → TASM via existing `compile_corpus` in train.rs)
- Each `(TIR ops, compiler TASM)` = one training pair
- Convert: `TIROps → TirGraph`, `TASM lines → vocab codes`
- Hand baselines (`benches/*.baseline.tasm`) = additional optimal-target pairs (map back to TIR via source .tri)
- Store as rkyv archives in `data/seed/` (committed)
- Train/holdout split: 45 train / 5 holdout

### Data augmentation (§4.1)

**`src/neural/training/augment.rs`:**

**Structural augmentations** (change graph topology):
1. Reorder independent ops within basic blocks (topological sort variations from TirGraph)
2. Inline leaf call sites (replace Call node with callee's subgraph)
3. Dead code insertion (add ops that don't affect output — model must learn to ignore)

**Output-space augmentations** (change TASM, same TIR):
4. Local random walk: swap adjacent independent TASM instructions → if passes `stack_verifier::verify_equivalent()`, keep as new training pair
5. Equivalent substitutions: `push 0; add` → `nop`, etc.

**Coverage measurement:** Cluster augmented TASM by instruction n-gram overlap. Track cluster entropy. If low entropy → need more random walk diversity.

**Target:** 50 seeds → 5,000-10,000 augmented pairs.

### Supervised trainer

**`src/neural/training/supervised.rs`:**
```rust
pub struct SupervisedTrainer<B: AutodiffBackend> {
    model: NeuralCompilerV2<B>,
    optimizer: AdamW<B>,
    lr_scheduler: CosineDecay,
}

impl SupervisedTrainer<B> {
    pub fn train_epoch(&mut self, pairs: &[(TirGraph, Vec<u32>)]) -> f32;
}
```

**Loss:** Cross-entropy + teacher forcing. Grammar mask as logit penalty before softmax (`masked_logits = logits + precomputed_masks`). Loss masked to exclude padding.

**Hyperparams (§4.2):** AdamW, lr=3e-4, cosine decay → 1e-5, weight_decay=0.01, batch=32, gradient_clip=1.0. Train until validation loss plateaus for 3 consecutive epochs.

**Phase A gate (§8):** Validity ≥ 80% on 5-example holdout. Measured with 1000-resample bootstrap CI (statrs). Gate is the 90% CI lower bound ≥ 70%, not the point estimate. Training loss plateau < 0.5 nats.

**`src/neural/training/mod.rs`**

---

## Phase 8: Training Stage 2 — GFlowNets

**Complexity: L | Depends on: Phase 7**

**`src/neural/training/gflownet.rs`:**

```rust
fn tb_loss<B: AutodiffBackend>(
    log_pf: Tensor<B, 1>,  // sum log P(token_t | history)
    log_pb: f32,            // uniform backward policy constant
    log_r: f32,             // log(reward), clipped ≥ log(1e-3)
    log_z: Tensor<B, 0>,   // learned log-partition, init=0, trained jointly
) -> Tensor<B, 0> {
    let residual = log_z + log_pf - log_pb - log_r;
    residual.powf_scalar(2.0)
}
```

**Reward (§4.3):**
```
R(tasm) = 1e-3 (ε, never zero — log(0)=NaN)              if !valid
        = 1 + max(0, (compiler_cycles - model_cycles)      if valid
                     / compiler_cycles)
```

**Partial credit shaping (first 1000 steps):**
```
R_shaped(tasm, k) = ε + (k / total_length) × validity_bonus
```
where k = step of first stack violation. Transition to pure R once validity ≥ 70%.

**Temperature annealing:** τ=2.0 → τ=0.5 over Stage 2. High early = diverse exploration. Low late = concentrate on good solutions.

---

## Phase 9: Training Stage 3 — Online Learning

**Complexity: M | Depends on: Phase 8**

**`src/neural/data/replay.rs`:**
```rust
#[derive(Archive, Serialize, Deserialize, rkyv::Serialize, rkyv::Deserialize)]
pub struct BuildResult {
    pub tir_hash: [u8; 32],             // Poseidon2 CID
    pub generated_tasm: Vec<String>,
    pub valid: bool,
    pub clock_cycles: Option<u64>,
    pub compiler_cycles: u64,
    pub fallback_used: bool,
    pub timestamp: u64,
    pub model_version: u32,
}

pub struct ReplayBuffer {
    entries: Vec<(f64, BuildResult)>,    // (priority=reward, result)
    capacity: usize,                     // 10,000
}
// Persistence: rkyv zero-copy archive. Append-only file + periodic compaction.
```

**`src/neural/training/online.rs`:**
- **Micro-finetune trigger:** ≥50 new results (or 24h elapsed) → 200 GFlowNet gradient steps on new batch + 10% historical sample (prevents forgetting).
- **Regression guard (§8):** Before activating updated checkpoint: run full eval set. If validity delta < -2pp vs current checkpoint → discard, log anomaly, keep old.
- **Phase B activation:** ≥100 valid non-fallback results in buffer. Phase B metrics become binding:

| Metric | Target |
|--------|--------|
| Validity rate | ≥ 95% |
| Improvement rate | ≥ 60% of valid beat compiler |
| Median cycle reduction | ≥ 10% |
| P90 inference latency | ≤ 200 ms |
| Fallback rate | ≤ 5% |

---

## Phase 10: Integration — Replace v1

**Complexity: L | Depends on: Phase 6, 7**

### Composite model

**`src/neural/model/mod.rs`:**
```rust
#[derive(Module)]
pub struct NeuralCompilerV2<B: Backend> {
    pub encoder: GnnEncoder<B>,
    pub decoder: StackAwareDecoder<B>,
}
```

### Public API

**`src/neural/mod.rs`** — top-level:
```rust
pub fn compile(tir: &[TIROp], device: &WgpuDevice) -> Result<Vec<String>, CompileError> {
    let graph = TirGraph::from_tir_ops(tir);
    let model = load_checkpoint(device)?;
    let result = BeamSearch::new(32, 256).search(&graph, &model);
    validate_and_rank(&result, &vocab, &baseline)
}
```

### Checkpoint management

**`src/neural/checkpoint.rs`:**
- Weights: `data/neural/v2/` directory (burn's native record format)
- Checkpoints: `stage1_best.bin`, `stage2_latest.bin`, `production.bin` (symlink)
- Model version tracking for replay buffer compatibility

### Replace v1 in existing files

**`src/ir/tir/lower/mod.rs`:**
- `SpeculativeLowering` uses v2 model instead of v1 `NeuralModel`
- `create_speculative_lowering()` loads v2 checkpoint
- Remove v1 `NeuralModel` import, use `neural::NeuralCompilerV2`

**`src/cli/train.rs`:**
- Replace evolutionary training with burn-based 3-stage pipeline
- Remove: `Population`, `Individual`, `evolve`, `train_one_compiled` evolution loop, `bootstrap_from_compiler`
- Keep: `CompiledFile`, `compile_corpus`, `discover_corpus`, proven verification via trisha, table display (adapted for new metrics)
- Stage auto-detection: no v2 weights → Stage 1. Stage 1 done → Stage 2. Replay ≥100 → Stage 3.

**`src/cli/build.rs`:**
- `--neural` flag loads v2 model (no `--v2` suffix, this IS the neural optimizer now)
- Run beam search, show decisions in report

### Remove v1 code

| File | Action |
|------|--------|
| `src/ir/tir/neural/model.rs` | Delete (v1 MLP) |
| `src/ir/tir/neural/evolve.rs` | Delete (evolutionary training) |
| `src/ir/tir/neural/weights.rs` | Delete (v1 weight format) |
| `src/ir/tir/neural/report.rs` | Keep — adapt for v2 decisions |
| `src/ir/tir/neural/mod.rs` | Reduce to re-export `crate::neural` |
| `src/ir/tir/encode.rs` | Keep — TIRBlock encoding still useful for other purposes |
| `src/gpu/neural_accel.rs` | Delete (v1 GPU batch forward) |
| `src/gpu/shaders/neural_forward.wgsl` | Delete (v1 WGSL MLP shader) |
| `src/gpu/mod.rs` | Keep — device init used by grammar mask shader |
| `src/gpu/shaders/goldilocks.wgsl` | Keep |
| `src/gpu/shaders/fixed_point.wgsl` | Keep |
| `src/ir/tir/lower/mod.rs` | Keep `decode_output`, `encode_tasm_line` (backward compat for bench) |

---

## Phase 11: End-to-end Benchmark

**Complexity: M | Depends on: Phase 10**

**`benches/end_to_end.rs`** — criterion benchmark validating §5 latency estimates:
- TIR parsing + graph build
- GNN encoder (CPU, single graph, 100 nodes)
- Decoder (GPU, K=32, 200 steps)
- Triton VM simulation × 32
- Total end-to-end

**Target:** P90 ≤ 200ms per function (§8).

**Gitignored data dirs:**
- `data/augmented/` — augmented training pairs
- `data/neural/v2/checkpoints/` — model checkpoints + optimizer state

**Committed data dirs:**
- `data/seed/` — 50 seed pairs as rkyv archives

---

## File Tree (all new files)

```
src/neural/
    mod.rs                          # pub API: compile(), pub mod data/model/training/inference
    checkpoint.rs                   # burn record save/load, version management
    data/
        mod.rs
        tir_graph.rs                # TirGraph, TirNode, EdgeKind, from_tir_ops()
        pairs.rs                    # Dataset loading, train/holdout split, seed extraction
        replay.rs                   # BuildResult, ReplayBuffer (PER)
    model/
        mod.rs                      # NeuralCompilerV2<B> composite
        encoder.rs                  # GnnEncoder<B> (GATv2 / GraphSAGE fallback)
        decoder.rs                  # StackAwareDecoder<B> (6-layer Transformer)
        grammar.rs                  # StackStateMachine, CPU masks, GPU shader dispatch
        grammar_tables.rs           # Static pop/push arity tables from TASM spec
        vocab.rs                    # Vocab (~120 tokens, full TASM ISA)
        gnn_ops.rs                  # scatter_add, batch_graphs, graph utilities
    training/
        mod.rs
        augment.rs                  # Structural TIR augmentations + output-space random walk
        supervised.rs               # Stage 1: CE + teacher forcing
        gflownet.rs                 # Stage 2: TB loss, temperature annealing, reward shaping
        online.rs                   # Stage 3: replay buffer, micro-finetune, regression guard
    inference/
        mod.rs
        beam.rs                     # BeamSearch (K=32, GPU-resident)
        execute.rs                  # rayon-parallel validation + ranking

src/gpu/shaders/grammar_mask.wgsl   # WGSL compute shader for stack state machine + mask
data/seed/                          # 50 seed (TirGraph, TASM) pairs as rkyv archives (committed)
benches/end_to_end.rs               # criterion: model latency vs compiler
```

## Files to modify (existing)

| File | Change |
|------|--------|
| `Cargo.toml` | Add burn, rayon, petgraph, serde, rkyv, statrs, criterion |
| `src/lib.rs` | Add `pub mod neural;` |
| `src/cli/train.rs` | Replace v1 evolutionary training with burn 3-stage pipeline |
| `src/cli/build.rs` | Update `--neural` to use v2 model |
| `src/ir/tir/lower/mod.rs` | SpeculativeLowering uses v2 model |
| `src/ir/tir/neural/mod.rs` | Re-export `crate::neural`, remove v1 model imports |
| `src/gpu/shaders.rs` | Add `GRAMMAR_MASK` shader constant |
| `.gitignore` | Add `data/augmented/`, `data/neural/v2/checkpoints/` |

## Files to delete

| File | Reason |
|------|--------|
| `src/ir/tir/neural/model.rs` | v1 MLP replaced by v2 |
| `src/ir/tir/neural/evolve.rs` | Evolution replaced by gradient training |
| `src/ir/tir/neural/weights.rs` | v1 weight format replaced by burn records |
| `src/gpu/neural_accel.rs` | v1 GPU batch forward replaced |
| `src/gpu/shaders/neural_forward.wgsl` | v1 MLP shader replaced |

---

## Dependency Graph

```
Phase 0 (Deps)
    ├──→ Phase 1 (TirGraph) ──────┐
    ├──→ Phase 2 (VOCAB) ─────────┤
    │                              ├──→ Phase 3 (GNN Encoder) ───┐
    │                              ├──→ Phase 4 (Transformer) ───┤
    │                              └──→ Phase 5 (Grammar Mask) ──┤
    │                                                            ├──→ Phase 6 (Beam+Validate)
    │                                                            │         │
    │                                                            │         v
    │                                                            └──→ Phase 7 (Stage 1)
    │                                                                      │
    │                                                                      v
    │                                                                Phase 8 (Stage 2)
    │                                                                      │
    │                                                                      v
    │                                                                Phase 9 (Stage 3)
    │                                                                      │
    │                                                                      v
    │                                                                Phase 10 (Replace v1)
    │                                                                      │
    │                                                                      v
    └──────────────────────────────────────────────────────────── Phase 11 (Benchmark)
```

**Max parallelism:** Phases 1+2 in parallel. Then Phases 3+4+5 in parallel (different files, non-overlapping per CLAUDE.md). Phase 6 waits for 4+5. Phase 7 waits for 1-6.

## Open Questions (resolve during implementation)

From §9 of design doc — each resolved before the dependent phase:

| Question | When | Resolution |
|----------|------|------------|
| Q1: WGSL shader perf at VOCAB=120 | Before Phase 5 | Profile K=32×120×200. If GPU slower than CPU+sync → chunked CPU fallback |
| Q2: GNN inference crossover | Before Phase 3 | Benchmark CPU vs GPU for 10/50/100/200 node graphs. Parameterize threshold |
| Q3: Triton VM sim cost distribution | Before Phase 6 | Measure `trisha run` latency across 50 seed functions. Adjust P90 target |
| Q4: GATv2 vs GraphSAGE | Before Phase 3 commit | Train Stage 1 with both. If diff < 5% → GraphSAGE |
| Q5: XFE immediate encoding | Phase 1 | BFE only. XFE ops get has_immediate=0 |

## Estimates

| Phase | Complexity | Pomodoros |
|-------|-----------|-----------|
| 0 — Dependencies | S | 2 |
| 1 — TirGraph | M | 4 |
| 2 — VOCAB | M | 3 |
| 3 — GNN Encoder | XL | 12 |
| 4 — Transformer Decoder | L | 8 |
| 5 — Grammar Mask | L | 6 |
| 6 — Beam Search + Validate | L | 6 |
| 7 — Stage 1 Training | L | 8 |
| 8 — Stage 2 GFlowNets | L | 6 |
| 9 — Stage 3 Online | M | 4 |
| 10 — Replace v1 | L | 8 |
| 11 — Benchmark | M | 4 |
| **Total** | | **71 (~12 sessions, ~8 critical path)** |

## Commit Sequence

One commit per phase, atomic and testable:

1. `feat: add burn/rayon/petgraph/serde/rkyv/statrs deps`
2. `feat: TirGraph with typed edge extraction from TIR ops`
3. `feat: VOCAB v2 (~120 tokens, full TASM ISA)`
4. `feat: GNN encoder (GATv2) in burn`
5. `feat: stack-aware Transformer decoder in burn`
6. `feat: grammar mask (CPU state machine + WGSL shader)`
7. `feat: GPU-resident beam search (K=32) + rayon validation`
8. `feat: supervised pre-training (Stage 1, CE + teacher forcing)`
9. `feat: GFlowNet training (Stage 2, TB loss + reward shaping)`
10. `feat: online learning (Stage 3, PER + regression guard)`
11. `refactor: replace v1 neural model with v2`
12. `feat: end-to-end latency benchmark`

## Verification

Per-phase: `cargo check` (zero warnings) + `cargo test` (all pass) + phase-specific test.

End-to-end:
1. `trident train --epochs 3` — runs Stage 1, shows training loss declining
2. `trident build std/crypto/poseidon2.tri --neural` — v2 inference, shows beam results
3. `trident bench` — still works, compares compiler vs neural vs hand baselines
4. Phase A: validity ≥ 80% on holdout (bootstrap CI)
5. `cargo bench --bench end_to_end` — P90 ≤ 200ms
