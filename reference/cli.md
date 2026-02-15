# ⌨️ CLI Reference

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
trident build <file> --save-costs <json>  # Save cost report to JSON
trident build <file> --compare <json>   # Compare against baseline costs
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

# Package
trident package <file>                  # Compile + hash + produce .deploy/ artifact
trident package <file> --target neptune # Package for specific OS/VM target
trident package <file> -o <dir>         # Output to custom directory
trident package <file> --verify         # Run verification before packaging
trident package <file> --dry-run        # Show what would be produced

# Deploy
trident deploy <file>                   # Compile, package, deploy to registry
trident deploy <dir>.deploy/            # Deploy pre-packaged artifact
trident deploy <file> --registry <url>  # Deploy to specific registry
trident deploy <file> --verify          # Verify before deploying
trident deploy <file> --dry-run         # Show what would be deployed

# Hash
trident hash <file>                     # Show function content hashes
trident hash <file> --full              # Show full 256-bit hashes

# View
trident view <name>                     # View a function definition
trident view <name> -i <file>           # From specific file

# Equivalence
trident equiv <file> <fn_a> <fn_b>      # Check two functions are equivalent

# Benchmarks
trident bench <dir>                     # Compare .tri vs .baseline.tasm

# Store (definitions store)
trident store add <file>                # Add definitions to codebase
trident store list                      # List all definitions
trident store lookup <hash>             # Find definition by hash
trident store diff <file>               # Show changed definitions

# Registry
trident registry serve                  # Start local registry server
trident registry publish                # Publish codebase to registry
trident registry pull <hash>            # Pull definition by hash
trident registry search <query>         # Search definitions
# Dependencies
trident deps list                       # Show declared dependencies
trident deps lock                       # Lock dependency versions
trident deps fetch                      # Download locked dependencies

# Project
trident init <name>                     # Create new program project
trident init --lib <name>               # Create new library project
trident generate <spec.tri>             # Generate scaffold from spec
trident lsp                             # Start LSP server
```

### Target Resolution

`--target <name>` resolves as:

1. Is `<name>` an OS? → load `os/<name>.toml`, derive VM from `vm` field
2. Is `<name>` a VM? → load `vm/<name>.toml`, no OS (bare compilation)
3. Neither → error: unknown target

See [targets.md](targets.md) for the full target registry.

---

## 🔗 See Also

- [Language Reference](language.md) — Types, operators, builtins, grammar, sponge, Merkle, extension field, proof composition
- [Standard Library](stdlib.md) — `std.*` modules
- [Grammar](grammar.md) — EBNF grammar
- [OS Reference](os.md) — OS concepts, `os.*` gold standard, extensions
- [Target Reference](targets.md) — All VMs and OSes
