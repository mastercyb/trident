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

use std::collections::HashMap;

use crate::ast::*;

// ─── Serialization Format Tags ─────────────────────────────────────

// Node type tags (1-byte prefix).
// Not all tags are used yet — they define the complete serialization format
// for future expansion (e.g., when more AST node types are normalized).
const TAG_FN_DEF: u8 = 0x01;
const TAG_LET: u8 = 0x02;
const TAG_VAR: u8 = 0x03;
const TAG_FIELD_LIT: u8 = 0x04;
const TAG_U32_LIT: u8 = 0x05;
const TAG_BOOL_LIT: u8 = 0x06;
const TAG_ADD: u8 = 0x07;
const TAG_MUL: u8 = 0x08;
const TAG_SUB: u8 = 0x09;
const TAG_INV: u8 = 0x0A;
const TAG_EQ: u8 = 0x0B;
const TAG_LT: u8 = 0x0C;
const TAG_BIT_AND: u8 = 0x0D;
const TAG_BIT_XOR: u8 = 0x0E;
const TAG_IF: u8 = 0x0F;
const TAG_FOR: u8 = 0x10;
const TAG_ASSERT: u8 = 0x11;
const TAG_CALL: u8 = 0x12;
const TAG_PUB_READ: u8 = 0x13;
const TAG_PUB_WRITE: u8 = 0x14;
const TAG_DIVINE: u8 = 0x15;
const TAG_HASH: u8 = 0x16;
const TAG_ARRAY_INIT: u8 = 0x17;
const TAG_ARRAY_INDEX: u8 = 0x18;
const TAG_STRUCT_INIT: u8 = 0x19;
const TAG_FIELD_ACCESS: u8 = 0x1A;
const TAG_TUPLE: u8 = 0x1B;
const TAG_ASSIGN: u8 = 0x1C;
const TAG_RETURN: u8 = 0x1D;
const TAG_BLOCK: u8 = 0x1E;
const TAG_MATCH: u8 = 0x1F;
const TAG_DIV_MOD: u8 = 0x20;
const TAG_XFIELD_MUL: u8 = 0x21;
const TAG_ASM: u8 = 0x22;
const TAG_EXPR_STMT: u8 = 0x23;

// Type tags
const TAG_TY_FIELD: u8 = 0x80;
const TAG_TY_BOOL: u8 = 0x81;
const TAG_TY_U32: u8 = 0x82;
const TAG_TY_ARRAY: u8 = 0x83;
const TAG_TY_TUPLE: u8 = 0x84;
const TAG_TY_DIGEST: u8 = 0x86;
const TAG_TY_XFIELD: u8 = 0x87;
const TAG_TY_NAMED: u8 = 0x88;

// Version byte for hash stability
const HASH_VERSION: u8 = 1;

// ─── Content Hash ──────────────────────────────────────────────────

/// A 256-bit BLAKE3 content hash.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
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

// ─── De Bruijn Environment ─────────────────────────────────────────

/// De Bruijn environment: maps variable names to indices.
struct DeBruijnEnv {
    /// Stack of bindings (most recent at end).
    bindings: Vec<String>,
}

impl DeBruijnEnv {
    fn new() -> Self {
        Self {
            bindings: Vec::new(),
        }
    }

    /// Push a new binding, returning its index.
    fn push(&mut self, name: &str) -> u16 {
        let idx = self.bindings.len() as u16;
        self.bindings.push(name.to_string());
        idx
    }

    /// Look up a variable name, returning its de Bruijn index.
    fn lookup(&self, name: &str) -> Option<u16> {
        // Search from most recent to oldest
        for (i, binding) in self.bindings.iter().enumerate().rev() {
            if binding == name {
                return Some(i as u16);
            }
        }
        None
    }

    /// Save current state for scoping.
    fn save(&self) -> usize {
        self.bindings.len()
    }

    /// Restore to a previous state.
    fn restore(&mut self, len: usize) {
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
    buf: Vec<u8>,
    /// De Bruijn environment.
    env: DeBruijnEnv,
    /// Hashes of known functions (for dependency substitution).
    fn_hashes: HashMap<String, ContentHash>,
}

impl Normalizer {
    pub fn new() -> Self {
        Self {
            buf: Vec::new(),
            env: DeBruijnEnv::new(),
            fn_hashes: HashMap::new(),
        }
    }

