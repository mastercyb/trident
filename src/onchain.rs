//! On-chain registry infrastructure: Merkle tree management, certificate
//! serialization, proof generation, and synchronization with the off-chain
//! registry.
//!
//! The on-chain registry stores definition entries in a depth-4 Merkle tree
//! (up to 16 entries per tree). Each leaf is:
//!   hash(content_hash[0..5], type_sig_hash, deps_hash, cert_hash, meta_hash, 0)
//!
//! Multiple trees can be chained via a root-of-roots for larger registries.
//!
//! This module provides:
//! - `MerkleRegistry`: in-memory Merkle tree of registry entries
//! - `RegistryEntry`: a single entry in the tree
//! - `VerificationCertificate`: serializable verification certificate
//! - `OnChainProof`: proof data for on-chain operations (register, verify, update)
//! - Sync logic: export/import between off-chain registry and on-chain trees

use crate::hash::ContentHash;
use std::collections::HashMap;

// ─── Constants ─────────────────────────────────────────────────────

/// Merkle tree depth (4 = 16 leaves).
pub const TREE_DEPTH: usize = 4;

/// Maximum entries per tree.
pub const MAX_ENTRIES: usize = 1 << TREE_DEPTH;

/// Goldilocks prime: p = 2^64 - 2^32 + 1
const GOLDILOCKS_P: u64 = 0xFFFF_FFFF_0000_0001;

// ─── Field Element ─────────────────────────────────────────────────

/// A Goldilocks field element (u64 < p).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FieldElement(pub u64);

impl FieldElement {
    pub fn zero() -> Self {
        Self(0)
    }

    pub fn new(val: u64) -> Self {
        Self(val % GOLDILOCKS_P)
    }

    /// Add two field elements mod p.
    pub fn add(self, other: Self) -> Self {
        let sum = (self.0 as u128) + (other.0 as u128);
        Self((sum % GOLDILOCKS_P as u128) as u64)
    }

    /// Multiply two field elements mod p.
    pub fn mul(self, other: Self) -> Self {
        let prod = (self.0 as u128) * (other.0 as u128);
        Self((prod % GOLDILOCKS_P as u128) as u64)
    }
}

impl std::fmt::Display for FieldElement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ─── Digest ────────────────────────────────────────────────────────

/// A Tip5 digest: 5 Goldilocks field elements.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Digest(pub [FieldElement; 5]);

impl Digest {
    pub fn zero() -> Self {
        Self([FieldElement::zero(); 5])
    }

    /// Create from 5 u64 values.
    pub fn from_u64s(vals: [u64; 5]) -> Self {
        Self([
            FieldElement::new(vals[0]),
            FieldElement::new(vals[1]),
            FieldElement::new(vals[2]),
            FieldElement::new(vals[3]),
            FieldElement::new(vals[4]),
        ])
    }

    /// Convert a ContentHash (32 bytes) into a Digest by packing bytes
    /// into field elements. First 5 elements from 6-byte chunks, last
    /// 2 bytes go into element 4.
    pub fn from_content_hash(hash: &ContentHash) -> Self {
        let b = &hash.0;
        // Pack 6 bytes per field element (48 bits), except last gets remainder
        let e0 = u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], 0, 0]);
        let e1 = u64::from_le_bytes([b[6], b[7], b[8], b[9], b[10], b[11], 0, 0]);
        let e2 = u64::from_le_bytes([b[12], b[13], b[14], b[15], b[16], b[17], 0, 0]);
        let e3 = u64::from_le_bytes([b[18], b[19], b[20], b[21], b[22], b[23], 0, 0]);
        let e4 = u64::from_le_bytes([b[24], b[25], b[26], b[27], b[28], b[29], b[30], b[31]]);
        Self::from_u64s([e0, e1, e2, e3, e4])
    }

    /// Serialize to a hex string (each element as 16-char hex, total 80 chars).
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|e| format!("{:016x}", e.0)).collect()
    }

    /// Deserialize from hex string (80 chars = 5 × 16).
    pub fn from_hex(hex: &str) -> Option<Self> {
        if hex.len() != 80 {
            return None;
        }
        let mut elems = [FieldElement::zero(); 5];
        for i in 0..5 {
            let chunk = &hex[i * 16..(i + 1) * 16];
            let val = u64::from_str_radix(chunk, 16).ok()?;
            elems[i] = FieldElement::new(val);
        }
        Some(Self(elems))
    }

    /// Simple simulated Tip5 hash (for tree construction).
    /// This is NOT the real Tip5 permutation — it's a deterministic
    /// stand-in used for building Merkle trees off-chain. The actual
    /// Tip5 runs in the VM during proof generation.
    pub fn simulated_hash(input: &[FieldElement; 10]) -> Self {
        // Use BLAKE3 on the serialized input to simulate Tip5.
        let mut data = Vec::with_capacity(80);
        for elem in input {
            data.extend_from_slice(&elem.0.to_le_bytes());
        }
        let hash = blake3::hash(&data);
        let bytes = hash.as_bytes();
        // Pack into 5 field elements
        let e0 = u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], 0, 0,
        ]);
        let e1 = u64::from_le_bytes([
            bytes[6], bytes[7], bytes[8], bytes[9], bytes[10], bytes[11], 0, 0,
        ]);
        let e2 = u64::from_le_bytes([
            bytes[12], bytes[13], bytes[14], bytes[15], bytes[16], bytes[17], 0, 0,
        ]);
        let e3 = u64::from_le_bytes([
            bytes[18], bytes[19], bytes[20], bytes[21], bytes[22], bytes[23], 0, 0,
        ]);
        let e4 = u64::from_le_bytes([
            bytes[24], bytes[25], bytes[26], bytes[27], bytes[28], bytes[29], bytes[30], bytes[31],
        ]);
        Self::from_u64s([e0, e1, e2, e3, e4])
    }
}

