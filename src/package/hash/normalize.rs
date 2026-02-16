use std::collections::BTreeMap;

use crate::ast::*;

use super::ContentHash;
use super::HASH_VERSION;

// ─── Serialization Format Tags ─────────────────────────────────────

// Node type tags (1-byte prefix).
// Not all tags are used yet — they define the complete serialization format
// for future expansion (e.g., when more AST node types are normalized).
pub(super) const TAG_FN_DEF: u8 = 0x01;
pub(super) const TAG_LET: u8 = 0x02;
pub(super) const TAG_VAR: u8 = 0x03;
pub(super) const TAG_FIELD_LIT: u8 = 0x04;
pub(super) const TAG_U32_LIT: u8 = 0x05;
pub(super) const TAG_BOOL_LIT: u8 = 0x06;
pub(super) const TAG_ADD: u8 = 0x07;
pub(super) const TAG_MUL: u8 = 0x08;
pub(super) const TAG_SUB: u8 = 0x09;
pub(super) const TAG_INV: u8 = 0x0A;
pub(super) const TAG_EQ: u8 = 0x0B;
pub(super) const TAG_LT: u8 = 0x0C;
pub(super) const TAG_BIT_AND: u8 = 0x0D;
pub(super) const TAG_BIT_XOR: u8 = 0x0E;
pub(super) const TAG_IF: u8 = 0x0F;
pub(super) const TAG_FOR: u8 = 0x10;
pub(super) const TAG_ASSERT: u8 = 0x11;
pub(super) const TAG_CALL: u8 = 0x12;
pub(super) const TAG_PUB_READ: u8 = 0x13;
pub(super) const TAG_PUB_WRITE: u8 = 0x14;
pub(super) const TAG_DIVINE: u8 = 0x15;
pub(super) const TAG_HASH: u8 = 0x16;
pub(super) const TAG_ARRAY_INIT: u8 = 0x17;
pub(super) const TAG_ARRAY_INDEX: u8 = 0x18;
pub(super) const TAG_STRUCT_INIT: u8 = 0x19;
pub(super) const TAG_FIELD_ACCESS: u8 = 0x1A;
pub(super) const TAG_TUPLE: u8 = 0x1B;
pub(super) const TAG_ASSIGN: u8 = 0x1C;
pub(super) const TAG_RETURN: u8 = 0x1D;
pub(super) const TAG_BLOCK: u8 = 0x1E;
pub(super) const TAG_MATCH: u8 = 0x1F;
pub(super) const TAG_DIV_MOD: u8 = 0x20;
pub(super) const TAG_XFIELD_MUL: u8 = 0x21;
pub(super) const TAG_ASM: u8 = 0x22;
pub(super) const TAG_EXPR_STMT: u8 = 0x23;
pub(super) const TAG_STRUCT_PAT: u8 = 0x24;

// Type tags
pub(super) const TAG_TY_FIELD: u8 = 0x80;
pub(super) const TAG_TY_BOOL: u8 = 0x81;
pub(super) const TAG_TY_U32: u8 = 0x82;
pub(super) const TAG_TY_ARRAY: u8 = 0x83;
pub(super) const TAG_TY_TUPLE: u8 = 0x84;
pub(super) const TAG_TY_DIGEST: u8 = 0x86;
pub(super) const TAG_TY_XFIELD: u8 = 0x87;
pub(super) const TAG_TY_NAMED: u8 = 0x88;

// Version byte for hash stability

// ─── De Bruijn Environment ─────────────────────────────────────────

/// De Bruijn environment: maps variable names to indices.
pub(super) struct DeBruijnEnv {
    /// Stack of bindings (most recent at end).
    bindings: Vec<String>,
}

impl DeBruijnEnv {
    pub(super) fn new() -> Self {
        Self {
            bindings: Vec::new(),
        }
    }

    /// Push a new binding, returning its index.
    pub(super) fn push(&mut self, name: &str) -> u16 {
        let idx = self.bindings.len() as u16;
        self.bindings.push(name.to_string());
        idx
    }

    /// Look up a variable name, returning its de Bruijn index.
    pub(super) fn lookup(&self, name: &str) -> Option<u16> {
        // Search from most recent to oldest
        for (i, binding) in self.bindings.iter().enumerate().rev() {
            if binding == name {
                return Some(i as u16);
            }
        }
        None
    }

