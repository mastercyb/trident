//! Universal Codebase Manager (UCM) — hash-keyed definitions store.
//!
//! Inspired by Unison: every function definition is stored by its content hash.
//! Names are metadata pointing to hashes. This allows instant rename, perfect
//! caching, and semantic deduplication.
//!
//! Persistence layout:
//! ```text
//! ~/.trident/codebase/
//!   defs/
//!     <2-char-prefix>/
//!       <full-hex-hash>.def
//!   names.txt
//!   history.txt
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::ast::{self, Expr, Item, Stmt, Type};
use crate::hash::{self, ContentHash};

// ─── Data Structures ───────────────────────────────────────────────

/// The UCM codebase database.
///
/// Stores function definitions by content hash, with name mappings.
/// Persisted to disk at `~/.trident/codebase/` (or `$TRIDENT_CODEBASE_DIR`).
pub struct Codebase {
    /// Hash -> definition source code.
    definitions: HashMap<ContentHash, Definition>,
    /// Name -> hash mapping (current bindings).
    names: HashMap<String, ContentHash>,
    /// Hash -> list of names that have pointed to it (history).
    name_history: HashMap<ContentHash, Vec<NameEntry>>,
    /// Root directory for persistence.
    root: PathBuf,
}

/// A stored function definition.
#[derive(Clone)]
pub struct Definition {
    /// The source code of the function (formatted).
    pub source: String,
    /// Module where this was last seen.
    pub module: String,
    /// Is it public?
    pub is_pub: bool,
    /// Parameters (name, type) pairs as strings.
    pub params: Vec<(String, String)>,
    /// Return type (as string), None for void.
    pub return_ty: Option<String>,
    /// Dependencies: hashes of functions called by this one.
    pub dependencies: Vec<ContentHash>,
    /// Spec annotations: preconditions.
    pub requires: Vec<String>,
    /// Spec annotations: postconditions.
    pub ensures: Vec<String>,
    /// When this was first stored (Unix timestamp).
    pub first_seen: u64,
}

/// A name binding entry in history.
pub struct NameEntry {
    pub name: String,
    pub timestamp: u64,
}

/// Result of adding a file to the codebase.
pub struct AddResult {
    /// New definitions stored.
    pub added: usize,
    /// Names rebound to new hashes.
    pub updated: usize,
    /// Already at same hash.
    pub unchanged: usize,
}

/// Codebase statistics.
pub struct CodebaseStats {
    /// Number of unique definitions.
    pub definitions: usize,
    /// Number of name bindings.
    pub names: usize,
    /// Total source bytes across all definitions.
    pub total_source_bytes: usize,
}

// ─── Codebase Implementation ───────────────────────────────────────

