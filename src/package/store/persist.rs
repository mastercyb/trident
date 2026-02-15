use std::collections::HashMap;
use std::path::PathBuf;

use crate::hash::ContentHash;

use super::{Codebase, Definition, NameEntry};

impl Codebase {
    // ─── Persistence: Load ─────────────────────────────────────

    pub(super) fn load(&mut self) -> std::io::Result<()> {
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
                if let Some(hash) = ContentHash::from_hex(hex.trim()) {
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
                let hash = match ContentHash::from_hex(stem) {
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
            let hash = match ContentHash::from_hex(parts[1]) {
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

pub(super) fn serialize_definition(def: &Definition) -> String {
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

pub(super) fn deserialize_definition(text: &str) -> Option<Definition> {
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
                .filter_map(|h| ContentHash::from_hex(h.trim()))
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
pub(super) fn escape_newlines(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\n', "\\n")
}

/// Unescape newlines from single-line storage.
pub(super) fn unescape_newlines(s: &str) -> String {
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

// ─── Helper: Codebase Directory ────────────────────────────────────

pub(super) fn codebase_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("TRIDENT_CODEBASE_DIR") {
        return Some(PathBuf::from(dir));
    }
    std::env::var("HOME")
        .ok()
        .map(|h| PathBuf::from(h).join(".trident").join("codebase"))
}

pub(super) fn unix_timestamp() -> u64 {
    crate::package::unix_timestamp()
}