// ─── Registry Entry ────────────────────────────────────────────────

/// Verification verdict (matches Trident's solve.rs Verdict enum).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnChainVerdict {
    Unknown = 0,
    Safe = 1,
    StaticViolation = 2,
    RandomViolation = 3,
    BmcViolation = 4,
}

impl OnChainVerdict {
    pub fn from_field(val: u64) -> Self {
        match val {
            1 => Self::Safe,
            2 => Self::StaticViolation,
            3 => Self::RandomViolation,
            4 => Self::BmcViolation,
            _ => Self::Unknown,
        }
    }

    pub fn to_field(self) -> FieldElement {
        FieldElement::new(self as u64)
    }
}

impl std::fmt::Display for OnChainVerdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown => write!(f, "unknown"),
            Self::Safe => write!(f, "safe"),
            Self::StaticViolation => write!(f, "static_violation"),
            Self::RandomViolation => write!(f, "random_violation"),
            Self::BmcViolation => write!(f, "bmc_violation"),
        }
    }
}

/// A verification certificate that can be stored on-chain.
#[derive(Clone, Debug)]
pub struct VerificationCertificate {
    pub verdict: OnChainVerdict,
    pub constraints_checked: u64,
    pub rounds: u64,
    pub timestamp: u64,
    pub verifier_auth: FieldElement,
}

impl VerificationCertificate {
    /// Hash this certificate to a single field element (for tree entry).
    pub fn hash(&self) -> Digest {
        let input = [
            self.verdict.to_field(),
            FieldElement::new(self.constraints_checked),
            FieldElement::new(self.rounds),
            FieldElement::new(self.timestamp),
            self.verifier_auth,
            FieldElement::zero(),
            FieldElement::zero(),
            FieldElement::zero(),
            FieldElement::zero(),
            FieldElement::zero(),
        ];
        Digest::simulated_hash(&input)
    }

    /// Serialize to JSON.
    pub fn to_json(&self) -> String {
        format!(
            "{{\"verdict\":\"{}\",\"constraints_checked\":{},\"rounds\":{},\"timestamp\":{},\"verifier_auth\":\"{}\"}}",
            self.verdict,
            self.constraints_checked,
            self.rounds,
            self.timestamp,
            self.verifier_auth.0
        )
    }

    /// Parse from JSON.
    pub fn from_json(json: &str) -> Option<Self> {
        let verdict_str = extract_json_value(json, "verdict")?;
        let verdict = match verdict_str.as_str() {
            "safe" => OnChainVerdict::Safe,
            "static_violation" => OnChainVerdict::StaticViolation,
            "random_violation" => OnChainVerdict::RandomViolation,
            "bmc_violation" => OnChainVerdict::BmcViolation,
            _ => OnChainVerdict::Unknown,
        };
        let constraints = extract_json_value(json, "constraints_checked")?
            .parse()
            .ok()?;
        let rounds = extract_json_value(json, "rounds")?.parse().ok()?;
        let timestamp = extract_json_value(json, "timestamp")?.parse().ok()?;
        let verifier_auth = extract_json_value(json, "verifier_auth")?
            .parse::<u64>()
            .ok()?;
        Some(Self {
            verdict,
            constraints_checked: constraints,
            rounds,
            timestamp,
            verifier_auth: FieldElement::new(verifier_auth),
        })
    }
}

/// Metadata for an on-chain registry entry.
#[derive(Clone, Debug)]
pub struct EntryMetadata {
    pub publisher_auth: FieldElement,
    pub timestamp: u64,
    pub tag_hash: FieldElement,
    pub name_hash: FieldElement,
}

impl EntryMetadata {
    pub fn hash(&self) -> Digest {
        let input = [
            self.publisher_auth,
            FieldElement::new(self.timestamp),
            self.tag_hash,
            self.name_hash,
            FieldElement::zero(),
            FieldElement::zero(),
            FieldElement::zero(),
            FieldElement::zero(),
            FieldElement::zero(),
            FieldElement::zero(),
        ];
        Digest::simulated_hash(&input)
    }
}

