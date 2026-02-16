/// Dead spill and dead store elimination.
use super::TIROp;
use std::collections::{BTreeMap, BTreeSet};

/// Eliminate dead spills and dead stores.
///
/// Two patterns are handled:
///
/// 1. **Spill/reload pairs** -- address written once and read once:
///    Write: `Push(addr), Swap(1), WriteMem(1), Pop(1)` -> removed
///    Read:  `Push(addr), ReadMem(1), Pop(1)` -> removed
///    The value stays on the stack instead of round-tripping through RAM.
///
/// 2. **Dead stores** -- address written but never read:
///    `Push(addr), Swap(1), WriteMem(1), Pop(1)` -> `Pop(1)`
///    The value was going to be discarded into RAM; just pop it.
///    Also handles: `Swap(D), Push(addr), Swap(1), WriteMem(1), Pop(1)` -> `Swap(D), Pop(1)`
pub(crate) fn eliminate_dead_spills(ops: Vec<TIROp>) -> Vec<TIROp> {
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
    // - "pair": 1 write, 1 read -> remove both (value stays on stack)
    // - "dead": writes only, 0 reads -> replace write with pop (discard value)
    let mut pair_addrs: BTreeSet<u64> = BTreeSet::new();
    let mut dead_addrs: BTreeSet<u64> = BTreeSet::new();

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