impl Codebase {
    /// Open or create a codebase at the default location.
    ///
    /// Uses `$TRIDENT_CODEBASE_DIR` if set, otherwise `~/.trident/codebase/`.
    pub fn open() -> std::io::Result<Self> {
        let root = codebase_dir().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "cannot determine codebase directory (no $HOME)",
            )
        })?;
        Self::open_at(&root)
    }

    /// Open or create a codebase at a specific directory.
    pub fn open_at(root: &Path) -> std::io::Result<Self> {
        std::fs::create_dir_all(root)?;
        std::fs::create_dir_all(root.join("defs"))?;

        let mut cb = Codebase {
            definitions: HashMap::new(),
            names: HashMap::new(),
            name_history: HashMap::new(),
            root: root.to_path_buf(),
        };

        cb.load()?;
        Ok(cb)
    }

    /// Add a parsed file to the codebase: hash all functions, store definitions.
    pub fn add_file(&mut self, file: &ast::File) -> AddResult {
        let fn_hashes = hash::hash_file(file);
        let module = file.name.node.clone();
        let now = unix_timestamp();

        let mut added = 0usize;
        let mut updated = 0usize;
        let mut unchanged = 0usize;

        for item in &file.items {
            if let Item::Fn(func) = &item.node {
                let name = func.name.node.clone();
                let Some(hash) = fn_hashes.get(&name).copied() else {
                    continue;
                };

                // Check if this name already points to this hash.
                if let Some(existing) = self.names.get(&name) {
                    if *existing == hash {
                        unchanged += 1;
                        continue;
                    }
                    // Name rebound to a new hash.
                    updated += 1;
                } else {
                    added += 1;
                }

                // Extract dependencies.
                let deps = extract_dependencies(func, &fn_hashes);

                // Build the Definition.
                let def = Definition {
                    source: format_fn_source(func),
                    module: module.clone(),
                    is_pub: func.is_pub,
                    params: func
                        .params
                        .iter()
                        .map(|p| (p.name.node.clone(), format_type(&p.ty.node)))
                        .collect(),
                    return_ty: func.return_ty.as_ref().map(|t| format_type(&t.node)),
                    dependencies: deps,
                    requires: func.requires.iter().map(|s| s.node.clone()).collect(),
                    ensures: func.ensures.iter().map(|s| s.node.clone()).collect(),
                    first_seen: self
                        .definitions
                        .get(&hash)
                        .map(|d| d.first_seen)
                        .unwrap_or(now),
                };

                self.definitions.insert(hash, def);

                // Record history entry.
                let entry = NameEntry {
                    name: name.clone(),
                    timestamp: now,
                };
                self.name_history.entry(hash).or_default().push(entry);

                // Update current name binding.
                self.names.insert(name, hash);
            }
        }

        AddResult {
            added,
            updated,
            unchanged,
        }
    }

    /// Look up a definition by name.
    pub fn lookup(&self, name: &str) -> Option<&Definition> {
        let hash = self.names.get(name)?;
        self.definitions.get(hash)
    }

    /// Get the content hash for a name.
    pub fn hash_for_name(&self, name: &str) -> Option<&ContentHash> {
        self.names.get(name)
    }

    /// Look up a definition by hash.
    pub fn lookup_hash(&self, hash: &ContentHash) -> Option<&Definition> {
        self.definitions.get(hash)
    }

    /// List all names in the codebase, sorted alphabetically.
    pub fn list_names(&self) -> Vec<(&str, &ContentHash)> {
        let mut list: Vec<(&str, &ContentHash)> =
            self.names.iter().map(|(n, h)| (n.as_str(), h)).collect();
        list.sort_by_key(|(name, _)| *name);
        list
    }

    /// Rename: rebind `new_name` to the hash currently bound to `old_name`,
    /// and remove the `old_name` binding.
    pub fn rename(&mut self, old_name: &str, new_name: &str) -> Result<(), String> {
        let hash = self
            .names
            .get(old_name)
            .copied()
            .ok_or_else(|| format!("name '{}' not found", old_name))?;
        if self.names.contains_key(new_name) {
            return Err(format!("name '{}' already exists", new_name));
        }
        self.names.remove(old_name);
        self.names.insert(new_name.to_string(), hash);

        // Record history for the new name.
        let entry = NameEntry {
            name: new_name.to_string(),
            timestamp: unix_timestamp(),
        };
        self.name_history.entry(hash).or_default().push(entry);

        Ok(())
    }

    /// Alias: add an additional name pointing to the same hash as `name`.
    pub fn alias(&mut self, name: &str, alias: &str) -> Result<(), String> {
        let hash = self
            .names
            .get(name)
            .copied()
            .ok_or_else(|| format!("name '{}' not found", name))?;
        if self.names.contains_key(alias) {
            return Err(format!("name '{}' already exists", alias));
        }
        self.names.insert(alias.to_string(), hash);

        let entry = NameEntry {
            name: alias.to_string(),
            timestamp: unix_timestamp(),
        };
        self.name_history.entry(hash).or_default().push(entry);

        Ok(())
    }

    /// Get history of a name: all hashes it has pointed to, with timestamps.
    pub fn name_history(&self, name: &str) -> Vec<(ContentHash, u64)> {
        let mut result = Vec::new();
        for (hash, entries) in &self.name_history {
            for entry in entries {
                if entry.name == name {
                    result.push((*hash, entry.timestamp));
                }
            }
        }
        result.sort_by_key(|(_, ts)| *ts);
        result
    }

    /// Get all names that currently point to a given hash.
    pub fn names_for_hash(&self, hash: &ContentHash) -> Vec<&str> {
        let mut names: Vec<&str> = self
            .names
            .iter()
            .filter(|(_, h)| *h == hash)
            .map(|(n, _)| n.as_str())
            .collect();
        names.sort();
        names
    }

    /// Get dependencies of a definition: (name, hash) pairs for each called function.
    pub fn dependencies(&self, hash: &ContentHash) -> Vec<(&str, &ContentHash)> {
        let def = match self.definitions.get(hash) {
            Some(d) => d,
            None => return Vec::new(),
        };
        let mut result = Vec::new();
        for dep_hash in &def.dependencies {
            // Find a name for this dependency hash.
            let name = self
                .names
                .iter()
                .find(|(_, h)| *h == dep_hash)
                .map(|(n, _)| n.as_str())
                .unwrap_or("<unnamed>");
            result.push((name, dep_hash));
        }
        result
    }

    /// Get reverse dependencies: definitions that depend on a given hash.
    pub fn dependents(&self, hash: &ContentHash) -> Vec<(&str, &ContentHash)> {
        let mut result = Vec::new();
        for (def_hash, def) in &self.definitions {
            if def.dependencies.contains(hash) {
                let name = self
                    .names
                    .iter()
                    .find(|(_, h)| *h == def_hash)
                    .map(|(n, _)| n.as_str())
                    .unwrap_or("<unnamed>");
                result.push((name, def_hash));
            }
        }
        result.sort_by_key(|(name, _)| *name);
        result
    }

    /// Codebase statistics.
    pub fn stats(&self) -> CodebaseStats {
        let total_source_bytes = self.definitions.values().map(|d| d.source.len()).sum();
        CodebaseStats {
            definitions: self.definitions.len(),
            names: self.names.len(),
            total_source_bytes,
        }
    }

    /// Save the codebase to disk.
    pub fn save(&self) -> std::io::Result<()> {
        // Write definitions.
        let defs_dir = self.root.join("defs");
        std::fs::create_dir_all(&defs_dir)?;

        for (hash, def) in &self.definitions {
            let hex = hash.to_hex();
            let prefix = &hex[..2];
            let prefix_dir = defs_dir.join(prefix);
            std::fs::create_dir_all(&prefix_dir)?;

            let def_path = prefix_dir.join(format!("{}.def", hex));
            let content = serialize_definition(def);
            std::fs::write(&def_path, &content)?;
        }

        // Write names.txt
        let names_path = self.root.join("names.txt");
        let mut names_content = String::new();
        let mut sorted_names: Vec<_> = self.names.iter().collect();
        sorted_names.sort_by_key(|(n, _)| (*n).clone());
        for (name, hash) in sorted_names {
            names_content.push_str(name);
            names_content.push('=');
            names_content.push_str(&hash.to_hex());
            names_content.push('\n');
        }
        std::fs::write(&names_path, &names_content)?;

        // Write history.txt
        let history_path = self.root.join("history.txt");
        let mut history_content = String::new();
        let mut all_entries: Vec<(&ContentHash, &NameEntry)> = Vec::new();
        for (hash, entries) in &self.name_history {
            for entry in entries {
                all_entries.push((hash, entry));
            }
        }
        all_entries.sort_by_key(|(_, e)| e.timestamp);
        for (hash, entry) in all_entries {
            history_content.push_str(&entry.name);
            history_content.push(' ');
            history_content.push_str(&hash.to_hex());
            history_content.push(' ');
            history_content.push_str(&entry.timestamp.to_string());
            history_content.push('\n');
        }
        std::fs::write(&history_path, &history_content)?;

        Ok(())
    }

    /// Store a definition directly by hash (used by registry publish).
    pub fn store_definition(&mut self, hash: ContentHash, def: Definition) {
        self.definitions.insert(hash, def);
    }

    /// Bind a name to a hash directly (used by registry pull).
    pub fn bind_name(&mut self, name: &str, hash: ContentHash) {
        self.names.insert(name.to_string(), hash);
        let entry = NameEntry {
            name: name.to_string(),
            timestamp: unix_timestamp(),
        };
        self.name_history.entry(hash).or_default().push(entry);
    }

    /// Pretty-print a definition by name.
    pub fn view(&self, name: &str) -> Option<String> {
        let hash = self.names.get(name)?;
        let def = self.definitions.get(hash)?;
        let mut out = String::new();

        // Header
        out.push_str(&format!("-- {} {}\n", name, hash));

        // Spec annotations
        for req in &def.requires {
            out.push_str(&format!("#[requires({})]\n", req));
        }
        for ens in &def.ensures {
            out.push_str(&format!("#[ensures({})]\n", ens));
        }

        // Source
        out.push_str(&def.source);
        if !out.ends_with('\n') {
            out.push('\n');
        }

        // Dependencies
        if !def.dependencies.is_empty() {
            out.push_str("\n-- Dependencies:\n");
            for dep_hash in &def.dependencies {
                let dep_name = self
                    .names
                    .iter()
                    .find(|(_, h)| *h == dep_hash)
                    .map(|(n, _)| n.as_str())
                    .unwrap_or("<unnamed>");
                out.push_str(&format!("--   {} {}\n", dep_name, dep_hash));
            }
        }

        Some(out)
    }

    /// Look up a definition by hash prefix (short hex or full hex).
    pub fn lookup_by_prefix(&self, prefix: &str) -> Option<(&ContentHash, &Definition)> {
        // Strip leading '#' if present.
        let prefix = prefix.strip_prefix('#').unwrap_or(prefix);
        for (hash, def) in &self.definitions {
            let hex = hash.to_hex();
            if hex.starts_with(prefix) {
                return Some((hash, def));
            }
            let short = hash.to_short();
            if short.starts_with(prefix) || short == prefix {
                return Some((hash, def));
            }
        }
        None
    }

    // ─── Persistence: Load ─────────────────────────────────────

    fn load(&mut self) -> std::io::Result<()> {
        self.load_names()?;
        self.load_definitions()?;
        self.load_history()?;
        Ok(())
    }

    fn load_names(&mut self) -> std::io::Result<()> {
        let path = self.root.join("names.txt");
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e),
        };

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some((name, hex)) = line.split_once('=') {
                if let Some(hash) = parse_hex_hash(hex.trim()) {
                    self.names.insert(name.trim().to_string(), hash);
                }
            }
        }
        Ok(())
    }

    fn load_definitions(&mut self) -> std::io::Result<()> {
        let defs_dir = self.root.join("defs");
        if !defs_dir.is_dir() {
            return Ok(());
        }

        for prefix_entry in std::fs::read_dir(&defs_dir)? {
            let prefix_entry = prefix_entry?;
            if !prefix_entry.file_type()?.is_dir() {
                continue;
            }
            for def_entry in std::fs::read_dir(prefix_entry.path())? {
                let def_entry = def_entry?;
                let path = def_entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("def") {
                    continue;
                }
                // Extract hash from filename.
                let stem = match path.file_stem().and_then(|s| s.to_str()) {
                    Some(s) => s,
                    None => continue,
                };
                let hash = match parse_hex_hash(stem) {
                    Some(h) => h,
                    None => continue,
                };
                let content = std::fs::read_to_string(&path)?;
                if let Some(def) = deserialize_definition(&content) {
                    self.definitions.insert(hash, def);
                }
            }
        }
        Ok(())
    }

    fn load_history(&mut self) -> std::io::Result<()> {
        let path = self.root.join("history.txt");
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e),
        };

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.splitn(3, ' ').collect();
            if parts.len() < 3 {
                continue;
            }
            let name = parts[0].to_string();
            let hash = match parse_hex_hash(parts[1]) {
                Some(h) => h,
                None => continue,
            };
            let timestamp: u64 = parts[2].parse().unwrap_or(0);
            let entry = NameEntry { name, timestamp };
            self.name_history.entry(hash).or_default().push(entry);
        }
        Ok(())
    }
}

