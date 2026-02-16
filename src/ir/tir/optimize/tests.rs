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
    // 10x swap 1; pop 1 -> swap 10; pop 5; pop 5
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
    // 24x swap 1; pop 1 -> swap 15; pop 5; pop 5; pop 5; swap 9; pop 5; pop 4
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
    // 5x swap 3; pop 1 (width-3 return, 5 dead locals) ->
    // swap 7; swap 6; swap 5; swap 4; swap 3; pop 5 (batch pop)
    let mut ops = Vec::new();
    for _ in 0..5 {
        ops.push(TIROp::Swap(3));
        ops.push(TIROp::Pop(1));
    }
    ops.push(TIROp::Return);
    let result = optimize(ops);
    // D=3, count=5, total_depth = 3+5-1 = 7 (<=15)
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
    // 4x swap 5; pop 1 -- total_depth = 5+4-1 = 8 (<=15)
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
