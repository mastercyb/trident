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
        // 2+3+1 = 6, emitted as Pop(5) + Pop(1)
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
        let addr = 1 << 30; // typical spill address
        let ops = vec![
            TIROp::Push(42),
            // spill pattern
            TIROp::Push(addr),
            TIROp::Swap(1),
            TIROp::WriteMem(1),
            TIROp::Pop(1),
            // some work
            TIROp::Add,
            // reload pattern
            TIROp::Push(addr),
            TIROp::ReadMem(1),
            TIROp::Pop(1),
        ];
        let result = optimize(ops);
        // spill and reload removed, only Push(42), Add remain
        assert_eq!(result.len(), 2);
        assert!(matches!(result[0], TIROp::Push(42)));
        assert!(matches!(result[1], TIROp::Add));
    }

    #[test]
    fn eliminate_dead_store() {
        let addr = 1 << 30;
        let ops = vec![
            TIROp::Push(42),
            // dead write pattern (never read back)
            TIROp::Push(addr),
            TIROp::Swap(1),
            TIROp::WriteMem(1),
            TIROp::Pop(1),
            TIROp::Add,
        ];
        let result = optimize(ops);
        // write replaced with pop 1, then merged with surrounding
        assert!(result.iter().any(|op| matches!(op, TIROp::Pop(_))));
        // write_mem should be gone
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
        // 2 reads → not eliminated
        assert_eq!(result.len(), 10);
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
            assert!(else_body.is_empty()); // Pop(0) eliminated
        } else {
            panic!("expected IfElse");
        }
    }
}
