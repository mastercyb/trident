//! Block-level TASM stack verifier for neural training.
//!
//! Executes straight-line TASM blocks on concrete u64 values using
//! Goldilocks field arithmetic. Used to verify neural-generated TASM
//! produces the same stack transformation as classical TASM.
//!
//! Not a full Triton VM — only handles the ~25 instructions that appear
//! in straight-line blocks. Crypto/IO/memory ops modeled by stack effects
//! only (correct push/pop counts, dummy values). Full verification uses
//! trisha (Triton VM execution).

use crate::field::goldilocks::{Goldilocks, MODULUS};
use crate::field::PrimeField;

/// Stack state after executing a TASM sequence.
#[derive(Clone, Debug)]
pub struct StackState {
    pub stack: Vec<u64>,
    pub error: bool,
}

impl StackState {
    pub fn new(initial: Vec<u64>) -> Self {
        Self {
            stack: initial,
            error: false,
        }
    }

    /// Execute a sequence of TASM lines. Stops on error or halt.
    pub fn execute(&mut self, lines: &[String]) {
        for line in lines {
            if self.error {
                return;
            }
            self.execute_line(line);
        }
    }

    /// Execute a single TASM instruction line.
    pub fn execute_line(&mut self, line: &str) {
        let t = line.trim();
        if t.is_empty() || t.starts_with("//") || t.ends_with(':') {
            return;
        }
        let parts: Vec<&str> = t.split_whitespace().collect();
        if parts.is_empty() {
            return;
        }
        let op = parts[0];
        let arg = parts.get(1).and_then(|s| s.parse::<i64>().ok());
        let arg_u = parts.get(1).and_then(|s| s.parse::<u64>().ok());

        match op {
            // --- Literals ---
            "push" => {
                let val = if let Some(v) = arg {
                    if v < 0 {
                        Goldilocks::from_u64(0)
                            .sub(Goldilocks::from_u64((-v) as u64))
                            .to_u64()
                    } else {
                        Goldilocks::from_u64(v as u64).to_u64()
                    }
                } else if let Some(v) = arg_u {
                    // Large positive literal (exceeds i64 range)
                    Goldilocks::from_u64(v).to_u64()
                } else {
                    0
                };
                self.stack.push(val);
            }

            // --- Stack manipulation ---
            "pop" => {
                let n = arg_u.unwrap_or(1) as usize;
                if self.stack.len() < n {
                    self.error = true;
                    return;
                }
                self.stack.truncate(self.stack.len() - n);
            }
            "dup" => {
                let depth = arg_u.unwrap_or(0) as usize;
                if self.stack.len() <= depth {
                    self.error = true;
                    return;
                }
                let idx = self.stack.len() - 1 - depth;
                let val = self.stack[idx];
                self.stack.push(val);
            }
            "swap" => {
                let depth = arg_u.unwrap_or(1) as usize;
                if depth == 0 || self.stack.len() <= depth {
                    self.error = true;
                    return;
                }
                let top = self.stack.len() - 1;
                self.stack.swap(top, top - depth);
            }
            "pick" => {
                let depth = arg_u.unwrap_or(0) as usize;
                if self.stack.len() <= depth {
                    self.error = true;
                    return;
                }
                let idx = self.stack.len() - 1 - depth;
                let val = self.stack.remove(idx);
                self.stack.push(val);
            }
            "place" => {
                let depth = arg_u.unwrap_or(0) as usize;
                if self.stack.is_empty() || self.stack.len() <= depth {
                    self.error = true;
                    return;
                }
                let val = self.stack.pop().unwrap();
                let idx = self.stack.len() - depth;
                self.stack.insert(idx, val);
            }

            // --- Arithmetic (Goldilocks field) ---
            "add" => {
                if self.stack.len() < 2 {
                    self.error = true;
                    return;
                }
                let b = Goldilocks(self.stack.pop().unwrap());
                let a = Goldilocks(self.stack.pop().unwrap());
                self.stack.push(a.add(b).to_u64());
            }
            "mul" => {
                if self.stack.len() < 2 {
                    self.error = true;
                    return;
                }
                let b = Goldilocks(self.stack.pop().unwrap());
                let a = Goldilocks(self.stack.pop().unwrap());
                self.stack.push(a.mul(b).to_u64());
            }
            "invert" => {
                // Negation (not multiplicative inverse) in Triton VM
                if self.stack.is_empty() {
                    self.error = true;
                    return;
                }
                let a = Goldilocks(self.stack.pop().unwrap());
                self.stack.push(a.neg().to_u64());
            }

            // --- Comparison ---
            "eq" => {
                if self.stack.len() < 2 {
                    self.error = true;
                    return;
                }
                let b = self.stack.pop().unwrap();
                let a = self.stack.pop().unwrap();
                self.stack.push(if a == b { 1 } else { 0 });
            }
            "lt" => {
                if self.stack.len() < 2 {
                    self.error = true;
                    return;
                }
                let b = self.stack.pop().unwrap();
                let a = self.stack.pop().unwrap();
                self.stack.push(if a < b { 1 } else { 0 });
            }

            // --- Bitwise ---
            "and" => {
                if self.stack.len() < 2 {
                    self.error = true;
                    return;
                }
                let b = self.stack.pop().unwrap();
                let a = self.stack.pop().unwrap();
                self.stack.push(a & b);
            }
            "xor" => {
                if self.stack.len() < 2 {
                    self.error = true;
                    return;
                }
                let b = self.stack.pop().unwrap();
                let a = self.stack.pop().unwrap();
                self.stack.push(a ^ b);
            }
            "split" => {
                // x → (hi, lo) where hi = x >> 32, lo = x & 0xFFFFFFFF
                if self.stack.is_empty() {
                    self.error = true;
                    return;
                }
                let x = self.stack.pop().unwrap();
                let lo = x & 0xFFFF_FFFF;
                let hi = x >> 32;
                self.stack.push(hi);
                self.stack.push(lo);
            }
            "div_mod" => {
                // (n, d) → (q, r) where q = n/d, r = n%d
                if self.stack.len() < 2 {
                    self.error = true;
                    return;
                }
                let d = self.stack.pop().unwrap();
                let n = self.stack.pop().unwrap();
                if d == 0 {
                    self.error = true;
                    return;
                }
                self.stack.push(n / d);
                self.stack.push(n % d);
            }
            "pow" => {
                // (base, exp) → base^exp mod p
                if self.stack.len() < 2 {
                    self.error = true;
                    return;
                }
                let exp = self.stack.pop().unwrap();
                let base = Goldilocks(self.stack.pop().unwrap());
                let mut result = Goldilocks::ONE;
                let mut b = base;
                let mut e = exp;
                while e > 0 {
                    if e & 1 == 1 {
                        result = result.mul(b);
                    }
                    b = b.mul(b);
                    e >>= 1;
                }
                self.stack.push(result.to_u64());
            }
            "log_2_floor" => {
                if self.stack.is_empty() {
                    self.error = true;
                    return;
                }
                let x = self.stack.pop().unwrap();
                if x == 0 {
                    self.error = true;
                    return;
                }
                self.stack.push(63 - x.leading_zeros() as u64);
            }
            "pop_count" => {
                if self.stack.is_empty() {
                    self.error = true;
                    return;
                }
                let x = self.stack.pop().unwrap();
                self.stack.push(x.count_ones() as u64);
            }

            // --- Control (straight-line only) ---
            "nop" => {}
            "halt" => {
                return;
            }
            "assert" => {
                if self.stack.is_empty() {
                    self.error = true;
                    return;
                }
                let v = self.stack.pop().unwrap();
                if v != 1 {
                    self.error = true;
                }
            }
            "assert_vector" => {
                // Assert top 5 elements equal next 5
                if self.stack.len() < 10 {
                    self.error = true;
                    return;
                }
                let len = self.stack.len();
                for i in 0..5 {
                    if self.stack[len - 1 - i] != self.stack[len - 6 - i] {
                        self.error = true;
                        return;
                    }
                }
                // Pop top 5
                self.stack.truncate(len - 5);
            }

            // --- I/O (modeled stack effects, dummy values) ---
            "read_io" => {
                let n = arg_u.unwrap_or(1) as usize;
                for _ in 0..n {
                    self.stack.push(0);
                }
            }
            "write_io" => {
                let n = arg_u.unwrap_or(1) as usize;
                if self.stack.len() < n {
                    self.error = true;
                    return;
                }
                self.stack.truncate(self.stack.len() - n);
            }
            "divine" => {
                let n = arg_u.unwrap_or(1) as usize;
                for _ in 0..n {
                    self.stack.push(0);
                }
            }

            // --- Memory (modeled stack effects) ---
            "read_mem" => {
                // pop address, push N values + adjusted address
                let n = arg_u.unwrap_or(1) as usize;
                if self.stack.is_empty() {
                    self.error = true;
                    return;
                }
                let _addr = self.stack.pop().unwrap();
                for _ in 0..n {
                    self.stack.push(0); // dummy values
                }
                self.stack.push(0); // adjusted address
            }
            "write_mem" => {
                // pop N values + address, push adjusted address
                let n = arg_u.unwrap_or(1) as usize;
                if self.stack.len() < n + 1 {
                    self.error = true;
                    return;
                }
                self.stack.truncate(self.stack.len() - n - 1);
                self.stack.push(0); // adjusted address
            }

            // --- Crypto (modeled stack effects only) ---
            "hash" => {
                // pop 10, push 5
                if self.stack.len() < 10 {
                    self.error = true;
                    return;
                }
                self.stack.truncate(self.stack.len() - 10);
                for _ in 0..5 {
                    self.stack.push(0);
                }
            }
            "sponge_init" => {}
            "sponge_absorb" => {
                if self.stack.len() < 10 {
                    self.error = true;
                    return;
                }
                self.stack.truncate(self.stack.len() - 10);
            }
            "sponge_squeeze" => {
                for _ in 0..10 {
                    self.stack.push(0);
                }
            }
            "sponge_absorb_mem" => {
                // Absorb from memory: pop address, push adjusted address
                if self.stack.is_empty() {
                    self.error = true;
                    return;
                }
                let _addr = self.stack.pop().unwrap();
                self.stack.push(0);
            }
            "merkle_step" | "merkle_step_mem" => {
                // Complex stack effects — skip in block verifier
            }

            // --- Extension field (modeled as nops for stack) ---
            "xb_mul" | "x_invert" | "xx_dot_step" | "xb_dot_step" => {}

            // --- Control flow — can't simulate branches, mark unsimulable ---
            "call" | "return" | "recurse" | "recurse_or_return" | "skiz" => {
                self.error = true;
            }

            // Unknown instruction — ignore (conservative)
            _ => {}
        }
    }