/// A single entry in the on-chain registry Merkle tree.
#[derive(Clone, Debug)]
pub struct RegistryEntry {
    /// Content hash of the function (from BLAKE3 normalization).
    pub content_hash: ContentHash,
    /// Hash of the type signature (params → return type).
    pub type_sig_hash: FieldElement,
    /// Hash of the dependency list.
    pub deps_hash: FieldElement,
    /// Verification certificate (None = unverified).
    pub certificate: Option<VerificationCertificate>,
    /// Metadata (publisher, timestamp, tags, name).
    pub metadata: EntryMetadata,
}

impl RegistryEntry {
    /// Compute the Merkle leaf for this entry.
    pub fn leaf_hash(&self) -> Digest {
        let content_digest = Digest::from_content_hash(&self.content_hash);
        let cert_hash = self
            .certificate
            .as_ref()
            .map(|c| c.hash().0[0])
            .unwrap_or(FieldElement::zero());
        let meta_hash = self.metadata.hash().0[0];

        let input = [
            content_digest.0[0],
            content_digest.0[1],
            content_digest.0[2],
            content_digest.0[3],
            content_digest.0[4],
            self.type_sig_hash,
            self.deps_hash,
            cert_hash,
            meta_hash,
            FieldElement::zero(),
        ];
        Digest::simulated_hash(&input)
    }

    /// Serialize to JSON.
    pub fn to_json(&self) -> String {
        let cert_json = self
            .certificate
            .as_ref()
            .map(|c| c.to_json())
            .unwrap_or_else(|| "null".to_string());
        format!(
            "{{\"content_hash\":\"{}\",\"type_sig_hash\":\"{}\",\"deps_hash\":\"{}\",\"certificate\":{},\"publisher_auth\":\"{}\",\"timestamp\":{},\"tag_hash\":\"{}\",\"name_hash\":\"{}\"}}",
            self.content_hash.to_hex(),
            self.type_sig_hash.0,
            self.deps_hash.0,
            cert_json,
            self.metadata.publisher_auth.0,
            self.metadata.timestamp,
            self.metadata.tag_hash.0,
            self.metadata.name_hash.0,
        )
    }
}

// ─── Merkle Tree ───────────────────────────────────────────────────

/// A Merkle proof: sibling digests from leaf to root.
#[derive(Clone, Debug)]
pub struct MerkleProof {
    /// Leaf index in the tree.
    pub leaf_index: usize,
    /// Sibling digests from leaf level upward.
    pub siblings: Vec<Digest>,
}

impl MerkleProof {
    /// Verify this proof against a known root.
    pub fn verify(&self, leaf: Digest, root: Digest) -> bool {
        let mut current = leaf;
        let mut idx = self.leaf_index;
        for sibling in &self.siblings {
            current = if idx % 2 == 0 {
                hash_pair(&current, sibling)
            } else {
                hash_pair(sibling, &current)
            };
            idx /= 2;
        }
        current == root
    }

    /// Serialize to JSON.
    pub fn to_json(&self) -> String {
        let siblings: Vec<String> = self
            .siblings
            .iter()
            .map(|s| format!("\"{}\"", s.to_hex()))
            .collect();
        format!(
            "{{\"leaf_index\":{},\"siblings\":[{}]}}",
            self.leaf_index,
            siblings.join(",")
        )
    }
}

/// Hash two child digests to produce a parent.
fn hash_pair(left: &Digest, right: &Digest) -> Digest {
    let input = [
        left.0[0], left.0[1], left.0[2], left.0[3], left.0[4], right.0[0], right.0[1], right.0[2],
        right.0[3], right.0[4],
    ];
    Digest::simulated_hash(&input)
}

/// Compute the empty leaf hash: hash(0, 0, 0, 0, 0, 0, 0, 0, 0, 0).
fn empty_leaf() -> Digest {
    Digest::simulated_hash(&[FieldElement::zero(); 10])
}

/// In-memory Merkle tree for the on-chain registry.
///
/// Depth-4 binary tree with 16 leaves. Each leaf is either an entry hash
/// or the empty leaf hash. Internal nodes are hash_pair(left, right).
pub struct MerkleRegistry {
    /// Stored entries (by leaf index).
    entries: HashMap<usize, RegistryEntry>,
    /// Leaf hashes (16 leaves).
    leaves: [Digest; MAX_ENTRIES],
    /// Name → leaf index mapping.
    name_index: HashMap<String, usize>,
    /// Content hash → leaf index mapping.
    hash_index: HashMap<ContentHash, usize>,
    /// Next free leaf index.
    next_free: usize,
}

