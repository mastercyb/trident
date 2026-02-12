# Trident Reference

[← Documentation Index](../README.md)

Canonical reference for all Trident design decisions. If reference docs
and code disagree, the reference wins.

---

## Core Reference

| Document | Description |
|----------|-------------|
| [Language](language.md) | Types, operators, builtins, syntax, memory model |
| [Grammar (EBNF)](grammar.md) | Complete formal grammar |
| [IR Design](ir.md) | TIR operations (54 ops, 4 tiers), lowering paths |
| [Target Reference](targets.md) | OS model, target profiles, cost models |

## Platform Reference

| Document | Description |
|----------|-------------|
| [VM Reference](vm.md) | Virtual machine architecture and instruction sets |
| [OS Reference](os.md) | Operating system model and bindings |
| [Standard Library](stdlib.md) | `std.*` modules (sha256, bigint, ecdsa, ...) |

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
