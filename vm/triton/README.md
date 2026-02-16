# 🔱 TRITON

[← Target Reference](../../reference/targets.md)

---

## Parameters

| Parameter | Value |
|---|---|
| Architecture | Stack |
| Field | Goldilocks (p = 2^64 - 2^32 + 1) |
| Field bits | 64 |
| Hash function | Tip5 |
| Digest width | 5 field elements |
| Hash rate | 10 field elements |
| Extension field | Cubic (degree 3) |
| Stack depth | 16 |
| Output format | `.tasm` |
| Cost model | 6 tables: processor, hash, u32, op_stack, ram, jump_stack |
| OS | Neptune |

The primary target. Tip5 is the native hash: 5 rounds per permutation,
6 hash table rows per hash operation.

---

## Native Instructions

Has native Merkle step instructions (`merkle_step`, `merkle_step_mem`), native
extension field arithmetic (`xx_dot_step`, `xb_dot_step`), and native U32
coprocessor (`lt`, `and`, `xor`, `div_mod`, `split`, `pow`, `log_2_floor`,
`pop_count`).

---

## Cost Model (6 tables)

Each instruction contributes rows to multiple tables simultaneously.
Proving cost is determined by the tallest table, not the sum.

| Table | What grows it | Rows per trigger |
|---|---|---|
| Processor | Every instruction | 1 per instruction |
| Hash | `hash`, `sponge_*`, `merkle_step*` + program attestation | 6 per hash op |
| U32 | `split`, `lt`, `and`, `xor`, `div_mod`, `pow`, `log_2_floor`, `pop_count` | Variable (worst-case 33) |
| Op Stack | Stack depth changes | 1 per stack op |
| RAM | `read_mem`, `write_mem`, `sponge_absorb_mem`, `xx_dot_step`, `xb_dot_step` | 1 per word |
| Jump Stack | `call`, `return` | 1 per jump |

The padded height is `2^ceil(log2(max_table_height))`. Crossing a power-of-2
boundary doubles proving time.

### Per-Instruction Costs

| Trident construct | Processor | Hash | U32 | OpStack | RAM |
|---|---:|---:|---:|---:|---:|
| `a + b` | 1 | 0 | 0 | 1 | 0 |
| `a * b` | 1 | 0 | 0 | 1 | 0 |
| `inv(a)` | 1 | 0 | 0 | 0 | 0 |
| `a == b` | 1 | 0 | 0 | 1 | 0 |
| `a < b` | 1 | 0 | 33 | 1 | 0 |
| `a & b` | 1 | 0 | 33 | 1 | 0 |
| `a ^ b` | 1 | 0 | 33 | 1 | 0 |
| `split(a)` | 1 | 0 | 33 | 1 | 0 |
| `a /% b` | 1 | 0 | 33 | 0 | 0 |
| `pow(b, e)` | 1 | 0 | 33 | 1 | 0 |
| `log2(a)` | 1 | 0 | 33 | 0 | 0 |
| `popcount(a)` | 1 | 0 | 33 | 0 | 0 |
| `hash(...)` | 1 | 6 | 0 | 1 | 0 |
| `sponge_init()` | 1 | 6 | 0 | 0 | 0 |
| `sponge_absorb(...)` | 1 | 6 | 0 | 1 | 0 |
| `sponge_squeeze()` | 1 | 6 | 0 | 1 | 0 |
| `sponge_absorb_mem(p)` | 1 | 6 | 0 | 1 | 10 |
| `merkle_step(i, d)` | 1 | 6 | 33 | 0 | 0 |
| `merkle_step_mem(...)` | 1 | 6 | 33 | 0 | 5 |
| `hint()` | 1 | 0 | 0 | 1 | 0 |
| `pub_read()` | 1 | 0 | 0 | 1 | 0 |
| `pub_write(v)` | 1 | 0 | 0 | 1 | 0 |
| `ram_read(addr)` | 2 | 0 | 0 | 2 | 1 |
| `ram_write(addr, v)` | 2 | 0 | 0 | 2 | 1 |
| `xx_dot_step(...)` | 1 | 0 | 0 | 0 | 6 |
| `xb_dot_step(...)` | 1 | 0 | 0 | 0 | 4 |
| `assert(x)` | 1 | 0 | 0 | 1 | 0 |
| `assert_digest(a, b)` | 2 | 0 | 0 | 2 | 0 |
| fn call+return | 2 | 0 | 0 | 0 | 2 |
| if/else overhead | 3 | 0 | 0 | 2 | 0 |
| for-loop overhead | 8 | 0 | 0 | 4 | 0 |

U32 table rows depend on operand bit-width; 33 is the worst-case (32-bit).

---

## Optimization Hints

The compiler provides actionable suggestions:

| Hint | Pattern | Suggestion |
|---|---|---|
| H0001 | Hash table dominates | Batch data before hashing, reduce Merkle depth |
| H0002 | Near power-of-2 boundary | Room for more complexity at zero cost |
| H0003 | Redundant range check | Remove duplicate `as_u32()` |
| H0004 | Loop bound waste | Tighten `bounded N` declaration |

---

## Cost CLI

```nu
trident build --costs                   # Cost report
trident build --hotspots                # Top cost contributors
trident build --hints                   # Optimization hints
trident build --annotate                # Per-line cost annotations
trident build --save-costs costs.json   # Save for comparison
trident build --compare costs.json      # Diff with previous build
```
