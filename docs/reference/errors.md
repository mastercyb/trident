# Error Catalog

[← Language Reference](language.md) | [IR Reference](ir.md) | [Target Reference](targets.md)

All Trident compiler diagnostics — errors, warnings, and optimization hints.
Derived from the language specification ([language.md](language.md)), target
constraints ([targets.md](targets.md)), and IR tier rules ([ir.md](ir.md)).

This catalog is the source of truth for diagnostics. If a rule in the reference
can be violated, the error must exist here. Entries marked **(planned)** are
specification-required but not yet implemented in the compiler.

---

## Derivation Methodology

The catalog is derived systematically from the specification, not
reverse-engineered from compiler source. Every violable rule produces
at least one diagnostic entry.

### The 5-step process

1. **Extract** — Scan language.md, targets.md, and ir.md for prohibition
   keywords: "must", "cannot", "only", "requires", "forbidden", "not
   supported", "rejected", "maximum", "minimum".

2. **Classify** — Is the constraint user-violable? Internal compiler
   invariants don't need user-facing errors. Only rules that a programmer
   could break in source code qualify.

3. **Map** — Each violable constraint maps to at least one catalog entry.
   Some constraints produce multiple errors (e.g., "no subtraction"
   catches `-`, `--`, `-=`).

4. **Audit** — Gaps cluster in predictable categories:
   - Excluded-feature diagnostics (every Rust/C keyword users try)
   - Tier-gating for compound features (seal uses sponge internally)
   - Semantic domain errors (inv(0), hash rate mismatches)
   - Attribute argument validation

5. **Maintain** — When adding a language feature, add its violation modes
   to the catalog simultaneously. The spec change and the error entry
   ship together.

### Completeness claim

156 diagnostics cover every user-violable "must"/"cannot"/"only" constraint
in the language reference (language.md, provable.md, grammar.md, patterns.md),
targets.md, and ir.md. The derivation was audited by scanning all reference
documents for prohibition keywords and cross-referencing each against the
catalog.

---

## Categories

| Category | File | Total | Impl | Planned |
|----------|------|------:|-----:|--------:|
| Lexer | [lexer.md](errors/lexer.md) | 20 | 7 | 13 |
| Parser | [parser.md](errors/parser.md) | 26 | 8 | 18 |
| Type | [types.md](errors/types.md) | 34 | 24 | 10 |
| Control flow | [control-flow.md](errors/control-flow.md) | 8 | 6 | 2 |
| Size generics | [size-generics.md](errors/size-generics.md) | 6 | 4 | 2 |
| Events | [events.md](errors/events.md) | 7 | 5 | 2 |
| Annotations | [annotations.md](errors/annotations.md) | 8 | 3 | 5 |
| Module | [modules.md](errors/modules.md) | 10 | 4 | 6 |
| Target | [targets.md](errors/targets.md) | 16 | 3 | 13 |
| Builtin type | [builtins.md](errors/builtins.md) | 7 | 0 | 7 |
| Inline assembly | [assembly.md](errors/assembly.md) | 2 | 0 | 2 |
| Warnings | [warnings.md](errors/warnings.md) | 7 | 3 | 4 |
| Hints | [hints.md](errors/hints.md) | 5 | 4 | 1 |
| **Total** | | **156** | **71** | **85** |

---

## See Also

- [Language Reference](language.md) — Types, operators, builtins, grammar
- [Target Reference](targets.md) — Target profiles, cost models, and OS model
- [IR Reference](ir.md) — 54 operations, 4 tiers, lowering paths
- [Tutorial](../tutorials/tutorial.md) — Step-by-step guide with working examples
- [For Developers](../tutorials/for-developers.md) — Why bounded loops? Why no heap?
- [Optimization Guide](../guides/optimization.md) — Cost reduction strategies
