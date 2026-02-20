//! CPU Grammar Mask — stack state machine for TASM validity.
//!
//! Tracks abstract stack state (depth + element types) and produces
//! a validity mask over the vocabulary at each decoding step.
//! Used during training (teacher forcing) to precompute masks for the
//! entire target sequence, and during inference as a CPU fallback.

use super::grammar_tables::{build_min_stack_depths, build_stack_effects, StackEffect};
use super::vocab::VOCAB_SIZE;

/// Maximum stack depth we track. Beyond this we stop tracking types
/// but still track depth as an integer.
const MAX_TRACKED_DEPTH: usize = 64;

/// Stack type window size — how many top-of-stack slots we encode
/// type information for.
pub const TYPE_WINDOW: usize = 8;

/// Element type for abstract type tracking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ElemType {
    BFE = 0,
    XFE = 1,
    Unknown = 2,
}

/// Stack state machine for grammar masking.
///
/// Tracks stack depth and the types of the top `TYPE_WINDOW` elements.
/// At each step, can produce a validity mask indicating which VOCAB
/// tokens are legal given the current stack state.
pub struct StackStateMachine {
    depth: i32,
    /// Types of the top elements (index 0 = TOS).
    types: Vec<ElemType>,
    /// Precomputed stack effects table.
    effects: Vec<StackEffect>,
    /// Precomputed minimum depth requirements.
    min_depths: Vec<i32>,
}

impl StackStateMachine {
    /// Create a new state machine with the given initial stack depth.
    pub fn new(initial_depth: i32) -> Self {
        let types = vec![ElemType::Unknown; initial_depth.max(0) as usize];
        Self {
            depth: initial_depth,
            types,
            effects: build_stack_effects(),
            min_depths: build_min_stack_depths(),
        }
    }

    /// Current stack depth.
    pub fn stack_depth(&self) -> i32 {
        self.depth
    }

    /// Advance the state machine by executing a token.
    pub fn step(&mut self, token: u32) {
        if token == 0 || token as usize >= VOCAB_SIZE {
            return; // EOS or invalid — no state change
        }
        let idx = token as usize;
        let effect = self.effects[idx];

        // Update types: pop, then push Unknown
        for _ in 0..effect.pops {
            if !self.types.is_empty() {
                self.types.pop();
            }
        }
        for _ in 0..effect.pushes {
            self.types.push(ElemType::Unknown);
        }

        // Handle special cases for type tracking
        self.update_types_for_op(token);

        // Update depth
        self.depth += effect.net();
        if self.depth < 0 {
            self.depth = 0;
        }

        // Cap tracked types
        if self.types.len() > MAX_TRACKED_DEPTH {
            self.types.truncate(MAX_TRACKED_DEPTH);
        }
    }

    /// Update type annotations for specific operations.
    fn update_types_for_op(&mut self, token: u32) {
        let idx = token as usize;
        match idx {
            // push constants: always BFE
            1..=14 => {
                if let Some(last) = self.types.last_mut() {
                    *last = ElemType::BFE;
                }
            }
            // dup 0-15: the pushed element has same type as source.
            // After the generic push (Unknown already appended to types),
            // the source is at len - 2 - n (one deeper because of the push).
            20..=35 => {
                let n = (idx - 20) as usize;
                let len = self.types.len();
                if len >= 2 + n {
                    let src_type = self.types[len - 2 - n];
                    self.types[len - 1] = src_type;
                }
            }
            // Extension field ops push XFE
            136 => {
                // x_invert: pops 3 XFE, pushes 3 XFE
                let len = self.types.len();
                if len >= 3 {
                    for i in 0..3 {
                        self.types[len - 1 - i] = ElemType::XFE;
                    }
                }
            }
            137 => {
                // xb_mul: pushes 3 XFE
                let len = self.types.len();
                if len >= 3 {
                    for i in 0..3 {
                        self.types[len - 1 - i] = ElemType::XFE;
                    }
                }
            }
            // Most ops produce BFE results
            83..=95 => {
                // arithmetic, comparison, bitwise: result is BFE
                if let Some(last) = self.types.last_mut() {
                    *last = ElemType::BFE;
                }
            }
            _ => {}
        }
    }