impl MerkleRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        let empty = empty_leaf();
        Self {
            entries: HashMap::new(),
            leaves: [empty; MAX_ENTRIES],
            name_index: HashMap::new(),
            hash_index: HashMap::new(),
            next_free: 0,
        }
    }

    /// Get the current Merkle root.
    pub fn root(&self) -> Digest {
        self.compute_root()
    }

    /// Number of registered entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Is the registry empty?
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Register a new entry. Returns the leaf index and proof, or an error.
    pub fn register(
        &mut self,
        entry: RegistryEntry,
        name: Option<&str>,
    ) -> Result<(usize, MerkleProof), String> {
        if self.next_free >= MAX_ENTRIES {
            return Err(format!(
                "registry full (max {} entries per tree)",
                MAX_ENTRIES
            ));
        }
        if self.hash_index.contains_key(&entry.content_hash) {
            return Err("definition already registered".to_string());
        }

        let idx = self.next_free;
        self.next_free += 1;

        let leaf = entry.leaf_hash();
        self.leaves[idx] = leaf;
        self.hash_index.insert(entry.content_hash, idx);
        if let Some(n) = name {
            self.name_index.insert(n.to_string(), idx);
        }
        self.entries.insert(idx, entry);

        let proof = self.prove(idx);
        Ok((idx, proof))
    }

    /// Look up an entry by content hash.
    pub fn lookup_by_hash(&self, hash: &ContentHash) -> Option<(usize, &RegistryEntry)> {
        let idx = self.hash_index.get(hash)?;
        let entry = self.entries.get(idx)?;
        Some((*idx, entry))
    }

    /// Look up an entry by name.
    pub fn lookup_by_name(&self, name: &str) -> Option<(usize, &RegistryEntry)> {
        let idx = self.name_index.get(name)?;
        let entry = self.entries.get(idx)?;
        Some((*idx, entry))
    }

    /// Update the verification certificate for an existing entry.
    pub fn update_certificate(
        &mut self,
        hash: &ContentHash,
        cert: VerificationCertificate,
    ) -> Result<(usize, MerkleProof), String> {
        let idx = *self
            .hash_index
            .get(hash)
            .ok_or_else(|| "definition not found in registry".to_string())?;
        let entry = self
            .entries
            .get_mut(&idx)
            .ok_or_else(|| "entry not found".to_string())?;
        entry.certificate = Some(cert);
        self.leaves[idx] = entry.leaf_hash();

        let proof = self.prove(idx);
        Ok((idx, proof))
    }

    /// Generate a Merkle proof for a leaf.
    pub fn prove(&self, leaf_index: usize) -> MerkleProof {
        let mut siblings = Vec::with_capacity(TREE_DEPTH);
        let mut nodes = self.leaves.to_vec();
        let mut idx = leaf_index;

        for _ in 0..TREE_DEPTH {
            let sibling_idx = idx ^ 1;
            siblings.push(if sibling_idx < nodes.len() {
                nodes[sibling_idx]
            } else {
                empty_leaf()
            });

            let mut next_level = Vec::with_capacity(nodes.len() / 2);
            for pair in nodes.chunks(2) {
                let left = pair[0];
                let right = if pair.len() > 1 {
                    pair[1]
                } else {
                    empty_leaf()
                };
                next_level.push(hash_pair(&left, &right));
            }
            nodes = next_level;
            idx /= 2;
        }

        MerkleProof {
            leaf_index,
            siblings,
        }
    }

    /// List all entries.
    pub fn entries(&self) -> Vec<(usize, &RegistryEntry)> {
        let mut result: Vec<_> = self.entries.iter().map(|(i, e)| (*i, e)).collect();
        result.sort_by_key(|(i, _)| *i);
        result
    }

    /// Serialize the full registry state to JSON.
    pub fn to_json(&self) -> String {
        let root = self.root();
        let entries_json: Vec<String> = self
            .entries()
            .iter()
            .map(|(idx, entry)| format!("{{\"index\":{},{}}}", idx, &entry.to_json()[1..]))
            .collect();
        let names_json: Vec<String> = self
            .name_index
            .iter()
            .map(|(name, idx)| format!("{{\"name\":\"{}\",\"index\":{}}}", name, idx))
            .collect();
        format!(
            "{{\"root\":\"{}\",\"depth\":{},\"entries\":[{}],\"names\":[{}],\"count\":{},\"capacity\":{}}}",
            root.to_hex(),
            TREE_DEPTH,
            entries_json.join(","),
            names_json.join(","),
            self.entries.len(),
            MAX_ENTRIES,
        )
    }

    // ─── Internal ──────────────────────────────────────────────────

    fn compute_root(&self) -> Digest {
        let mut nodes = self.leaves.to_vec();
        for _ in 0..TREE_DEPTH {
            let mut next_level = Vec::with_capacity(nodes.len() / 2);
            for pair in nodes.chunks(2) {
                let left = pair[0];
                let right = if pair.len() > 1 {
                    pair[1]
                } else {
                    empty_leaf()
                };
                next_level.push(hash_pair(&left, &right));
            }
            nodes = next_level;
        }
        nodes[0]
    }
}

// ─── On-Chain Proof Generation ─────────────────────────────────────

/// Proof data for the `register` operation (op 0).
#[derive(Clone, Debug)]
pub struct RegisterProof {
    pub old_root: Digest,
    pub new_root: Digest,
    pub leaf_index: usize,
    pub content_digest: Digest,
    pub type_sig_hash: FieldElement,
    pub deps_hash: FieldElement,
    pub meta_hash: FieldElement,
    pub old_proof: MerkleProof,
    pub new_proof: MerkleProof,
}

