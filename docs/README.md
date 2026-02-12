# Trident Documentation

[← Project Root](../README.md)

Organized following the [Diataxis](https://diataxis.fr/) framework:
tutorials, how-to guides, reference, and explanation.

---

## Tutorials (learning-oriented)

| Document | Description |
|----------|-------------|
| [Tutorial](tutorials/tutorial.md) | Build your first program step by step |
| [For Developers](tutorials/for-developers.md) | Zero-knowledge from scratch |
| [For Blockchain Devs](tutorials/for-blockchain-devs.md) | Mental model migration from Solidity/Anchor/CosmWasm |
| [Prompt Templates](tutorials/prompts.md) | AI-assisted development prompts |

## How-to Guides (task-oriented)

| Document | Description |
|----------|-------------|
| [Writing a Program](guides/writing-a-program.md) | Source structure, modules, types |
| [Compiling a Program](guides/compiling-a-program.md) | Build, check, cost analysis |
| [Running a Program](guides/running-a-program.md) | Execute, test, debug |
| [Deploying a Program](guides/deploying-a-program.md) | Neptune scripts, multi-target deployment |
| [Generating Proofs](guides/generating-proofs.md) | Execution trace to STARK proof |
| [Verifying Proofs](guides/verifying-proofs.md) | Proof checking, on-chain verification |
| [Optimization](guides/optimization.md) | Cost reduction strategies |

## Reference (information-oriented)

| Document | Description |
|----------|-------------|
| [Language Reference](reference/language.md) | Types, operators, builtins, grammar |
| [Grammar (EBNF)](reference/grammar.md) | Complete formal specification |
| [IR Design](reference/ir.md) | TIR operations, tiers, lowering |
| [Target Reference](reference/targets.md) | OS model, target profiles, cost models |
| [VM Reference](reference/vm.md) | Virtual machine architecture |
| [OS Reference](reference/os.md) | Operating system model |
| [Standard Library](reference/stdlib.md) | `std.*` module reference |
| [Provable Computation](reference/provable.md) | Tier 2-3 operations |
| [CLI Reference](reference/cli.md) | Command-line interface |
| [Error Catalog](reference/errors.md) | Every error message explained |
| [Agent Briefing](reference/briefing.md) | Compact format for AI code generation |

Per-target documentation lives alongside its config:
- [OS Registry](../os/README.md) — all 25 operating systems
- [VM Registry](../vm/README.md) — all 20 virtual machines

## Explanation (understanding-oriented)

| Document | Description |
|----------|-------------|
| [Vision](explanation/vision.md) | Why Trident exists |
| [Multi-Target Compilation](explanation/multi-target.md) | One source, every chain |
| [OS Abstraction](explanation/os-abstraction.md) | How Trident abstracts over 25 operating systems |
| [Programming Model](explanation/programming-model.md) | Execution model and stack semantics |
| [How STARK Proofs Work](explanation/stark-proofs.md) | From traces to quantum-safe proofs |
| [Provable Computing](explanation/provable-computing.md) | Comparative analysis of ZK systems |
| [Formal Verification](explanation/formal-verification.md) | Symbolic execution, SMT, invariant synthesis |
| [Content-Addressed Code](explanation/content-addressing.md) | Hashing, caching, registry, equivalence |
| [Neptune Gold Standard](explanation/gold-standard.md) | ZK-native financial primitives |
| [Cyber License](explanation/cyber-license.md) | Don't trust. Don't fear. Don't beg. |

## Project

| Document | Description |
|----------|-------------|
| [Development Plan](ROADMAP.md) | Roadmap and status |