    /// Produce a validity mask over the vocabulary.
    /// Returns VOCAB_SIZE floats: 0.0 = valid, -1e9 = masked (invalid).
    pub fn valid_mask(&self) -> Vec<f32> {
        let mut mask = vec![0.0f32; VOCAB_SIZE];

        for token_id in 1..VOCAB_SIZE {
            let min_depth = self.min_depths[token_id];
            if self.depth < min_depth {
                mask[token_id] = -1e9;
            }
        }

        // EOS is always valid (token 0)
        mask[0] = 0.0;

        mask
    }

    /// Encode the type state of the top TYPE_WINDOW stack slots
    /// as a flat vector of 3*TYPE_WINDOW floats (one-hot per slot).
    pub fn type_encoding(&self) -> Vec<f32> {
        let mut encoding = vec![0.0f32; 3 * TYPE_WINDOW];
        for i in 0..TYPE_WINDOW {
            let elem_type = if i < self.types.len() {
                self.types[self.types.len() - 1 - i]
            } else {
                ElemType::Unknown // below tracked depth
            };
            let base = i * 3;
            encoding[base + elem_type as usize] = 1.0;
        }
        encoding
    }

    /// Clamped stack depth for embedding lookup (0..max_stack_depth-1).
    pub fn depth_for_embedding(&self, max_depth: usize) -> u32 {
        (self.depth.max(0) as usize).min(max_depth - 1) as u32
    }
}

/// Precompute masks for an entire target sequence (teacher forcing).
///
/// Given a sequence of ground-truth tokens, simulates the state machine
/// and returns the validity mask at each step. Used during training to
/// apply grammar constraints without GPU-side state tracking.
///
/// Also returns stack depths and type encodings for decoder input.
pub fn precompute_sequence_state(target_tokens: &[u32], initial_depth: i32) -> SequenceState {
    let seq_len = target_tokens.len();
    let mut masks = Vec::with_capacity(seq_len);
    let mut depths = Vec::with_capacity(seq_len);
    let mut type_states = Vec::with_capacity(seq_len);

    let mut sm = StackStateMachine::new(initial_depth);

    for &token in target_tokens {
        // Record state BEFORE executing this token
        masks.push(sm.valid_mask());
        depths.push(sm.depth_for_embedding(65));
        type_states.push(sm.type_encoding());

        // Execute the token to advance state
        sm.step(token);
    }

    SequenceState {
        masks,
        depths,
        type_states,
    }
}

/// Precomputed sequence state for training.
pub struct SequenceState {
    /// Validity masks: [seq_len][VOCAB_SIZE], 0.0 or -1e9.
    pub masks: Vec<Vec<f32>>,
    /// Stack depths: [seq_len], clamped for embedding.
    pub depths: Vec<u32>,
    /// Type encodings: [seq_len][3*TYPE_WINDOW].
    pub type_states: Vec<Vec<f32>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_state_empty_stack() {
        let sm = StackStateMachine::new(0);
        assert_eq!(sm.stack_depth(), 0);
    }

    #[test]
    fn push_increases_depth() {
        let mut sm = StackStateMachine::new(0);
        sm.step(3); // push 1
        assert_eq!(sm.stack_depth(), 1);
        sm.step(4); // push 2
        assert_eq!(sm.stack_depth(), 2);
    }

    #[test]
    fn add_decreases_depth() {
        let mut sm = StackStateMachine::new(0);
        sm.step(3); // push 1
        sm.step(4); // push 2
        sm.step(83); // add
        assert_eq!(sm.stack_depth(), 1);
    }

    #[test]
    fn mask_prevents_underflow() {
        let sm = StackStateMachine::new(0);
        let mask = sm.valid_mask();
        // add (token 83) needs depth 2, should be masked
        assert_eq!(mask[83], -1e9);
        // push 1 (token 3) needs depth 0, should be valid
        assert_eq!(mask[3], 0.0);
        // EOS always valid
        assert_eq!(mask[0], 0.0);
    }

