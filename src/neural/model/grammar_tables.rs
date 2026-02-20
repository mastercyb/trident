//! Static grammar tables for TASM instruction set.
//!
//! Per-instruction stack effect tables derived from the Triton VM ISA spec.
//! Used by the grammar mask to determine which instructions are valid
//! given the current stack state.

use super::vocab::VOCAB_SIZE;

/// Stack effect for a single instruction: how many elements it pops
/// and how many it pushes.
#[derive(Clone, Copy, Debug)]
pub struct StackEffect {
    pub pops: i32,
    pub pushes: i32,
}

impl StackEffect {
    pub const fn new(pops: i32, pushes: i32) -> Self {
        Self { pops, pushes }
    }

    /// Net change in stack depth.
    pub const fn net(&self) -> i32 {
        self.pushes - self.pops
    }
}

/// Build the stack effect table for all VOCAB_SIZE tokens.
/// Index by token ID → StackEffect.
///
/// Token 0 = EOS (no effect, always valid).
pub fn build_stack_effects() -> Vec<StackEffect> {
    let mut effects = vec![StackEffect::new(0, 0); VOCAB_SIZE];

    // EOS (0): no effect
    effects[0] = StackEffect::new(0, 0);

    // push constants (1-14): push 1 element
    for i in 1..=14 {
        effects[i] = StackEffect::new(0, 1);
    }

    // pop 1-5 (15-19)
    effects[15] = StackEffect::new(1, 0); // pop 1
    effects[16] = StackEffect::new(2, 0); // pop 2
    effects[17] = StackEffect::new(3, 0); // pop 3
    effects[18] = StackEffect::new(4, 0); // pop 4
    effects[19] = StackEffect::new(5, 0); // pop 5

    // dup 0-15 (20-35): read at depth, push 1
    for i in 20..=35 {
        let depth = (i - 20) as i32;
        // dup N requires depth >= N+1, pushes 1
        // We encode min_depth separately; here just pops=0, pushes=1
        effects[i] = StackEffect::new(0, 1);
        // But dup N requires at least N+1 elements on stack
        // This is handled by min_stack_depth table below
        let _ = depth; // used in min_stack_depth
    }

    // swap 1-15 (36-50): in-place, no net change
    for i in 36..=50 {
        effects[i] = StackEffect::new(0, 0);
    }

    // pick 0-15 (51-66): remove at depth, push to top → net 0
    for i in 51..=66 {
        effects[i] = StackEffect::new(0, 0);
    }

    // place 0-15 (67-82): remove from top, insert at depth → net 0
    for i in 67..=82 {
        effects[i] = StackEffect::new(0, 0);
    }

    // ── arithmetic (83-90) ──
    effects[83] = StackEffect::new(2, 1); // add: pop 2, push 1
    effects[84] = StackEffect::new(2, 1); // mul: pop 2, push 1
    effects[85] = StackEffect::new(1, 1); // invert: pop 1, push 1
    effects[86] = StackEffect::new(1, 2); // split: pop 1, push 2 (hi, lo)
    effects[87] = StackEffect::new(2, 2); // div_mod: pop 2, push 2 (q, r)
    effects[88] = StackEffect::new(2, 1); // pow: pop 2, push 1
    effects[89] = StackEffect::new(1, 1); // log_2_floor: pop 1, push 1
    effects[90] = StackEffect::new(1, 1); // pop_count: pop 1, push 1

    // ── comparison (91-92) ──
    effects[91] = StackEffect::new(2, 1); // eq: pop 2, push 1
    effects[92] = StackEffect::new(2, 1); // lt: pop 2, push 1

    // ── bitwise (93-95) ──
    effects[93] = StackEffect::new(2, 1); // and: pop 2, push 1
    effects[94] = StackEffect::new(2, 1); // xor: pop 2, push 1
    effects[95] = StackEffect::new(2, 1); // or: pop 2, push 1

    // ── control (96-103) ──
    effects[96] = StackEffect::new(0, 0); // nop
    effects[97] = StackEffect::new(0, 0); // halt
    effects[98] = StackEffect::new(1, 0); // assert: pop 1
    effects[99] = StackEffect::new(5, 0); // assert_vector: pop 5 (top 5 after comparing with next 5, needs 10)
    effects[100] = StackEffect::new(1, 0); // skiz: pop 1
    effects[101] = StackEffect::new(0, 0); // return
    effects[102] = StackEffect::new(0, 0); // recurse
    effects[103] = StackEffect::new(0, 0); // recurse_or_return

    // ── read_io 1-5 (104-108) ──
    effects[104] = StackEffect::new(0, 1); // read_io 1
    effects[105] = StackEffect::new(0, 2); // read_io 2
    effects[106] = StackEffect::new(0, 3); // read_io 3
    effects[107] = StackEffect::new(0, 4); // read_io 4
    effects[108] = StackEffect::new(0, 5); // read_io 5

    // ── write_io 1-5 (109-113) ──
    effects[109] = StackEffect::new(1, 0); // write_io 1
    effects[110] = StackEffect::new(2, 0); // write_io 2
    effects[111] = StackEffect::new(3, 0); // write_io 3
    effects[112] = StackEffect::new(4, 0); // write_io 4
    effects[113] = StackEffect::new(5, 0); // write_io 5

    // ── divine 1-5 (114-118) ──
    effects[114] = StackEffect::new(0, 1); // divine 1
    effects[115] = StackEffect::new(0, 2); // divine 2
    effects[116] = StackEffect::new(0, 3); // divine 3
    effects[117] = StackEffect::new(0, 4); // divine 4
    effects[118] = StackEffect::new(0, 5); // divine 5

    // ── read_mem 1-5 (119-123): pop addr, push N values + adjusted addr ──
    effects[119] = StackEffect::new(1, 2); // read_mem 1: -addr +val +addr'
    effects[120] = StackEffect::new(1, 3); // read_mem 2
    effects[121] = StackEffect::new(1, 4); // read_mem 3
    effects[122] = StackEffect::new(1, 5); // read_mem 4
    effects[123] = StackEffect::new(1, 6); // read_mem 5

    // ── write_mem 1-5 (124-128): pop N values + addr, push adjusted addr ──
    effects[124] = StackEffect::new(2, 1); // write_mem 1: -(val+addr) +addr'
    effects[125] = StackEffect::new(3, 1); // write_mem 2
    effects[126] = StackEffect::new(4, 1); // write_mem 3
    effects[127] = StackEffect::new(5, 1); // write_mem 4
    effects[128] = StackEffect::new(6, 1); // write_mem 5

    // ── crypto (129-135) ──
    effects[129] = StackEffect::new(10, 5); // hash: pop 10, push 5
    effects[130] = StackEffect::new(0, 0); // sponge_init
    effects[131] = StackEffect::new(10, 0); // sponge_absorb: pop 10
    effects[132] = StackEffect::new(0, 10); // sponge_squeeze: push 10
    effects[133] = StackEffect::new(1, 1); // sponge_absorb_mem: pop addr, push addr'
    effects[134] = StackEffect::new(0, 0); // merkle_step: complex, approximate as neutral
    effects[135] = StackEffect::new(0, 0); // merkle_step_mem: complex, approximate as neutral

    // ── extension field (136-139) ──
    effects[136] = StackEffect::new(3, 3); // x_invert: pop 3 XFE, push 3 XFE
    effects[137] = StackEffect::new(4, 3); // xb_mul: pop 3 XFE + 1 BFE, push 3 XFE
    effects[138] = StackEffect::new(0, 0); // xx_dot_step: complex accumulator
    effects[139] = StackEffect::new(0, 0); // xb_dot_step: complex accumulator

    effects
}