// ─── Serialization ─────────────────────────────────────────────────

fn serialize_definition(def: &Definition) -> String {
    let mut out = String::new();
    // Escape newlines in source for single-value storage.
    out.push_str("source=");
    out.push_str(&escape_newlines(&def.source));
    out.push('\n');

    out.push_str("module=");
    out.push_str(&def.module);
    out.push('\n');

    out.push_str("is_pub=");
    out.push_str(if def.is_pub { "true" } else { "false" });
    out.push('\n');

    out.push_str("params=");
    let params_str: Vec<String> = def
        .params
        .iter()
        .map(|(n, t)| format!("{}:{}", n, t))
        .collect();
    out.push_str(&params_str.join(","));
    out.push('\n');

    out.push_str("return_ty=");
    if let Some(ref ty) = def.return_ty {
        out.push_str(ty);
    }
    out.push('\n');

    out.push_str("dependencies=");
    let deps_str: Vec<String> = def.dependencies.iter().map(|h| h.to_hex()).collect();
    out.push_str(&deps_str.join(","));
    out.push('\n');

    out.push_str("requires=");
    out.push_str(&def.requires.join(";"));
    out.push('\n');

    out.push_str("ensures=");
    out.push_str(&def.ensures.join(";"));
    out.push('\n');

    out.push_str("first_seen=");
    out.push_str(&def.first_seen.to_string());
    out.push('\n');

    out
}

fn deserialize_definition(text: &str) -> Option<Definition> {
    let mut map: HashMap<String, String> = HashMap::new();
    for line in text.lines() {
        if let Some((key, value)) = line.split_once('=') {
            map.insert(key.trim().to_string(), value.to_string());
        }
    }

    let source = unescape_newlines(map.get("source")?);
    let module = map.get("module").cloned().unwrap_or_default();
    let is_pub = map.get("is_pub").map(|v| v == "true").unwrap_or(false);

    let params: Vec<(String, String)> = map
        .get("params")
        .map(|s| {
            if s.is_empty() {
                return Vec::new();
            }
            s.split(',')
                .filter_map(|pair| {
                    let (n, t) = pair.split_once(':')?;
                    Some((n.to_string(), t.to_string()))
                })
                .collect()
        })
        .unwrap_or_default();

    let return_ty =
        map.get("return_ty")
            .and_then(|s| if s.is_empty() { None } else { Some(s.clone()) });

    let dependencies: Vec<ContentHash> = map
        .get("dependencies")
        .map(|s| {
            if s.is_empty() {
                return Vec::new();
            }
            s.split(',')
                .filter_map(|h| parse_hex_hash(h.trim()))
                .collect()
        })
        .unwrap_or_default();

    let requires: Vec<String> = map
        .get("requires")
        .map(|s| {
            if s.is_empty() {
                return Vec::new();
            }
            s.split(';').map(|r| r.to_string()).collect()
        })
        .unwrap_or_default();

    let ensures: Vec<String> = map
        .get("ensures")
        .map(|s| {
            if s.is_empty() {
                return Vec::new();
            }
            s.split(';').map(|r| r.to_string()).collect()
        })
        .unwrap_or_default();

    let first_seen: u64 = map
        .get("first_seen")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    Some(Definition {
        source,
        module,
        is_pub,
        params,
        return_ty,
        dependencies,
        requires,
        ensures,
        first_seen,
    })
}