    /// Check if execution completed without errors.
    pub fn is_valid(&self) -> bool {
        !self.error
    }
}

/// Generate a deterministic test stack for a given seed.
pub fn generate_test_stack(seed: u64, size: usize) -> Vec<u64> {
    let mut stack = Vec::with_capacity(size);
    let mut state = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    for _ in 0..size {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        // Keep values in valid Goldilocks range
        let val = state % MODULUS;
        stack.push(val);
    }
    stack
}

/// Verify that candidate TASM produces the same stack as baseline TASM.
/// Tests with 8 independent seeds — all must pass. A single concrete test
/// case is trivially gamed by the neural optimizer; 8 distinct Goldilocks
/// stacks catch wrong operand order, off-by-one dup/swap depth, and
/// missing operations.
/// Conservative: rejects candidates when baseline can't be simulated.
pub fn verify_equivalent(baseline_tasm: &[String], candidate_tasm: &[String], seed: u64) -> bool {
    // Only allow instructions the verifier simulates with exact Goldilocks
    // arithmetic. The neural optimizer exploits every dummy-value instruction
    // (divine, hash, read_io, read_mem, sponge_*) and every nop-modeled
    // instruction (xb_mul, merkle_step) to produce "equivalent" output that
    // is actually wrong on real Triton VM.
    const ALLOWED: &[&str] = &[
        "push",
        "pop",
        "dup",
        "swap",
        "pick",
        "place",
        "add",
        "mul",
        "invert",
        "eq",
        "lt",
        "and",
        "xor",
        // split is NOT allowed — split(push_const) produces deterministic
        // hi/lo that make test stacks irrelevant for that branch.
        "div_mod",
        "pow",
        "log_2_floor",
        "pop_count",
        "nop",
        // halt is NOT allowed — it terminates the program mid-block,
        // preventing all subsequent code from running.
        // write_io is NOT allowed — it has I/O side effects that
        // the verifier ignores (only checks stack state). Removing
        // write_io operations changes program output.
        // assert is NOT allowed — the neural model can insert asserts
        // that crash on inputs the test stacks don't cover.
    ];
    // Reject blocks whose baselines use side-effecting or dummy-value ops.
    // - write_io/halt/assert: side effects the verifier can't check
    // - divine: pushes dummy zeros in verifier, real values on Triton VM
    // - split: hi part depends on value magnitude; split(small) always gives
    //   hi=0 which makes the verifier path-dependent on test values
    for line in baseline_tasm {
        let op = line.trim().split_whitespace().next().unwrap_or("");
        if matches!(
            op,
            "write_io" | "halt" | "assert" | "assert_vector" | "divine" | "split"
        ) {
            return false;
        }
    }
    for line in candidate_tasm {
        let op = line.trim().split_whitespace().next().unwrap_or("");
        if op.is_empty() || op.starts_with("//") || op.ends_with(':') {
            continue;
        }
        if !ALLOWED.contains(&op) {
            return false;
        }
    }

    // Test stacks must include structured values, not just random ones.
    // Random Goldilocks values make eq/lt comparisons near-deterministic:
    //   P(random a == random b) ≈ 2^-64 → "push 0" fakes "dup 1 | dup 1 | eq"
    // Structured stacks expose these exploits by including zeros, duplicates,
    // and ordered values where comparisons actually produce different results.
    let test_stacks: Vec<Vec<u64>> = {
        let mut stacks = Vec::with_capacity(12);
        // 4 random seeds (catch arithmetic/stack depth errors)
        for i in 0..4u64 {
            let s = seed.wrapping_mul(6364136223846793005).wrapping_add(i);
            stacks.push(generate_test_stack(s, 16));
        }
        // All zeros (eq always returns 1, catches "push 0" faking eq)
        stacks.push(vec![0; 16]);
        // All ones
        stacks.push(vec![1; 16]);
        // Adjacent pairs equal: [5,5,3,3,7,7,...] (catches dup+eq exploits)
        stacks.push(vec![5, 5, 3, 3, 7, 7, 2, 2, 9, 9, 1, 1, 4, 4, 8, 8]);
        // Same value everywhere (catches any eq/dup combination)
        stacks.push(vec![42; 16]);
        // Ascending small values (catches lt/comparison order exploits)
        stacks.push((0..16).collect());
        // Descending (opposite lt behavior)
        stacks.push((0..16).rev().collect());
        // Mixed: zeros and large values (catches split/div_mod edge cases)
        stacks.push(vec![
            0,
            MODULUS - 1,
            0,
            1,
            0,
            MODULUS - 1,
            0,
            1,
            0,
            MODULUS - 1,
            0,
            1,
            0,
            MODULUS - 1,
            0,
            1,
        ]);
        // Powers of 2 (catches pop_count, log_2_floor, split edge cases)
        stacks.push(vec![
            1,
            2,
            4,
            8,
            16,
            32,
            64,
            128,
            256,
            512,
            1024,
            2048,
            1u64 << 32,
            1u64 << 33,
            1u64 << 48,
            1u64 << 63,
        ]);
        stacks
    };

    for test_stack in &test_stacks {
        let mut baseline_state = StackState::new(test_stack.clone());
        baseline_state.execute(baseline_tasm);

        // If baseline can't be simulated, we can't verify — reject candidate.
        if baseline_state.error {
            return false;
        }

        let mut candidate_state = StackState::new(test_stack.clone());
        candidate_state.execute(candidate_tasm);

        if candidate_state.error {
            return false;
        }

        if baseline_state.stack != candidate_state.stack {
            return false;
        }
    }
    true
}

