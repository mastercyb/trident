//! Pretty-printing and diff utilities for Trident definitions.
//!
//! Used by `trident view` to reconstruct human-readable source from
//! stored ASTs and to display definition metadata (hashes, history).

use crate::ast::{File, FileKind, FnDef, Item};
use crate::format;
use crate::hash::ContentHash;
use crate::span::Spanned;

// ─── Pretty-print a single function ───────────────────────────────

/// Pretty-print a single function definition by wrapping it in a
/// minimal synthetic `File` and running the canonical formatter.
pub fn format_function(func: &FnDef) -> String {
    // Build a minimal File containing only this function.
    let file = File {
        kind: FileKind::Program,
        name: Spanned::dummy("_view".to_string()),
        uses: Vec::new(),
        declarations: Vec::new(),
        items: vec![Spanned::dummy(Item::Fn(func.clone()))],
    };

    let formatted = format::format_file(&file, &[], "");

    // The formatter emits "program _view\n\n<fn>\n".
    // Strip the synthetic header to isolate the function text.
    strip_synthetic_header(&formatted)
}

/// Pretty-print a function with an optional cost annotation appended
/// as a trailing comment on the signature line.
pub fn format_function_with_cost(func: &FnDef, cost: Option<&str>) -> String {
    let base = format_function(func);
    match cost {
        Some(c) => {
            // Insert cost comment after the opening brace of the function
            if let Some(brace_pos) = base.find('{') {
                let (before, after) = base.split_at(brace_pos + 1);
                format!("{} // cost: {}{}", before.trim_end(), c, after)
            } else {
                // No body (intrinsic) — append at end of first line
                let mut lines: Vec<&str> = base.lines().collect();
                if let Some(first) = lines.first_mut() {
                    return format!("{} // cost: {}", first, c);
                }
                base
            }
        }
        None => base,
    }
}

/// Strip the synthetic "program _view\n\n" header produced by the
/// formatter when we wrap a single function in a dummy File.
fn strip_synthetic_header(formatted: &str) -> String {
    // The formatter produces: "program _view\n\n<items>\n"
    // Find the first blank line and take everything after it.
    if let Some(pos) = formatted.find("\n\n") {
        let rest = &formatted[pos + 2..];
        rest.to_string()
    } else {
        formatted.to_string()
    }
}

// ─── Line-based diff ──────────────────────────────────────────────

/// Compute a simple line-based diff between two source strings.
///
/// Uses a longest-common-subsequence algorithm to produce unified-diff
/// style output:
/// - Lines only in `old`: `"- <line>"`
/// - Lines only in `new`: `"+ <line>"`
/// - Lines in both:       `"  <line>"`
pub fn diff(old: &str, new: &str) -> String {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();

    let lcs = lcs_table(&old_lines, &new_lines);
    let edits = backtrack_diff(&lcs, &old_lines, &new_lines);

    let mut out = String::new();
    for edit in &edits {
        match edit {
            DiffLine::Keep(line) => {
                out.push_str("  ");
                out.push_str(line);
                out.push('\n');
            }
            DiffLine::Remove(line) => {
                out.push_str("- ");
                out.push_str(line);
                out.push('\n');
            }
            DiffLine::Add(line) => {
                out.push_str("+ ");
                out.push_str(line);
                out.push('\n');
            }
        }
    }
    out
}

enum DiffLine<'a> {
    Keep(&'a str),
    Remove(&'a str),
    Add(&'a str),
}

/// Build the LCS length table for two slices of lines.
fn lcs_table<'a>(old: &[&'a str], new: &[&'a str]) -> Vec<Vec<u32>> {
    let m = old.len();
    let n = new.len();
    let mut table = vec![vec![0u32; n + 1]; m + 1];
    for i in 1..=m {
        for j in 1..=n {
            if old[i - 1] == new[j - 1] {
                table[i][j] = table[i - 1][j - 1] + 1;
            } else {
                table[i][j] = table[i - 1][j].max(table[i][j - 1]);
            }
        }
    }
    table
}

/// Backtrack through the LCS table to produce diff edits.
fn backtrack_diff<'a>(table: &[Vec<u32>], old: &[&'a str], new: &[&'a str]) -> Vec<DiffLine<'a>> {
    let mut edits = Vec::new();
    let mut i = old.len();
    let mut j = new.len();

    while i > 0 || j > 0 {
        if i > 0 && j > 0 && old[i - 1] == new[j - 1] {
            edits.push(DiffLine::Keep(old[i - 1]));
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || table[i][j - 1] >= table[i - 1][j]) {
            edits.push(DiffLine::Add(new[j - 1]));
            j -= 1;
        } else {
            edits.push(DiffLine::Remove(old[i - 1]));
            i -= 1;
        }
    }

    edits.reverse();
    edits
}

// ─── Definition summary ──────────────────────────────────────────

/// Format a one-line definition summary: hash, name, and signature.
///
/// Extracts the signature from the first line of formatted source.
///
/// Example output: `#a1b2c3d4  main  fn main() -> Field`
pub fn format_summary(name: &str, hash: &ContentHash, source: &str) -> String {
    let sig = extract_signature(source);
    format!("{}  {:<16}  {}", hash, name, sig)
}

/// Extract the function signature from formatted source text.
///
/// Takes the first line that starts with `fn ` or `pub fn `, stripping
/// the body opening brace if present.
fn extract_signature(source: &str) -> String {
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("fn ") || trimmed.starts_with("pub fn ") {
            // Remove trailing " {" if present
            let sig = trimmed.trim_end_matches(" {").trim_end_matches('{');
            return sig.trim_end().to_string();
        }
    }
    // Fallback: use first non-empty line
    source
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim()
        .to_string()
}

