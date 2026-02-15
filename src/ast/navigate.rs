//! AST navigation: find functions by name or content hash.

use std::collections::BTreeMap;

use super::{File, FnDef, Item};
use crate::hash::ContentHash;

/// Find a function by name in a parsed file.
pub fn find_function<'a>(file: &'a File, name: &str) -> Option<&'a FnDef> {
    for item in &file.items {
        if let Item::Fn(func) = &item.node {
            if func.name.node == name {
                return Some(func);
            }
        }
    }
    None
}

/// Find a function by content hash prefix in a parsed file.
///
/// Returns `Some((name, func))` if exactly one function matches the
/// given hex prefix. Returns `None` if no match or ambiguous.
pub fn find_function_by_hash<'a>(
    file: &'a File,
    fn_hashes: &BTreeMap<String, ContentHash>,
    prefix: &str,
) -> Option<(String, &'a FnDef)> {
    let prefix_lower = prefix.to_lowercase();
    let mut matches: Vec<(String, &FnDef)> = Vec::new();

    for item in &file.items {
        if let Item::Fn(func) = &item.node {
            if let Some(hash) = fn_hashes.get(&func.name.node) {
                let hex = hash.to_hex();
                let short = hash.to_short();
                if hex.starts_with(&prefix_lower) || short.starts_with(&prefix_lower) {
                    matches.push((func.name.node.clone(), func));
                }
            }
        }
    }

    if matches.len() == 1 {
        Some(matches.into_iter().next().unwrap())
    } else {
        None
    }
}

/// Check if a string looks like a hex hash prefix (all hex digits).
pub fn looks_like_hash(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash;

    fn parse_file(source: &str) -> File {
        crate::parse_source_silent(source, "test.tri").unwrap()
    }

    #[test]
    fn test_find_function_by_name() {
        let source =
            "program test\n\nfn main() {\n    pub_write(0)\n}\n\nfn helper(x: Field) -> Field {\n    x + 1\n}\n";
        let file = parse_file(source);
        assert!(find_function(&file, "main").is_some());
        assert!(find_function(&file, "helper").is_some());
        assert!(find_function(&file, "nonexistent").is_none());
    }

    #[test]
    fn test_find_function_by_hash_prefix() {
        let source =
            "program test\n\nfn main() {\n    pub_write(0)\n}\n\nfn helper(x: Field) -> Field {\n    x + 1\n}\n";
        let file = parse_file(source);
        let fn_hashes = hash::hash_file(&file);

        // Get the hash for "main" and use its first 6 hex chars as prefix
        let main_hash = &fn_hashes["main"];
        let prefix = &main_hash.to_hex()[..6];

        let result = find_function_by_hash(&file, &fn_hashes, prefix);
        assert!(result.is_some());
        let (name, _func) = result.unwrap();
        assert_eq!(name, "main");
    }

    #[test]
    fn test_looks_like_hash() {
        assert!(looks_like_hash("a1b2c3d4"));
        assert!(looks_like_hash("ABCDEF"));
        assert!(looks_like_hash("0123456789"));
        assert!(!looks_like_hash("main"));
        assert!(!looks_like_hash(""));
        assert!(!looks_like_hash("a1b2g3")); // 'g' is not hex
    }
}
