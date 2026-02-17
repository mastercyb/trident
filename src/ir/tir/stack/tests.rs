use super::*;

#[test]
fn test_basic_push_pop() {
    let mut sm = StackManager::new();
    sm.push_named("a", 1);
    sm.push_named("b", 1);
    assert_eq!(sm.stack_depth(), 2);
    assert_eq!(sm.stack_len(), 2);
    assert_eq!(sm.access_var("b"), 0);
    assert_eq!(sm.access_var("a"), 1);
    sm.pop();
    assert_eq!(sm.stack_depth(), 1);
}

#[test]
fn test_no_spill_under_16() {
    let mut sm = StackManager::new();
    for i in 0..16 {
        sm.push_named(&format!("v{}", i), 1);
    }
    assert_eq!(sm.stack_depth(), 16);
    assert!(sm.drain_side_effects().is_empty());
}

#[test]
fn test_spill_at_17() {
    let mut sm = StackManager::new();
    // Push 16 variables
    for i in 0..16 {
        sm.push_named(&format!("v{}", i), 1);
    }
    // Access v15 to make it recently used
    sm.access_var("v15");

    // Push one more — should spill the LRU (v0)
    sm.push_named("v16", 1);
    let effects = sm.drain_side_effects();
    // Should have spill instructions
    assert!(!effects.is_empty(), "expected spill instructions");
    // v0 should be spilled
    assert!(sm.spilled.iter().any(|v| v.name.as_deref() == Some("v0")));
}

#[test]
fn test_reload_spilled_var() {
    let mut sm = StackManager::new();
    for i in 0..16 {
        sm.push_named(&format!("v{}", i), 1);
    }
    // Push one more to spill v0
    sm.push_named("v16", 1);
    sm.drain_side_effects(); // clear

    // Access v0 — should reload it
    let depth = sm.access_var("v0");
    let effects = sm.drain_side_effects();
    assert!(!effects.is_empty(), "expected reload instructions");
    assert_eq!(depth, 0); // reloaded to top
}

#[test]
fn test_temp_push() {
    let mut sm = StackManager::new();
    sm.push_temp(1);
    assert_eq!(sm.stack_depth(), 1);
    assert!(sm.last().unwrap().name.is_none());
}

#[test]
fn test_multi_width_spill() {
    let mut sm = StackManager::new();
    // Push a Digest (width 5) and fill up stack
    sm.push_named("digest", 5);
    for i in 0..11 {
        sm.push_named(&format!("v{}", i), 1);
    }
    assert_eq!(sm.stack_depth(), 16);

    // Push one more — should spill digest (LRU, earliest pushed)
    sm.push_named("extra", 1);
    let effects = sm.drain_side_effects();
    assert!(!effects.is_empty());
    // Digest with width 5 should have 5 write_mem instructions
    let write_count = effects.iter().filter(|l| l.contains("write_mem")).count();
    assert_eq!(write_count, 5, "expected 5 write_mem for Digest spill");
}

#[test]
fn test_spill_all_named() {
    let mut sm = StackManager::new();
    sm.push_named("a", 1);
    sm.push_named("b", 1);
    sm.push_named("c", 1);
    sm.push_temp(1); // anonymous temp
    assert_eq!(sm.stack_depth(), 4);

    sm.spill_all_named();
    let effects = sm.drain_side_effects();
    // 3 named variables spilled → 3 write_mem instructions
    let write_count = effects.iter().filter(|l| l.contains("write_mem")).count();
    assert_eq!(write_count, 3, "expected 3 write_mem for 3 named vars");

    // Only the anonymous temp should remain on stack
    assert_eq!(sm.stack_len(), 1, "only anonymous temp should remain");
    assert!(
        sm.last().unwrap().name.is_none(),
        "remaining entry should be anonymous"
    );
}

#[test]
fn test_spill_all_named_empty() {
    let mut sm = StackManager::new();
    sm.push_temp(1);
    sm.push_temp(1);
    sm.spill_all_named();
    let effects = sm.drain_side_effects();
    assert!(effects.is_empty(), "no named vars → no spill");
    assert_eq!(sm.stack_len(), 2);
}
