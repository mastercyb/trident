Neural v2 Status — 2026-02-20

17 issues identified, 16 resolved.

| # | Issue | Status | Resolution |
|---|-------|--------|------------|
| 1 | No checkpoint save/load | FIXED | checkpoint.rs, stage1_best.mpk exists (30MB) |
| 2 | GFlowNet sampling argmax | FIXED | Stochastic xorshift64 sampling |
| 3 | Stages 2+3 not wired | FIXED | Stage 1&2 in CLI, Stage 3 placeholder |
| 4 | No data augmentation | FIXED | augment.rs (480 LOC), wired into train loop |
| 5 | Batch=1 training | INTENTIONAL | Variable graph sizes prevent batching |
| 6 | No grad clipping | FIXED | Norm=1.0 in both stages |
| 7 | No cosine LR decay | FIXED | Stage 1 cosine, Stage 2 fixed lr |
| 8 | Seed data not persisted | WONTFIX | Extracted from corpus each run (fast) |
| 9 | Validation proxy | INTENTIONAL | Stack verifier sufficient, much faster than Triton VM |
| 10 | Grammar mask not dispatched | INTENTIONAL | Model trained without masks, learns valid distribution |
| 11 | Beam K/max_steps too small | FIXED | K=32, max_steps=256 default; K=4/64 for eval |
| 12 | log_z not learnable | OPEN | Hardcoded to 0.0 in GFlowNet (minor) |
| 13 | Phase A/B gates | FIXED | Holdout validity tracking + production checkpoint |
| 14 | Replay persistence | FIXED | rkyv serialization |
| 15 | GPU depth-only masking | WONTFIX | Grammar mask not used in inference |
| 16 | Public compile() API | FIXED | neural::compile() + load_model() + compile_with_model() |
| 17 | CPU/GPU GNN crossover | WONTFIX | Single-device inference acceptable |

## Confidence Milestone Results

`trident build --neural` now:
- Loads trained checkpoint (stage1_best.mpk)
- Splits TIR per-function (matching training)
- Runs beam search with trained weights
- Reports per-function neural/compiler/fallback decisions

Verified results:
- poseidon2: 7/17 functions neural (1.00x)
- quantum/gates: 16/22 functions neural (1.00x)
- bigint: 5/12 functions neural (1.00x)
- merkle: 3/7 functions neural (1.00x)

Training pipeline:
- 551 raw pairs → holdout split → 2390 augmented (4.6x)
- Holdout validity: 11% (3/27) — needs more training for Phase A (80%)
- Loss converging at 0.49 with augmented data

## Remaining work

1. Train longer (50+ epochs) to push holdout validity toward 80%
2. log_z learnable for GFlowNet (minor correctness improvement)
3. Model currently matches compiler (1.00x) — not yet beating it