/// Minimum stack depth required to execute each instruction.
/// Separate from stack effects because dup/swap/pick/place need
/// specific depths but don't consume elements in the pop/push sense.
pub fn build_min_stack_depths() -> Vec<i32> {
    let mut depths = vec![0i32; VOCAB_SIZE];

    // EOS: no requirement
    depths[0] = 0;

    // push: no requirement
    for i in 1..=14 {
        depths[i] = 0;
    }

    // pop 1-5: need that many elements
    depths[15] = 1;
    depths[16] = 2;
    depths[17] = 3;
    depths[18] = 4;
    depths[19] = 5;

    // dup N: need at least N+1 elements
    for i in 20..=35 {
        let n = (i - 20) as i32;
        depths[i] = n + 1;
    }

    // swap N: need at least N+1 elements
    for i in 36..=50 {
        let n = (i - 36 + 1) as i32; // swap 1..15
        depths[i] = n + 1;
    }

    // pick N: need at least N+1 elements
    for i in 51..=66 {
        let n = (i - 51) as i32;
        depths[i] = n + 1;
    }

    // place N: need at least N+1 elements
    for i in 67..=82 {
        let n = (i - 67) as i32;
        depths[i] = n + 1;
    }

    // arithmetic: need operands
    depths[83] = 2; // add
    depths[84] = 2; // mul
    depths[85] = 1; // invert
    depths[86] = 1; // split
    depths[87] = 2; // div_mod
    depths[88] = 2; // pow
    depths[89] = 1; // log_2_floor
    depths[90] = 1; // pop_count

    // comparison
    depths[91] = 2; // eq
    depths[92] = 2; // lt

    // bitwise
    depths[93] = 2; // and
    depths[94] = 2; // xor
    depths[95] = 2; // or

    // control
    depths[96] = 0; // nop
    depths[97] = 0; // halt
    depths[98] = 1; // assert
    depths[99] = 10; // assert_vector: need 10 (compare top 5 with next 5)
    depths[100] = 1; // skiz
    depths[101] = 0; // return
    depths[102] = 0; // recurse
    depths[103] = 0; // recurse_or_return

    // read_io: no stack requirement
    for i in 104..=108 {
        depths[i] = 0;
    }

    // write_io 1-5
    depths[109] = 1;
    depths[110] = 2;
    depths[111] = 3;
    depths[112] = 4;
    depths[113] = 5;

    // divine: no stack requirement
    for i in 114..=118 {
        depths[i] = 0;
    }

    // read_mem 1-5: need address on stack
    for i in 119..=123 {
        depths[i] = 1;
    }

    // write_mem 1-5: need N values + address
    depths[124] = 2; // write_mem 1
    depths[125] = 3; // write_mem 2
    depths[126] = 4; // write_mem 3
    depths[127] = 5; // write_mem 4
    depths[128] = 6; // write_mem 5

    // crypto
    depths[129] = 10; // hash
    depths[130] = 0; // sponge_init
    depths[131] = 10; // sponge_absorb
    depths[132] = 0; // sponge_squeeze
    depths[133] = 1; // sponge_absorb_mem
    depths[134] = 0; // merkle_step
    depths[135] = 0; // merkle_step_mem

    // extension field
    depths[136] = 3; // x_invert
    depths[137] = 4; // xb_mul
    depths[138] = 0; // xx_dot_step
    depths[139] = 0; // xb_dot_step

    depths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effects_table_size() {
        let effects = build_stack_effects();
        assert_eq!(effects.len(), VOCAB_SIZE);
    }

    #[test]
    fn depths_table_size() {
        let depths = build_min_stack_depths();
        assert_eq!(depths.len(), VOCAB_SIZE);
    }

    #[test]
    fn push_effect_is_plus_one() {
        let effects = build_stack_effects();
        for i in 1..=14 {
            assert_eq!(effects[i].net(), 1, "push token {} should be +1", i);
        }
    }

    #[test]
    fn add_pops_two_pushes_one() {
        let effects = build_stack_effects();
        assert_eq!(effects[83].pops, 2);
        assert_eq!(effects[83].pushes, 1);
    }

    #[test]
    fn dup_needs_depth() {
        let depths = build_min_stack_depths();
        // dup 0 needs 1, dup 15 needs 16
        assert_eq!(depths[20], 1); // dup 0
        assert_eq!(depths[35], 16); // dup 15
    }

    #[test]
    fn swap_needs_depth() {
        let depths = build_min_stack_depths();
        // swap 1 needs 2, swap 15 needs 16
        assert_eq!(depths[36], 2); // swap 1
        assert_eq!(depths[50], 16); // swap 15
    }

    #[test]
    fn assert_vector_needs_ten() {
        let depths = build_min_stack_depths();
        assert_eq!(depths[99], 10);

        let effects = build_stack_effects();
        assert_eq!(effects[99].pops, 5); // pops top 5
    }

    #[test]
    fn write_mem_5_needs_six() {
        let depths = build_min_stack_depths();
        assert_eq!(depths[128], 6); // 5 values + 1 address
    }
}
