//! Content addressing for Trident: AST normalization + Poseidon2 hashing.
//!
//! Every function definition gets a cryptographic identity (Poseidon2 hash)
//! based on its normalized AST. Names are replaced with de Bruijn indices,
//! dependency references are replaced with their hashes, and the result is
//! deterministically serialized before hashing.
//!
//! Uses Poseidon2 over the Goldilocks field for SNARK-friendly content
//! addressing — content hashes are cheaply provable inside ZK proofs,
//! enabling trustless compilation verification and on-chain registries.
//!
//! Properties:
//! - Two functions with identical computation but different variable names
//!   produce the same hash.
//! - Changing any dependency changes the hash of all dependents.
//! - Renaming a function does not change its hash.
//! - Adding/removing comments or formatting does not change the hash.

use std::collections::BTreeMap;

use crate::ast::*;

const HASH_VERSION: u8 = 1;

// ─── Content Hash ──────────────────────────────────────────────────

/// A 256-bit BLAKE3 content hash.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContentHash(pub [u8; 32]);

impl ContentHash {
    /// Zero hash (used as placeholder).
    pub fn zero() -> Self {
        Self([0u8; 32])
    }

    /// Display as full hex.
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{:02x}", b)).collect()
    }

    /// Parse a 64-character hex string into a ContentHash.
    pub fn from_hex(hex: &str) -> Option<Self> {
        if hex.len() != 64 {
            return None;
        }
        let mut bytes = [0u8; 32];
        for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
            if i >= 32 || chunk.len() < 2 {
                return None;
            }
            let hi = hex_digit(chunk[0])?;
            let lo = hex_digit(chunk[1])?;
            bytes[i] = (hi << 4) | lo;
        }
        Some(ContentHash(bytes))
    }

    /// Display as short base-32 (8 characters, 40 bits).
    pub fn to_short(&self) -> String {
        // Take first 5 bytes (40 bits), encode as base-32
        const ALPHABET: &[u8] = b"0123456789abcdefghjkmnpqrstuvwxyz";
        let val = u64::from_be_bytes([
            0, 0, 0, self.0[0], self.0[1], self.0[2], self.0[3], self.0[4],
        ]);
        let mut result = String::with_capacity(8);
        for i in (0..8).rev() {
            let idx = ((val >> (i * 5)) & 0x1F) as usize;
            result.push(ALPHABET[idx] as char);
        }
        result
    }
}

impl std::fmt::Debug for ContentHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}", self.to_short())
    }
}

impl std::fmt::Display for ContentHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}", self.to_short())
    }
}

pub(crate) mod normalize;
mod serialize;
pub use normalize::Normalizer;

// ─── Public API ────────────────────────────────────────────────────

/// Hash a single function.
pub fn hash_function(func: &FnDef, deps: BTreeMap<String, ContentHash>) -> ContentHash {
    Normalizer::hash_fn(func, deps)
}

/// Hash all functions in a file, returning name → hash map.
pub fn hash_file(file: &File) -> BTreeMap<String, ContentHash> {
    Normalizer::hash_file(file)
}

/// Hash a complete file's content (all items serialized together).
/// Uses Poseidon2 for SNARK-friendly file-level content addressing.
pub fn hash_file_content(file: &File) -> ContentHash {
    let fn_hashes = hash_file(file);
    let mut buf = Vec::new();
    buf.push(HASH_VERSION);
    // Hash file metadata
    buf.extend_from_slice(file.name.node.as_bytes());
    // Hash all function hashes in sorted order for determinism
    let mut sorted: Vec<_> = fn_hashes.iter().collect();
    sorted.sort_by_key(|(name, _)| (*name).clone());
    for (name, hash) in sorted {
        buf.extend_from_slice(name.as_bytes());
        buf.extend_from_slice(&hash.0);
    }
    ContentHash(crate::poseidon2::hash_bytes(&buf))
}

/// Parse a single hex digit (0-9, a-f, A-F) to its numeric value.
fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