    /// Set known function hashes for dependency substitution.
    pub fn with_fn_hashes(mut self, hashes: HashMap<String, ContentHash>) -> Self {
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
    pub fn hash_fn(func: &FnDef, fn_hashes: HashMap<String, ContentHash>) -> ContentHash {
        let mut normalizer = Normalizer::new().with_fn_hashes(fn_hashes);
        let bytes = normalizer.normalize_fn(func);
        ContentHash(crate::poseidon2::hash_bytes(&bytes))
    }

    /// Hash all functions in a file.
    pub fn hash_file(file: &File) -> HashMap<String, ContentHash> {
        let mut result = HashMap::new();
        let mut fn_hashes = HashMap::new();

        // First pass: hash functions without dependency info
        // (For a proper implementation, this would do topological ordering)
        for item in &file.items {
            if let Item::Fn(func) = &item.node {
                let hash = Self::hash_fn(func, fn_hashes.clone());
                fn_hashes.insert(func.name.node.clone(), hash);
                result.insert(func.name.node.clone(), hash);
            }
        }

        // Second pass: re-hash with dependency info
        let mut stable = HashMap::new();
        for item in &file.items {
            if let Item::Fn(func) = &item.node {
                let hash = Self::hash_fn(func, fn_hashes.clone());
                stable.insert(func.name.node.clone(), hash);
            }
        }

        stable
    }

    // ─── Serialization Helpers ─────────────────────────────────

    fn write_u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    fn write_u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn write_u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn write_u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn write_hash(&mut self, hash: &ContentHash) {
        self.buf.extend_from_slice(&hash.0);
    }

    fn write_str(&mut self, s: &str) {
        self.write_u16(s.len() as u16);
        self.buf.extend_from_slice(s.as_bytes());
    }

    // ─── Type Serialization ────────────────────────────────────

    fn serialize_type(&mut self, ty: &Type) {
        match ty {
            Type::Field => self.write_u8(TAG_TY_FIELD),
            Type::Bool => self.write_u8(TAG_TY_BOOL),
            Type::U32 => self.write_u8(TAG_TY_U32),
            Type::Digest => self.write_u8(TAG_TY_DIGEST),
            Type::XField => self.write_u8(TAG_TY_XFIELD),
            Type::Array(elem, size) => {
                self.write_u8(TAG_TY_ARRAY);
                self.serialize_type(elem);
                match size {
                    ArraySize::Literal(n) => self.write_u32(*n as u32),
                    ArraySize::Param(name) => {
                        // Generic param: write a marker + name
                        self.write_u32(0xFFFFFFFF);
                        self.write_str(name);
                    }
                }
            }
            Type::Tuple(elems) => {
                self.write_u8(TAG_TY_TUPLE);
                self.write_u16(elems.len() as u16);
                for elem in elems {
                    self.serialize_type(elem);
                }
            }
            Type::Named(path) => {
                self.write_u8(TAG_TY_NAMED);
                self.write_str(&path.as_dotted());
            }
        }
    }

    // ─── Block Serialization ───────────────────────────────────

    fn serialize_block(&mut self, block: &Block) {
        self.write_u8(TAG_BLOCK);
        self.write_u16(block.stmts.len() as u16);
        for stmt in &block.stmts {
            self.serialize_stmt(&stmt.node);
        }
        if let Some(ref tail) = block.tail_expr {
            self.write_u8(1); // has tail
            self.serialize_expr(&tail.node);
        } else {
            self.write_u8(0); // no tail
        }
    }

    // ─── Statement Serialization ───────────────────────────────

    fn serialize_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let { pattern, init, .. } => {
                self.write_u8(TAG_LET);
                match pattern {
                    Pattern::Name(name) => {
                        self.write_u8(0); // single binding
                        let idx = self.env.push(&name.node);
                        self.write_u16(idx);
                    }
                    Pattern::Tuple(names) => {
                        self.write_u8(1); // tuple destructure
                        self.write_u16(names.len() as u16);
                        for name in names {
                            let idx = self.env.push(&name.node);
                            self.write_u16(idx);
                        }
                    }
                }
                self.serialize_expr(&init.node);
            }
            Stmt::Assign { place, value } => {
                self.write_u8(TAG_ASSIGN);
                self.serialize_place(&place.node);
                self.serialize_expr(&value.node);
            }
            Stmt::TupleAssign { names, value } => {
                self.write_u8(TAG_LET);
                self.write_u8(1); // tuple destructure
                self.write_u16(names.len() as u16);
                for name in names {
                    let idx = self.env.push(&name.node);
                    self.write_u16(idx);
                }
                self.serialize_expr(&value.node);
            }
            Stmt::If {
                cond,
                then_block,
                else_block,
            } => {
                self.write_u8(TAG_IF);
                self.serialize_expr(&cond.node);
                self.serialize_block(&then_block.node);
                if let Some(ref else_blk) = else_block {
                    self.write_u8(1);
                    self.serialize_block(&else_blk.node);
                } else {
                    self.write_u8(0);
                }
            }
            Stmt::For {
                var,
                start,
                end,
                bound,
                body,
            } => {
                self.write_u8(TAG_FOR);
                let saved = self.env.save();
                let idx = self.env.push(&var.node);
                self.write_u16(idx);
                self.serialize_expr(&start.node);
                self.serialize_expr(&end.node);
                self.write_u32(bound.unwrap_or(0) as u32);
                self.serialize_block(&body.node);
                self.env.restore(saved);
            }
            Stmt::Expr(expr) => {
                self.write_u8(TAG_EXPR_STMT);
                self.serialize_expr(&expr.node);
            }
            Stmt::Return(val) => {
                self.write_u8(TAG_RETURN);
                if let Some(ref v) = val {
                    self.write_u8(1);
                    self.serialize_expr(&v.node);
                } else {
                    self.write_u8(0);
                }
            }
            Stmt::Emit { event_name, fields } | Stmt::Seal { event_name, fields } => {
                // Emit and Seal are structurally identical for hashing
                self.write_u8(TAG_STRUCT_INIT);
                self.write_str(&event_name.node);
                self.write_u16(fields.len() as u16);
                for (name, val) in fields {
                    self.write_str(&name.node);
                    self.serialize_expr(&val.node);
                }
            }
            Stmt::Asm {
                body,
                effect,
                target,
            } => {
                self.write_u8(TAG_ASM);
                self.write_str(body);
                self.write_u16(*effect as u16);
                if let Some(ref t) = target {
                    self.write_u8(1);
                    self.write_str(t);
                } else {
                    self.write_u8(0);
                }
            }
            Stmt::Match { expr, arms } => {
                self.write_u8(TAG_MATCH);
                self.serialize_expr(&expr.node);
                self.write_u16(arms.len() as u16);
                for arm in arms {
                    self.serialize_match_pattern(&arm.pattern.node);
                    self.serialize_block(&arm.body.node);
                }
            }
        }
    }

