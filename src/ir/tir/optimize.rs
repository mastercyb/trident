/// TIR peephole optimizer.
///
/// Runs pattern-based rewrites on Vec<TIROp> to reduce instruction count.
/// Applied between TIR building and lowering to target assembly.
use super::TIROp;

/// Apply all peephole optimizations until no more changes occur.
pub(crate) fn optimize(ops: Vec<TIROp>) -> Vec<TIROp> {
    let mut ir = ops;
    loop {
        let before = ir.len();
        ir = merge_hints(ir);
        ir = merge_pops(ir);
        ir = eliminate_nops(ir);
        ir = eliminate_dead_spills(ir);
        ir = eliminate_dup_pop_nops(ir);
        ir = eliminate_double_swaps(ir);
        ir = collapse_swap_pop_chains(ir);
        ir = collapse_epilogue_cleanup(ir);
        ir = optimize_nested(ir);
        if ir.len() == before {
            break;
        }
    }
    ir
}

/// Merge consecutive Hint(a), Hint(b) → Hint(a+b).
/// Turns 10× `divine 1` into 1× `divine 10`.
fn merge_hints(ops: Vec<TIROp>) -> Vec<TIROp> {
    let mut out: Vec<TIROp> = Vec::with_capacity(ops.len());
    let mut i = 0;
    while i < ops.len() {
        if let TIROp::Hint(n) = &ops[i] {
            let mut total = *n;
            let mut j = i + 1;
            while j < ops.len() {
                if let TIROp::Hint(m) = &ops[j] {
                    total += m;
                    j += 1;
                } else {
                    break;
                }
            }
            out.push(TIROp::Hint(total));
            i = j;
        } else {
            out.push(ops[i].clone());
            i += 1;
        }
    }
    out
}

/// Merge consecutive Pop(a), Pop(b) → Pop(a+b), capped at 5 per instruction.
fn merge_pops(ops: Vec<TIROp>) -> Vec<TIROp> {
    let mut out: Vec<TIROp> = Vec::with_capacity(ops.len());
    let mut i = 0;
    while i < ops.len() {
        if let TIROp::Pop(n) = &ops[i] {
            let mut total = *n;
            let mut j = i + 1;
            while j < ops.len() {
                if let TIROp::Pop(m) = &ops[j] {
                    total += m;
                    j += 1;
                } else {
                    break;
                }
            }
            // Emit in batches of 5 (Triton VM limit)
            while total > 0 {
                let batch = total.min(5);
                out.push(TIROp::Pop(batch));
                total -= batch;
            }
            i = j;
        } else {
            out.push(ops[i].clone());
            i += 1;
        }
    }
    out
}

/// Remove no-op instructions: Swap(0), Pop(0).
fn eliminate_nops(ops: Vec<TIROp>) -> Vec<TIROp> {
    ops.into_iter()
        .filter(|op| !matches!(op, TIROp::Swap(0) | TIROp::Pop(0)))
        .collect()
}