/// Escape newlines for single-line storage.
fn escape_newlines(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\n', "\\n")
}

/// Unescape newlines from single-line storage.
fn unescape_newlines(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.peek() {
                Some('n') => {
                    result.push('\n');
                    chars.next();
                }
                Some('\\') => {
                    result.push('\\');
                    chars.next();
                }
                _ => {
                    result.push('\\');
                }
            }
        } else {
            result.push(ch);
        }
    }
    result
}

// ─── Hex Hash Parsing ──────────────────────────────────────────────

fn parse_hex_hash(hex: &str) -> Option<ContentHash> {
    if hex.len() != 64 {
        return None;
    }
    let mut bytes = [0u8; 32];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        if i >= 32 {
            return None;
        }
        let hi = hex_digit(chunk[0])?;
        let lo = hex_digit(chunk[1])?;
        bytes[i] = (hi << 4) | lo;
    }
    Some(ContentHash(bytes))
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

// ─── Helper: Codebase Directory ────────────────────────────────────

fn codebase_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("TRIDENT_CODEBASE_DIR") {
        return Some(PathBuf::from(dir));
    }
    std::env::var("HOME")
        .ok()
        .map(|h| PathBuf::from(h).join(".trident").join("codebase"))
}

fn unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ─── Function Source Formatter ─────────────────────────────────────
//
// Reconstructs source from AST fields. This is a simple formatter
// for storage in the codebase; it does not need to handle comments
// or preserve formatting (format.rs does that for the full file).

fn format_fn_source(func: &ast::FnDef) -> String {
    let mut out = String::new();

    if func.is_pub {
        out.push_str("pub ");
    }
    out.push_str("fn ");
    out.push_str(&func.name.node);

    // Type params.
    if !func.type_params.is_empty() {
        out.push('<');
        for (i, tp) in func.type_params.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(&tp.node);
        }
        out.push('>');
    }

    // Parameters.
    out.push('(');
    for (i, param) in func.params.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&param.name.node);
        out.push_str(": ");
        out.push_str(&format_type(&param.ty.node));
    }
    out.push(')');

    // Return type.
    if let Some(ref ret) = func.return_ty {
        out.push_str(" -> ");
        out.push_str(&format_type(&ret.node));
    }

    // Body.
    match &func.body {
        Some(body) => {
            out.push_str(" {\n");
            format_block(&body.node, &mut out, 1);
            out.push('}');
        }
        None => {
            // Intrinsic/extern: no body.
        }
    }

    out
}

fn format_block(block: &ast::Block, out: &mut String, indent: usize) {
    let pad = "    ".repeat(indent);
    for stmt in &block.stmts {
        format_stmt(&stmt.node, out, &pad, indent);
    }
    if let Some(ref tail) = block.tail_expr {
        out.push_str(&pad);
        out.push_str(&format_expr(&tail.node));
        out.push('\n');
    }
}

fn format_stmt(stmt: &Stmt, out: &mut String, pad: &str, indent: usize) {
    match stmt {
        Stmt::Let {
            mutable,
            pattern,
            ty,
            init,
        } => {
            out.push_str(pad);
            out.push_str("let ");
            if *mutable {
                out.push_str("mut ");
            }
            match pattern {
                ast::Pattern::Name(name) => out.push_str(&name.node),
                ast::Pattern::Tuple(names) => {
                    out.push('(');
                    for (i, n) in names.iter().enumerate() {
                        if i > 0 {
                            out.push_str(", ");
                        }
                        out.push_str(&n.node);
                    }
                    out.push(')');
                }
            }
            if let Some(t) = ty {
                out.push_str(": ");
                out.push_str(&format_type(&t.node));
            }
            out.push_str(" = ");
            out.push_str(&format_expr(&init.node));
            out.push('\n');
        }
        Stmt::Assign { place, value } => {
            out.push_str(pad);
            out.push_str(&format_place(&place.node));
            out.push_str(" = ");
            out.push_str(&format_expr(&value.node));
            out.push('\n');
        }
        Stmt::TupleAssign { names, value } => {
            out.push_str(pad);
            out.push('(');
            for (i, n) in names.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&n.node);
            }
            out.push_str(") = ");
            out.push_str(&format_expr(&value.node));
            out.push('\n');
        }
        Stmt::If {
            cond,
            then_block,
            else_block,
        } => {
            out.push_str(pad);
            out.push_str("if ");
            out.push_str(&format_expr(&cond.node));
            out.push_str(" {\n");
            format_block(&then_block.node, out, indent + 1);
            if let Some(else_blk) = else_block {
                out.push_str(pad);
                out.push_str("} else {\n");
                format_block(&else_blk.node, out, indent + 1);
            }
            out.push_str(pad);
            out.push_str("}\n");
        }
        Stmt::For {
            var,
            start,
            end,
            bound,
            body,
        } => {
            out.push_str(pad);
            out.push_str("for ");
            out.push_str(&var.node);
            out.push_str(" in ");
            out.push_str(&format_expr(&start.node));
            out.push_str("..");
            out.push_str(&format_expr(&end.node));
            if let Some(b) = bound {
                out.push_str(" bounded ");
                out.push_str(&b.to_string());
            }
            out.push_str(" {\n");
            format_block(&body.node, out, indent + 1);
            out.push_str(pad);
            out.push_str("}\n");
        }
        Stmt::Expr(expr) => {
            out.push_str(pad);
            out.push_str(&format_expr(&expr.node));
            out.push('\n');
        }
        Stmt::Return(val) => {
            out.push_str(pad);
            out.push_str("return");
            if let Some(v) = val {
                out.push(' ');
                out.push_str(&format_expr(&v.node));
            }
            out.push('\n');
        }
        Stmt::Emit { event_name, fields } | Stmt::Seal { event_name, fields } => {
            let kw = if matches!(stmt, Stmt::Emit { .. }) {
                "emit"
            } else {
                "seal"
            };
            out.push_str(pad);
            out.push_str(kw);
            out.push(' ');
            out.push_str(&event_name.node);
            out.push_str(" { ");
            for (i, (name, val)) in fields.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&name.node);
                out.push_str(": ");
                out.push_str(&format_expr(&val.node));
            }
            out.push_str(" }\n");
        }
        Stmt::Asm {
            body,
            effect,
            target,
        } => {
            out.push_str(pad);
            out.push_str("asm");
            match (target.as_deref(), *effect != 0) {
                (Some(tag), true) => {
                    if *effect > 0 {
                        out.push_str(&format!("({}, +{})", tag, effect));
                    } else {
                        out.push_str(&format!("({}, {})", tag, effect));
                    }
                }
                (Some(tag), false) => {
                    out.push_str(&format!("({})", tag));
                }
                (None, true) => {
                    if *effect > 0 {
                        out.push_str(&format!("(+{})", effect));
                    } else {
                        out.push_str(&format!("({})", effect));
                    }
                }
                (None, false) => {}
            }
            out.push_str(" {\n");
            let inner_pad = "    ".repeat(indent + 1);
            for line in body.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    out.push('\n');
                } else {
                    out.push_str(&inner_pad);
                    out.push_str(trimmed);
                    out.push('\n');
                }
            }
            out.push_str(pad);
            out.push_str("}\n");
        }
        Stmt::Match { expr, arms } => {
            out.push_str(pad);
            out.push_str("match ");
            out.push_str(&format_expr(&expr.node));
            out.push_str(" {\n");
            let arm_pad = "    ".repeat(indent + 1);
            for arm in arms {
                out.push_str(&arm_pad);
                match &arm.pattern.node {
                    ast::MatchPattern::Literal(ast::Literal::Integer(n)) => {
                        out.push_str(&n.to_string());
                    }
                    ast::MatchPattern::Literal(ast::Literal::Bool(b)) => {
                        out.push_str(if *b { "true" } else { "false" });
                    }
                    ast::MatchPattern::Wildcard => {
                        out.push('_');
                    }
                }
                out.push_str(" => {\n");
                format_block(&arm.body.node, out, indent + 2);
                out.push_str(&arm_pad);
                out.push_str("}\n");
            }
            out.push_str(pad);
            out.push_str("}\n");
        }
    }
}

