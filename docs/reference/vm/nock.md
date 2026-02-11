# Nock VM — Target Profile

[← Target Registry](../targets.md)

---

## Parameters

| Parameter | Value |
|-----------|-------|
| Architecture | Tree (combinator) |
| VM | Nock (13 opcodes) |
| OS | Nockchain |
| Field | Goldilocks 64-bit (p = 2^64 - 2^32 + 1) |
| Extension field | Cubic ([Belt; 3] = Felt) |
| Hash function | Tip5 |
| Digest width | 5 field elements |
| Hash rate | 10 field elements |
| Hash rounds | 7 |
| Sponge state | 16 field elements (rate 10 + capacity 6) |
| Data model | Noun = Atom or Cell (binary tree) |
| Word size | 64-bit (tagged union u64) |
| Memory model | Bidirectional arena (NockStack) |
| Stack thread | 256 MB |
| Output format | `.jam` (serialized noun) |
| Tier support | 0-3 (full) |
| Currency | Nicks (1 NOCK = 2^16 nicks) |

## VM — The 13 Opcodes

Nock is a combinator VM. Every computation is `[subject formula] → product`.
The subject is the data; the formula is the program. Both are nouns (binary trees).

| Opcode | Name | Semantics |
|--------|------|-----------|
| 0 | Slot | `/axis subject` — tree lookup by axis |
| 1 | Constant | Ignore subject, produce constant |
| 2 | Evaluate | `[subject formula]` — recursive eval |
| 3 | Cell test | 0 if cell, 1 if atom |
| 4 | Increment | Atom + 1 |
| 5 | Equals | Structural equality test |
| 6 | Branch | `if test then else` |
| 7 | Compose | Evaluate b against result of a |
| 8 | Push | Evaluate b with `[result-of-a subject]` |
| 9 | Invoke | Pull formula from core, evaluate |
| 10 | Edit | Replace subtree at axis |
| 11 | Hint | Advisory metadata (jet matching) |
| 12 | Scry | External lookup (I/O) |

## Field Alignment

Nockchain uses the same Goldilocks field as Triton VM and Miden VM:

| Type | Definition | Width |
|------|-----------|-------|
| Belt | `u64` mod p | 1 field element |
| Felt | `[Belt; 3]` | 3 field elements (cubic extension) |
| F6lt | `[Belt; 6]` | 6 field elements (Cheetah curve) |
| Digest | `[Belt; 5]` | 5 field elements (Tip5 hash output) |

This means Trident's `Field`, `XField`, and `Digest` map directly to
Nockchain's Belt, Felt, and Digest — no emulation needed.

## Jet System

Jets are Rust-native functions that transparently replace Nock formulas
by formula hash matching. The Nock evaluator checks if a formula's hash
matches a registered jet; if so, it runs the Rust implementation instead
of interpreting the Nock tree.

| Jet Category | What it accelerates |
|-------------|---------------------|
| `BASE_FIELD_JETS` | Belt add, sub, mul, inv, neg, eq |
| `BASE_POLY_JETS` | Polynomial operations over Belt |
| `EXTENSION_FIELD_JETS` | Felt (cubic extension) arithmetic |
| `ZTD_JETS` | Tip5 hash, sponge, Merkle authentication |
| `CURVE_JETS` | Cheetah curve point operations (F6lt) |
| `ZKVM_TABLE_JETS_V2` | STARK verification table lookups |
| `XTRA_JETS` | FRI verification, STARK recursion |
| `KEYGEN_JETS` | Key generation |

## TIR Mapping

| TIR Op | Nock Translation | Jet |
|--------|-----------------|-----|
| Push(v) | `[8 [1 v] ...]` (Nock 8) | -- |
| Dup(n) | `[0 axis]` (Nock 0) | -- |
| Add | `[11 %add ...]` | BASE_FIELD_JETS |
| Mul | `[11 %mul ...]` | BASE_FIELD_JETS |
| Hash | `[11 %tip5 ...]` | ZTD_JETS |
| IfElse | `[6 test then else]` (Nock 6) | -- |
| Call | `[9 axis core]` (Nock 9) | -- |
| SpongeInit | `[11 %sinit ...]` | ZTD_JETS |
| MerkleStep | `[11 %ms ...]` | ZTD_JETS |
| ExtMul | `[11 %xmul ...]` | EXTENSION_FIELD_JETS |
| FoldExt | `[11 %fext ...]` | XTRA_JETS |

## OS — Nockchain

- **Consensus**: Bitcoin-style PoW with STARK proofs (2016-block epochs)
- **Block model**: Pages (blocks) containing transactions
- **Transaction model**: UTXO with Notes (inputs consumed, outputs created)
- **Signatures**: Schnorr over Cheetah curve (F6lt)
- **Mining**: STARK Proof-of-Work — mining IS proving
- **Transaction versions**: V0 (legacy), V1 (with zero-knowledge)
- **Framework**: NockApp (kernel + IO drivers, poke/peek/effect)

## Lowering Path

```
TIR → TreeLowering → Noun → .jam serialization
```

Tree lowering is distinct from stack and register paths. The operand stack
becomes a right-nested cons tree (the subject). Stack operations become
tree construction and axis addressing. Control flow maps to Nock 6 (branch)
and Nock 7 (compose). Function calls become Nock 9 (invoke) on cores.

Performance depends on jet matching — naive tree interpretation without
jets would be extremely slow. The lowering must produce formulas whose
hashes match the registered jets for all cryptographic operations.

---

## Cost Model (Nock reductions)

Single metric: formula evaluation steps (Nock reductions). Jet calls
count as 1 reduction regardless of internal complexity.

| Operation | Cost | Notes |
|---|---|---|
| Arithmetic (`+`, `*`, `<`) | 1 | Jetted |
| Bitwise (`&`, `^`, `/%`) | 1 | Jetted |
| `hash(...)` | 1 | Tip5 jet |
| `sponge_*()` | 1 | Tip5 jet |
| `merkle_step(...)` | 1 | Tip5 jet |
| Tree edit (RAM) | 1 | Subject mutation |
| Nock 6 (branch) | 2 | Condition + branch |
| Nock 9 (call) | 3 | Core invoke + return |

Detailed per-instruction cost model not yet implemented — jet performance
varies by runtime (Ares, Vere, Sword).

---

*See [TreeLowering](../../src/tree/lower/mod.rs) for the implementation.*