impl RegisterProof {
    /// Serialize to JSON for use by the prover.
    pub fn to_json(&self) -> String {
        format!(
            "{{\"operation\":\"register\",\"old_root\":\"{}\",\"new_root\":\"{}\",\"leaf_index\":{},\"content_digest\":\"{}\",\"type_sig_hash\":\"{}\",\"deps_hash\":\"{}\",\"meta_hash\":\"{}\",\"old_proof\":{},\"new_proof\":{}}}",
            self.old_root.to_hex(),
            self.new_root.to_hex(),
            self.leaf_index,
            self.content_digest.to_hex(),
            self.type_sig_hash.0,
            self.deps_hash.0,
            self.meta_hash.0,
            self.old_proof.to_json(),
            self.new_proof.to_json(),
        )
    }
}

/// Proof data for the `verify_membership` operation (op 1).
#[derive(Clone, Debug)]
pub struct MembershipProof {
    pub root: Digest,
    pub content_digest: Digest,
    pub leaf_index: usize,
    pub type_sig_hash: FieldElement,
    pub deps_hash: FieldElement,
    pub cert_hash: FieldElement,
    pub meta_hash: FieldElement,
    pub proof: MerkleProof,
}

impl MembershipProof {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"operation\":\"verify\",\"root\":\"{}\",\"content_digest\":\"{}\",\"leaf_index\":{},\"type_sig_hash\":\"{}\",\"deps_hash\":\"{}\",\"cert_hash\":\"{}\",\"meta_hash\":\"{}\",\"proof\":{}}}",
            self.root.to_hex(),
            self.content_digest.to_hex(),
            self.leaf_index,
            self.type_sig_hash.0,
            self.deps_hash.0,
            self.cert_hash.0,
            self.meta_hash.0,
            self.proof.to_json(),
        )
    }
}

/// Proof data for the `update_certificate` operation (op 2).
#[derive(Clone, Debug)]
pub struct UpdateCertProof {
    pub old_root: Digest,
    pub new_root: Digest,
    pub leaf_index: usize,
    pub content_digest: Digest,
    pub type_sig_hash: FieldElement,
    pub deps_hash: FieldElement,
    pub old_cert_hash: FieldElement,
    pub meta_hash: FieldElement,
    pub new_certificate: VerificationCertificate,
    pub old_proof: MerkleProof,
    pub new_proof: MerkleProof,
}

impl UpdateCertProof {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"operation\":\"update_certificate\",\"old_root\":\"{}\",\"new_root\":\"{}\",\"leaf_index\":{},\"content_digest\":\"{}\",\"type_sig_hash\":\"{}\",\"deps_hash\":\"{}\",\"old_cert_hash\":\"{}\",\"meta_hash\":\"{}\",\"new_certificate\":{},\"old_proof\":{},\"new_proof\":{}}}",
            self.old_root.to_hex(),
            self.new_root.to_hex(),
            self.leaf_index,
            self.content_digest.to_hex(),
            self.type_sig_hash.0,
            self.deps_hash.0,
            self.old_cert_hash.0,
            self.meta_hash.0,
            self.new_certificate.to_json(),
            self.old_proof.to_json(),
            self.new_proof.to_json(),
        )
    }
}

/// Proof data for an equivalence claim (op 4).
#[derive(Clone, Debug)]
pub struct EquivalenceProof {
    pub old_root: Digest,
    pub new_root: Digest,
    pub leaf_index: usize,
    pub hash_a: Digest,
    pub hash_b: Digest,
    pub method: u64,
    pub verifier_auth: FieldElement,
    pub old_proof: MerkleProof,
    pub new_proof: MerkleProof,
}

impl EquivalenceProof {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"operation\":\"equivalence\",\"old_root\":\"{}\",\"new_root\":\"{}\",\"leaf_index\":{},\"hash_a\":\"{}\",\"hash_b\":\"{}\",\"method\":{},\"verifier_auth\":\"{}\",\"old_proof\":{},\"new_proof\":{}}}",
            self.old_root.to_hex(),
            self.new_root.to_hex(),
            self.leaf_index,
            self.hash_a.to_hex(),
            self.hash_b.to_hex(),
            self.method,
            self.verifier_auth.0,
            self.old_proof.to_json(),
            self.new_proof.to_json(),
        )
    }
}

// ─── Sync: Off-chain ↔ On-chain ───────────────────────────────────

/// Generate proof data for registering a definition from the off-chain
/// registry into an on-chain Merkle tree.
pub fn generate_register_proof(
    registry: &mut MerkleRegistry,
    entry: RegistryEntry,
    name: Option<&str>,
) -> Result<RegisterProof, String> {
    let old_root = registry.root();

    // Generate proof of empty old leaf at the target index
    let target_idx = registry.next_free;
    if target_idx >= MAX_ENTRIES {
        return Err(format!("registry full (max {} entries)", MAX_ENTRIES));
    }
    let old_proof = registry.prove(target_idx);

    let content_digest = Digest::from_content_hash(&entry.content_hash);
    let type_sig_hash = entry.type_sig_hash;
    let deps_hash = entry.deps_hash;
    let meta_hash = entry.metadata.hash().0[0];

    // Register the entry (mutates tree)
    let (idx, new_proof) = registry.register(entry, name)?;
    let new_root = registry.root();

    Ok(RegisterProof {
        old_root,
        new_root,
        leaf_index: idx,
        content_digest,
        type_sig_hash,
        deps_hash,
        meta_hash,
        old_proof,
        new_proof,
    })
}