/// Eliminate dead spills and dead stores.
///
/// Two patterns are handled:
///
/// 1. **Spill/reload pairs** — address written once and read once:
///    Write: `Push(addr), Swap(1), WriteMem(1), Pop(1)` → removed
///    Read:  `Push(addr), ReadMem(1), Pop(1)` → removed
///    The value stays on the stack instead of round-tripping through RAM.
///
/// 2. **Dead stores** — address written but never read:
///    `Push(addr), Swap(1), WriteMem(1), Pop(1)` → `Pop(1)`
///    The value was going to be discarded into RAM; just pop it.
///    Also handles: `Swap(D), Push(addr), Swap(1), WriteMem(1), Pop(1)` → `Swap(D), Pop(1)`
fn eliminate_dead_spills(ops: Vec<TIROp>) -> Vec<TIROp> {
    use std::collections::BTreeMap;

    // First pass: count writes and reads per address.
    let mut write_addrs: BTreeMap<u64, usize> = BTreeMap::new();
    let mut read_addrs: BTreeMap<u64, usize> = BTreeMap::new();

    for window in ops.windows(4) {
        if let (TIROp::Push(addr), TIROp::Swap(1), TIROp::WriteMem(1), TIROp::Pop(1)) =
            (&window[0], &window[1], &window[2], &window[3])
        {
            *write_addrs.entry(*addr).or_insert(0) += 1;
        }
    }
    for window in ops.windows(3) {
        if let (TIROp::Push(addr), TIROp::ReadMem(1), TIROp::Pop(1)) =
            (&window[0], &window[1], &window[2])
        {
            *read_addrs.entry(*addr).or_insert(0) += 1;
        }
    }

    // Classify addresses:
    // - "pair": 1 write, 1 read → remove both (value stays on stack)
    // - "dead": writes only, 0 reads → replace write with pop (discard value)
    let mut pair_addrs: std::collections::BTreeSet<u64> = std::collections::BTreeSet::new();
    let mut dead_addrs: std::collections::BTreeSet<u64> = std::collections::BTreeSet::new();

    for (addr, wc) in &write_addrs {
        let rc = read_addrs.get(addr).copied().unwrap_or(0);
        if *wc == 1 && rc == 1 {
            pair_addrs.insert(*addr);
        } else if rc == 0 {
            dead_addrs.insert(*addr);
        }
    }

    if pair_addrs.is_empty() && dead_addrs.is_empty() {
        return ops;
    }

    // Second pass: rewrite.
    let mut out: Vec<TIROp> = Vec::with_capacity(ops.len());
    let mut i = 0;
    while i < ops.len() {
        // Check for write pattern: Push(addr), Swap(1), WriteMem(1), Pop(1)
        if i + 3 < ops.len() {
            if let (TIROp::Push(addr), TIROp::Swap(1), TIROp::WriteMem(1), TIROp::Pop(1)) =
                (&ops[i], &ops[i + 1], &ops[i + 2], &ops[i + 3])
            {
                if pair_addrs.contains(addr) {
                    i += 4; // remove entirely (value stays on stack)
                    continue;
                }
                if dead_addrs.contains(addr) {
                    // Value is on top, replace write with pop to discard it
                    out.push(TIROp::Pop(1));
                    i += 4;
                    continue;
                }
            }
        }
        // Check for read pattern: Push(addr), ReadMem(1), Pop(1)
        if i + 2 < ops.len() {
            if let (TIROp::Push(addr), TIROp::ReadMem(1), TIROp::Pop(1)) =
                (&ops[i], &ops[i + 1], &ops[i + 2])
            {
                if pair_addrs.contains(addr) {
                    i += 3; // remove entirely
                    continue;
                }
            }
        }
        out.push(ops[i].clone());
        i += 1;
    }
    out
}

/// Eliminate `Dup(0); Pop(1)` and `Dup(0); Swap(1); Pop(1)` no-ops.
///
/// `dup 0; pop 1` duplicates the top element then immediately discards it.
/// `dup 0; swap 1; pop 1` copies top, swaps with element below, pops — net
/// effect is identity (the original value below is replaced by an identical copy).
fn eliminate_dup_pop_nops(ops: Vec<TIROp>) -> Vec<TIROp> {
    let mut out: Vec<TIROp> = Vec::with_capacity(ops.len());
    let mut i = 0;
    while i < ops.len() {
        // Pattern: Dup(0), Swap(1), Pop(1) → skip all three
        if i + 2 < ops.len() {
            if let (TIROp::Dup(0), TIROp::Swap(1), TIROp::Pop(1)) =
                (&ops[i], &ops[i + 1], &ops[i + 2])
            {
                i += 3;
                continue;
            }
        }
        // Pattern: Dup(0), Pop(1) → skip both
        if i + 1 < ops.len() {
            if let (TIROp::Dup(0), TIROp::Pop(1)) = (&ops[i], &ops[i + 1]) {
                i += 2;
                continue;
            }
        }
        out.push(ops[i].clone());
        i += 1;
    }
    out
}