// ─── Definition listing ──────────────────────────────────────────

/// Format a tabular definition listing.
///
/// Each entry is `(name, hash, formatted_source)`.
///
/// Output:
/// ```text
/// HASH        NAME              SIGNATURE
/// #a1b2c3d4   main              fn main()
/// #f6e5d4c3   helper            fn helper(x: Field) -> Field
/// ```
pub fn format_listing(entries: &[(String, ContentHash, String)]) -> String {
    let mut out = String::new();

    // Header
    out.push_str(&format!(
        "{:<12}  {:<18}  {}\n",
        "HASH", "NAME", "SIGNATURE"
    ));

    for (name, hash, source) in entries {
        let sig = extract_signature(source);
        out.push_str(&format!(
            "{:<12}  {:<18}  {}\n",
            hash.to_string(),
            name,
            sig
        ));
    }

    out
}

// ─── History formatting ──────────────────────────────────────────

/// Format history entries for a named definition.
///
/// Each entry is `(hash, unix_timestamp)`. The first entry is marked
/// as `(current)`.
///
/// Output:
/// ```text
/// NAME: main
///   #a1b2c3d4  2026-02-10 09:00:00  (current)
///   #f6e5d4c3  2026-02-09 15:30:00
/// ```
pub fn format_history(name: &str, entries: &[(ContentHash, u64)]) -> String {
    let mut out = String::new();
    out.push_str(&format!("NAME: {}\n", name));

    for (i, (hash, timestamp)) in entries.iter().enumerate() {
        let time_str = format_unix_timestamp(*timestamp);
        let current = if i == 0 { "  (current)" } else { "" };
        out.push_str(&format!("  {}  {}{}\n", hash, time_str, current));
    }

    out
}

/// Convert a Unix timestamp to "YYYY-MM-DD HH:MM:SS" using basic
/// arithmetic (no external dependency).
fn format_unix_timestamp(ts: u64) -> String {
    // Seconds per unit
    const SECS_PER_MIN: u64 = 60;
    const SECS_PER_HOUR: u64 = 3600;
    const SECS_PER_DAY: u64 = 86400;

    let mut remaining = ts;

    // Compute time of day
    let days_since_epoch = remaining / SECS_PER_DAY;
    remaining %= SECS_PER_DAY;
    let hours = remaining / SECS_PER_HOUR;
    remaining %= SECS_PER_HOUR;
    let minutes = remaining / SECS_PER_MIN;
    let seconds = remaining % SECS_PER_MIN;

    // Compute date from days since 1970-01-01 (civil calendar)
    let (year, month, day) = days_to_civil(days_since_epoch);

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        year, month, day, hours, minutes, seconds
    )
}

/// Convert days since Unix epoch (1970-01-01) to (year, month, day).
///
/// Uses the algorithm from Howard Hinnant's `chrono`-compatible date
/// library (public domain).
fn days_to_civil(days: u64) -> (i64, u32, u32) {
    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64; // day of era [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // year of era [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year [0, 365]
    let mp = (5 * doy + 2) / 153; // month proxy [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // day [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // month [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    (y, m as u32, d as u32)
}

// ─── Helpers for the CLI ─────────────────────────────────────────

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
    fn_hashes: &std::collections::HashMap<String, ContentHash>,
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