/// Generate proof data for verifying membership of a definition.
pub fn generate_membership_proof(
    registry: &MerkleRegistry,
    content_hash: &ContentHash,
) -> Result<MembershipProof, String> {
    let (idx, entry) = registry
        .lookup_by_hash(content_hash)
        .ok_or_else(|| "definition not found in on-chain registry".to_string())?;

    let content_digest = Digest::from_content_hash(&entry.content_hash);
    let cert_hash = entry
        .certificate
        .as_ref()
        .map(|c| c.hash().0[0])
        .unwrap_or(FieldElement::zero());
    let meta_hash = entry.metadata.hash().0[0];
    let proof = registry.prove(idx);

    Ok(MembershipProof {
        root: registry.root(),
        content_digest,
        leaf_index: idx,
        type_sig_hash: entry.type_sig_hash,
        deps_hash: entry.deps_hash,
        cert_hash,
        meta_hash,
        proof,
    })
}

/// Generate proof data for updating a verification certificate.
pub fn generate_update_cert_proof(
    registry: &mut MerkleRegistry,
    content_hash: &ContentHash,
    cert: VerificationCertificate,
) -> Result<UpdateCertProof, String> {
    let old_root = registry.root();

    let (idx, entry) = registry
        .lookup_by_hash(content_hash)
        .ok_or_else(|| "definition not found".to_string())?;
    let content_digest = Digest::from_content_hash(&entry.content_hash);
    let type_sig_hash = entry.type_sig_hash;
    let deps_hash = entry.deps_hash;
    let old_cert_hash = entry
        .certificate
        .as_ref()
        .map(|c| c.hash().0[0])
        .unwrap_or(FieldElement::zero());
    let meta_hash = entry.metadata.hash().0[0];
    let old_proof = registry.prove(idx);

    let (_, new_proof) = registry.update_certificate(content_hash, cert.clone())?;
    let new_root = registry.root();

    Ok(UpdateCertProof {
        old_root,
        new_root,
        leaf_index: idx,
        content_digest,
        type_sig_hash,
        deps_hash,
        old_cert_hash,
        meta_hash,
        new_certificate: cert,
        old_proof,
        new_proof,
    })
}

// ─── JSON Helpers ──────────────────────────────────────────────────

fn extract_json_value(json: &str, key: &str) -> Option<String> {
    let needle = format!("\"{}\":", key);
    let pos = json.find(&needle)?;
    let after = json[pos + needle.len()..].trim_start();
    if after.starts_with('"') {
        // String value
        let inner = &after[1..];
        let end = inner.find('"')?;
        Some(inner[..end].to_string())
    } else {
        // Numeric or bool value
        let end = after.find(|c: char| c == ',' || c == '}' || c == ']')?;
        Some(after[..end].trim().to_string())
    }
}