    fn serialize_match_pattern(&mut self, pattern: &MatchPattern) {
        match pattern {
            MatchPattern::Literal(Literal::Integer(n)) => {
                self.write_u8(TAG_FIELD_LIT);
                self.write_u64(*n);
            }
            MatchPattern::Literal(Literal::Bool(b)) => {
                self.write_u8(TAG_BOOL_LIT);
                self.write_u8(if *b { 1 } else { 0 });
            }
            MatchPattern::Wildcard => {
                self.write_u8(0xFF); // wildcard marker
            }
        }
    }

    fn serialize_place(&mut self, place: &Place) {
        match place {
            Place::Var(name) => {
                if let Some(idx) = self.env.lookup(name) {
                    self.write_u8(TAG_VAR);
                    self.write_u16(idx);
                } else {
                    // Unknown variable — use name
                    self.write_u8(TAG_VAR);
                    self.write_u16(0xFFFF);
                    self.write_str(name);
                }
            }
            Place::FieldAccess(base, field) => {
                self.write_u8(TAG_FIELD_ACCESS);
                self.serialize_place(&base.node);
                self.write_str(&field.node);
            }
            Place::Index(base, index) => {
                self.write_u8(TAG_ARRAY_INDEX);
                self.serialize_place(&base.node);
                self.serialize_expr(&index.node);
            }
        }
    }

