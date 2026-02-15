# ⚡ Optimization Guide

Strategies for reducing the proving cost of Trident programs.

> Note: This guide focuses on the Triton VM target. Other backends have different cost profiles.

## 📊 Understanding Cost

[Triton VM](https://triton-vm.org/) proves computation correctness using [six execution tables](../explanation/stark-proofs.md#4-triton-vms-six-tables). The proving cost is determined by the tallest table, padded to the next power of two. Reducing the tallest table has the most impact; reducing a table that is already shorter than the tallest has no effect on proving cost. See [How STARK Proofs Work](../explanation/stark-proofs.md) Section 11 for the exact proving time formula.

### The Six Tables

| Table | Grows With | Typical Driver |
|-------|-----------|----------------|
| Processor | Every instruction executed | Loop iterations, function calls |
| Hash | Every `hash` instruction | [Tip5](https://eprint.iacr.org/2023/107) calls (6 rows per hash) |
| U32 | Range checks, bitwise ops | `as_u32`, `split`, `&`, `^`, `log2`, `pow` |
| Op Stack | Stack underflow handling | Deep variable access, large structs |
| RAM | Memory read/write | Array indexing, struct field access, spilling |
| Jump Stack | `call` and `return` | Function calls, if/else branches |

### Reading a Cost Report

```bash
trident build main.tri --costs
```

Output shows each function's cost across all tables. The dominant table is the one that determines the padded height. Focus optimization efforts there.

### Tracking Costs Over Time

Save a baseline and compare after changes:

```bash
trident build main.tri --save-costs before.json
# ... make changes ...
trident build main.tri --compare before.json
```

The comparison shows which functions got cheaper or more expensive.

## ⚡ Optimization Strategies

### 1. Reduce Hash Table Cost

The Hash table is often the tallest because each `hash` / [`tip5`](https://eprint.iacr.org/2023/107) call adds 6 rows.

#### Strategies

- Batch hashing: Each `tip5` call costs 6 Hash Table rows regardless of how many of its 10 inputs you actually use. Batching 3 single-value hashes into 1 call saves 12 hash rows. Pack up to 10 field elements into a single `tip5` call:

```trident
// Expensive: 3 hash calls = 18 hash rows
let h1: Digest = tip5(a, 0, 0, 0, 0, 0, 0, 0, 0, 0)
let h2: Digest = tip5(b, 0, 0, 0, 0, 0, 0, 0, 0, 0)
let h3: Digest = tip5(c, 0, 0, 0, 0, 0, 0, 0, 0, 0)

// Cheaper: 1 hash call = 6 hash rows
let h: Digest = tip5(a, b, c, 0, 0, 0, 0, 0, 0, 0)
```

- Use sponge for streaming: For more than 10 elements, use the sponge API instead of multiple `tip5` calls:

```trident
sponge_init()
sponge_absorb(e0, e1, e2, e3, e4, e5, e6, e7, e8, e9)
sponge_absorb(e10, e11, e12, e13, e14, e15, e16, e17, e18, e19)
let d: Digest = sponge_squeeze()
```

- Reduce [Merkle tree](https://en.wikipedia.org/wiki/Merkle_tree) depth: Each level costs 6 hash rows. Depth-3 = 18 hash rows per proof. Depth-4 = 24. Depth-20 = 120. If you're near a power-of-2 boundary, even one extra level can double proving cost.

### 2. Reduce Processor Table Cost

The Processor table grows with every instruction. Loops are the main contributor.

#### Strategies

- Minimize loop body size: Move invariant computations outside the loop:

```trident
// Before: constant recomputed each iteration
for i in 0..100 bounded 100 {
    let threshold: Field = compute_threshold()  // same every time
    process(i, threshold)
}

// After: compute once
let threshold: Field = compute_threshold()
for i in 0..100 bounded 100 {
    process(i, threshold)
}
```

- Reduce iteration count: If possible, restructure to need fewer iterations.

- Use tighter bounds: The `bounded` value determines the worst-case unrolling. Set it as tight as possible.

### 3. Reduce U32 Table Cost

U32 operations (range checks, bitwise ops) are relatively expensive.

#### Strategies

- Stay in Field when possible: If you don't need range-checked 32-bit arithmetic, use `Field` instead of `U32`. Field operations use the Processor table (cheap) rather than the U32 table.

```trident
// Expensive: U32 operations
let a: U32 = as_u32(pub_read())
let b: U32 = as_u32(pub_read())
let sum: U32 = a + b

// Cheaper (if range check not needed): Field operations
let a: Field = pub_read()
let b: Field = pub_read()
let sum: Field = a + b
```

- Minimize `as_u32` conversions: Each `as_u32` call uses `split` which costs U32 table rows. Convert once and reuse.

- Avoid unnecessary `split`: The `/% (divmod)` operator implicitly uses `split`. If you only need the quotient, you still pay for both.

### 4. Reduce Op Stack and RAM Table Cost

These grow with deep variable access and memory operations.

#### Strategies

- Keep hot variables shallow: Variables accessed frequently should be declared close to their use. The compiler uses LRU-based spilling -- frequently accessed variables stay on the stack, infrequent ones spill to RAM.

- Minimize struct size: Larger structs consume more stack depth per access. If a struct has fields you rarely use, consider splitting it.

- Prefer stack over RAM: Direct stack operations (dup, swap) are cheaper than RAM read/write. The compiler manages this automatically, but keeping your function's live variable count under 16 field elements avoids spilling entirely.

### 5. Reduce Jump Stack Cost

Every function call adds 2 rows (call + return) to the Jump Stack table. Every if/else branch also uses calls internally.

#### Strategies

- Inline small functions: If a function is called in a tight loop and has a small body, consider inlining it manually. The compiler does not perform automatic inlining.

- Reduce branching in loops: Each if/else inside a loop adds jump stack overhead per iteration.

```trident
// More jump stack overhead (2 calls per iteration)
for i in 0..100 bounded 100 {
    if condition {
        do_a()
    } else {
        do_b()
    }
}
```

## 💡 Compiler Hints

The compiler provides optimization hints with `--hints`:

| Hint | Meaning | Action |
|------|---------|--------|
| H0001 | Hash-dominated cost | Batch hash inputs, reduce Merkle depth |
| H0002 | Large array access pattern | Consider RAM-based access or smaller arrays |
| H0003 | Deep function call chain | Inline hot functions |
| H0004 | Stack boundary warning | Reduce live variables or struct sizes |

## 📝 Per-Line Cost Annotations

Use `--annotate` to see which lines contribute most:

```bash
trident build main.tri --annotate
```

Each line shows its cost contribution in compact form: `cc` (clock cycles), `hash`, `u32`. Lines with no cost are unmarked. Focus on the lines with the highest numbers.

## 🔥 Hotspot Analysis

Use `--hotspots` to see the top 5 most expensive functions:

```bash
trident build main.tri --hotspots
```

This immediately shows where to focus optimization efforts.

## 📋 Common Patterns

### [Merkle](https://en.wikipedia.org/wiki/Merkle_tree) Proof Verification

Merkle proofs are hash-heavy. Each level adds 6 hash rows + U32 table rows. Use the shallowest tree that fits your data:

| Depth | Hash Rows | Padded Height Impact |
|-------|-----------|---------------------|
| 1 | 6 | Negligible |
| 3 | 18 | Low (Neptune kernel default) |
| 10 | 60 | Moderate |
| 20 | 120 | High (can dominate) |
| 32 | 192 | Very high |

```trident
// Depth 3: 3 hash calls = 18 hash rows (Neptune default)
std.crypto.merkle.verify3(leaf, root, idx)

// Depth 1: 1 hash call = 6 hash rows
std.crypto.merkle.verify1(leaf, root, idx)
```

### Token Operations

For coins, the main cost drivers are:
1. Leaf hashing (computing Merkle leaves)
2. Merkle authentication (proving leaf membership)
3. State updates (new leaf computation)

Minimize the number of Merkle operations per transaction.

### Digest Comparison

Comparing digests requires comparing all 5 field elements:

```trident
// Use the std.debug.assert.digest builtin (5 equality checks)
std.debug.assert.digest(actual, expected)
```

This is cheaper than manual element-by-element comparison because it uses native `assert` instructions.

### RAM-Aware Hashing

When absorbing data that's already in RAM, prefer `sponge_absorb_mem` over reading to the stack first:

```trident
// Expensive: 10 RAM reads + sponge_absorb = 10 cc + 10 RAM rows + 6 hash rows
let a: Field = vm.io.mem.read(addr)
// ... read 9 more values ...
vm.crypto.hash.sponge_absorb(a, b, c, d, e, f, g, h, i, j)

// Cheaper: sponge_absorb_mem = 1 cc + 10 RAM rows + 6 hash rows
vm.crypto.hash.sponge_absorb_mem(addr)
```

Both cost the same 6 Hash Table rows, but `sponge_absorb_mem` saves ~10 processor cycles by avoiding individual `read_mem` instructions.

## 📋 Summary

1. Identify the dominant table with `--costs`
2. Find the expensive functions with `--hotspots`
3. Locate expensive lines with `--annotate`
4. Apply targeted optimizations for the dominant table
5. Verify improvement with `--compare`

The goal is not to minimize every table, but to bring the tallest table down. Once two tables are similar height, optimize the one that is now tallest.

## 🔗 See Also

- [Compiling a Program](compiling-a-program.md) -- Build pipeline, `--costs` / `--hotspots` flags
- [Generating Proofs](generating-proofs.md) -- How proving cost translates to proof generation time
- [IR Reference](../../reference/ir.md) -- TIR operations and lowering paths