fn format_expr(expr: &Expr) -> String {
    match expr {
        Expr::Literal(ast::Literal::Integer(n)) => n.to_string(),
        Expr::Literal(ast::Literal::Bool(b)) => b.to_string(),
        Expr::Var(name) => name.clone(),
        Expr::BinOp { op, lhs, rhs } => {
            let l = format_expr_prec(&lhs.node, op, true);
            let r = format_expr_prec(&rhs.node, op, false);
            format!("{} {} {}", l, op.as_str(), r)
        }
        Expr::Call {
            path,
            generic_args,
            args,
        } => {
            let args_str: Vec<String> = args.iter().map(|a| format_expr(&a.node)).collect();
            if generic_args.is_empty() {
                format!("{}({})", path.node.as_dotted(), args_str.join(", "))
            } else {
                let ga: Vec<String> = generic_args.iter().map(|a| a.node.to_string()).collect();
                format!(
                    "{}<{}>({})",
                    path.node.as_dotted(),
                    ga.join(", "),
                    args_str.join(", ")
                )
            }
        }
        Expr::FieldAccess { expr, field } => {
            format!("{}.{}", format_expr(&expr.node), field.node)
        }
        Expr::Index { expr, index } => {
            format!("{}[{}]", format_expr(&expr.node), format_expr(&index.node))
        }
        Expr::StructInit { path, fields } => {
            let fields_str: Vec<String> = fields
                .iter()
                .map(|(name, val)| format!("{}: {}", name.node, format_expr(&val.node)))
                .collect();
            format!("{} {{ {} }}", path.node.as_dotted(), fields_str.join(", "))
        }
        Expr::ArrayInit(elems) => {
            let inner: Vec<String> = elems.iter().map(|e| format_expr(&e.node)).collect();
            format!("[{}]", inner.join(", "))
        }
        Expr::Tuple(elems) => {
            let inner: Vec<String> = elems.iter().map(|e| format_expr(&e.node)).collect();
            format!("({})", inner.join(", "))
        }
    }
}

fn format_expr_prec(expr: &Expr, parent_op: &ast::BinOp, _is_left: bool) -> String {
    if let Expr::BinOp { op, .. } = expr {
        if op_precedence(op) < op_precedence(parent_op) {
            return format!("({})", format_expr(expr));
        }
    }
    format_expr(expr)
}

fn op_precedence(op: &ast::BinOp) -> u8 {
    match op {
        ast::BinOp::Eq => 2,
        ast::BinOp::Lt => 4,
        ast::BinOp::Add => 6,
        ast::BinOp::Mul | ast::BinOp::XFieldMul => 8,
        ast::BinOp::BitAnd | ast::BinOp::BitXor => 10,
        ast::BinOp::DivMod => 12,
    }
}

fn format_place(place: &ast::Place) -> String {
    match place {
        ast::Place::Var(name) => name.clone(),
        ast::Place::FieldAccess(base, field) => {
            format!("{}.{}", format_place(&base.node), field.node)
        }
        ast::Place::Index(base, idx) => {
            format!("{}[{}]", format_place(&base.node), format_expr(&idx.node))
        }
    }
}

fn format_type(ty: &Type) -> String {
    match ty {
        Type::Field => "Field".to_string(),
        Type::XField => "XField".to_string(),
        Type::Bool => "Bool".to_string(),
        Type::U32 => "U32".to_string(),
        Type::Digest => "Digest".to_string(),
        Type::Array(inner, size) => format!("[{}; {}]", format_type(inner), size),
        Type::Tuple(elems) => {
            let inner: Vec<String> = elems.iter().map(|e| format_type(e)).collect();
            format!("({})", inner.join(", "))
        }
        Type::Named(path) => path.as_dotted(),
    }
}

// ─── Dependency Extraction ─────────────────────────────────────────

