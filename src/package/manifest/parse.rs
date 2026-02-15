use std::collections::BTreeMap;
use std::path::PathBuf;

use super::{Dependency, Manifest};

// ─── Parsing ───────────────────────────────────────────────────────

/// Parse the `[dependencies]` section from trident.toml content.
///
/// Handles three forms:
///   name = "64hexchars"                          -> Hash dep
///   name = { name = "x", registry = "url" }      -> Registry dep
///   name = { path = "relative/path" }             -> Path dep
pub fn parse_dependencies(toml_content: &str) -> Manifest {
    let mut deps: BTreeMap<String, Dependency> = BTreeMap::new();
    let mut in_deps_section = false;

    for line in toml_content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Detect section headers.
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let section = trimmed[1..trimmed.len() - 1].trim();
            in_deps_section = section == "dependencies";
            continue;
        }

        if !in_deps_section {
            continue;
        }

        if let Some((key, value)) = trimmed.split_once('=') {
            let key = key.trim().trim_matches('"');
            let value = value.trim();

            if value.starts_with('{') {
                // Inline table: parse key-value pairs inside braces.
                let inner = value.trim_start_matches('{').trim_end_matches('}').trim();
                let fields = parse_inline_table(inner);

                if let Some(path_val) = fields.get("path") {
                    deps.insert(
                        key.to_string(),
                        Dependency::Path {
                            path: PathBuf::from(path_val),
                        },
                    );
                } else if let Some(reg_name) = fields.get("name") {
                    let registry = fields.get("registry").cloned().unwrap_or_default();
                    deps.insert(
                        key.to_string(),
                        Dependency::Registry {
                            name: reg_name.clone(),
                            registry,
                        },
                    );
                }
            } else {
                // Plain string value — strip quotes.
                let val = value.trim_matches('"');
                if is_hex_hash(val) {
                    deps.insert(
                        key.to_string(),
                        Dependency::Hash {
                            hash: val.to_string(),
                        },
                    );
                }
            }
        }
    }

    Manifest { dependencies: deps }
}

/// Parse a TOML inline table body: `name = "x", registry = "url"`.
pub(super) fn parse_inline_table(s: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for pair in s.split(',') {
        let pair = pair.trim();
        if let Some((k, v)) = pair.split_once('=') {
            let k = k.trim().trim_matches('"');
            let v = v.trim().trim_matches('"');
            map.insert(k.to_string(), v.to_string());
        }
    }
    map
}

/// Check whether a string looks like a 64-char hex hash.
pub(super) fn is_hex_hash(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}