/// Eliminate consecutive `Swap(N); Swap(N)` pairs (double swap is identity).
fn eliminate_double_swaps(ops: Vec<TIROp>) -> Vec<TIROp> {
    let mut out: Vec<TIROp> = Vec::with_capacity(ops.len());
    let mut i = 0;
    while i < ops.len() {
        if i + 1 < ops.len() {
            if let (TIROp::Swap(a), TIROp::Swap(b)) = (&ops[i], &ops[i + 1]) {
                if a == b {
                    i += 2;
                    continue;
                }
            }
        }
        out.push(ops[i].clone());
        i += 1;
    }
    out
}

/// Collapse `swap D; pop 1` chains used for stack cleanup.
///
/// Pattern 1: `swap 1; pop 1; return` means the top element is the return value
/// and the element below it is garbage. This is already minimal (2 instructions).
///
/// Pattern 2: Multiple consecutive `swap D; pop 1` pairs with decreasing D
/// right before `return` — these remove locals from below the return value.
/// When the return value width is 1 and all elements below it are being removed,
/// we can sometimes replace the entire chain with `swap N; pop N` followed by return.
///
/// Pattern 3: `dup D; dup D; ... (K times); swap K; pop K` — this duplicates
/// K elements from depth D, then removes the originals. If the originals aren't
/// needed after, this is just copying. When the dups reference a contiguous block
/// that is immediately popped, the net effect is a no-op (elements stay in place).
fn collapse_swap_pop_chains(ops: Vec<TIROp>) -> Vec<TIROp> {
    let mut out: Vec<TIROp> = Vec::with_capacity(ops.len());
    let mut i = 0;
    while i < ops.len() {
        // Pattern: dup D, dup D, ..., dup D (N times), swap N, pop N₁, pop N₂, ...
        // where the total popped equals N and D == N-1.
        // This is "extract copy of block at depth D, discard original."
        // Net: the N elements stay on the stack without the dup+pop round trip.
        if let TIROp::Dup(d) = &ops[i] {
            let d_val = *d;
            // Count consecutive dup D instructions with the same D value.
            let mut dup_count = 0u32;
            let mut j = i;
            while j < ops.len() {
                if let TIROp::Dup(dd) = &ops[j] {
                    if *dd == d_val {
                        dup_count += 1;
                        j += 1;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
            // After the dups, check for swap(dup_count) followed by pop totaling dup_count.
            if dup_count >= 2 && j < ops.len() {
                if let TIROp::Swap(s) = &ops[j] {
                    if *s == dup_count {
                        let after_swap = j + 1;
                        let mut total_popped = 0u32;
                        let mut k = after_swap;
                        while k < ops.len() {
                            if let TIROp::Pop(p) = &ops[k] {
                                total_popped += p;
                                k += 1;
                                if total_popped >= dup_count {
                                    break;
                                }
                            } else {
                                break;
                            }
                        }
                        if total_popped == dup_count && d_val + 1 == dup_count {
                            // The dup+swap+pop sequence is a no-op: elements are
                            // already in the right position. Skip everything.
                            i = k;
                            continue;
                        }
                    }
                }
            }
        }
        out.push(ops[i].clone());
        i += 1;
    }
    out
}

/// Collapse sequential `Swap(N); Pop(1)` cleanup chains.
///
/// Two sub-patterns are handled:
///
/// **Constant-depth chains**: N consecutive `swap 1; pop 1` pairs each remove
/// one element below the top. Net effect: keep top, discard N elements below.
/// Collapsed to `swap min(N,15); pop min(N,15)` in chunks (swap max is 15).
///
/// **Decreasing-depth chains**: `swap D; pop 1; swap D-1; pop 1; ...` chains
/// where each pair brings a deeper dead element to the top. Collapsed to
/// `swap first_D; pop count`.
fn collapse_epilogue_cleanup(ops: Vec<TIROp>) -> Vec<TIROp> {
    let mut out: Vec<TIROp> = Vec::with_capacity(ops.len());
    let mut i = 0;
    while i < ops.len() {
        if i + 3 < ops.len() {
            if let (TIROp::Swap(d), TIROp::Pop(1)) = (&ops[i], &ops[i + 1]) {
                let first_d = *d;

                // Count consecutive swap(D); pop(1) pairs.
                let mut count = 1u32;
                let mut is_constant_depth = true;
                let mut j = i + 2;
                while j + 1 < ops.len() {
                    if let (TIROp::Swap(dd), TIROp::Pop(1)) = (&ops[j], &ops[j + 1]) {
                        if *dd == first_d {
                            // Same depth — constant-depth chain continues.
                            count += 1;
                            j += 2;
                        } else if first_d == 1 {
                            // Constant-depth with D=1 is strict.
                            break;
                        } else if *dd + count == first_d || *dd < first_d {
                            // Decreasing-depth chain.
                            is_constant_depth = false;
                            count += 1;
                            j += 2;
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }

                if count >= 3 {
                    if first_d == 1 {
                        // Constant-depth: keep top, discard `count` elements
                        // below. Emit in chunks of 15 (max swap depth).
                        let mut remaining = count;
                        while remaining > 0 {
                            let chunk = remaining.min(15);
                            out.push(TIROp::Swap(chunk));
                            let mut pop_left = chunk;
                            while pop_left > 0 {
                                let batch = pop_left.min(5);
                                out.push(TIROp::Pop(batch));
                                pop_left -= batch;
                            }
                            remaining -= chunk;
                        }
                    } else if is_constant_depth {
                        // Constant-depth with D > 1: removing `count`
                        // dead elements from below a D-wide return value.
                        // When D + count - 1 <= 15, bring all dead elements
                        // to the top with decreasing swaps, then batch pop.
                        let total_depth = first_d + count - 1;
                        if total_depth <= 15 {
                            for offset in 0..count {
                                out.push(TIROp::Swap(total_depth - offset));
                            }
                        } else {
                            for _ in 0..count {
                                out.push(TIROp::Swap(first_d));
                            }
                        }
                        let mut remaining = count;
                        while remaining > 0 {
                            let batch = remaining.min(5);
                            out.push(TIROp::Pop(batch));
                            remaining -= batch;
                        }
                    } else {
                        // Decreasing-depth chain.
                        out.push(TIROp::Swap(first_d));
                        let mut remaining = count;
                        while remaining > 0 {
                            let batch = remaining.min(5);
                            out.push(TIROp::Pop(batch));
                            remaining -= batch;
                        }
                    }
                    i = j;
                    continue;
                }
            }
        }
        out.push(ops[i].clone());
        i += 1;
    }
    out
}

/// Recursively optimize nested bodies (IfElse, IfOnly, Loop, ProofBlock).
fn optimize_nested(ops: Vec<TIROp>) -> Vec<TIROp> {
    ops.into_iter()
        .map(|op| match op {
            TIROp::IfElse {
                then_body,
                else_body,
            } => TIROp::IfElse {
                then_body: optimize(then_body),
                else_body: optimize(else_body),
            },
            TIROp::IfOnly { then_body } => TIROp::IfOnly {
                then_body: optimize(then_body),
            },
            TIROp::Loop { label, body } => TIROp::Loop {
                label,
                body: optimize(body),
            },
            TIROp::ProofBlock { program_hash, body } => TIROp::ProofBlock {
                program_hash,
                body: optimize(body),
            },
            other => other,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_consecutive_hints() {
        let ops = vec![
            TIROp::Hint(1),
            TIROp::Hint(1),
            TIROp::Hint(1),
            TIROp::Add,
            TIROp::Hint(1),
            TIROp::Hint(1),
        ];
        let result = optimize(ops);
        assert_eq!(result.len(), 3);
        assert!(matches!(result[0], TIROp::Hint(3)));
        assert!(matches!(result[1], TIROp::Add));
        assert!(matches!(result[2], TIROp::Hint(2)));
    }

    #[test]
    fn merge_consecutive_pops() {
        let ops = vec![TIROp::Pop(2), TIROp::Pop(3), TIROp::Pop(1)];
        let result = optimize(ops);
        assert_eq!(result.len(), 2);
        assert!(matches!(result[0], TIROp::Pop(5)));
        assert!(matches!(result[1], TIROp::Pop(1)));
    }

    #[test]
    fn eliminate_swap_zero() {
        let ops = vec![TIROp::Push(1), TIROp::Swap(0), TIROp::Add];
        let result = optimize(ops);
        assert_eq!(result.len(), 2);
        assert!(matches!(result[0], TIROp::Push(1)));
        assert!(matches!(result[1], TIROp::Add));
    }

    #[test]
    fn eliminate_spill_reload_pair() {
        let addr = 1 << 30;
        let ops = vec![
            TIROp::Push(42),
            TIROp::Push(addr),
            TIROp::Swap(1),
            TIROp::WriteMem(1),
            TIROp::Pop(1),
            TIROp::Add,
            TIROp::Push(addr),
            TIROp::ReadMem(1),
            TIROp::Pop(1),
        ];
        let result = optimize(ops);
        assert_eq!(result.len(), 2);
        assert!(matches!(result[0], TIROp::Push(42)));
        assert!(matches!(result[1], TIROp::Add));
    }

    #[test]
    fn eliminate_dead_store() {
        let addr = 1 << 30;
        let ops = vec![
            TIROp::Push(42),
            TIROp::Push(addr),
            TIROp::Swap(1),
            TIROp::WriteMem(1),
            TIROp::Pop(1),
            TIROp::Add,
        ];
        let result = optimize(ops);
        assert!(result.iter().any(|op| matches!(op, TIROp::Pop(_))));
        assert!(!result.iter().any(|op| matches!(op, TIROp::WriteMem(_))));
    }

    #[test]
    fn no_eliminate_when_multiple_reads() {
        let addr = 1 << 30;
        let ops = vec![
            TIROp::Push(addr),
            TIROp::Swap(1),
            TIROp::WriteMem(1),
            TIROp::Pop(1),
            TIROp::Push(addr),
            TIROp::ReadMem(1),
            TIROp::Pop(1),
            TIROp::Push(addr),
            TIROp::ReadMem(1),
            TIROp::Pop(1),
        ];
        let result = optimize(ops);
        assert_eq!(result.len(), 10);
    }

    #[test]
    fn eliminate_dup0_pop1_nop() {
        let ops = vec![TIROp::Push(42), TIROp::Dup(0), TIROp::Pop(1), TIROp::Add];
        let result = optimize(ops);
        assert_eq!(result.len(), 2);
        assert!(matches!(result[0], TIROp::Push(42)));
        assert!(matches!(result[1], TIROp::Add));
    }

    #[test]
    fn eliminate_dup0_swap1_pop1_nop() {
        let ops = vec![
            TIROp::Push(42),
            TIROp::Dup(0),
            TIROp::Swap(1),
            TIROp::Pop(1),
            TIROp::Add,
        ];
        let result = optimize(ops);
        assert_eq!(result.len(), 2);
        assert!(matches!(result[0], TIROp::Push(42)));
        assert!(matches!(result[1], TIROp::Add));
    }

    #[test]
    fn eliminate_double_swap() {
        let ops = vec![TIROp::Push(1), TIROp::Swap(15), TIROp::Swap(15), TIROp::Add];
        let result = optimize(ops);
        assert_eq!(result.len(), 2);
        assert!(matches!(result[0], TIROp::Push(1)));
        assert!(matches!(result[1], TIROp::Add));
    }

    #[test]
    fn eliminate_many_double_swaps() {
        let mut ops = Vec::new();
        for _ in 0..12 {
            ops.push(TIROp::Swap(15));
        }
        ops.push(TIROp::Return);
        let result = optimize(ops);
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0], TIROp::Return));
    }

    #[test]
    fn collapse_epilogue_swap_pop_chain() {
        let ops = vec![
            TIROp::Swap(5),
            TIROp::Pop(1),
            TIROp::Swap(4),
            TIROp::Pop(1),
            TIROp::Swap(3),
            TIROp::Pop(1),
            TIROp::Return,
        ];
        let result = optimize(ops);
        assert!(matches!(result[0], TIROp::Swap(5)));
        assert!(matches!(result[1], TIROp::Pop(3)));
        assert!(matches!(result[2], TIROp::Return));
    }

    #[test]
    fn collapse_constant_depth_swap1_pop1_chain() {
        // 10× swap 1; pop 1 → swap 10; pop 5; pop 5
        let mut ops = Vec::new();
        for _ in 0..10 {
            ops.push(TIROp::Swap(1));
            ops.push(TIROp::Pop(1));
        }
        ops.push(TIROp::Return);
        let result = optimize(ops);
        assert!(matches!(result[0], TIROp::Swap(10)));
        assert!(matches!(result[1], TIROp::Pop(5)));
        assert!(matches!(result[2], TIROp::Pop(5)));
        assert!(matches!(result[3], TIROp::Return));
        assert_eq!(result.len(), 4);
    }

    #[test]
    fn collapse_large_constant_depth_chain() {
        // 24× swap 1; pop 1 → swap 15; pop 5; pop 5; pop 5; swap 9; pop 5; pop 4
        let mut ops = Vec::new();
        for _ in 0..24 {
            ops.push(TIROp::Swap(1));
            ops.push(TIROp::Pop(1));
        }
        ops.push(TIROp::Return);
        let result = optimize(ops);
        // First chunk: swap 15; pop 5; pop 5; pop 5
        assert!(matches!(result[0], TIROp::Swap(15)));
        assert!(matches!(result[1], TIROp::Pop(5)));
        assert!(matches!(result[2], TIROp::Pop(5)));
        assert!(matches!(result[3], TIROp::Pop(5)));
        // Second chunk: swap 9; pop 5; pop 4
        assert!(matches!(result[4], TIROp::Swap(9)));
        assert!(matches!(result[5], TIROp::Pop(5)));
        assert!(matches!(result[6], TIROp::Pop(4)));
        assert!(matches!(result[7], TIROp::Return));
        assert_eq!(result.len(), 8);
    }

    #[test]
    fn collapse_constant_depth_swap_k_pop1_chain() {
        // 5× swap 3; pop 1 (width-3 return, 5 dead locals) →
        // swap 7; swap 6; swap 5; swap 4; swap 3; pop 5 (batch pop)
        let mut ops = Vec::new();
        for _ in 0..5 {
            ops.push(TIROp::Swap(3));
            ops.push(TIROp::Pop(1));
        }
        ops.push(TIROp::Return);
        let result = optimize(ops);
        // D=3, count=5, total_depth = 3+5-1 = 7 (≤15)
        assert!(matches!(result[0], TIROp::Swap(7)));
        assert!(matches!(result[1], TIROp::Swap(6)));
        assert!(matches!(result[2], TIROp::Swap(5)));
        assert!(matches!(result[3], TIROp::Swap(4)));
        assert!(matches!(result[4], TIROp::Swap(3)));
        assert!(matches!(result[5], TIROp::Pop(5)));
        assert!(matches!(result[6], TIROp::Return));
        assert_eq!(result.len(), 7); // was 11
    }

    #[test]
    fn collapse_constant_depth_swap_k_large_chain() {
        // 4× swap 5; pop 1 — total_depth = 5+4-1 = 8 (≤15)
        let mut ops = Vec::new();
        for _ in 0..4 {
            ops.push(TIROp::Swap(5));
            ops.push(TIROp::Pop(1));
        }
        let result = optimize(ops);
        assert!(matches!(result[0], TIROp::Swap(8)));
        assert!(matches!(result[1], TIROp::Swap(7)));
        assert!(matches!(result[2], TIROp::Swap(6)));
        assert!(matches!(result[3], TIROp::Swap(5)));
        assert!(matches!(result[4], TIROp::Pop(4)));
        assert_eq!(result.len(), 5); // was 8
    }

    #[test]
    fn optimize_nested_bodies() {
        let ops = vec![TIROp::IfElse {
            then_body: vec![TIROp::Hint(1), TIROp::Hint(1)],
            else_body: vec![TIROp::Pop(0)],
        }];
        let result = optimize(ops);
        if let TIROp::IfElse {
            then_body,
            else_body,
        } = &result[0]
        {
            assert_eq!(then_body.len(), 1);
            assert!(matches!(then_body[0], TIROp::Hint(2)));
            assert!(else_body.is_empty());
        } else {
            panic!("expected IfElse");
        }
    }
}
