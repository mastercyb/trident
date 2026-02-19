# `trident audit` + Persistent Plans

## Context

Two changes requested:

1. **Persistent plans**: All signed-off design docs must live in the project repo (`.cortex/plans/`), not in ephemeral `.claude/` storage. Currently `.cortex/` is gitignored — needs to be un-ignored so plans are committed.

2. **`trident audit`**: A new default-mode execution correctness tool. Iterates all modules (same discovery as bench), compiles each to TASM, runs through trisha (execute, prove, verify). The key development tool — used during every optimization cycle to confirm nothing is broken.

The "Compile" column in `trident bench --full` currently times Rust compilation, not native Rust execution. True Rust execution needs `reference.rs` implementations (future work).

## Step 1: Persist plans in repo

### `.gitignore` change
Remove the `/.cortex/` ignore line. Add selective ignores to keep only structured content:

```
# Was: /.cortex/
# Now: only ignore scratch, keep plans/audits/reports
/.cortex/scratch/
```

### `CLAUDE.md` — update Agent Memory section
Change `.claude/` references to `.cortex/` in-repo. Plans the user signs off on get committed. Already partially edited (will finalize during implementation).

### Move current plan
Copy this plan to `.cortex/plans/audit-correctness.md` after approval, commit it.

## Step 2: Rename root `tests/` file

`tests/stdlib_crypto.rs` → `tests/audit_stdlib.rs`

Semantic rename only. These tests check compilation correctness (functions appear in TASM output) — aligns with "audit" not "test". `cargo test` auto-discovers by filename, no config change needed.

## Step 3: Extract trisha helpers from bench

Create `src/cli/trisha.rs` with shared subprocess utilities currently duplicated or only in bench:

- `pub fn trisha_available() -> bool`
- `pub struct TrishaResult { output, cycle_count, elapsed_ms }`
- `pub fn run_trisha(args: &[&str]) -> Result<TrishaResult, String>`

Update `src/cli/bench.rs` to import from `cli::trisha` instead of local definitions.
Add `pub mod trisha;` to `src/cli/mod.rs`.

## Step 4: New `trident audit` default mode

### Current `trident audit`
- Takes required `input: PathBuf`
- Does symbolic verification (SMT/Z3 constraints)
- Keep this behavior unchanged when a file argument is provided

### New default (no input arg)
When called without arguments, run execution correctness across all modules:

```
trident audit              # execution correctness — iterate all modules
trident audit <file.tri>   # symbolic audit (existing behavior, unchanged)
```

### Changes to `src/cli/audit.rs`

Make `input` optional in `AuditArgs`:
```rust
pub struct AuditArgs {
    pub input: Option<PathBuf>,  // was: PathBuf (required)
    // ... all existing flags unchanged ...
}
```

In `cmd_audit()`:
```rust
if let Some(input) = args.input {
    cmd_audit_symbolic(input, args);  // existing behavior
} else {
    cmd_audit_exec(args);             // new: execution correctness
}
```

### `cmd_audit_exec()` implementation

Discovery: find all `.tri` source files that have matching `.baseline.tasm` in `benches/` (same pattern as bench — reuse `find_baseline_files` and path resolution from bench, or extract shared discovery).

For each module:
1. **Compile**: `trident::compile_project_with_options()` → TASM string
2. **Execute**: write temp `.tasm`, `trisha run --tasm <file>` → check exit code
3. **Prove**: `trisha prove --tasm <file> --output <proof>` → check success
4. **Verify**: `trisha verify <proof>` → check success
5. Record pass/fail per stage

### Output format

```
trident audit

Module                              Compile  Execute  Prove    Verify
----------------------------------------------------------------------
os::neptune::kernel                    OK      OK      OK       OK
os::neptune::recursive                 OK      OK      OK       OK
std::crypto::auth                      OK      OK      OK       OK
std::crypto::poseidon                  OK      OK      OK       OK
...
----------------------------------------------------------------------
13/13 compile  13/13 execute  13/13 prove  13/13 verify

All modules pass.
```

On failure:
```
std::crypto::poseidon                  OK      FAIL    -        -
  execute: trisha exited with code 1: stack underflow at instruction 42
```

Exit code: 0 if all pass, 1 if any fail.

## Step 5: Update `src/main.rs`

`AuditArgs.input` changes from required to optional — clap handles this automatically. No dispatch change needed (still `Command::Audit(args) => cli::audit::cmd_audit(args)`).

## Files to modify

| File | Change |
|------|--------|
| `.gitignore` | Un-ignore `.cortex/plans/`, `.cortex/audits/`, `.cortex/reports/` |
| `CLAUDE.md` | Update Agent Memory section — plans persist in `.cortex/` |
| `tests/stdlib_crypto.rs` | Rename to `tests/audit_stdlib.rs` |
| `src/cli/mod.rs` | Add `pub mod trisha;` |
| `src/cli/trisha.rs` | **NEW**: shared trisha subprocess helpers (extracted from bench) |
| `src/cli/bench.rs` | Import `trisha::*` from `cli::trisha`, remove local definitions |
| `src/cli/audit.rs` | Make `input` optional, add `cmd_audit_exec()` |
| `src/main.rs` | No change needed (AuditArgs already handles optional) |

## Verification

1. `cargo check` — zero warnings
2. `cargo test` — all pass (renamed test file still discovered)
3. `trident audit` (no args) — execution correctness table, all 13 modules pass
4. `trident audit std/crypto/poseidon.tri` — symbolic audit still works
5. `trident bench` and `trident bench --full` — unchanged behavior
6. `.cortex/plans/` contains this plan, committed to git
