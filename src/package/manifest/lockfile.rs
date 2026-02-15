use std::collections::BTreeMap;
use std::path::Path;

use super::parse::parse_inline_table;
use super::{LockedDep, Lockfile};

// ─── Lockfile I/O ──────────────────────────────────────────────────

/// Load a lockfile from disk.
///
/// Format:
/// ```text
/// # trident.lock — DO NOT EDIT MANUALLY
/// [lock]
/// name = { hash = "abc...", source = "registry:https://..." }
/// ```
pub fn load_lockfile(path: &Path) -> Result<Lockfile, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read lockfile '{}': {}", path.display(), e))?;

    let mut locked: BTreeMap<String, LockedDep> = BTreeMap::new();
    let mut in_lock_section = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let section = trimmed[1..trimmed.len() - 1].trim();
            in_lock_section = section == "lock";
            continue;
        }
        if !in_lock_section {
            continue;
        }

        if let Some((key, value)) = trimmed.split_once('=') {
            let name = key.trim().to_string();
            let value = value.trim();

            if value.starts_with('{') {
                let inner = value.trim_start_matches('{').trim_end_matches('}').trim();
                let fields = parse_inline_table(inner);
                let hash = fields.get("hash").cloned().unwrap_or_default();
                let source = fields.get("source").cloned().unwrap_or_default();
                locked.insert(
                    name.clone(),
                    LockedDep {
                        name: name.clone(),
                        hash,
                        source,
                    },
                );
            }
        }
    }

    Ok(Lockfile { locked })
}

/// Save a lockfile to disk.
pub fn save_lockfile(path: &Path, lockfile: &Lockfile) -> Result<(), String> {
    let mut out = String::new();
    out.push_str("# trident.lock — DO NOT EDIT MANUALLY\n");
    out.push_str("[lock]\n");

    // Sort entries for deterministic output.
    let mut entries: Vec<_> = lockfile.locked.iter().collect();
    entries.sort_by_key(|(k, _)| (*k).clone());

    for (name, dep) in entries {
        out.push_str(&format!(
            "{} = {{ hash = \"{}\", source = \"{}\" }}\n",
            name, dep.hash, dep.source,
        ));
    }

    std::fs::write(path, &out)
        .map_err(|e| format!("cannot write lockfile '{}': {}", path.display(), e))
}
