# Trident

A building block for superintelligence. Trident is a provable language
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

## Pipeline Contract

```
Source → Lexer → Parser → AST → TypeCheck → KIR → TIR → LIR → Target
```

Output of stage N must be valid input for stage N+1. When modifying a
stage, verify both its input and its output still connect.

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
`trident verify` checks these every commit.

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

## Agent Cortex

`.cortex/` is shared memory for agents — insights, audit results,
intermediate findings, open questions, anything useful.

Budget: 1000 lines total. Rules for every update:

1. Read what's already there before writing.
2. Add what you have — no format restrictions.
3. If budget exceeded, compress, merge, or delete the weakest entries.
4. Every update must increase information density.

Agents self-organize. The budget is the only constraint.
Gitignored for now (experimental).

## Self-Verification

Every commit:
- `cargo check` — zero warnings
- `cargo test` — all tests pass
- `trident bench` — no regressions vs baselines
- `trident verify` — formal properties still hold
- If anything fails, fix before reporting done.

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

## Build & Test

- `cargo test` must pass before committing.
- Test names describe the property, not the method
  (e.g., `nested_if_else_preserves_scope` not `test_if`).
- Snapshot tests: `cargo insta review`, never manually.

## License

Cyber License: Don't trust. Don't fear. Don't beg.
