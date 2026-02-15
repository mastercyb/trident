# Trident — Claude Code Instructions

## Source of Truth

`reference/` is the canonical reference for all Trident design decisions.
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
reference doc first, then propagate to code. If reference/ and code
disagree, reference/ wins.

## Four-Tier Namespace

```
vm.*              Compiler intrinsics       TIR ops (hash, sponge, pub_read, assert)
std.*             Real libraries            Implemented in Trident (token, coin, card, skill, crypto)
os.*              Portable runtime          os.signal, os.neuron, os.state, os.time
os.<os>.*         OS-specific APIs          os.neptune.xfield, os.solana.pda
```

Source tree:

```
src/          Compiler in Rust            Shrinks as self-hosting progresses
vm/           VM intrinsics in Trident    vm/core/, vm/io/, vm/crypto/ — source code
std/          Standard library in Trident token, coin, card, skill, crypto, io — source code
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
- Module resolution: `src/config/resolve/`

## Compilation Pipeline

```
Source → Lexer → Parser → AST → TypeCheck → KIR → TIR → LIR → Target
syntax/          syntax/   ast/   typecheck/       ir/          (per-VM)
```

Changes to any stage must preserve the pipeline contract: output of
stage N is valid input for stage N+1. When modifying a stage, check
both its input (does it still accept what the previous stage emits?)
and its output (does the next stage still accept it?).

## src/ Module Map

Update this map when files are added, removed, renamed, or modules are
reorganized. Do NOT update for line count changes or content-only edits.

~153 files, ~36k lines. Modules listed in pipeline order, then support.

**Pipeline stages:**

```
syntax/            ~4.5k LOC   Lexer, parser, formatter, spans
  span.rs             ~60       Source span tracking (Span, Spanned<T>)
  lexer/             ~800       Tokenizer
    mod.rs           ~440         Keywords, operators, literals
    tests.rs         ~360         Lexer tests
  lexeme.rs          ~170       Token types and display
  parser/          ~2.1k       Recursive descent parser
    mod.rs           ~180         Core parser (expect, peek, advance)
    expr.rs          ~280         Expression parsing (precedence climbing)
    stmts.rs         ~370         Statement parsing (let, if, while, for, return)
    items.rs         ~460         Top-level items (fn, struct, enum, impl, use)
    types.rs         ~110         Type annotation parsing
    tests/           ~700         Parser tests
      basics.rs      ~340           Core parsing tests
      advanced.rs    ~350           Error recovery, edge cases
  format/          ~1.3k       Code formatter (trident fmt)
    mod.rs           ~220         Formatter core + indent tracking
    expr.rs          ~100         Expression formatting
    stmts.rs         ~330         Statement formatting
    items.rs         ~200         Item formatting
    tests.rs         ~440         Formatter tests

ast/                 ~600 LOC   Abstract syntax tree
  mod.rs             ~390       AST node definitions (Expr, Stmt, Item, Type)
  navigate.rs        ~100       Tree navigation helpers
  display.rs         ~120       Pretty-printing AST nodes

typecheck/         ~3.2k LOC   Type checker
  mod.rs             ~440       Environment, type context, entry point
  types.rs            ~80       Semantic types (Ty, StructTy, width)
  expr.rs            ~450       Expression type inference
  stmt.rs            ~420       Statement checking (assignment, control flow)
  block.rs           ~200       Block, fn body, event, place checking
  builtins.rs        ~350       Built-in function signatures (vm.*)
  resolve.rs         ~100       Name resolution
  analysis.rs        ~270       Type analysis utilities
  tests/             ~880       Type checker tests
    basics.rs        ~450         Core type checking tests
    advanced.rs      ~440         Error messages, edge cases