/// Extract dependencies from a function body by walking for Call expressions.
fn extract_dependencies(
    func: &ast::FnDef,
    fn_hashes: &HashMap<String, ContentHash>,
) -> Vec<ContentHash> {
    let mut deps = Vec::new();
    let mut seen = std::collections::HashSet::new();

    if let Some(ref body) = func.body {
        walk_block_for_calls(&body.node, fn_hashes, &func.name.node, &mut deps, &mut seen);
    }

    deps
}

fn walk_block_for_calls(
    block: &ast::Block,
    fn_hashes: &HashMap<String, ContentHash>,
    self_name: &str,
    deps: &mut Vec<ContentHash>,
    seen: &mut std::collections::HashSet<ContentHash>,
) {
    for stmt in &block.stmts {
        walk_stmt_for_calls(&stmt.node, fn_hashes, self_name, deps, seen);
    }
    if let Some(ref tail) = block.tail_expr {
        walk_expr_for_calls(&tail.node, fn_hashes, self_name, deps, seen);
    }
}

fn walk_stmt_for_calls(
    stmt: &Stmt,
    fn_hashes: &HashMap<String, ContentHash>,
    self_name: &str,
    deps: &mut Vec<ContentHash>,
    seen: &mut std::collections::HashSet<ContentHash>,
) {
    match stmt {
        Stmt::Let { init, .. } => {
            walk_expr_for_calls(&init.node, fn_hashes, self_name, deps, seen);
        }
        Stmt::Assign { value, .. } => {
            walk_expr_for_calls(&value.node, fn_hashes, self_name, deps, seen);
        }
        Stmt::TupleAssign { value, .. } => {
            walk_expr_for_calls(&value.node, fn_hashes, self_name, deps, seen);
        }
        Stmt::If {
            cond,
            then_block,
            else_block,
        } => {
            walk_expr_for_calls(&cond.node, fn_hashes, self_name, deps, seen);
            walk_block_for_calls(&then_block.node, fn_hashes, self_name, deps, seen);
            if let Some(ref else_blk) = else_block {
                walk_block_for_calls(&else_blk.node, fn_hashes, self_name, deps, seen);
            }
        }
        Stmt::For {
            start, end, body, ..
        } => {
            walk_expr_for_calls(&start.node, fn_hashes, self_name, deps, seen);
            walk_expr_for_calls(&end.node, fn_hashes, self_name, deps, seen);
            walk_block_for_calls(&body.node, fn_hashes, self_name, deps, seen);
        }
        Stmt::Expr(expr) => {
            walk_expr_for_calls(&expr.node, fn_hashes, self_name, deps, seen);
        }
        Stmt::Return(Some(expr)) => {
            walk_expr_for_calls(&expr.node, fn_hashes, self_name, deps, seen);
        }
        Stmt::Return(None) | Stmt::Asm { .. } => {}
        Stmt::Emit { fields, .. } | Stmt::Seal { fields, .. } => {
            for (_, val) in fields {
                walk_expr_for_calls(&val.node, fn_hashes, self_name, deps, seen);
            }
        }
        Stmt::Match { expr, arms } => {
            walk_expr_for_calls(&expr.node, fn_hashes, self_name, deps, seen);
            for arm in arms {
                walk_block_for_calls(&arm.body.node, fn_hashes, self_name, deps, seen);
            }
        }
    }
}

fn walk_expr_for_calls(
    expr: &Expr,
    fn_hashes: &HashMap<String, ContentHash>,
    self_name: &str,
    deps: &mut Vec<ContentHash>,
    seen: &mut std::collections::HashSet<ContentHash>,
) {
    match expr {
        Expr::Call { path, args, .. } => {
            let name = path.node.as_dotted();
            let short = path.node.0.last().map(|s| s.as_str()).unwrap_or("");
            // Don't add self as a dependency.
            if name != self_name && short != self_name {
                // Try full name first, then short name.
                let hash = fn_hashes.get(&name).or_else(|| fn_hashes.get(short));
                if let Some(h) = hash {
                    if seen.insert(*h) {
                        deps.push(*h);
                    }
                }
            }
            for arg in args {
                walk_expr_for_calls(&arg.node, fn_hashes, self_name, deps, seen);
            }
        }
        Expr::BinOp { lhs, rhs, .. } => {
            walk_expr_for_calls(&lhs.node, fn_hashes, self_name, deps, seen);
            walk_expr_for_calls(&rhs.node, fn_hashes, self_name, deps, seen);
        }
        Expr::FieldAccess { expr, .. } => {
            walk_expr_for_calls(&expr.node, fn_hashes, self_name, deps, seen);
        }
        Expr::Index { expr, index } => {
            walk_expr_for_calls(&expr.node, fn_hashes, self_name, deps, seen);
            walk_expr_for_calls(&index.node, fn_hashes, self_name, deps, seen);
        }
        Expr::StructInit { fields, .. } => {
            for (_, val) in fields {
                walk_expr_for_calls(&val.node, fn_hashes, self_name, deps, seen);
            }
        }
        Expr::ArrayInit(elems) | Expr::Tuple(elems) => {
            for elem in elems {
                walk_expr_for_calls(&elem.node, fn_hashes, self_name, deps, seen);
            }
        }
        Expr::Literal(_) | Expr::Var(_) => {}
    }
}