// ─── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_hash() -> ContentHash {
        ContentHash([
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
            0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c,
            0x1d, 0x1e, 0x1f, 0x20,
        ])
    }

    fn test_entry(hash: ContentHash) -> RegistryEntry {
        RegistryEntry {
            content_hash: hash,
            type_sig_hash: FieldElement::new(42),
            deps_hash: FieldElement::new(0),
            certificate: None,
            metadata: EntryMetadata {
                publisher_auth: FieldElement::new(100),
                timestamp: 1700000000,
                tag_hash: FieldElement::zero(),
                name_hash: FieldElement::zero(),
            },
        }
    }

    #[test]
    fn test_field_element_arithmetic() {
        let a = FieldElement::new(10);
        let b = FieldElement::new(20);
        assert_eq!(a.add(b).0, 30);
        assert_eq!(a.mul(b).0, 200);
    }

    #[test]
    fn test_field_element_modular() {
        let max = FieldElement::new(GOLDILOCKS_P - 1);
        let one = FieldElement::new(1);
        assert_eq!(max.add(one).0, 0);
    }

    #[test]
    fn test_digest_from_content_hash() {
        let hash = test_hash();
        let digest = Digest::from_content_hash(&hash);
        assert_ne!(digest, Digest::zero());
    }

    #[test]
    fn test_digest_hex_roundtrip() {
        let digest = Digest::from_u64s([1, 2, 3, 4, 5]);
        let hex = digest.to_hex();
        let parsed = Digest::from_hex(&hex).unwrap();
        assert_eq!(digest, parsed);
    }

    #[test]
    fn test_digest_hex_invalid() {
        assert!(Digest::from_hex("too_short").is_none());
        assert!(Digest::from_hex(&"g".repeat(80)).is_none());
    }

    #[test]
    fn test_simulated_hash_deterministic() {
        let input = [FieldElement::new(1); 10];
        let h1 = Digest::simulated_hash(&input);
        let h2 = Digest::simulated_hash(&input);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_simulated_hash_different_inputs() {
        let a = [FieldElement::new(1); 10];
        let mut b = a;
        b[0] = FieldElement::new(2);
        assert_ne!(Digest::simulated_hash(&a), Digest::simulated_hash(&b));
    }

    #[test]
    fn test_empty_registry() {
        let reg = MerkleRegistry::new();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
        let root = reg.root();
        assert_ne!(root, Digest::zero());
    }

    #[test]
    fn test_register_entry() {
        let mut reg = MerkleRegistry::new();
        let hash = test_hash();
        let entry = test_entry(hash);
        let result = reg.register(entry, Some("add"));
        assert!(result.is_ok());
        let (idx, proof) = result.unwrap();
        assert_eq!(idx, 0);
        assert_eq!(proof.siblings.len(), TREE_DEPTH);
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn test_register_changes_root() {
        let mut reg = MerkleRegistry::new();
        let root_before = reg.root();
        let entry = test_entry(test_hash());
        reg.register(entry, None).unwrap();
        let root_after = reg.root();
        assert_ne!(root_before, root_after);
    }

    #[test]
    fn test_lookup_by_hash() {
        let mut reg = MerkleRegistry::new();
        let hash = test_hash();
        let entry = test_entry(hash);
        reg.register(entry, Some("add")).unwrap();
        let result = reg.lookup_by_hash(&hash);
        assert!(result.is_some());
        let (idx, _) = result.unwrap();
        assert_eq!(idx, 0);
    }

    #[test]
    fn test_lookup_by_name() {
        let mut reg = MerkleRegistry::new();
        let entry = test_entry(test_hash());
        reg.register(entry, Some("my_func")).unwrap();
        let result = reg.lookup_by_name("my_func");
        assert!(result.is_some());
        assert!(reg.lookup_by_name("nonexistent").is_none());
    }

    #[test]
    fn test_duplicate_registration() {
        let mut reg = MerkleRegistry::new();
        let hash = test_hash();
        let entry1 = test_entry(hash);
        let entry2 = test_entry(hash);
        reg.register(entry1, None).unwrap();
        let result = reg.register(entry2, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already registered"));
    }

    #[test]
    fn test_registry_full() {
        let mut reg = MerkleRegistry::new();
        for i in 0..MAX_ENTRIES {
            let mut bytes = [0u8; 32];
            bytes[0] = i as u8;
            bytes[1] = (i >> 8) as u8;
            let entry = test_entry(ContentHash(bytes));
            reg.register(entry, None).unwrap();
        }
        assert_eq!(reg.len(), MAX_ENTRIES);
        let entry = test_entry(ContentHash([0xff; 32]));
        let result = reg.register(entry, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("full"));
    }

    #[test]
    fn test_merkle_proof_valid() {
        let mut reg = MerkleRegistry::new();
        let hash = test_hash();
        let entry = test_entry(hash);
        let leaf = entry.leaf_hash();
        let (idx, _) = reg.register(entry, None).unwrap();
        let proof = reg.prove(idx);
        let root = reg.root();
        assert!(proof.verify(leaf, root));
    }

    #[test]
    fn test_merkle_proof_invalid() {
        let mut reg = MerkleRegistry::new();
        let entry = test_entry(test_hash());
        let (idx, _) = reg.register(entry, None).unwrap();
        let proof = reg.prove(idx);
        let root = reg.root();
        // Wrong leaf should fail
        assert!(!proof.verify(Digest::zero(), root));
    }

    #[test]
    fn test_update_certificate() {
        let mut reg = MerkleRegistry::new();
        let hash = test_hash();
        let entry = test_entry(hash);
        reg.register(entry, None).unwrap();
        let root_before = reg.root();

        let cert = VerificationCertificate {
            verdict: OnChainVerdict::Safe,
            constraints_checked: 42,
            rounds: 100,
            timestamp: 1700000001,
            verifier_auth: FieldElement::new(999),
        };
        let result = reg.update_certificate(&hash, cert);
        assert!(result.is_ok());

        let root_after = reg.root();
        assert_ne!(root_before, root_after);

        let (_, entry) = reg.lookup_by_hash(&hash).unwrap();
        assert!(entry.certificate.is_some());
        assert_eq!(
            entry.certificate.as_ref().unwrap().verdict,
            OnChainVerdict::Safe
        );
    }

    #[test]
    fn test_update_certificate_not_found() {
        let mut reg = MerkleRegistry::new();
        let cert = VerificationCertificate {
            verdict: OnChainVerdict::Safe,
            constraints_checked: 1,
            rounds: 1,
            timestamp: 0,
            verifier_auth: FieldElement::zero(),
        };
        let result = reg.update_certificate(&ContentHash([0; 32]), cert);
        assert!(result.is_err());
    }

    #[test]
    fn test_verification_certificate_json_roundtrip() {
        let cert = VerificationCertificate {
            verdict: OnChainVerdict::Safe,
            constraints_checked: 42,
            rounds: 100,
            timestamp: 1700000000,
            verifier_auth: FieldElement::new(12345),
        };
        let json = cert.to_json();
        let parsed = VerificationCertificate::from_json(&json).unwrap();
        assert_eq!(parsed.verdict, OnChainVerdict::Safe);
        assert_eq!(parsed.constraints_checked, 42);
        assert_eq!(parsed.rounds, 100);
        assert_eq!(parsed.timestamp, 1700000000);
    }

    #[test]
    fn test_on_chain_verdict_roundtrip() {
        for v in [
            OnChainVerdict::Unknown,
            OnChainVerdict::Safe,
            OnChainVerdict::StaticViolation,
            OnChainVerdict::RandomViolation,
            OnChainVerdict::BmcViolation,
        ] {
            let field = v.to_field();
            let back = OnChainVerdict::from_field(field.0);
            assert_eq!(v, back);
        }
    }

    #[test]
    fn test_generate_register_proof() {
        let mut reg = MerkleRegistry::new();
        let hash = test_hash();
        let entry = test_entry(hash);
        let result = generate_register_proof(&mut reg, entry, Some("test_fn"));
        assert!(result.is_ok());
        let proof = result.unwrap();
        assert_eq!(proof.leaf_index, 0);
        assert_ne!(proof.old_root, proof.new_root);
    }

    #[test]
    fn test_generate_membership_proof() {
        let mut reg = MerkleRegistry::new();
        let hash = test_hash();
        let entry = test_entry(hash);
        reg.register(entry, None).unwrap();
        let result = generate_membership_proof(&reg, &hash);
        assert!(result.is_ok());
        let proof = result.unwrap();
        assert_eq!(proof.leaf_index, 0);
    }

    #[test]
    fn test_generate_update_cert_proof() {
        let mut reg = MerkleRegistry::new();
        let hash = test_hash();
        let entry = test_entry(hash);
        reg.register(entry, None).unwrap();
        let cert = VerificationCertificate {
            verdict: OnChainVerdict::Safe,
            constraints_checked: 10,
            rounds: 50,
            timestamp: 1700000001,
            verifier_auth: FieldElement::new(42),
        };
        let result = generate_update_cert_proof(&mut reg, &hash, cert);
        assert!(result.is_ok());
        let proof = result.unwrap();
        assert_ne!(proof.old_root, proof.new_root);
    }

    #[test]
    fn test_registry_to_json() {
        let mut reg = MerkleRegistry::new();
        let entry = test_entry(test_hash());
        reg.register(entry, Some("add")).unwrap();
        let json = reg.to_json();
        assert!(json.contains("\"root\":"));
        assert!(json.contains("\"count\":1"));
        assert!(json.contains("\"capacity\":16"));
        assert!(json.contains("\"name\":\"add\""));
    }

    #[test]
    fn test_multiple_entries_merkle_consistency() {
        let mut reg = MerkleRegistry::new();
        let mut hashes = Vec::new();

        for i in 0u8..5 {
            let mut bytes = [0u8; 32];
            bytes[0] = i;
            let hash = ContentHash(bytes);
            hashes.push(hash);
            let entry = test_entry(hash);
            reg.register(entry, None).unwrap();
        }

        let root = reg.root();

        // Every entry should have a valid proof
        for (i, hash) in hashes.iter().enumerate() {
            let (idx, entry) = reg.lookup_by_hash(hash).unwrap();
            assert_eq!(idx, i);
            let proof = reg.prove(idx);
            assert!(proof.verify(entry.leaf_hash(), root));
        }
    }

    #[test]
    fn test_entry_leaf_hash_changes_with_cert() {
        let hash = test_hash();
        let entry1 = test_entry(hash);
        let leaf1 = entry1.leaf_hash();

        let mut entry2 = test_entry(hash);
        entry2.certificate = Some(VerificationCertificate {
            verdict: OnChainVerdict::Safe,
            constraints_checked: 1,
            rounds: 1,
            timestamp: 0,
            verifier_auth: FieldElement::zero(),
        });
        let leaf2 = entry2.leaf_hash();

        assert_ne!(leaf1, leaf2);
    }

    #[test]
    fn test_merkle_proof_json() {
        let mut reg = MerkleRegistry::new();
        let entry = test_entry(test_hash());
        let (idx, _) = reg.register(entry, None).unwrap();
        let proof = reg.prove(idx);
        let json = proof.to_json();
        assert!(json.contains("\"leaf_index\":0"));
        assert!(json.contains("\"siblings\":"));
    }

    #[test]
    fn test_register_proof_json() {
        let mut reg = MerkleRegistry::new();
        let entry = test_entry(test_hash());
        let proof = generate_register_proof(&mut reg, entry, Some("f")).unwrap();
        let json = proof.to_json();
        assert!(json.contains("\"operation\":\"register\""));
        assert!(json.contains("\"old_root\":"));
        assert!(json.contains("\"new_root\":"));
    }
}
