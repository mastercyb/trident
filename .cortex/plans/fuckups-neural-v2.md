Gap Analysis: v2 Design Doc vs Implementation

### CRITICAL (model can't learn without these)

**1. No checkpoint save/load** — `src/neural/checkpoint.rs` doesn't exist. Training runs to completion and discards all learned weights. Nothing persists between sessions. This is why `trident train` is useless right now — the model reinitiates randomly every run.

**2. GFlowNet sampling is argmax** — `gflownet.rs:sample_categorical()` is deterministic argmax, not stochastic sampling. Stage 2 can't explore — every rollout produces the same sequence. GFlowNets fundamentally require stochasticity. Dead module.

**3. Stages 2+3 not wired into CLI** — `train.rs` runs Stage 1 only. GFlowNet (`gflownet.rs`) and online learning (`online.rs`) exist as standalone modules but are never called. No stage auto-detection ("no weights → Stage 1, Stage 1 done → Stage 2, replay ≥100 → Stage 3").

**4. No data augmentation** — `training/augment.rs` doesn't exist. The 50 seed pairs → 5,000-10,000 augmented pairs expansion (§4.1: reorder independent ops, inline calls, dead code insertion, TASM random walk) is entirely missing. Training on ~31 raw pairs is too few for a 13M-param model.

**5. Training is batch_size=1** — `supervised.rs` processes one pair at a time. Design doc says batch=32 with `batch_graphs()` packing. GPU sits idle most of the time. `gnn_ops::batch_graphs()` is implemented but never called.

### SERIOUS (correctness/quality)

**6. No gradient clipping** — `SupervisedConfig` has `grad_clip: 1.0` but it's never applied in `train_epoch()`. Exploding gradients → loss=168M.

**7. No cosine LR decay** — constant learning rate. Doc specifies AdamW with cosine decay 3e-4 → 1e-5. Without decay, training oscillates or diverges.

**8. Seed data not persisted** — `data/seed/` directory doesn't exist. No committed rkyv archives of the 50 seed pairs. Every training run re-extracts pairs from corpus.

**9. Validation uses proxy, not Triton VM** — `execute.rs` uses trident's stack verifier + cost scorer, not `triton_vm::execute()` + actual clock cycles. Phase A/B metrics in the doc are defined against real VM execution.

**10. WGSL grammar mask shader not dispatched** — shader exists in `src/gpu/shaders/grammar_mask.wgsl`, compiled into binary via `include_str!`, but zero wgpu dispatch code. Beam search does CPU-only masking. The GPU-resident decode loop (§3.4: no CPU↔GPU sync) is not implemented.

**11. Beam K=8, max_steps=64 in CLI** — doc says K=32, max_steps=256. Current settings limit both exploration width and sequence length.

### GAPS (functional but deferred)

**12. `log_z` not a learnable parameter** — GFlowNet TB loss needs Z as a jointly-trained scalar. Currently passed as argument with no optimizer integration.

**13. Phase A/B gates not implemented** — bootstrap CI validity ≥80% on holdout (Phase A), Phase B activation after ≥100 valid results. Functions exist in `online.rs` but nothing calls them.

**14. Replay buffer has no persistence** — `replay.rs` is in-memory only. No save/load to disk. Online learning across sessions impossible.

**15. GPU shader does depth-only masking** — CPU grammar tracks stack types (BFE/XFE/Unknown), but WGSL shader only tracks depth. GPU path could accept type-invalid instructions.

**16. Public `compile()` API missing** — `src/neural/mod.rs` has no top-level `compile(tir, device) -> Result<Vec<String>>` entry point as specified in the plan.

**17. No CPU/GPU GNN crossover** — doc says single-graph inference on CPU, batch training on GPU. No conditional dispatch logic exists.

---

### Root cause of loss=168M

Gaps #5 (batch=1), #6 (no grad clipping), #7 (no LR decay) together explain the astronomical loss. The 13M-param model gets unconstrained gradient updates from single examples with a constant high learning rate. Gradients explode, weights go to NaN-adjacent values, and cross-entropy loss skyrockets.

### Priority order for fixing

1. **Checkpoint save/load** (#1) — without this, nothing else matters
2. **Grad clipping + cosine LR decay** (#6, #7) — make Stage 1 actually converge
3. **Batch training with `batch_graphs()`** (#5) — GPU utilization + stable gradients
4. **Data augmentation** (#4) — enough data for 13M params
5. **Wire Stage 2 + fix sampling** (#2, #3) — enable GFlowNet exploration
6. **Wire Stage 3 + replay persistence** (#3, #14) — online learning loop
7. Rest follows naturally

Want me to start fixing these in priority order?