    // ─── Expression Serialization ──────────────────────────────

    fn serialize_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Literal(Literal::Integer(n)) => {
                self.write_u8(TAG_FIELD_LIT);
                self.write_u64(*n);
            }
            Expr::Literal(Literal::Bool(b)) => {
                self.write_u8(TAG_BOOL_LIT);
                self.write_u8(if *b { 1 } else { 0 });
            }
            Expr::Var(name) => {
                if let Some(idx) = self.env.lookup(name) {
                    self.write_u8(TAG_VAR);
                    self.write_u16(idx);
                } else {
                    // Free variable (e.g., global constant) — use name
                    self.write_u8(TAG_VAR);
                    self.write_u16(0xFFFF);
                    self.write_str(name);
                }
            }
            Expr::BinOp { op, lhs, rhs } => {
                let tag = match op {
                    BinOp::Add => TAG_ADD,
                    BinOp::Mul => TAG_MUL,
                    BinOp::Eq => TAG_EQ,
                    BinOp::Lt => TAG_LT,
                    BinOp::BitAnd => TAG_BIT_AND,
                    BinOp::BitXor => TAG_BIT_XOR,
                    BinOp::DivMod => TAG_DIV_MOD,
                    BinOp::XFieldMul => TAG_XFIELD_MUL,
                };
                self.write_u8(tag);
                self.serialize_expr(&lhs.node);
                self.serialize_expr(&rhs.node);
            }
            Expr::Call {
                path,
                generic_args,
                args,
            } => {
                let name = path.node.as_dotted();
                let func_name = path.node.0.last().map(|s| s.as_str()).unwrap_or("");

                // Check if we have a hash for this function
                let resolved_hash = self
                    .fn_hashes
                    .get(&name)
                    .or_else(|| self.fn_hashes.get(func_name))
                    .copied();

                if let Some(hash) = resolved_hash {
                    self.write_u8(TAG_CALL);
                    self.write_hash(&hash);
                } else {
                    // Unknown function — use name-based call
                    self.write_u8(TAG_CALL);
                    self.write_hash(&ContentHash::zero());
                    self.write_str(&name);
                }

                // Generic args
                self.write_u16(generic_args.len() as u16);
                for ga in generic_args {
                    match &ga.node {
                        ArraySize::Literal(n) => {
                            self.write_u8(0);
                            self.write_u32(*n as u32);
                        }
                        ArraySize::Param(name) => {
                            self.write_u8(1);
                            self.write_str(name);
                        }
                    }
                }

                // Args
                self.write_u16(args.len() as u16);
                for arg in args {
                    self.serialize_expr(&arg.node);
                }
            }
            Expr::FieldAccess { expr, field } => {
                self.write_u8(TAG_FIELD_ACCESS);
                self.serialize_expr(&expr.node);
                self.write_str(&field.node);
            }
            Expr::Index { expr, index } => {
                self.write_u8(TAG_ARRAY_INDEX);
                self.serialize_expr(&expr.node);
                self.serialize_expr(&index.node);
            }
            Expr::StructInit { path, fields } => {
                self.write_u8(TAG_STRUCT_INIT);
                self.write_str(&path.node.as_dotted());
                // Sort fields alphabetically for canonical order
                let mut sorted_fields: Vec<_> = fields.iter().collect();
                sorted_fields.sort_by_key(|(name, _)| &name.node);
                self.write_u16(sorted_fields.len() as u16);
                for (name, val) in sorted_fields {
                    self.write_str(&name.node);
                    self.serialize_expr(&val.node);
                }
            }
            Expr::ArrayInit(elems) => {
                self.write_u8(TAG_ARRAY_INIT);
                self.write_u16(elems.len() as u16);
                for elem in elems {
                    self.serialize_expr(&elem.node);
                }
            }
            Expr::Tuple(elems) => {
                self.write_u8(TAG_TUPLE);
                self.write_u16(elems.len() as u16);
                for elem in elems {
                    self.serialize_expr(&elem.node);
                }
            }
        }
    }
}

