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

## Compilation Pipeline

```
Source → Lexer → Parser → AST → TypeCheck → KIR → TIR → LIR → Target
syntax/          syntax/   ast/   typecheck/  kir/  tir/  lir/  (per-VM)
```

Changes to any stage must preserve the pipeline contract: output of
stage N is valid input for stage N+1. When modifying a stage, check
both its input (does it still accept what the previous stage emits?)
and its output (does the next stage still accept it?).

## src/ Module Map

Update this map when files are added, removed, renamed, or modules are
reorganized. Do NOT update for line count changes or content-only edits.

97 files, ~35,700 lines. Modules listed in pipeline order, then support.

**Pipeline stages:**

```
syntax/            4,347 LOC   Lexer, parser, formatter
  lexer.rs           796       Tokenizer (keywords, operators, literals)
  lexeme.rs          172       Token types and display
  parser/          2,082       Recursive descent parser
    mod.rs           183         Core parser (expect, peek, advance)
    expr.rs          280         Expression parsing (precedence climbing)
    stmts.rs         368         Statement parsing (let, if, while, for, return)
    items.rs         457         Top-level items (fn, struct, enum, impl, use)
    types.rs         109         Type annotation parsing
    tests.rs         685         Parser tests
  format/          1,293       Code formatter (trident fmt)
    mod.rs           216         Formatter core + indent tracking
    expr.rs          100         Expression formatting
    stmts.rs         333         Statement formatting
    items.rs         205         Item formatting
    tests.rs         439         Formatter tests

ast/                 603 LOC   Abstract syntax tree
  mod.rs             387       AST node definitions (Expr, Stmt, Item, Type)
  navigate.rs        101       Tree navigation helpers
  display.rs         115       Pretty-printing AST nodes

typecheck/         3,084 LOC   Type checker
  mod.rs             443       Environment, type context, entry point
  expr.rs            448       Expression type inference
  stmt.rs            606       Statement checking (assignment, control flow)
  builtins.rs        347       Built-in function signatures (vm.*)
  resolve.rs         101       Name resolution
  analysis.rs        270       Type analysis utilities
  tests.rs           869       Type checker tests

kir/                  56 LOC   Kernel IR (high-level typed IR)
  mod.rs              25       KIR definitions
  lower/mod.rs        31       KIR → TIR lowering stub

tir/               3,678 LOC   Trident IR (stack-based, target-generic)
  mod.rs             422       TIROp enum, program representation
  stack.rs           474       Stack effect tracking and validation
  builder/         2,242       AST → TIR compilation
    mod.rs           388         Builder context, function compilation
    expr.rs          351         Expression lowering
    stmt.rs          484         Statement lowering
    call.rs          290         Function call compilation
    helpers.rs       143         Shared builder utilities
    layout.rs        131         Struct field layout computation
    tests.rs         455         Builder tests
  lower/             540       TIR → target lowering
    mod.rs            23         Lowering trait definition
    triton.rs        305         TIR → Triton VM assembly
    tests.rs         212         Lowering tests

lir/                 768 LOC   Low-level IR (target-specific)
  mod.rs             609       LIR instruction set
  convert.rs         130       LIR conversion utilities
  lower/mod.rs        29       LIR lowering stub

tree/                207 LOC   Tree IR (structured control flow)
  mod.rs              33       Tree node definitions
  lower/mod.rs       174       Tree → flat lowering
```

**Support modules:**

```
config/            2,268 LOC   Project configuration
  project.rs         199       Trident.toml parsing
  resolve.rs         669       Module path resolution (vm/std/os dispatch)
  scaffold.rs        645       Project scaffolding (trident init)
  target.rs          751       Target registry (VM + OS loading)

cost/              1,966 LOC   Cost modeling
  mod.rs             449       Cost types, table definitions
  analyzer.rs        605       AST cost annotation
  report.rs          504       Cost report generation (JSON, text)
  model/             408       Per-target cost models
    mod.rs           251         Cost model trait + generic table
    triton.rs        157         Triton VM cycle costs

cli/               2,347 LOC   Command-line interface
  mod.rs             361       Arg parsing (clap), command dispatch
  build.rs           151       trident build
  check.rs            45       trident check
  bench.rs           119       trident bench
  deploy.rs          148       trident deploy
  deps.rs            144       trident deps
  doc.rs              50       trident doc
  fmt.rs              67       trident fmt
  generate.rs         51       trident generate
  hash.rs             39       trident hash
  init.rs             73       trident init
  package.rs          97       trident package
  registry.rs        172       trident registry
  store.rs           194       trident store
  test.rs             39       trident test
  verify.rs          218       trident verify
  view.rs            379       trident view (AST/TIR inspector)

package/           5,290 LOC   Package management
  store.rs         1,706       Content-addressed artifact store
  registry.rs      1,018       Package registry (publish, fetch)
  manifest.rs        856       Package manifest parsing
  hash.rs            806       Content hashing (blake3, Merkle)
  poseidon2.rs       452       Poseidon2 hash for proof-friendly addressing
  cache.rs           445       Download and build cache

verify/            5,570 LOC   Formal verification
  synthesize.rs    1,307       Theorem synthesis from Trident code
  solve.rs         1,032       Constraint solving
  equiv.rs         1,015       Equivalence checking (optimized vs original)
  sym.rs           1,005       Symbolic execution engine
  report.rs          671       Verification report generation
  smt.rs             534       SMT-LIB2 formula encoding

lsp/               1,625 LOC   Language Server Protocol
  mod.rs             397       LSP server (tower-lsp, hover, diagnostics)
  intelligence.rs    339       Go-to-definition, find-references
  builtins.rs        317       Builtin docs for hover
  util.rs            572       LSP utilities (position mapping, etc.)
```

