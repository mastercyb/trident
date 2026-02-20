//! Parallel validation and ranking of beam search candidates.
//!
//! Takes K candidate token sequences from beam search, decodes them
//! to TASM, validates equivalence with baseline using rayon parallel
//! iteration, and returns the cheapest valid candidate.

use rayon::prelude::*;

use crate::cost::scorer::profile_tasm;
use crate::cost::stack_verifier::verify_equivalent;
use crate::neural::model::vocab::Vocab;

/// Result of validating and ranking beam candidates.
pub struct RankedResult {
    /// Best valid TASM sequence (if any).
    pub tasm_lines: Vec<String>,
    /// Clock cycles (table cost) of the best candidate.
    pub cost: u64,
    /// How many candidates were valid out of total.
    pub valid_count: usize,
    /// Total candidates evaluated.
    pub total_count: usize,
}

/// Validate beam search candidates against a baseline and return the best.
///
/// Each candidate is decoded from token IDs to TASM strings, then verified
/// for equivalence with the baseline TASM using the stack verifier.
/// Valid candidates are profiled for cost, and the cheapest is returned.
///
/// Uses rayon for parallel validation across all K candidates.
///
/// Returns `None` if no valid candidate is found (fallback to compiler).
pub fn validate_and_rank(
    candidates: &[Vec<u32>],
    vocab: &Vocab,
    baseline_tasm: &[String],
    seed: u64,
) -> Option<RankedResult> {
    if candidates.is_empty() || baseline_tasm.is_empty() {
        return None;
    }

    let results: Vec<Option<(Vec<String>, u64)>> = candidates
        .par_iter()
        .map(|token_ids| {
            // Decode tokens to TASM lines
            let tasm_lines = vocab.decode_sequence(token_ids);
            if tasm_lines.is_empty() {
                return None;
            }

            // Verify equivalence with baseline
            if !verify_equivalent(baseline_tasm, &tasm_lines, seed) {
                return None;
            }

            // Profile for cost
            let line_refs: Vec<&str> = tasm_lines.iter().map(|s| s.as_str()).collect();
            let profile = profile_tasm(&line_refs);

            Some((tasm_lines, profile.cost()))
        })
        .collect();

    let valid_count = results.iter().filter(|r| r.is_some()).count();
    let total_count = candidates.len();

    // Find cheapest valid candidate
    let best = results
        .into_iter()
        .flatten()
        .min_by_key(|(_, cost)| *cost)?;

    Some(RankedResult {
        tasm_lines: best.0,
        cost: best.1,
        valid_count,
        total_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_empty_candidates() {
        let vocab = Vocab::new();
        let result = validate_and_rank(&[], &vocab, &["push 1".into()], 42);
        assert!(result.is_none());
    }

    #[test]
    fn validate_empty_baseline() {
        let vocab = Vocab::new();
        let result = validate_and_rank(&[vec![3, 0]], &vocab, &[], 42);
        assert!(result.is_none());
    }

    #[test]
    fn validate_equivalent_candidate() {
        let vocab = Vocab::new();
        // Baseline: push 1, push 2, add → result 3
        let baseline: Vec<String> = vec!["push 1".into(), "push 2".into(), "add".into()];
        // Candidate: push 3 (token 5) → same result
        let candidates = vec![vec![5]]; // push 3
        let result = validate_and_rank(&candidates, &vocab, &baseline, 42);
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.valid_count, 1);
        assert_eq!(r.tasm_lines, vec!["push 3"]);
    }

    #[test]
    fn validate_picks_cheapest() {
        let vocab = Vocab::new();
        // Baseline: push 3
        let baseline: Vec<String> = vec!["push 3".into()];
        // Two equivalent candidates:
        //   push 3 (1 instruction) — token 5
        //   push 3, nop (2 instructions) — tokens 5, 96
        let candidates = vec![
            vec![5, 96], // push 3, nop
            vec![5],     // push 3
        ];
        let result = validate_and_rank(&candidates, &vocab, &baseline, 42);
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.valid_count, 2);
        // Cheapest should be the 1-instruction version
        assert_eq!(r.tasm_lines, vec!["push 3"]);
    }

    #[test]
    fn validate_rejects_invalid() {
        let vocab = Vocab::new();
        let baseline: Vec<String> = vec!["push 1".into(), "push 2".into(), "add".into()];
        // Candidate: push 4 (wrong result)
        let candidates = vec![vec![6]]; // push 4
        let result = validate_and_rank(&candidates, &vocab, &baseline, 42);
        assert!(result.is_none());
    }
}