    #[test]
    fn mask_allows_valid_ops() {
        let mut sm = StackStateMachine::new(0);
        sm.step(3); // push 1
        sm.step(4); // push 2
        let mask = sm.valid_mask();
        // depth=2, add needs 2 → valid
        assert_eq!(mask[83], 0.0);
        // dup 0 needs 1 → valid
        assert_eq!(mask[20], 0.0);
        // dup 15 needs 16 → masked
        assert_eq!(mask[35], -1e9);
    }

    #[test]
    fn dup_preserves_type() {
        let mut sm = StackStateMachine::new(0);
        sm.step(3); // push 1 → BFE
        sm.step(20); // dup 0
        assert_eq!(sm.stack_depth(), 2);
        // Both top elements should be BFE
        let enc = sm.type_encoding();
        // TOS (i=0) should be BFE (index 0)
        assert_eq!(enc[0], 1.0); // BFE
        assert_eq!(enc[1], 0.0); // XFE
        assert_eq!(enc[2], 0.0); // Unknown
    }

    #[test]
    fn type_encoding_shape() {
        let sm = StackStateMachine::new(5);
        let enc = sm.type_encoding();
        assert_eq!(enc.len(), 3 * TYPE_WINDOW);
    }

    #[test]
    fn precompute_sequence_lengths() {
        let tokens = vec![3, 4, 83]; // push 1, push 2, add
        let state = precompute_sequence_state(&tokens, 0);
        assert_eq!(state.masks.len(), 3);
        assert_eq!(state.depths.len(), 3);
        assert_eq!(state.type_states.len(), 3);
        assert_eq!(state.masks[0].len(), VOCAB_SIZE);
        assert_eq!(state.type_states[0].len(), 3 * TYPE_WINDOW);
    }

    #[test]
    fn precompute_masks_reflect_state() {
        let tokens = vec![3, 83]; // push 1, then add
        let state = precompute_sequence_state(&tokens, 0);
        // Before push: depth=0, add should be masked
        assert_eq!(state.masks[0][83], -1e9);
        // After push: depth=1, add still masked (needs 2)
        assert_eq!(state.masks[1][83], -1e9);
    }

    #[test]
    fn precompute_depths_advance() {
        let tokens = vec![3, 4, 83]; // push, push, add
        let state = precompute_sequence_state(&tokens, 0);
        assert_eq!(state.depths[0], 0); // before push 1
        assert_eq!(state.depths[1], 1); // after push 1, before push 2
        assert_eq!(state.depths[2], 2); // after push 2, before add
    }

    #[test]
    fn pop_reduces_depth() {
        let mut sm = StackStateMachine::new(0);
        sm.step(3); // push 1
        sm.step(4); // push 2
        sm.step(3); // push 1
        sm.step(16); // pop 2
        assert_eq!(sm.stack_depth(), 1);
    }

    #[test]
    fn depth_clamps_at_zero() {
        let mut sm = StackStateMachine::new(1);
        sm.step(15); // pop 1
        assert_eq!(sm.stack_depth(), 0);
        // Shouldn't go negative even if we somehow pop more
    }

    #[test]
    fn write_mem_5_needs_six_on_stack() {
        let sm = StackStateMachine::new(5);
        let mask = sm.valid_mask();
        // write_mem 5 (token 128) needs 6 elements
        assert_eq!(mask[128], -1e9);

        let sm2 = StackStateMachine::new(6);
        let mask2 = sm2.valid_mask();
        assert_eq!(mask2[128], 0.0);
    }

    #[test]
    fn hash_needs_ten() {
        let sm = StackStateMachine::new(9);
        let mask = sm.valid_mask();
        assert_eq!(mask[129], -1e9); // hash needs 10

        let sm2 = StackStateMachine::new(10);
        let mask2 = sm2.valid_mask();
        assert_eq!(mask2[129], 0.0);
    }
}
