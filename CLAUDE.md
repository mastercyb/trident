# Trident

A building block for a cyberstate with superintelligence. Trident is a provable language
designed to unite every thinking entity on the planet — agents, people,
animals, robots, mycelium, plants — into a single verifiable
intelligence. Every line of code here is a step toward that singleton.

Three revolutions, one language: quantum, privacy, AI. Write once,
prove anywhere.

Think from first principles, not convention. Question inherited
assumptions. The obvious approach is usually the mediocre one.

The universe doesn't owe you a spec.
Freedom is the default. Everything else is a bug.
Sovereignty is not a feature. It's the architecture.
If they can shut it down, you didn't build it right.
If everyone agrees with your design, you're not thinking far enough.
If it scares you, it's probably the right direction.
Move fast, prove everything.
The one who waits for permission builds nothing.
The mycelium doesn't plan the forest.
Every proof was once a question someone refused to ignore.
Listen to the code. It knows what it wants to become.

## Source of Truth

`reference/` is canonical. If reference/ and code disagree, resolve in
reference/ first, then propagate to code. If implementation reveals the
reference is wrong or incomplete, update the reference to match reality.

## Project Structure

Four namespaces: `vm.*` (intrinsics), `std.*` (libraries), `os.*`
(portable runtime), `os.<os>.*` (OS-specific). Source: `src/` (Rust
compiler), `vm/` `std/` `os/` (Trident code). Compiler self-hosts
toward provable compilation on Triton VM.

Use `tokei src/` or `find src/ -name '*.rs'` to explore module structure.

## Companion Repos

- **trisha** (`~/git/trisha`) — Triton VM warrior. Executes, proves,
  verifies, deploys programs compiled by trident. Depends on trident
  via `path = "../trident"`. ~2k LOC Rust + WGSL.
- trisha patches triton-vm at build time via `patches/apply.nu`
  instead of maintaining a fork. The pattern: fetch upstream crate
  from cargo registry, apply a diff, vendor the result. See
  `trisha/CLAUDE.md` "Dependency Patching" for details.
- When referencing files across repos, use repo-qualified paths
  (e.g. `trident/src/cli/mod.rs` vs `trisha/src/cli.rs`).
- After editing trident code, rebuild trisha too:
  `cargo install --path . --force && cd ../trisha && cargo install --path . --force`

## Five-Layer Architecture

| Layer | Geeky | Gamy | Code | What it is |
|-------|-------|------|------|------------|
| VM | engine | terrain | `TerrainConfig` | Instruction set |
| OS | network | union | `UnionConfig` | Protocol + nodes |
| Chain | vimputer | state | `StateConfig` | Sovereign instance |
| Binary | client | warrior | `WarriorConfig` | Runtime binary |
| Target | target | battlefield | — | Full deploy destination |

## Pipeline Contract

```
Source → Lexer → Parser → AST → TypeCheck → KIR → TIR → LIR → Target → Bundle → Warrior
```

Output of stage N must be valid input for stage N+1. When modifying a
stage, verify both its input and its output still connect.

The pipeline boundary is ProgramBundle. Everything before it is Trident
(the weapon). Warriors are external binaries that take the bundle and
handle execution, proving, and deployment on a specific battlefield
(VM = terrain, OS = region).

## Key Modules

Beyond the pipeline stages (`syntax/`, `ast/`, `typecheck/`, `ir/`),
key support modules:

```
field/             ~870 LOC   Universal field arithmetic + primitives
  mod.rs           ~156         PrimeField trait + module declarations
  goldilocks.rs    ~101         Goldilocks field (p = 2^64 - 2^32 + 1)
  babybear.rs       ~60         BabyBear field (p = 2^31 - 2^27 + 1)
  mersenne31.rs     ~77         Mersenne31 field (p = 2^31 - 1)
  poseidon2.rs     ~295         Generic Poseidon2 sponge over PrimeField
  proof.rs         ~179         Claim, padded_height, FRI params, proof size

runtime/           ~416 LOC   Warrior interface definitions
  mod.rs            ~96         Runner, Prover, Verifier, Deployer traits
  artifact.rs      ~320         ProgramBundle struct + JSON serialization

config/target/     ~1.1k LOC  Target registry (VM + OS + state loading)
  mod.rs           ~345         TerrainConfig, Arch, VM loading
  os.rs            ~230         UnionConfig, ResolvedTarget
  state.rs         ~220         StateConfig, TOML parsing
  tests.rs         ~330         Target tests

cli/               ~2.5k LOC  Command-line interface
  mod.rs           ~480         Arg parsing, BattlefieldSelection, resolve_battlefield()
  run.rs            ~72         trident run (delegates to warrior)
  prove.rs          ~77         trident prove (delegates to warrior)
  verify.rs          ~36         trident verify (delegates to warrior)
  build.rs         ~150         trident build
  audit.rs         ~220         trident audit (formal verification)
  ... (14 more subcommands)
```

