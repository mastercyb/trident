//! VOCAB v2 — full TASM instruction set vocabulary.
//!
//! Expands from v1's 64 tokens to the complete Triton VM ISA.
//! Token 0 = EOS (end of sequence). Every valid TASM instruction
//! that the model can emit has a unique token ID.

use std::collections::HashMap;

/// Maximum sequence length for decoder output.
pub const MAX_SEQ: usize = 256;

// ─── Vocabulary Definition ────────────────────────────────────────

/// Complete TASM instruction vocabulary.
/// Order: EOS, stack ops (push/pop/dup/swap/pick/place),
/// arithmetic, comparison, bitwise, control, I/O, memory,
/// crypto, assertions, extension field.
const TOKENS: &[&str] = &[
    // 0: End of sequence
    "",
    // ── push constants (1-14) ──
    "push -1",
    "push 0",
    "push 1",
    "push 2",
    "push 3",
    "push 4",
    "push 5",
    "push 6",
    "push 7",
    "push 8",
    "push 9",
    "push 10",
    "push 16",
    "push 32",
    // ── pop 1-5 (15-19) ──
    "pop 1",
    "pop 2",
    "pop 3",
    "pop 4",
    "pop 5",
    // ── dup 0-15 (20-35) ──
    "dup 0",
    "dup 1",
    "dup 2",
    "dup 3",
    "dup 4",
    "dup 5",
    "dup 6",
    "dup 7",
    "dup 8",
    "dup 9",
    "dup 10",
    "dup 11",
    "dup 12",
    "dup 13",
    "dup 14",
    "dup 15",
    // ── swap 1-15 (36-50) ──
    "swap 1",
    "swap 2",
    "swap 3",
    "swap 4",
    "swap 5",
    "swap 6",
    "swap 7",
    "swap 8",
    "swap 9",
    "swap 10",
    "swap 11",
    "swap 12",
    "swap 13",
    "swap 14",
    "swap 15",
    // ── pick 0-15 (51-66) ──
    "pick 0",
    "pick 1",
    "pick 2",
    "pick 3",
    "pick 4",
    "pick 5",
    "pick 6",
    "pick 7",
    "pick 8",
    "pick 9",
    "pick 10",
    "pick 11",
    "pick 12",
    "pick 13",
    "pick 14",
    "pick 15",
    // ── place 0-15 (67-82) ──
    "place 0",
    "place 1",
    "place 2",
    "place 3",
    "place 4",
    "place 5",
    "place 6",
    "place 7",
    "place 8",
    "place 9",
    "place 10",
    "place 11",
    "place 12",
    "place 13",
    "place 14",
    "place 15",
    // ── arithmetic (83-94) ──
    "add",
    "mul",
    "invert",
    "split",
    "div_mod",
    "pow",
    "log_2_floor",
    "pop_count",
    // ── comparison (91-92) ──
    "eq",
    "lt",
    // ── bitwise (93-95) ──
    "and",
    "xor",
    "or",
    // ── control (96-103) ──
    "nop",
    "halt",
    "assert",
    "assert_vector",
    "skiz",
    "return",
    "recurse",
    "recurse_or_return",
    // ── I/O: read_io 1-5 (104-108) ──
    "read_io 1",
    "read_io 2",
    "read_io 3",
    "read_io 4",
    "read_io 5",
    // ── I/O: write_io 1-5 (109-113) ──
    "write_io 1",
    "write_io 2",
    "write_io 3",
    "write_io 4",
    "write_io 5",
    // ── I/O: divine 1-5 (114-118) ──
    "divine 1",
    "divine 2",
    "divine 3",
    "divine 4",
    "divine 5",
    // ── memory: read_mem 1-5 (119-123) ──
    "read_mem 1",
    "read_mem 2",
    "read_mem 3",
    "read_mem 4",
    "read_mem 5",
    // ── memory: write_mem 1-5 (124-128) ──
    "write_mem 1",
    "write_mem 2",
    "write_mem 3",
    "write_mem 4",
    "write_mem 5",
    // ── crypto (129-135) ──
    "hash",
    "sponge_init",
    "sponge_absorb",
    "sponge_squeeze",
    "sponge_absorb_mem",
    "merkle_step",
    "merkle_step_mem",
    // ── extension field (136-139) ──
    "x_invert",
    "xb_mul",
    "xx_dot_step",
    "xb_dot_step",
];

/// Total vocabulary size including EOS.
pub const VOCAB_SIZE: usize = TOKENS.len();

// ─── Vocab Struct ─────────────────────────────────────────────────

/// Bidirectional vocabulary for encoding/decoding TASM instructions.
pub struct Vocab {
    encode_map: HashMap<String, u32>,
    decode_map: Vec<String>,
}

impl Vocab {
    /// Create a new vocabulary from the full TASM ISA.
    pub fn new() -> Self {
        let mut encode_map = HashMap::with_capacity(TOKENS.len());
        let mut decode_map = Vec::with_capacity(TOKENS.len());

        for (i, &token) in TOKENS.iter().enumerate() {
            encode_map.insert(token.to_string(), i as u32);
            decode_map.push(token.to_string());
        }

        Vocab {
            encode_map,
            decode_map,
        }
    }

    /// Encode a TASM instruction string to a token ID.
    pub fn encode(&self, line: &str) -> Option<u32> {
        self.encode_map.get(line.trim()).copied()
    }

    /// Decode a token ID to a TASM instruction string.
    pub fn decode(&self, code: u32) -> Option<&str> {
        self.decode_map.get(code as usize).map(|s| s.as_str())
    }

