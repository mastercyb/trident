//! Definitions store — hash-keyed definitions storage.
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

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::ast::{self, Item};
use crate::hash::{self, ContentHash};

// ─── Data Structures ───────────────────────────────────────────────

/// The codebase database.
///
/// Stores function definitions by content hash, with name mappings.
/// Persisted to disk at `~/.trident/codebase/` (or `$TRIDENT_CODEBASE_DIR`).
pub struct Codebase {
    /// Hash -> definition source code.
    pub(super) definitions: BTreeMap<ContentHash, Definition>,
    /// Name -> hash mapping (current bindings).
    pub(super) names: BTreeMap<String, ContentHash>,
    /// Hash -> list of names that have pointed to it (history).
    pub(super) name_history: BTreeMap<ContentHash, Vec<NameEntry>>,
    /// Root directory for persistence.
    pub(super) root: PathBuf,
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

mod deps;
mod format;
mod persist;

use deps::extract_dependencies;
use format::{format_fn_source, format_type};
use persist::{codebase_dir, serialize_definition, unix_timestamp};

#[cfg(test)]
mod tests;

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
            definitions: BTreeMap::new(),
            names: BTreeMap::new(),
            name_history: BTreeMap::new(),
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
}
