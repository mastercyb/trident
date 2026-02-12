# CLI Reference

[← Language Reference](language.md)

---

```bash
# Build
trident build <file>                    # Compile to target assembly
trident build <file> --target neptune   # OS target → derives TRITON
trident build <file> --target ethereum  # OS target → derives EVM
trident build <file> --target linux     # OS target → derives x86-64
trident build <file> --target triton    # Bare VM target (no OS)
trident build <file> --target miden     # Bare VM → .masm
trident build <file> --costs            # Print cost analysis
trident build <file> --hotspots         # Top cost contributors
trident build <file> --hints            # Optimization hints (H0001-H0004)
trident build <file> --annotate         # Per-line cost annotations
trident build <file> -o <out>           # Custom output path

# Check
trident check <file>                    # Type-check only
trident check <file> --costs            # Type-check + cost analysis

# Format
trident fmt <file>                      # Format in place
trident fmt <dir>/                      # Format all .tri in directory
trident fmt <file> --check              # Check only (exit 1 if unformatted)

# Test
trident test <file>                     # Run #[test] functions

# Verify
trident verify <file>                   # Verify #[requires]/#[ensures]
trident verify <file> --z3              # Formal verification via Z3

# Docs
trident doc <file>                      # Generate documentation
trident doc <file> -o <docs.md>         # Generate to file

# Project
trident init <name>                     # Create new program project
trident init --lib <name>               # Create new library project
trident hash <file>                     # Show function content hashes
trident lsp                             # Start LSP server
```

### Target Resolution

`--target <name>` resolves as:

1. Is `<name>` an OS? → load `os/<name>.toml`, derive VM from `vm` field
2. Is `<name>` a VM? → load `vm/<name>.toml`, no OS (bare compilation)
3. Neither → error: unknown target

See [targets.md](targets.md) for the full target registry.

---

## See Also

- [Language Reference](language.md) — Core language (types, operators, statements)
- [Provable Computation](provable.md) — Hash, Merkle, extension field, proof composition
- [Standard Library](stdlib.md) — `std.*` modules
- [Grammar](grammar.md) — EBNF grammar
- [OS Reference](os.md) — OS concepts, `os.*` gold standard, extensions
- [Target Reference](targets.md) — All VMs and OSes