    /// Vocabulary size.
    pub fn size(&self) -> usize {
        self.decode_map.len()
    }

    /// EOS token ID (always 0).
    pub fn eos(&self) -> u32 {
        0
    }

    /// Encode a sequence of TASM lines, skipping unknown tokens.
    /// Appends EOS at the end.
    pub fn encode_sequence(&self, lines: &[String]) -> Vec<u32> {
        let mut codes: Vec<u32> = lines.iter().filter_map(|l| self.encode(l)).collect();
        codes.push(self.eos());
        codes
    }

    /// Decode a sequence of token IDs back to TASM lines.
    /// Stops at EOS.
    pub fn decode_sequence(&self, codes: &[u32]) -> Vec<String> {
        let mut lines = Vec::new();
        for &code in codes {
            if code == self.eos() {
                break;
            }
            if let Some(s) = self.decode(code) {
                if !s.is_empty() {
                    lines.push(s.to_string());
                }
            }
        }
        lines
    }
}

impl Default for Vocab {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vocab_size_is_correct() {
        let vocab = Vocab::new();
        assert_eq!(vocab.size(), VOCAB_SIZE);
        assert_eq!(vocab.size(), TOKENS.len());
        // Verify we have 140 tokens (EOS + 139 instructions)
        assert_eq!(VOCAB_SIZE, 140);
    }

    #[test]
    fn eos_is_zero() {
        let vocab = Vocab::new();
        assert_eq!(vocab.eos(), 0);
        assert_eq!(vocab.decode(0), Some(""));
    }

    #[test]
    fn roundtrip_encode_decode() {
        let vocab = Vocab::new();
        for (i, &token) in TOKENS.iter().enumerate().skip(1) {
            let encoded = vocab
                .encode(token)
                .unwrap_or_else(|| panic!("failed to encode '{}'", token));
            assert_eq!(encoded, i as u32, "wrong code for '{}'", token);
            let decoded = vocab.decode(encoded).unwrap();
            assert_eq!(decoded, token);
        }
    }

    #[test]
    fn all_dup_variants_present() {
        let vocab = Vocab::new();
        for d in 0..=15 {
            let token = format!("dup {}", d);
            assert!(vocab.encode(&token).is_some(), "missing {}", token);
        }
    }

    #[test]
    fn all_swap_variants_present() {
        let vocab = Vocab::new();
        for d in 1..=15 {
            let token = format!("swap {}", d);
            assert!(vocab.encode(&token).is_some(), "missing {}", token);
        }
    }

    #[test]
    fn all_pick_place_variants_present() {
        let vocab = Vocab::new();
        for d in 0..=15 {
            assert!(
                vocab.encode(&format!("pick {}", d)).is_some(),
                "missing pick {}",
                d
            );
            assert!(
                vocab.encode(&format!("place {}", d)).is_some(),
                "missing place {}",
                d
            );
        }
    }

    #[test]
    fn encode_sequence_appends_eos() {
        let vocab = Vocab::new();
        let lines = vec![
            "push 1".to_string(),
            "push 2".to_string(),
            "add".to_string(),
        ];
        let codes = vocab.encode_sequence(&lines);
        assert_eq!(codes.len(), 4);
        assert_eq!(*codes.last().unwrap(), 0); // EOS
    }

    #[test]
    fn decode_sequence_stops_at_eos() {
        let vocab = Vocab::new();
        let codes = vec![3, 83, 0, 85]; // push 1, add, EOS, invert (should stop at EOS)
        let lines = vocab.decode_sequence(&codes);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "push 1");
        assert_eq!(lines[1], "add");
    }

    #[test]
    fn unknown_token_returns_none() {
        let vocab = Vocab::new();
        assert_eq!(vocab.encode("push 999999"), None);
        assert_eq!(vocab.decode(9999), None);
    }

    #[test]
    fn no_duplicate_tokens() {
        let mut seen = std::collections::HashSet::new();
        for &token in TOKENS.iter().skip(1) {
            assert!(seen.insert(token), "duplicate token: '{}'", token);
        }
    }

    #[test]
    fn covers_v1_vocab() {
        // Every instruction from v1's 64-token VOCAB should be in v2
        let v1_instructions = [
            "push 0",
            "push 1",
            "push -1",
            "pop 1",
            "pop 2",
            "pop 3",
            "pop 4",
            "pop 5",
            "dup 0",
            "dup 1",
            "dup 2",
            "dup 3",
            "dup 4",
            "dup 5",
            "swap 1",
            "swap 2",
            "swap 3",
            "swap 4",
            "swap 5",
            "add",
            "mul",
            "eq",
            "lt",
            "and",
            "xor",
            "div_mod",
            "split",
            "pop_count",
            "log_2_floor",
            "nop",
            "assert",
            "dup 9",
            "write_io 1",
            "dup 11",
            "dup 12",
            "divine 1",
            "dup 14",
            "dup 15",
            "swap 10",
            "swap 11",
            "swap 12",
            "swap 13",
            "halt",
            "swap 15",
            "write_io 5",
            "pick 2",
            "pick 3",
            "divine 5",
            "pick 5",
            "place 1",
            "place 2",
            "place 3",
            "place 4",
            "place 5",
            "push 2",
            "push 3",
            "assert_vector",
            "dup 6",
            "dup 7",
            "swap 6",
            "swap 7",
            "swap 8",
            "swap 9",
        ];
        let vocab = Vocab::new();
        for inst in &v1_instructions {
            assert!(
                vocab.encode(inst).is_some(),
                "v1 instruction '{}' missing in v2",
                inst
            );
        }
    }
}