// ─── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_file(source: &str) -> ast::File {
        crate::parse_source_silent(source, "test.tri").unwrap()
    }

    #[test]
    fn test_add_file_creates_definitions_and_names() {
        let tmp = tempfile::tempdir().unwrap();
        let mut cb = Codebase::open_at(tmp.path()).unwrap();

        let file = parse_file(
            "program test\nfn helper(x: Field) -> Field { x + 1 }\nfn main() { pub_write(helper(pub_read())) }\n",
        );
        let result = cb.add_file(&file);

        assert_eq!(result.added, 2);
        assert_eq!(result.updated, 0);
        assert_eq!(result.unchanged, 0);

        // Both names should exist.
        assert!(cb.lookup("helper").is_some());
        assert!(cb.lookup("main").is_some());

        // Stats should reflect 2 definitions and 2 names.
        let stats = cb.stats();
        assert_eq!(stats.definitions, 2);
        assert_eq!(stats.names, 2);
        assert!(stats.total_source_bytes > 0);
    }

    #[test]
    fn test_lookup_by_name() {
        let tmp = tempfile::tempdir().unwrap();
        let mut cb = Codebase::open_at(tmp.path()).unwrap();

        let file = parse_file(
            "program test\nfn add(a: Field, b: Field) -> Field { a + b }\nfn main() { }\n",
        );
        cb.add_file(&file);

        let def = cb.lookup("add").unwrap();
        assert!(def.source.contains("fn add"));
        assert_eq!(def.params.len(), 2);
        assert_eq!(def.params[0].0, "a");
        assert_eq!(def.params[0].1, "Field");
        assert_eq!(def.return_ty.as_deref(), Some("Field"));
        assert!(!def.is_pub);
    }

    #[test]
    fn test_rename() {
        let tmp = tempfile::tempdir().unwrap();
        let mut cb = Codebase::open_at(tmp.path()).unwrap();

        let file =
            parse_file("program test\nfn old_name(x: Field) -> Field { x }\nfn main() { }\n");
        cb.add_file(&file);

        assert!(cb.lookup("old_name").is_some());
        cb.rename("old_name", "new_name").unwrap();

        assert!(cb.lookup("old_name").is_none());
        assert!(cb.lookup("new_name").is_some());

        // Both old and new name should appear in history.
        let history = cb.name_history("new_name");
        assert!(!history.is_empty());
    }

    #[test]
    fn test_alias() {
        let tmp = tempfile::tempdir().unwrap();
        let mut cb = Codebase::open_at(tmp.path()).unwrap();

        let file =
            parse_file("program test\nfn original(x: Field) -> Field { x }\nfn main() { }\n");
        cb.add_file(&file);

        cb.alias("original", "shortcut").unwrap();

        // Both names should resolve to the same hash.
        let hash_orig = cb.names.get("original").unwrap();
        let hash_alias = cb.names.get("shortcut").unwrap();
        assert_eq!(hash_orig, hash_alias);

        // Both names should appear in names_for_hash.
        let names = cb.names_for_hash(hash_orig);
        assert!(names.contains(&"original"));
        assert!(names.contains(&"shortcut"));
    }

    #[test]
    fn test_add_updated_file_rebinds_name() {
        let tmp = tempfile::tempdir().unwrap();
        let mut cb = Codebase::open_at(tmp.path()).unwrap();

        // First version.
        let file1 =
            parse_file("program test\nfn helper(x: Field) -> Field { x + 1 }\nfn main() { }\n");
        cb.add_file(&file1);
        let hash1 = *cb.names.get("helper").unwrap();

        // Updated version (different body).
        let file2 =
            parse_file("program test\nfn helper(x: Field) -> Field { x + 2 }\nfn main() { }\n");
        let result = cb.add_file(&file2);

        let hash2 = *cb.names.get("helper").unwrap();
        assert_ne!(hash1, hash2, "hash should change when body changes");
        assert!(result.updated >= 1);

        // History should have both entries.
        let history = cb.name_history("helper");
        assert!(history.len() >= 2);
    }

    #[test]
    fn test_stats_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let cb = Codebase::open_at(tmp.path()).unwrap();

        let stats = cb.stats();
        assert_eq!(stats.definitions, 0);
        assert_eq!(stats.names, 0);
        assert_eq!(stats.total_source_bytes, 0);
    }

    #[test]
    fn test_empty_lookups_return_none() {
        let tmp = tempfile::tempdir().unwrap();
        let cb = Codebase::open_at(tmp.path()).unwrap();

        assert!(cb.lookup("nonexistent").is_none());
        assert!(cb.lookup_hash(&ContentHash::zero()).is_none());
        assert!(cb.view("nonexistent").is_none());
    }

    #[test]
    fn test_persistence_save_and_reload() {
        let tmp = tempfile::tempdir().unwrap();

        // Create and populate a codebase.
        {
            let mut cb = Codebase::open_at(tmp.path()).unwrap();
            let file = parse_file(
                "program test\npub fn add(a: Field, b: Field) -> Field { a + b }\nfn main() { pub_write(add(pub_read(), pub_read())) }\n",
            );
            cb.add_file(&file);
            cb.save().unwrap();
        }

        // Reload from disk.
        {
            let cb = Codebase::open_at(tmp.path()).unwrap();
            assert_eq!(cb.stats().definitions, 2);
            assert_eq!(cb.stats().names, 2);

            let def = cb.lookup("add").unwrap();
            assert!(def.source.contains("fn add"));
            assert_eq!(def.module, "test");
            assert!(def.is_pub);
            assert_eq!(def.params.len(), 2);
            assert_eq!(def.return_ty.as_deref(), Some("Field"));
        }
    }

    #[test]
    fn test_dependencies_extracted() {
        let tmp = tempfile::tempdir().unwrap();
        let mut cb = Codebase::open_at(tmp.path()).unwrap();

        let file = parse_file(
            "program test\nfn helper(x: Field) -> Field { x + 1 }\nfn main() { pub_write(helper(pub_read())) }\n",
        );
        cb.add_file(&file);

        let main_hash = cb.names.get("main").unwrap();
        let main_def = cb.lookup_hash(main_hash).unwrap();

        // main should depend on helper.
        let helper_hash = cb.names.get("helper").unwrap();
        assert!(
            main_def.dependencies.contains(helper_hash),
            "main should depend on helper"
        );

        // helper should have no dependencies (only calls builtins).
        let helper_def = cb.lookup_hash(helper_hash).unwrap();
        assert!(
            helper_def.dependencies.is_empty(),
            "helper should have no function deps"
        );
    }

    #[test]
    fn test_dependents() {
        let tmp = tempfile::tempdir().unwrap();
        let mut cb = Codebase::open_at(tmp.path()).unwrap();

        let file = parse_file(
            "program test\nfn helper(x: Field) -> Field { x + 1 }\nfn main() { pub_write(helper(pub_read())) }\n",
        );
        cb.add_file(&file);

        let helper_hash = cb.names.get("helper").unwrap();
        let dependents = cb.dependents(helper_hash);
        assert!(
            dependents.iter().any(|(name, _)| *name == "main"),
            "main should be a dependent of helper"
        );
    }

    #[test]
    fn test_view_output() {
        let tmp = tempfile::tempdir().unwrap();
        let mut cb = Codebase::open_at(tmp.path()).unwrap();

        let file = parse_file("program test\nfn id(x: Field) -> Field { x }\nfn main() { }\n");
        cb.add_file(&file);

        let view = cb.view("id").unwrap();
        assert!(view.contains("-- id #"));
        assert!(view.contains("fn id"));
    }

    #[test]
    fn test_rename_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let mut cb = Codebase::open_at(tmp.path()).unwrap();

        let file = parse_file("program test\nfn a() { }\nfn b() { }\nfn main() { }\n");
        cb.add_file(&file);

        // Rename nonexistent.
        assert!(cb.rename("nonexistent", "c").is_err());

        // Rename to existing name.
        assert!(cb.rename("a", "b").is_err());
    }

    #[test]
    fn test_alias_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let mut cb = Codebase::open_at(tmp.path()).unwrap();

        let file = parse_file("program test\nfn a() { }\nfn b() { }\nfn main() { }\n");
        cb.add_file(&file);

        // Alias nonexistent.
        assert!(cb.alias("nonexistent", "c").is_err());

        // Alias to existing name.
        assert!(cb.alias("a", "b").is_err());
    }

    #[test]
    fn test_escape_unescape_roundtrip() {
        let original = "fn main() {\n    pub_write(pub_read())\n}";
        let escaped = escape_newlines(original);
        assert!(!escaped.contains('\n'));
        let unescaped = unescape_newlines(&escaped);
        assert_eq!(unescaped, original);
    }

    #[test]
    fn test_escape_backslashes() {
        let original = "line1\\nstill_line1\nline2";
        let escaped = escape_newlines(original);
        let unescaped = unescape_newlines(&escaped);
        assert_eq!(unescaped, original);
    }

    #[test]
    fn test_parse_hex_hash() {
        let hash = ContentHash([0xAB; 32]);
        let hex = hash.to_hex();
        let parsed = parse_hex_hash(&hex).unwrap();
        assert_eq!(parsed, hash);

        // Invalid: too short.
        assert!(parse_hex_hash("abcd").is_none());
        // Invalid: wrong chars.
        assert!(parse_hex_hash(&"zz".repeat(32)).is_none());
    }

    #[test]
    fn test_definition_serialization_roundtrip() {
        let def = Definition {
            source: "fn add(a: Field, b: Field) -> Field {\n    a + b\n}".to_string(),
            module: "test".to_string(),
            is_pub: true,
            params: vec![
                ("a".to_string(), "Field".to_string()),
                ("b".to_string(), "Field".to_string()),
            ],
            return_ty: Some("Field".to_string()),
            dependencies: vec![ContentHash([0x01; 32])],
            requires: vec!["a > 0".to_string()],
            ensures: vec!["result == a + b".to_string()],
            first_seen: 1707580000,
        };

        let serialized = serialize_definition(&def);
        let deserialized = deserialize_definition(&serialized).unwrap();

        assert_eq!(deserialized.source, def.source);
        assert_eq!(deserialized.module, def.module);
        assert_eq!(deserialized.is_pub, def.is_pub);
        assert_eq!(deserialized.params, def.params);
        assert_eq!(deserialized.return_ty, def.return_ty);
        assert_eq!(deserialized.dependencies.len(), 1);
        assert_eq!(deserialized.dependencies[0], ContentHash([0x01; 32]));
        assert_eq!(deserialized.requires, def.requires);
        assert_eq!(deserialized.ensures, def.ensures);
        assert_eq!(deserialized.first_seen, def.first_seen);
    }

    #[test]
    fn test_lookup_by_prefix() {
        let tmp = tempfile::tempdir().unwrap();
        let mut cb = Codebase::open_at(tmp.path()).unwrap();

        let file = parse_file("program test\nfn id(x: Field) -> Field { x }\nfn main() { }\n");
        cb.add_file(&file);

        let hash = *cb.names.get("id").unwrap();
        let hex = hash.to_hex();

        // Lookup by hex prefix.
        let (found_hash, _) = cb.lookup_by_prefix(&hex[..8]).unwrap();
        assert_eq!(*found_hash, hash);

        // Lookup by short hash.
        let short = hash.to_short();
        let (found_hash, _) = cb.lookup_by_prefix(&short).unwrap();
        assert_eq!(*found_hash, hash);
    }

    #[test]
    fn test_unchanged_on_readd() {
        let tmp = tempfile::tempdir().unwrap();
        let mut cb = Codebase::open_at(tmp.path()).unwrap();

        let file = parse_file("program test\nfn id(x: Field) -> Field { x }\nfn main() { }\n");
        cb.add_file(&file);

        // Adding the same file again should show unchanged.
        let result = cb.add_file(&file);
        assert_eq!(result.added, 0);
        assert_eq!(result.updated, 0);
        assert_eq!(result.unchanged, 2);
    }

    #[test]
    fn test_list_names_sorted() {
        let tmp = tempfile::tempdir().unwrap();
        let mut cb = Codebase::open_at(tmp.path()).unwrap();

        let file = parse_file("program test\nfn zzz() { }\nfn aaa() { }\nfn main() { }\n");
        cb.add_file(&file);

        let names: Vec<&str> = cb.list_names().iter().map(|(n, _)| *n).collect();
        assert_eq!(names, vec!["aaa", "main", "zzz"]);
    }

    #[test]
    fn test_pub_function_stored() {
        let tmp = tempfile::tempdir().unwrap();
        let mut cb = Codebase::open_at(tmp.path()).unwrap();

        let file =
            parse_file("program test\npub fn visible(x: Field) -> Field { x }\nfn main() { }\n");
        cb.add_file(&file);

        let def = cb.lookup("visible").unwrap();
        assert!(def.is_pub);
    }

    #[test]
    fn test_spec_annotations_stored() {
        let tmp = tempfile::tempdir().unwrap();
        let mut cb = Codebase::open_at(tmp.path()).unwrap();

        let file = parse_file(
            "program test\n#[requires(a > 0)]\n#[ensures(result == a + 1)]\nfn inc(a: Field) -> Field { a + 1 }\nfn main() { }\n",
        );
        cb.add_file(&file);

        let def = cb.lookup("inc").unwrap();
        assert_eq!(def.requires, vec!["a > 0"]);
        assert_eq!(def.ensures, vec!["result == a + 1"]);
    }
}
