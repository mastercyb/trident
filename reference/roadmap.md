# Trident Development Plan

Status as of February 2026. Checked items are shipped. Unchecked items
are the remaining roadmap.

---

## Milestone 1: Neptune Production Readiness

- [x] Complete — Neptune TX validation rewrite, benchmark suite, gadget library (SHA-256, Keccak), recursive STARK verifier, language spec v0.5

## Milestone 2: Formal Verification (Embedded)

- [x] Phase 1 Complete — Symbolic execution, algebraic solver (Schwartz-Zippel), bounded model checker, `trident verify`, redundant assertion elimination, counterexample generation
- [x] Phase 2 Complete — `#[requires]`/`#[ensures]`/`#[invariant]` annotations, Z3/CVC5 SMT backend, Goldilocks field theory, witness checking, Hoare logic, verification certificates
- [x] Phase 4 Complete — Template-based invariant synthesis (CEGIS), specification inference

### Phase 3: LLM Integration Framework
- [x] Machine-readable verification output, `trident generate`, structured counterexamples, LLM-optimized reference
- [ ] Benchmark: 20 contract specs with known-correct implementations

## Milestone 3: Content-Addressed Codebase

- [x] Complete — AST normalization, content hashing, store (add/list/view/rename/stats/history/deps), compilation caching, global registry (HTTP API, publish/pull, search, certificate sharing), semantic equivalence checking
- [ ] Atlas on-chain registry (target: 0.2) — TSP-2 Card collection per OS: each package is a Card (`asset_id = hash(name)`, `metadata_hash = content_hash(artifact)`), publishing = mint, version update = metadata update, three-tier resolution (local → cache → on-chain)

## Milestone 4: Multi-Target Backends

Architecture is in place (TargetConfig, StackBackend trait, CostModel
trait, target-tagged asm blocks, 5 target TOML configs). Only Triton VM
backend is fully implemented; others are stubs.

- [x] Target abstraction (TargetConfig, StackBackend, CostModel, target-tagged asm, cross-target testing)
- [ ] Miden backend (full implementation, not stub)
- [ ] OpenVM backend (full implementation)
- [ ] SP1 backend (full implementation)
- [ ] Cairo backend (full implementation)
- [ ] Verified compiler: prove backend emission preserves semantics (Coq/Lean)

## Milestone 5: Ecosystem

- [x] Package manager, crypto library (11 modules), token standards (TSP-1/TSP-2), bridge validators (BTC/ETH light clients)
- [ ] Landing page + web playground (compile .tri to TASM in browser)
- [ ] Browser extension integration library
- [ ] ZK coprocessor programs (Axiom, Brevis, Herodotus integration)
- [ ] Editor extension download page + marketplace listings
- [ ] Reimplement store as Atlas (TSP-2 Card collection) with per-OS namespace governance
- [ ] Ship `std.token`, `std.coin`, `std.card`, `std.skill.*` (23 skills) as standard library modules

## Language Evolution

- [x] Pattern matching on structs, const generic expressions, `#[pure]` annotation, proof composition primitives
- [ ] Indexed assignment (`arr[i] = val`, `s.field = val` via Place::FieldAccess/Index)
- [ ] Trait-like interfaces for backend extensions (generic over hash function)

## Research Directions

Long-term, exploratory, not committed.

- [ ] Optimal backend selection (given cost/memory constraints, pick best target)
- [ ] Cost-driven compilation (transform to minimize proving cost per target)
- [ ] Incremental proving (prove modules independently, compose proofs)
- [ ] Verifiable AI/ML inference (fixed-architecture neural networks in Trident)
- [ ] Provable data pipelines (ETL, aggregation, supply chain verification)
- [ ] Hardware acceleration backends (FPGA, ASIC, GPU proving)
- [ ] Self-proving compiler — Trident compiles itself to a provable target,
      then proves its own compilation correctness. Every `trident build`
      produces a proof certificate alongside the assembly. Atlas
      already stores content-addressed hashes; add proof certificates
      alongside and you get trustless package distribution — you don't
      trust the compiler binary, you verify the proof. The endgame:
      source → TIR → assembly where each arrow is a proven transformation,
      chained into a single certificate that says "this assembly correctly
      implements this source program."