**Root files:**

```
lib.rs             2,245       Crate root — re-exports, compile/format entry points
deploy.rs            558       Artifact deployment (copy, verify, sign)
doc.rs               302       Documentation generation (trident doc)
pipeline.rs          206       Compilation pipeline orchestration
diagnostic.rs        173       Error/warning diagnostic rendering (ariadne)
linker.rs            134       Multi-module linking
main.rs              110       Binary entry point (clap dispatch)
types.rs              77       Shared type definitions (Span re-export, etc.)
span.rs               61       Source span tracking
```

## File Size Limit

No single `.rs` file should exceed 500 lines. If it does, split it
into submodules. `lib.rs` is the only exception (re-exports).

When auditing files > 500 lines, split the audit into sections:
read the file in chunks (offset/limit), report per-section. Never
try to hold an entire large file in a single agent context.

## Forbidden Patterns

- No `HashMap` in deterministic paths — use `BTreeMap` or indexed vec
- No `println!` in library code — use the diagnostic system
- No `std::process::exit` outside `main.rs`
- No `.unwrap()` outside tests
- No floating point anywhere
- No `async` in the compilation pipeline (only in LSP and CLI)

## Builtin Sync Rule

Builtins are defined in 4 places that must stay in sync:

1. `docs/reference/language.md` (canonical)
2. `src/typecheck/` (type signatures)
3. `src/tir/` (IR lowering)
4. `src/cost/` (cost tables)

Adding or removing a builtin requires updating all 4.

## Do Not Touch

Do not modify without explicit request:

- `Cargo.toml` dependencies (minimal by design)
- `docs/reference/` structure (canonical, changes need discussion)
- `vm/*/target.toml` and `os/*/target.toml` (configuration, not code)
- `LICENSE.md`

## Parallel Agents

When a task touches many files across the repo (bold cleanup, renaming,
cross-reference updates), split it into parallel agents with
non-overlapping file scopes. Before launching agents, partition by
directory or filename so no two agents edit the same file. Never let
scopes overlap — conflicting writes cause agents to revert each other's
work.

Recommended agent partitions for full-repo work:

- `syntax/` (lexer + parser + format)
- `ast/` + `typecheck/`
- `tir/` + `kir/` + `lir/`
- `cost/` + `verify/`
- `cli/` + `deploy` + `pipeline`
- `package/` (store, registry, manifest, hash)
- `lsp/` + `doc`
- `docs/` (by subdirectory)
- `vm/` + `std/` + `os/` (.tri files)

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

## Agent Audit Workspace

Long-running parallel agents (audits, reviews, large refactors) MUST
persist their findings to `.audit/` so results survive context overflow.

Workflow:

1. **Before launching agents**, create `.audit/` if it doesn't exist.
2. **Each agent writes its report** to `.audit/<scope>.md`
   (e.g., `.audit/syntax-lexer.md`, `.audit/syntax-parser.md`).
   The report must include: file reviewed, findings (bugs, dead code,
   safety issues, inconsistencies), and suggested fixes.
3. **After all agents finish**, the main session reads `.audit/*.md`,
   summarizes findings, and applies fixes with atomic commits.
4. **Clean up** — delete `.audit/` contents after fixes are committed.

The `.audit/` directory is gitignored — it's a scratch workspace only.
This prevents losing hours of agent work to context window limits.

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

## Review Passes

Instead of "make it perfect", invoke passes by number.
Example: "Run PASS 3 and PASS 7 on this module."
When i askK make the audit - run all passed in paralel using agents and prepare plant of fixes to confirm.

### PASS 1: DETERMINISM
- No floating point anywhere — all arithmetic over Goldilocks p = 2^64 - 2^32 + 1
- No HashMap iteration (non-deterministic order) — use BTreeMap or indexed vec
- No system clock, no randomness without explicit seed
- Serialization is canonical — single valid encoding per value
- Cross-platform: same input → same state root, always

Ask: *"Find any source of non-determinism in this code."*

### PASS 2: BOUNDED LOCALITY
- Every function's read-set is O(k)-bounded — trace it
- No hidden global state access (singletons, lazy_static with mutation)
- Graph walks have explicit depth/hop limits
- State updates touch only declared write-set — no side effects beyond it
- Verify: local change cannot trigger unbounded cascade

Ask: *"What is the maximum read-set and write-set of this function? Can a local change cascade globally?"*