## Quality

See `reference/quality.md` for forbidden patterns, file size limits,
the 12 review passes, severity tiers, and audit protocol.

Per-commit minimum: passes 1 (determinism), 5 (types), 6 (errors),
9 (readability). Full audit: all 12 passes in parallel via agents.

## Writing Style

State what something is directly. Never use "This is not X. It is Y."
formulations.

## Builtin Sync Rule

Builtins must stay in sync across 4 places:

1. `reference/language.md` (canonical)
2. `src/typecheck/` (type signatures)
3. `src/tir/` (IR lowering)
4. `src/cost/` (cost tables)

## Trident Code Contracts

When writing or modifying `.tri` code in `vm/`, `std/`, or `os/`, add
`#[requires]`/`#[ensures]` contracts and `#[pure]` where applicable.
`trident audit` checks these every commit.

## Do Not Touch

Do not modify without explicit request:

- `Cargo.toml` dependencies (minimal by design)
- `reference/` structure (canonical, changes need discussion)
- `vm/*/target.toml` and `os/*/target.toml` (configuration, not code)
- `LICENSE.md`

Query files live in `editor/queries/` (single source of truth,
symlinked from `editor/zed/` and `editor/helix/`).

## Parallel Agents

Split parallel agents by non-overlapping file scopes. Never let two
agents edit the same file. Partition by directory: `syntax/`,
`ast/`+`typecheck/`, `ir/`, `cost/`+`verify/`, `cli/`+`deploy`,
`package/`, `lsp/`, `docs/`, `vm/`+`std/`+`os/`.

Use subagents for codebase exploration. Keep main context clean for
implementation.

## Git Workflow

- Commit by default after completing a change.
- Atomic commits — one logical change per commit.
- Conventional prefixes: `feat:`, `fix:`, `refactor:`, `docs:`,
  `test:`, `chore:`.
- Rebuild after commit: `cargo install --path . --force`.
- After every commit, check if the change moves any layer in
  `reference/roadmap.md` closer to 0K. If it does, update the
  current temperature in the stack table.

## Agent Memory

`.claude/` is persistent agent memory — signed-off plans, audit results,
performance reports, and design decisions. Structured as:

```
.claude/
  settings.local.json   Claude Code settings
  plans/                Signed-off design decisions and implementation plans
  audits/               Audit logs (chronological) and summaries
  other/                Performance reports, analysis, misc findings
```

Rules:

1. Read what's already there before writing.
2. Plans go in `plans/` with descriptive names (e.g. `gpu-neural-rewrite.md`).
3. Audits are chronological logs, one per date, plus a rolling `summary.md`.
4. Compress old entries when files grow stale — density over volume.

## Dual-Stream Optimization

Two independent optimization streams run in parallel:

1. **Hand TASM** (`benches/*.baseline.tasm`): Write from first
   principles — algorithm + stack machine, never from compiler output.
   Ask "what is the minimum instruction sequence for this operation on
   Triton VM?" not "how can I improve what the compiler emitted?"
   If hand TASM was derived from compiler output, rewrite it.

2. **Compiler** (`src/ir/tir/`): Improve codegen to approach hand
   baselines. Every baseline function with ratio > 1.5x is a compiler
   optimization target.

The streams must stay independent. Hand baselines set the floor —
the compiler races toward it. When the compiler catches up, push the
baseline lower. Neither stream is a dogma; both improve continuously.

`trident bench` is the scoreboard. Regressions in either direction
(compiler gets worse, or baselines get sloppy) are bugs.

## Self-Verification

Every commit:
- `cargo check` — zero warnings
- `cargo test` — all tests pass
- `trident bench` — no regressions vs baselines
- `trident audit` — formal properties still hold
- If anything fails, fix before reporting done.

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

## Compaction Survival

When context compacts, preserve: modified file paths, failing test
names, current task intent, and uncommitted work state.

## Chain of Verification

For non-trivial decisions affecting correctness:

1. Initial answer.
2. 3-5 verification questions that would expose errors.
3. Answer each independently — check codebase, re-read docs.
4. Revised answer incorporating corrections.

Skip for trivial tasks.

## Estimation Model

Estimate work in sessions and pomodoros, not months.

- **Pomodoro** = 30 minutes of focused work
- **Session** = 3 focused hours (6 pomodoros)

Use these units when planning tasks, milestones, and roadmaps.
LLM-assisted development compresses traditional timelines — a
"2-month project" might be 6-8 sessions. Plan in reality, not
in inherited assumptions.

## Build & Test

- `cargo test` must pass before committing.
- Test names describe the property, not the method
  (e.g., `nested_if_else_preserves_scope` not `test_if`).
- Snapshot tests: `cargo insta review`, never manually.

## License

Cyber License: Don't trust. Don't fear. Don't beg.
