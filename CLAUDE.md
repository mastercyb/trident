# Trident — Claude Code Instructions

## Source of Truth

`docs/reference/` is the canonical reference for all Trident design decisions.
Each file owns a specific domain:

- `language.md` — syntax, types, operators, builtins, attributes,
  memory model, type checking rules, permanent exclusions, sponge, Merkle,
  extension field, proof composition (Tier 2-3)
- `ir.md` — TIROp variant names, counts, tier assignments, lowering paths,
  naming conventions, architecture diagrams, pipeline
- `targets.md` — OS model, integration tracking, how-to-add checklists
- `vm.md` — VM registry, lowering paths, tier/type/builtin tables,
  cost models
- `os.md` — OS concepts (neuron/signal/token), `os.*` portable APIs,
  `os.<os>.*` OS-specific extensions, OS registry
- `stdlib.md` — `std.*` library modules, common patterns
- `errors.md` — error codes and diagnostic messages
- `grammar.md` — EBNF grammar
- `cli.md` — compiler commands and flags
- `briefing.md` — AI-optimized compact cheat-sheet

Any change to the IR, language, or target model MUST update the corresponding
reference doc first, then propagate to code. If docs/reference/ and code
disagree, docs/reference/ wins.

## Four-Tier Namespace

```
vm.*              Compiler intrinsics       TIR ops (hash, sponge, pub_read, assert)
std.*             Real libraries            Implemented in Trident (sha256, bigint, ecdsa)
os.*              Portable runtime          os.signal, os.neuron, os.state, os.time
os.<os>.*         OS-specific APIs          os.neptune.xfield, os.solana.pda
```

Source tree:

```
src/          Compiler in Rust            Shrinks as self-hosting progresses
vm/           VM intrinsics in Trident    vm/core/, vm/io/, vm/crypto/ — source code
std/          Standard library in Trident sha256, bigint, ecdsa — source code
os/           OS bindings in Trident      Per-OS config, docs, and extensions
```

The endgame is provable compilation: the compiler self-hosts on Triton VM,
compiling Trident code and producing a STARK proof that compilation was correct.
As self-hosting progresses, `src/` (Rust) shrinks and the `.tri` tree grows.
`vm/`, `std/`, and `os/` are the Trident source directories.

Layout:

- `vm/{name}/` — per-VM directory: `target.toml` (config) + `README.md` (docs)
- `vm/core/`, `vm/io/`, `vm/crypto/` — shared VM intrinsic `.tri` source
- `os/{name}/` — per-OS directory: `target.toml` (config) + `README.md` (docs) + `.tri` extensions
- `std/` — pure Trident library code (no `#[intrinsic]`)
- Module resolution: `src/config/resolve.rs`

## Parallel Agents

When a task touches many files across the repo (bold cleanup, renaming,
cross-reference updates), split it into parallel agents with
non-overlapping file scopes. Before launching agents, partition by
directory or filename so no two agents edit the same file. Example
partitions: `docs/explanation/` vs `docs/reference/` vs `docs/guides/`
vs `os/` vs `vm/`. Never let scopes overlap — conflicting writes cause
agents to revert each other's work.

## Git Workflow

- Commit by default. After completing a change, commit it. Don't wait
  for the user to say "commit". Only stage without committing when the user
  explicitly asks to stage.
- Atomic commits. One logical change per commit. Never combine two
  independent features, fixes, or refactors in a single commit. If you
  made two separate changes, make two separate commits. Don't commit
  half-finished work either — if unsure whether the change is complete,
  ask before committing.
- Conventional commits. Use prefixes: `feat:`, `fix:`, `refactor:`,
  `docs:`, `test:`, `chore:`.

## Chain of Verification

When answering non-trivial questions or making decisions that affect
correctness (architecture, bug fixes, refactoring plans, cost models,
type system changes), follow this protocol:

1. **Initial answer.** Provide your best answer or plan.
2. **Verification questions.** Generate 3-5 questions that would expose
   errors, omissions, or wrong assumptions in your initial answer.
3. **Independent answers.** Answer each verification question
   independently — check the codebase, re-read docs, test assumptions.
4. **Revised answer.** Provide your final answer incorporating any
   corrections discovered during verification.

This applies to: design decisions, audit findings, migration plans,
bug root-cause analysis, and any claim about how the codebase works.
Skip for trivial tasks (single-line edits, formatting, obvious fixes).

## Build & Test

```
cargo check          # type-check
cargo test           # 756+ tests
cargo build --release
```

## License

Cyber License: Don't trust. Don't fear. Don't beg.