/// Score a neural model's raw output against a baseline block.
/// Decodes the output, verifies equivalence, and returns the lower cost
/// (or baseline cost if candidate is invalid/worse).
pub fn score_neural_output(
    raw_codes: &[u32],
    block_baseline: u64,
    baseline_tasm: &[String],
    block_seed: u64,
) -> u64 {
    use crate::ir::tir::lower::decode_output;

    let codes: Vec<u64> = raw_codes
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u64)
        .collect();
    if codes.is_empty() {
        return block_baseline;
    }
    let candidate_lines = decode_output(&codes);
    if candidate_lines.is_empty() {
        return block_baseline;
    }
    // No baseline = nothing to verify against = reject.
    if baseline_tasm.is_empty() || !verify_equivalent(baseline_tasm, &candidate_lines, block_seed) {
        return block_baseline;
    }
    let profile = crate::cost::scorer::profile_tasm(
        &candidate_lines
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>(),
    );
    profile.cost().min(block_baseline)
}

/// Score improvement of a neural candidate over baseline.
/// Returns 0 for failures or equal/worse cost, positive value for genuine wins.
/// Used by training to reward only actual improvement (not negated cost).
pub fn score_neural_improvement(
    raw_codes: &[u32],
    block_baseline: u64,
    baseline_tasm: &[String],
    block_seed: u64,
) -> u64 {
    let cost = score_neural_output(raw_codes, block_baseline, baseline_tasm, block_seed);
    block_baseline.saturating_sub(cost)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(s: &[&str]) -> Vec<String> {
        s.iter().map(|l| l.to_string()).collect()
    }

    #[test]
    fn push_add() {
        let mut s = StackState::new(vec![]);
        s.execute(&lines(&["push 1", "push 2", "add"]));
        assert!(s.is_valid());
        assert_eq!(s.stack, vec![3]);
    }

    #[test]
    fn dup_swap() {
        let mut s = StackState::new(vec![10, 20]);
        s.execute(&lines(&["dup 1", "swap 1"]));
        assert!(s.is_valid());
        // [10, 20] → dup 1 → [10, 20, 10] → swap 1 → [10, 10, 20]
        assert_eq!(s.stack, vec![10, 10, 20]);
    }

    #[test]
    fn underflow_is_error() {
        let mut s = StackState::new(vec![]);
        s.execute(&lines(&["add"]));
        assert!(!s.is_valid());
    }

    #[test]
    fn goldilocks_arithmetic() {
        let mut s = StackState::new(vec![]);
        // push p-1, push 2, add → should wrap to 0 (since (p-1)+2 = p+1 ≡ 1 mod p... wait)
        // Actually (p-1) + 1 = p ≡ 0 mod p
        s.execute(&lines(&["push 18446744069414584320", "push 1", "add"]));
        assert!(s.is_valid());
        assert_eq!(s.stack, vec![0]); // (MODULUS - 1) + 1 = 0 mod p
    }

    #[test]
    fn mul_field() {
        let mut s = StackState::new(vec![]);
        s.execute(&lines(&["push 3", "push 5", "mul"]));
        assert!(s.is_valid());
        assert_eq!(s.stack, vec![15]);
    }

    #[test]
    fn split_instruction() {
        let mut s = StackState::new(vec![]);
        // 0x0000_0003_0000_0005 = 3 * 2^32 + 5
        let val = 3u64 * (1u64 << 32) + 5;
        s.stack.push(val);
        s.execute(&lines(&["split"]));
        assert!(s.is_valid());
        assert_eq!(s.stack, vec![3, 5]); // hi=3, lo=5
    }

    #[test]
    fn eq_comparison() {
        let mut s = StackState::new(vec![42, 42]);
        s.execute(&lines(&["eq"]));
        assert!(s.is_valid());
        assert_eq!(s.stack, vec![1]);

        let mut s2 = StackState::new(vec![42, 43]);
        s2.execute(&lines(&["eq"]));
        assert!(s2.is_valid());
        assert_eq!(s2.stack, vec![0]);
    }

    #[test]
    fn negative_push() {
        let mut s = StackState::new(vec![]);
        s.execute(&lines(&["push 5", "push -1", "add"]));
        assert!(s.is_valid());
        assert_eq!(s.stack, vec![4]);
    }

    #[test]
    fn control_flow_is_error() {
        let mut s = StackState::new(vec![1]);
        s.execute(&lines(&["skiz"]));
        assert!(!s.is_valid());
    }

    #[test]
    fn comments_and_labels_ignored() {
        let mut s = StackState::new(vec![]);
        s.execute(&lines(&["// comment", "__label:", "push 1", ""]));
        assert!(s.is_valid());
        assert_eq!(s.stack, vec![1]);
    }

    #[test]
    fn verify_equivalent_same() {
        let baseline = lines(&["push 1", "push 2", "add"]);
        let candidate = lines(&["push 3"]); // same result, different path
        assert!(verify_equivalent(&baseline, &candidate, 42));
    }

    #[test]
    fn verify_equivalent_different() {
        let baseline = lines(&["push 1", "push 2", "add"]);
        let candidate = lines(&["push 4"]); // different result
        assert!(!verify_equivalent(&baseline, &candidate, 42));
    }

    #[test]
    fn verify_with_stack_input() {
        // Both should add TOS to second element
        let baseline = lines(&["dup 0", "dup 2", "add"]);
        let candidate = lines(&["dup 0", "dup 2", "add"]);
        assert!(verify_equivalent(&baseline, &candidate, 123));
    }

    #[test]
    fn pow_instruction() {
        let mut s = StackState::new(vec![]);
        s.execute(&lines(&["push 2", "push 10", "pow"]));
        assert!(s.is_valid());
        assert_eq!(s.stack, vec![1024]); // 2^10
    }

    #[test]
    fn pop_count_instruction() {
        let mut s = StackState::new(vec![0b1010_1010]);
        s.execute(&lines(&["pop_count"]));
        assert!(s.is_valid());
        assert_eq!(s.stack, vec![4]);
    }

    #[test]
    fn sbox_pattern() {
        // x^5 via dup/mul chain (from poseidon baseline)
        let x = 7u64;
        let mut s = StackState::new(vec![x]);
        s.execute(&lines(&[
            "dup 0", "dup 0", "mul", // x, x^2
            "dup 0", "mul", // x, x^4
            "mul", // x^5
        ]));
        assert!(s.is_valid());
        // 7^5 = 16807
        assert_eq!(s.stack, vec![16807]);
    }

    #[test]
    fn verify_rejects_when_baseline_errors() {
        // Baseline has control flow (can't simulate) — candidate must be rejected
        let baseline = lines(&["push 1", "call some_fn", "add"]);
        let candidate = lines(&["push 42"]);
        assert!(!verify_equivalent(&baseline, &candidate, 42));
    }

    #[test]
    fn verify_rejects_when_candidate_errors() {
        let baseline = lines(&["push 1", "push 2", "add"]);
        let _candidate = lines(&["add"]); // underflow on empty-ish stack (after 16 elements, add pops 2 and pushes 1 — actually this succeeds)
                                          // Use a candidate that definitely errors
        let bad_candidate = lines(&["pop 100"]); // underflow
        assert!(!verify_equivalent(&baseline, &bad_candidate, 42));
    }

    #[test]
    fn verify_rejects_both_error() {
        // Both error — conservative: reject (no free passes)
        let baseline = lines(&["call foo"]); // errors
        let candidate = lines(&["call bar"]); // also errors
        assert!(!verify_equivalent(&baseline, &candidate, 42));
    }

    #[test]
    fn generate_test_stack_deterministic() {
        let a = generate_test_stack(42, 8);
        let b = generate_test_stack(42, 8);
        assert_eq!(a, b);
        // Different seed → different stack
        let c = generate_test_stack(43, 8);
        assert_ne!(a, c);
    }

    #[test]
    fn generate_test_stack_in_range() {
        let stack = generate_test_stack(99, 100);
        for val in &stack {
            assert!(*val < MODULUS, "value {} >= MODULUS", val);
        }
    }
}