ir/                ~4.9k LOC   Intermediate representations
  mod.rs              ~15       Module declarations
  kir/                ~60       Kernel IR (high-level typed IR)
    mod.rs             ~25         KIR definitions
    lower/mod.rs       ~30         KIR → TIR lowering stub
  tir/             ~3.8k       Trident IR (stack-based, target-generic)
    mod.rs            ~420         TIROp enum, program representation
    linker.rs         ~130         Multi-module TASM linking
    stack.rs          ~470         Stack effect tracking and validation
    builder/        ~2.2k         AST → TIR compilation
      mod.rs          ~390           Builder context, function compilation
      expr.rs         ~350           Expression lowering
      stmt.rs         ~480           Statement lowering
      call.rs         ~290           Function call compilation
      helpers.rs      ~140           Shared builder utilities
      layout.rs       ~130           Struct field layout computation
      tests.rs        ~460           Builder tests
    lower/            ~540         TIR → target lowering
      mod.rs           ~25           Lowering trait definition
      triton.rs       ~300           TIR → Triton VM assembly
      tests.rs        ~210           Lowering tests
  lir/               ~770       Low-level IR (register targets)
    mod.rs            ~390         LIR instruction set
    convert.rs        ~130         LIR conversion utilities
    tests.rs          ~220         LIR tests
    lower/mod.rs       ~30         LIR lowering stub
  tree/              ~210       Tree IR (tree targets)
    mod.rs             ~35         Tree node definitions
    lower/mod.rs      ~175         Tree → flat lowering
```

**Support modules:**

```
config/            ~2.3k LOC   Project configuration
  project.rs         ~200       Trident.toml parsing
  resolve/           ~670       Module path resolution (vm/std/os dispatch)
    mod.rs           ~190         Entry point, imports
    resolver.rs      ~380         ModuleResolver, scan_module_header
    tests.rs         ~100         Resolution tests
  scaffold/          ~650       Project scaffolding (trident init)
    mod.rs           ~400         Scaffold logic
    tests.rs         ~240         Scaffold tests
  target/            ~760       Target registry (VM + OS loading)
    mod.rs           ~340         TargetConfig, Arch, VM loading
    os.rs            ~190         OsConfig, ResolvedTarget
    tests.rs         ~220         Target tests

cost/              ~2.0k LOC   Cost modeling
  mod.rs             ~450       Cost types, table definitions
  analyzer.rs        ~340       AST cost annotation
  visit.rs           ~270       Per-expression cost visitor
  report.rs          ~220       Cost report rendering
  json.rs            ~290       JSON serialization, comparison
  model/             ~410       Per-target cost models
    mod.rs           ~250         Cost model trait + generic table
    triton.rs        ~160         Triton VM cycle costs

cli/               ~2.3k LOC   Command-line interface
  mod.rs             ~360       Arg parsing (clap), command dispatch
  build.rs           ~150       trident build
  check.rs            ~45       trident check
  bench.rs           ~120       trident bench
  deploy.rs          ~150       trident deploy
  deps.rs            ~140       trident deps
  doc.rs              ~50       trident doc
  fmt.rs              ~70       trident fmt
  generate.rs         ~50       trident generate
  hash.rs             ~40       trident hash
  init.rs             ~75       trident init
  package.rs          ~100      trident package
  registry.rs        ~170       trident atlas
  store.rs           ~190       trident store
  test.rs             ~40       trident test
  verify.rs          ~220       trident verify
  view.rs            ~380       trident view (AST/TIR inspector)

package/           ~5.3k LOC   Package management
  store/           ~1,700       Content-addressed artifact store
    mod.rs           ~470         Store API, put/get/list
    format.rs        ~400         TOML serialization
    persist.rs       ~310         Filesystem persistence
    deps.rs          ~140         Dependency tracking
    tests.rs         ~390         Store tests
  registry/        ~1,000       Package registry (publish, fetch)
    mod.rs            ~20         Module wiring
    client.rs        ~330         HTTP registry client
    json.rs          ~320         JSON encoding/decoding
    types.rs          ~70         Wire format types
    store_integration.rs ~130     Registry ↔ store bridge
    tests.rs         ~160         Registry tests
  manifest/          ~860       Package manifest parsing
    mod.rs            ~60         Data types (Dependency, Lockfile)
    parse.rs          ~100        TOML parsing helpers
    lockfile.rs       ~80         Lockfile read/write
    resolve.rs       ~220         Dependency resolution + caching
    tests.rs         ~400         Manifest tests
  hash/              ~800       Content hashing (blake3, Merkle)
    mod.rs           ~100         ContentHash, hash_source
    serialize.rs     ~370         Canonical serialization
    tests.rs         ~100         Hash tests
  poseidon2.rs       ~450       Poseidon2 hash for proof-friendly addressing
  cache.rs           ~450       Download and build cache

