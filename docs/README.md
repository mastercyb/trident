# 🔱 Trident Documentation

[← Project Root](../README.md)

Organized following the [Diataxis](https://diataxis.fr/) framework:
tutorials, how-to guides, reference, and explanation.

---

## 🎓 Tutorials: learning-oriented

The Builder's Journey — six chapters that build one complete private
application, from a four-line proof to a sovereign DAO.

| # | Document | You Build |
|---|----------|-----------|
| 1 | [Prove a Secret](tutorials/hello-proof.md) | A hash-locked proof — the primitive behind everything else |
| 2 | [Build a Coin](tutorials/build-a-coin.md) | A private token with pay, mint, and burn |
| 3 | [Build a Name Service](tutorials/build-a-name.md) | An ENS-like registry of unique assets |
| 4 | [Build a Liquidity Strategy](tutorials/build-a-strategy.md) | A constant-product AMM for TIDE |
| 5 | [Auction Names with Hidden Bids](tutorials/build-an-auction.md) | A Vickrey auction — sealed bids, honest pricing |
| 6 | [Upgrade to a DAO](tutorials/build-a-dao.md) | Private coin-weighted voting that governs the name service |

### Foundations

| Document | Description |
|----------|-------------|
| [Tutorial](tutorials/tutorial.md) | Language walkthrough — types, functions, modules, inline asm |

## 🔧 Guides: task-oriented

| Document | Description |
|----------|-------------|
| [Compiling a Program](guides/compiling-a-program.md) | Build, check, cost analysis |
| [Running a Program](guides/running-a-program.md) | Execute, test, debug |
| [Deploying a Program](guides/deploying-a-program.md) | Neptune scripts, multi-target deployment |
| [Generating Proofs](guides/generating-proofs.md) | Execution trace to STARK proof |
| [Verifying Proofs](guides/verifying-proofs.md) | Proof checking, on-chain verification |
| [Optimization](guides/optimization.md) | Cost reduction strategies |
| [Prompt Templates](guides/prompts.md) | AI-assisted development prompts |

## 📖 Reference: information-oriented

| Document | Description |
|----------|-------------|
| [Language Reference](reference/language.md) | Types, operators, builtins, sponge, Merkle, proof composition |
| [Grammar (EBNF)](reference/grammar.md) | Complete formal specification |
| [IR Design](reference/ir.md) | TIR operations, tiers, lowering |
| [Target Reference](reference/targets.md) | OS model, target profiles, cost models |
| [VM Reference](reference/vm.md) | Virtual machine architecture |
| [OS Reference](reference/os.md) | Operating system model |
| [Standard Library](reference/stdlib.md) | `std.*` module reference |
| [CLI Reference](reference/cli.md) | Command-line interface |
| [Error Catalog](reference/errors.md) | Every error message explained |
| [Agent Briefing](reference/briefing.md) | Compact format for AI code generation |

Per-target documentation lives alongside its config:
- [OS Registry](../os/README.md) — all 25 operating systems
- [VM Registry](../vm/README.md) — all 20 virtual machines

## 💡 Explanation: understanding-oriented

| Document | Description |
|----------|-------------|
| [Vision](explanation/vision.md) | Three revolutions, one field — why Trident exists |
| [Multi-Target Compilation](explanation/multi-target.md) | One source, every chain |
| [Programming Model](explanation/programming-model.md) | Execution model, OS abstraction, six concerns |
| [How STARK Proofs Work](explanation/stark-proofs.md) | From traces to quantum-safe proofs |
| [Provable Computing](explanation/provable-computing.md) | Comparative analysis of ZK systems |
| [Formal Verification](explanation/formal-verification.md) | Symbolic execution, SMT, invariant synthesis |
| [Content-Addressed Code](explanation/content-addressing.md) | Hashing, caching, registry, equivalence |
| [The Gold Standard](explanation/gold-standard.md) | PLUMB framework, TSP-1 (Coin), TSP-2 (Card) |
| [Skill Library](explanation/skill-library.md) | 23 composable token capabilities |
| [For Offchain Devs](explanation/for-offchain-devs.md) | Zero-knowledge from scratch |
| [For Onchain Devs](explanation/for-onchain-devs.md) | Mental model migration from Solidity/Anchor/CosmWasm |
| [Privacy](explanation/privacy.md) | The privacy trilateral: ZK + FHE + MPC over one field |
| [Quantum Computing](explanation/quantum.md) | Why prime field arithmetic is quantum-native |
| [Verifiable AI](explanation/trident-ai-zkml-deep-dive.md) | Why the next generation of zkML starts from prime fields |
| [Cyber License](explanation/cyber-license.md) | Don't trust. Don't fear. Don't beg. |

## 🗺️ Project

| Document | Description |
|----------|-------------|
| [Development Plan](ROADMAP.md) | Roadmap and status |
