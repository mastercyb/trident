# Trident — Claude Code Instructions

## Source of Truth

`docs/reference/` is the canonical reference for all Trident design decisions.
Each file owns a specific domain:

- **`ir.md`** — TIROp variant names, counts, tier assignments, lowering paths,
  naming conventions, architecture diagrams, pipeline
- **`language.md`** — syntax, types, operators, builtins, grammar, attributes,
  memory model, type checking rules, permanent exclusions
- **`targets.md`** — OS model, target profiles, cost models, type/builtin
  availability per target, tier compatibility
- **`errors.md`** — error codes and diagnostic messages

Any change to the IR, language, or target model MUST update the corresponding
reference doc first, then propagate to code. If docs/reference/ and code
disagree, docs/reference/ wins.

## Build & Test

```
cargo check          # type-check
cargo test           # 731+ tests
cargo build --release
```

## License

Cyber License: Don't trust. Don't fear. Don't beg.
