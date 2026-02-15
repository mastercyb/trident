# 📖 Trident Reference

[← Documentation Index](../docs/README.md)

Canonical reference for all Trident design decisions. If reference docs
and code disagree, the reference wins.

---

## Language Reference

[language.md](language.md) — the complete language in one file, 16 sections.

### Language

| # | Section | What it covers |
|---|---------|----------------|
| 1 | [Programs and Modules](language.md#1-programs-and-modules) | `program`, `module`, `use`, visibility, project layout |
| 2 | [Types](language.md#2-types) | Field, Bool, U32, Digest, arrays, tuples, structs, widths |
| 3 | [Declarations](language.md#3-declarations) | Functions, size generics, structs, events, constants, I/O |
| 4 | [Expressions and Operators](language.md#4-expressions-and-operators) | Arithmetic, comparison, bitwise, field access, indexing |
| 5 | [Statements](language.md#5-statements) | Let, assignment, if/else, for, match, return |
| 6 | [Builtin Functions](language.md#6-builtin-functions) | I/O, field math, U32 ops, assertions, memory, hash, `os.*` |
| 7 | [Attributes](language.md#7-attributes) | `#[cfg]`, `#[test]`, `#[pure]`, `#[requires]`, `#[ensures]` |
| 8 | [Memory Model](language.md#8-memory-model) | Stack (16 slots), RAM (word-addressed), no heap |
| 9 | [Inline Assembly](language.md#9-inline-assembly) | `asm` blocks, target-tagged, stack effect annotations |
| 10 | [Events](language.md#10-events) | `event` declaration, `reveal` (public), `seal` (committed) |
| 11 | [Type Checking Rules](language.md#11-type-checking-rules) | No implicit conversions, exhaustive match, purity |
| 12 | [Permanent Exclusions](language.md#12-permanent-exclusions) | What Trident will never add, and why |

### Provable Computation

| # | Section | What it covers |
|---|---------|----------------|
| 13 | [Sponge](language.md#13-sponge) | `sponge_init`, `sponge_absorb`, `sponge_squeeze` |
| 14 | [Merkle Authentication](language.md#14-merkle-authentication) | `merkle_step`, Merkle path verification |
| 15 | [Extension Field](language.md#15-extension-field) | XField type, `*.` operator, dot-step builtins |
| 16 | [Proof Composition](language.md#16-proof-composition-tier-3) | `proof_block`, STARK-in-STARK recursion |

## Other Core Reference

| Document | Description |
|----------|-------------|
| [Grammar](grammar.md) | Complete formal grammar (EBNF) |
| [Intermediate Representation](ir.md) | TIR operations (54 ops, 4 tiers), lowering paths |
| [Target Reference](targets.md) | OS model, target profiles, cost models |

## Token Standards

| Document | Description |
|----------|-------------|
| [PLUMB Framework](plumb.md) | Shared token framework: config, auth, hooks, proof envelope, security |
| [TSP-1 — Coin](tsp1-coin.md) | Divisible asset standard. Conservation: `sum(balances) = supply` |
| [TSP-2 — Card](tsp2-card.md) | Unique asset standard. Conservation: `owner_count(id) = 1` |

## Platform Reference

| Document | Description |
|----------|-------------|
| [VM Reference](vm.md) | Virtual machine architecture and instruction sets |
| [OS Reference](os.md) | Operating system model and bindings |
| [Standard Library](stdlib.md) | `std.*` modules (field, token, nn, private, quantum, ...) |
| [Skill Reference](skills.md) | All 23 skills: spec tables, recipes, hook IDs, glossary |

Per-target specs live alongside their config:
- [OS Registry](../../os/README.md) — `os/{name}/README.md` for each of 25 OSes
- [VM Registry](../../vm/README.md) — `vm/{name}/README.md` for each of 20 VMs

## Tools

| Document | Description |
|----------|-------------|
| [CLI Reference](cli.md) | Command-line interface |
| [Agent Briefing](briefing.md) | Compact format for AI code generation |

## Error Catalog

[Error Catalog](errors.md) — all diagnostics, organized by category:

| Category | Description |
|----------|-------------|
| [Lexer](errors/lexer.md) | Tokenization errors |
| [Parser](errors/parser.md) | Syntax errors |
| [Types](errors/types.md) | Type checking errors |
| [Builtins](errors/builtins.md) | Builtin type errors |
| [Control Flow](errors/control-flow.md) | Loop, match, if/else errors |
| [Modules](errors/modules.md) | Import and module resolution |
| [Events](errors/events.md) | Event declaration and emission |
| [Annotations](errors/annotations.md) | Attribute and annotation errors |
| [Assembly](errors/assembly.md) | Inline assembly errors |
| [Size Generics](errors/size-generics.md) | Parameterized size errors |
| [Targets](errors/targets.md) | Target compatibility errors |
| [Warnings](errors/warnings.md) | Non-fatal diagnostics |
| [Hints](errors/hints.md) | Optimization suggestions |