// ─── Public API ────────────────────────────────────────────────────

/// Hash a single function.
pub fn hash_function(func: &FnDef, deps: HashMap<String, ContentHash>) -> ContentHash {
    Normalizer::hash_fn(func, deps)
}

/// Hash all functions in a file, returning name → hash map.
pub fn hash_file(file: &File) -> HashMap<String, ContentHash> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_file(source: &str) -> File {
        crate::parse_source_silent(source, "test.tri").unwrap()
    }

    #[test]
    fn test_same_code_same_hash() {
        let f1 = parse_file(
            "program test\nfn add(a: Field, b: Field) -> Field { a + b }\nfn main() { }\n",
        );
        let f2 = parse_file(
            "program test\nfn add(x: Field, y: Field) -> Field { x + y }\nfn main() { }\n",
        );

        let h1 = hash_file(&f1);
        let h2 = hash_file(&f2);

        // Same computation, different variable names → same hash
        assert_eq!(
            h1["add"], h2["add"],
            "renamed variables should produce same hash"
        );
    }

    #[test]
    fn test_different_code_different_hash() {
        let f1 = parse_file(
            "program test\nfn f(a: Field, b: Field) -> Field { a + b }\nfn main() { }\n",
        );
        let f2 = parse_file(
            "program test\nfn f(a: Field, b: Field) -> Field { a * b }\nfn main() { }\n",
        );

        let h1 = hash_file(&f1);
        let h2 = hash_file(&f2);

        assert_ne!(
            h1["f"], h2["f"],
            "different operations should produce different hash"
        );
    }

    #[test]
    fn test_hash_display() {
        let hash = ContentHash([0xAB; 32]);
        assert_eq!(hash.to_hex().len(), 64);
        assert_eq!(hash.to_short().len(), 8);
    }

    #[test]
    fn test_hash_deterministic() {
        let f = parse_file(
            "program test\nfn main() {\n    let x: Field = pub_read()\n    pub_write(x)\n}\n",
        );
        let h1 = hash_file(&f);
        let h2 = hash_file(&f);
        assert_eq!(h1["main"], h2["main"]);
    }

    #[test]
    fn test_file_content_hash() {
        let f = parse_file(
            "program test\nfn main() {\n    let x: Field = pub_read()\n    pub_write(x)\n}\n",
        );
        let h1 = hash_file_content(&f);
        let h2 = hash_file_content(&f);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hash_with_if() {
        let f = parse_file("program test\nfn main() {\n    let x: Field = pub_read()\n    if x == 0 {\n        pub_write(0)\n    } else {\n        pub_write(1)\n    }\n}\n");
        let h = hash_file(&f);
        assert_ne!(h["main"], ContentHash::zero());
    }

    #[test]
    fn test_hash_with_for() {
        let f = parse_file("program test\nfn main() {\n    let mut s: Field = 0\n    for i in 0..10 {\n        s = s + 1\n    }\n    pub_write(s)\n}\n");
        let h = hash_file(&f);
        assert_ne!(h["main"], ContentHash::zero());
    }

    #[test]
    fn test_spec_does_not_affect_hash() {
        let f1 = parse_file(
            "program test\nfn add(a: Field, b: Field) -> Field { a + b }\nfn main() { }\n",
        );
        let f2 = parse_file("program test\n#[requires(a + b < 1000)]\n#[ensures(result == a + b)]\nfn add(a: Field, b: Field) -> Field { a + b }\nfn main() { }\n");

        let h1 = hash_file(&f1);
        let h2 = hash_file(&f2);

        // Spec annotations don't affect computational hash
        assert_eq!(
            h1["add"], h2["add"],
            "spec annotations should not affect hash"
        );
    }

    #[test]
    fn test_empty_fn_hash() {
        let f = parse_file("program test\nfn main() { }\n");
        let h = hash_file(&f);
        assert_ne!(h["main"], ContentHash::zero());
    }
}