// ─── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash;

    fn parse_file(source: &str) -> File {
        crate::parse_source_silent(source, "test.tri").unwrap()
    }

    #[test]
    fn test_format_function_produces_valid_source() {
        let source = "program test\n\nfn add(a: Field, b: Field) -> Field {\n    a + b\n}\n";
        let file = parse_file(source);
        let func = find_function(&file, "add").expect("add function should exist");
        let formatted = format_function(func);

        assert!(formatted.contains("fn add("));
        assert!(formatted.contains("a: Field, b: Field"));
        assert!(formatted.contains("-> Field"));
        assert!(formatted.contains("a + b"));
    }

    #[test]
    fn test_format_function_with_annotations() {
        let source = "program test\n\n#[requires(a + b < 1000)]\n#[ensures(result == a + b)]\nfn add(a: Field, b: Field) -> Field {\n    a + b\n}\n";
        let file = parse_file(source);
        let func = find_function(&file, "add").expect("add function should exist");
        let formatted = format_function(func);

        assert!(formatted.contains("#[requires("));
        assert!(formatted.contains("#[ensures("));
        assert!(formatted.contains("fn add("));
    }

    #[test]
    fn test_format_function_pub() {
        let source = "module test\n\npub fn helper(x: Field) -> Field {\n    x + 1\n}\n";
        let file = parse_file(source);
        let func = find_function(&file, "helper").expect("helper function should exist");
        let formatted = format_function(func);

        assert!(formatted.contains("pub fn helper("));
    }

    #[test]
    fn test_format_function_with_cost() {
        let source = "program test\n\nfn add(a: Field, b: Field) -> Field {\n    a + b\n}\n";
        let file = parse_file(source);
        let func = find_function(&file, "add").expect("add function should exist");
        let formatted = format_function_with_cost(func, Some("cc=5, hash=0"));

        assert!(formatted.contains("// cost: cc=5, hash=0"));
    }

    #[test]
    fn test_format_summary_extracts_signature() {
        let hash = ContentHash([0xAB; 32]);
        let source = "fn main() -> Field {\n    42\n}\n";
        let summary = format_summary("main", &hash, source);

        assert!(summary.contains("main"));
        assert!(summary.contains("fn main() -> Field"));
        assert!(summary.contains(&hash.to_string()));
    }

    #[test]
    fn test_format_listing_tabular() {
        let h1 = ContentHash([0xAA; 32]);
        let h2 = ContentHash([0xBB; 32]);
        let entries = vec![
            (
                "main".to_string(),
                h1,
                "fn main() {\n    pub_write(0)\n}\n".to_string(),
            ),
            (
                "helper".to_string(),
                h2,
                "fn helper(x: Field) -> Field {\n    x + 1\n}\n".to_string(),
            ),
        ];

        let listing = format_listing(&entries);

        assert!(listing.contains("HASH"));
        assert!(listing.contains("NAME"));
        assert!(listing.contains("SIGNATURE"));
        assert!(listing.contains("main"));
        assert!(listing.contains("helper"));
        assert!(listing.contains("fn main()"));
        assert!(listing.contains("fn helper(x: Field) -> Field"));
    }

    #[test]
    fn test_diff_additions_and_removals() {
        let old = "fn main() {\n    let x: Field = 1\n    pub_write(x)\n}\n";
        let new =
            "fn main() {\n    let x: Field = 2\n    let y: Field = 3\n    pub_write(x + y)\n}\n";

        let d = diff(old, new);

        // Common lines should be prefixed with "  "
        assert!(d.contains("  fn main() {"));
        // Removed line
        assert!(d.contains("- "));
        // Added lines
        assert!(d.contains("+ "));
    }

    #[test]
    fn test_diff_identical() {
        let text = "fn main() {\n    42\n}\n";
        let d = diff(text, text);

        // All lines should be "keep" (prefixed with "  ")
        for line in d.lines() {
            assert!(
                line.starts_with("  "),
                "identical diff should only have keep lines, got: {:?}",
                line
            );
        }
    }

    #[test]
    fn test_diff_empty_to_something() {
        let d = diff("", "hello\nworld\n");
        assert!(d.contains("+ hello"));
        assert!(d.contains("+ world"));
    }

    #[test]
    fn test_format_history_chronological() {
        let h1 = ContentHash([0xAA; 32]);
        let h2 = ContentHash([0xBB; 32]);
        // 2026-02-10 09:00:00 UTC = 1770681600
        // 2026-02-09 15:30:00 UTC = 1770618600
        let entries = vec![(h1, 1770681600), (h2, 1770618600)];

        let history = format_history("main", &entries);

        assert!(history.contains("NAME: main"));
        assert!(history.contains("(current)"));
        assert!(history.contains(&h1.to_string()));
        assert!(history.contains(&h2.to_string()));
        // Only the first entry should be marked current
        let lines: Vec<&str> = history.lines().collect();
        assert!(lines[1].contains("(current)"));
        assert!(!lines[2].contains("(current)"));
    }

    #[test]
    fn test_format_unix_timestamp() {
        // 2026-02-10 00:00:00 UTC = 1770681600
        let ts = format_unix_timestamp(1770681600);
        assert_eq!(ts, "2026-02-10 00:00:00");
    }

    #[test]
    fn test_format_unix_timestamp_epoch() {
        let ts = format_unix_timestamp(0);
        assert_eq!(ts, "1970-01-01 00:00:00");
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
    fn test_extract_signature() {
        assert_eq!(extract_signature("fn main() {\n    42\n}\n"), "fn main()");
        assert_eq!(
            extract_signature("pub fn helper(x: Field) -> Field {\n    x\n}\n"),
            "pub fn helper(x: Field) -> Field"
        );
        assert_eq!(
            extract_signature("#[requires(x > 0)]\nfn f(x: Field) -> Field {\n    x\n}\n"),
            "fn f(x: Field) -> Field"
        );
    }
}