    /// Save current state for scoping.
    pub(super) fn save(&self) -> usize {
        self.bindings.len()
    }

    /// Restore to a previous state.
    pub(super) fn restore(&mut self, len: usize) {
        self.bindings.truncate(len);
    }
}

// ─── Normalizer + Serializer ───────────────────────────────────────

/// Normalize and serialize a function definition to bytes.
///
/// The resulting bytes are deterministic: same computation → same bytes,
/// regardless of variable names or formatting.
pub struct Normalizer {
    /// Output buffer.
    pub(super) buf: Vec<u8>,
    /// De Bruijn environment.
    pub(super) env: DeBruijnEnv,
    /// Hashes of known functions (for dependency substitution).
    pub(super) fn_hashes: BTreeMap<String, ContentHash>,
}

impl Normalizer {
    pub fn new() -> Self {
        Self {
            buf: Vec::new(),
            env: DeBruijnEnv::new(),
            fn_hashes: BTreeMap::new(),
        }
    }

    /// Set known function hashes for dependency substitution.
    pub fn with_fn_hashes(mut self, hashes: BTreeMap<String, ContentHash>) -> Self {
        self.fn_hashes = hashes;
        self
    }

    /// Normalize and serialize a function definition.
    pub fn normalize_fn(&mut self, func: &FnDef) -> Vec<u8> {
        self.buf.clear();
        self.env = DeBruijnEnv::new();

        // Version prefix
        self.buf.push(HASH_VERSION);

        // Function tag
        self.buf.push(TAG_FN_DEF);

        // Parameter count
        self.write_u16(func.params.len() as u16);

        // Parameter types (in order)
        for param in &func.params {
            // Bind parameter name to de Bruijn index
            self.env.push(&param.name.node);
            self.serialize_type(&param.ty.node);
        }

        // Return type (if any)
        if let Some(ref ret_ty) = func.return_ty {
            self.buf.push(1); // has return type
            self.serialize_type(&ret_ty.node);
        } else {
            self.buf.push(0); // no return type
        }

        // Body
        if let Some(ref body) = func.body {
            self.buf.push(1); // has body
            self.serialize_block(&body.node);
        } else {
            self.buf.push(0); // no body (intrinsic)
        }

        self.buf.clone()
    }

    /// Hash a single function definition using Poseidon2 over Goldilocks.
    pub fn hash_fn(func: &FnDef, fn_hashes: BTreeMap<String, ContentHash>) -> ContentHash {
        let mut normalizer = Normalizer::new().with_fn_hashes(fn_hashes);
        let bytes = normalizer.normalize_fn(func);
        ContentHash(crate::poseidon2::hash_bytes(&bytes))
    }

    /// Hash all functions in a file.
    pub fn hash_file(file: &File) -> BTreeMap<String, ContentHash> {
        let mut fn_hashes = BTreeMap::new();

        // First pass: hash functions, building dependency info incrementally
        for item in &file.items {
            if let Item::Fn(func) = &item.node {
                let mut normalizer = Normalizer::new();
                normalizer.fn_hashes.clone_from(&fn_hashes);
                let bytes = normalizer.normalize_fn(func);
                let hash = ContentHash(crate::poseidon2::hash_bytes(&bytes));
                fn_hashes.insert(func.name.node.clone(), hash);
            }
        }

        // Second pass: re-hash with complete dependency info (single clone)
        let mut stable = BTreeMap::new();
        let mut normalizer = Normalizer::new().with_fn_hashes(fn_hashes);
        for item in &file.items {
            if let Item::Fn(func) = &item.node {
                let bytes = normalizer.normalize_fn(func);
                let hash = ContentHash(crate::poseidon2::hash_bytes(&bytes));
                stable.insert(func.name.node.clone(), hash);
            }
        }

        stable
    }

    // ─── Serialization Helpers ─────────────────────────────────

    pub(super) fn write_u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    pub(super) fn write_u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub(super) fn write_u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub(super) fn write_u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub(super) fn write_hash(&mut self, hash: &ContentHash) {
        self.buf.extend_from_slice(&hash.0);
    }

    pub(super) fn write_str(&mut self, s: &str) {
        self.write_u16(s.len() as u16);
        self.buf.extend_from_slice(s.as_bytes());
    }
}
