# Optimization Hints

[Back to Error Catalog](../errors.md)

The compiler produces hints (not errors) when it detects cost antipatterns.
These appear with `trident build --hints`.

---

### H0001: Hash table dominance

```text
hint[H0001]: hash table is 3.2x taller than processor table
```

The hash table dominates proving cost. Processor-level optimizations will
not reduce proving time.

Action: Batch data before hashing, reduce Merkle depth, use
`sponge_absorb_mem` instead of repeated `sponge_absorb`.

---

### H0002: Power-of-2 headroom

```text
hint[H0002]: padded height is 1024, but max table height is only 519
```

Significant headroom below the next power-of-2 boundary. The program could
be more complex at zero additional proving cost.

---

### H0003: Redundant range check

```text
hint[H0003]: as_u32(x) is redundant — value is already proven U32
```

A value that was already range-checked is being checked again.

Action: Remove the redundant `as_u32()` call.

---

### H0004: Loop bound waste

```text
hint[H0004]: loop in 'process' bounded 128 but iterates only 10 times
```

The declared loop bound is much larger than the actual constant iteration
count. This inflates worst-case cost analysis.

Action: Tighten the `bounded` declaration to match actual usage.

---

### H0005: Unnecessary spill (planned)

```text
hint[H0005]: variable 'x' spilled to RAM but used immediately after
  help: reorder declarations to keep frequently-used variables in the top 16 stack positions
```

The compiler's LRU spill policy pushed a variable to RAM unnecessarily.

Action: Reorder variable declarations or split large blocks into functions.

Spec: language.md Section 8 (stack: 16 elements, LRU spill to RAM).