verify/            ~5.6k LOC   Formal verification
  synthesize/      ~1,300       Theorem synthesis from Trident code
    mod.rs           ~270         Entry point, data structures
    templates.rs     ~380         Pattern matching (accum, monotonic)
    infer.rs         ~350         CEGIS refinement, postcondition inference
    tests.rs         ~330         Synthesis tests
  solve/           ~1,000       Constraint solving
    mod.rs           ~450         Verification report, combined verification
    eval.rs          ~220         Field arithmetic, evaluator
    solver.rs        ~250         SolverConfig, bounded model checking
    tests.rs         ~130         Solver tests
  equiv/           ~1,000       Equivalence checking
    mod.rs           ~290         Entry point, hash equiv, signatures
    polynomial.rs    ~240         Polynomial normalization
    differential.rs  ~190         Differential testing
    tests.rs         ~300         Equivalence tests
  sym/             ~1,000       Symbolic execution engine
    mod.rs           ~350         SymValue, constraints, analysis
    executor.rs      ~320         Statement execution
    expr.rs          ~210         Expression evaluation
    tests.rs         ~130         Symbolic tests
  report/            ~670       Verification report generation
    mod.rs           ~500         Report formatting
    tests.rs         ~170         Report tests
  smt/               ~530       SMT-LIB2 formula encoding
    mod.rs           ~460         SMT encoding
    tests.rs          ~75         SMT tests

lsp/               ~1.6k LOC   Language Server Protocol
  mod.rs             ~400       LSP server (tower-lsp, hover, diagnostics)
  intelligence.rs    ~340       Go-to-definition, find-references
  builtins.rs        ~320       Builtin docs for hover
  util/              ~570       LSP utilities
    mod.rs           ~230         Position mapping, etc.
    tests.rs         ~340         Utility tests
```

**Public API:**

```
api/               ~2.7k LOC   Public API functions
  mod.rs             ~340       CompileOptions, compile/check/format entry points
  pipeline.rs        ~210       Shared resolve → parse → typecheck pipeline
  doc.rs             ~300       Documentation generation (trident doc)
  tools.rs           ~260       Cost, docs, verify, annotate entry points
  tests/           ~1,550       Integration tests (7 files)
    compile.rs       ~400         Single-file + project compilation
    check.rs         ~130         Type-check, parse, discovery
    format.rs        ~120         Formatting roundtrips
    cost.rs          ~180         Cost analysis, annotation, JSON
    features.rs      ~270         Generics, cfg, match, pure
    docs.rs          ~120         generate_docs
    neptune.rs       ~360         Neptune programs, proofs, XField
```

**Root files:**

```
lib.rs              ~75        Crate root — module decls, re-exports, parse helpers
diagnostic/        ~170        Error/warning diagnostic rendering
  mod.rs           ~170          Diagnostic, Severity, ariadne rendering

deploy/            ~560        Artifact deployment
  mod.rs           ~390          Copy, verify, sign
  tests.rs         ~170          Deploy tests

main.rs            ~110        Binary entry point (clap dispatch)
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

## Writing Style

Never use "This is not X. It is Y." or "X is not Y — it is Z."
formulations. State what something is directly. Say "Privacy is a
requirement" instead of "Privacy is not a feature. It is a requirement."

## Builtin Sync Rule

Builtins are defined in 4 places that must stay in sync:

1. `reference/language.md` (canonical)
2. `src/typecheck/` (type signatures)
3. `src/tir/` (IR lowering)
4. `src/cost/` (cost tables)

Adding or removing a builtin requires updating all 4.

## Do Not Touch

Do not modify without explicit request:

- `Cargo.toml` dependencies (minimal by design)
- `reference/` structure (canonical, changes need discussion)
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
- `ir/` (kir, tir, lir, tree)
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