### PASS 3: FIELD ARITHMETIC CORRECTNESS
- All reductions are correct mod p — no overflow before reduce
- Multiplication uses proper widening (u64 → u128 → reduce)
- Inverse/division handles zero case explicitly (panic or Option)
- Batch operations maintain invariant: individual vs batch results match
- Montgomery/Barrett boundaries are correct at edge values (0, 1, p-1, p)

Ask: *"Check edge cases: 0, 1, p-1, and values near 2^64. Does reduction ever overflow?"*

### PASS 4: CRYPTO HYGIENE
- No secret-dependent branching (constant-time operations)
- No secret data in error messages, logs, or Debug impls
- Zeroize sensitive memory on drop
- Hash domain separation — every use has unique prefix/tag
- Commitment scheme: binding + hiding properties preserved
- Proof constraints are neither under-constrained (soundness hole) nor over-constrained (completeness break)

Ask: *"Is there any path where secret material leaks through timing, errors, or logs?"*

### PASS 5: TYPE SAFETY & INVARIANTS
- Newtypes for distinct domains: ParticleId != NeuronId != CyberlinkId
- States encoded in types (e.g., `Unverified<Proof>` vs `Verified<Proof>`)
- `unsafe` blocks have safety comments explaining why invariant holds
- No `.unwrap()` on fallible paths — use `?` or explicit error
- Phantom types or sealed traits prevent invalid state construction

Ask: *"Can a caller construct an invalid state? Can types from different domains be accidentally mixed?"*

### PASS 6: ERROR HANDLING & DEGRADATION
- Every error type is meaningful — no `anyhow` in library code
- Errors propagate without losing context (error chains)
- No panic in library code — only in binary entry points or proved-unreachable
- Resource cleanup on all error paths (RAII, Drop impls)
- Partial failure doesn't corrupt shared state

Ask: *"What happens when this fails halfway through? Is state still consistent?"*

### PASS 7: ADVERSARIAL INPUT
- All external inputs are validated before processing
- Sizes, lengths, indices are bounds-checked
- No allocation proportional to untrusted input without cap
- Graph operations handle: empty graph, single node, disconnected components, self-loops, duplicate edges
- Malformed proofs/signatures rejected before expensive computation

Ask: *"What's the cheapest input an attacker can craft to cause maximum damage (CPU, memory, state corruption)?"*

### PASS 8: ARCHITECTURE & COMPOSABILITY
- Module has single responsibility — one reason to change
- Dependencies point inward (domain <- application <- infra)
- Traits define boundaries — implementations are swappable
- No circular dependencies between modules
- Public API is minimal — nothing exposed without reason

Ask: *"Can I replace the implementation behind this trait without touching callers? What would break?"*

### PASS 9: READABILITY & NAMING
- Names match the whitepaper terminology exactly
- Functions do what their name says — no hidden side effects
- Complex logic has comments explaining *why*, not *what*
- Magic numbers are named constants with units in the name
- Code reads top-down — high-level flow visible without diving into helpers

Ask: *"Can someone reading only this file understand what it does and why, without reading other files?"*

### PASS 10: COMPACTNESS & ELIMINATION
- No dead code, no commented-out blocks
- No premature abstraction — if only one impl exists, don't trait it yet
- No duplicate logic — if two functions share structure, extract or explain why not
- No unnecessary allocations (clone, to_vec, collect where iter suffices)
- Ask: "what can I delete?" before "what should I add?"

Ask: *"What can be removed from this code without changing behavior?"*

### PASS 11: PERFORMANCE & SCALABILITY
- Hot path is allocation-free (pre-allocated buffers, arena allocation)
- No O(n^2) or worse without explicit justification and n-bound
- Batch operations exist for anything called in loops
- Cache-friendly access patterns (sequential over random)
- Profiled, not guessed — benchmark before and after optimization

Ask: *"What is the complexity of this at 10^9 nodes? Where does it break first?"*

### PASS 12: TESTABILITY
- Pure functions where possible — output depends only on input
- Side effects are injected (trait objects, closures), not hardcoded
- Property-based tests for invariants (proptest/quickcheck)
- Edge case tests: empty, one, max, overflow, malicious
- Test names describe the property being verified, not the method being called

Ask: *"What property should always hold? Write a proptest for it."*

### Severity Tiers

| Tier | Passes | When |
|------|--------|------|
| **Every commit** | 1, 5, 6, 9 | Determinism, types, errors, readability |
| **Every PR** | + 2, 7, 8, 10 | Locality, adversarial, architecture, compactness |
| **Every release** | + 3, 4, 11, 12 | Crypto, field math, performance, full test coverage |

## Build & Test

```
cargo check          # type-check
cargo test           # 756+ tests
cargo build --release
```

- `cargo test` must pass before committing.
- New parser/typecheck features need tests in the corresponding `tests.rs`.
- Test names describe the property, not the method
  (e.g., `nested_if_else_preserves_scope` not `test_if`).
- Snapshot tests: update with `cargo insta review`, never manually.

## License

Cyber License: Don't trust. Don't fear. Don't beg.
