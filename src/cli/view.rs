use std::path::PathBuf;
use std::process;

use clap::Args;

use super::{load_and_parse, resolve_input};

#[derive(Args)]
pub struct ViewArgs {
    /// Function name or content hash prefix
    pub name: String,
    /// Input .tri file or directory with trident.toml
    #[arg(short, long)]
    pub input: Option<PathBuf>,
    /// Show full hash instead of short form
    #[arg(long)]
    pub full: bool,
}

pub fn cmd_view(args: ViewArgs) {
    let ViewArgs { name, input, full } = args;
    let input =
        input.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let ri = resolve_input(&input);
    let (_, file) = load_and_parse(&ri.entry);
    let filename = ri.entry.to_string_lossy().to_string();

    let fn_hashes = trident::hash::hash_file(&file);

    // Try to find the function: by hash prefix or by name
    let (fn_name, func) = if trident::ast::navigate::looks_like_hash(&name) {
        if let Some((found_name, found_func)) =
            trident::ast::navigate::find_function_by_hash(&file, &fn_hashes, &name)
        {
            (found_name, found_func.clone())
        } else if let Some(found_func) = trident::ast::navigate::find_function(&file, &name) {
            (name.clone(), found_func.clone())
        } else {
            eprintln!("error: no function matching '{}' found", name);
            process::exit(1);
        }
    } else if let Some(found_func) = trident::ast::navigate::find_function(&file, &name) {
        (name.clone(), found_func.clone())
    } else {
        eprintln!("error: function '{}' not found in '{}'", name, filename);
        eprintln!("\nAvailable functions:");
        for item in &file.items {
            if let trident::ast::Item::Fn(f) = &item.node {
                if let Some(hash) = fn_hashes.get(&f.name.node) {
                    eprintln!("  {}  {}", hash, f.name.node);
                }
            }
        }
        process::exit(1);
    };

    let formatted = trident::ast::display::format_function(&func);

    if let Some(hash) = fn_hashes.get(&fn_name) {
        if full {
            eprintln!("Hash: {}", hash.to_hex());
        } else {
            eprintln!("Hash: {}", hash);
        }
    }

    print!("{}", formatted);
}

// ─── Line-based diff ──────────────────────────────────────────────

use trident::hash::ContentHash;

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
pub fn format_summary(name: &str, hash: &ContentHash, source: &str) -> String {
    let sig = extract_signature(source);
    format!("{}  {:<16}  {}", hash, name, sig)
}

/// Extract the function signature from formatted source text.
fn extract_signature(source: &str) -> String {
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("fn ") || trimmed.starts_with("pub fn ") {
            let sig = trimmed.trim_end_matches(" {").trim_end_matches('{');
            return sig.trim_end().to_string();
        }
    }
    source
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim()
        .to_string()
}

// ─── Definition listing ──────────────────────────────────────────

/// Format a tabular definition listing.
pub fn format_listing(entries: &[(String, ContentHash, String)]) -> String {
    let mut out = String::new();

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

/// Convert a Unix timestamp to "YYYY-MM-DD HH:MM:SS".
fn format_unix_timestamp(ts: u64) -> String {
    const SECS_PER_MIN: u64 = 60;
    const SECS_PER_HOUR: u64 = 3600;
    const SECS_PER_DAY: u64 = 86400;

    let mut remaining = ts;

    let days_since_epoch = remaining / SECS_PER_DAY;
    remaining %= SECS_PER_DAY;
    let hours = remaining / SECS_PER_HOUR;
    remaining %= SECS_PER_HOUR;
    let minutes = remaining / SECS_PER_MIN;
    let seconds = remaining % SECS_PER_MIN;

    let (year, month, day) = trident::deploy::days_to_date(days_since_epoch);

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        year, month, day, hours, minutes, seconds
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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

        assert!(d.contains("  fn main() {"));
        assert!(d.contains("- "));
        assert!(d.contains("+ "));
    }

    #[test]
    fn test_diff_identical() {
        let text = "fn main() {\n    42\n}\n";
        let d = diff(text, text);

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
        let entries = vec![(h1, 1770681600), (h2, 1770618600)];

        let history = format_history("main", &entries);

        assert!(history.contains("NAME: main"));
        assert!(history.contains("(current)"));
        assert!(history.contains(&h1.to_string()));
        assert!(history.contains(&h2.to_string()));
        let lines: Vec<&str> = history.lines().collect();
        assert!(lines[1].contains("(current)"));
        assert!(!lines[2].contains("(current)"));
    }

    #[test]
    fn test_format_unix_timestamp() {
        let ts = format_unix_timestamp(1770681600);
        assert_eq!(ts, "2026-02-10 00:00:00");
    }

    #[test]
    fn test_format_unix_timestamp_epoch() {
        let ts = format_unix_timestamp(0);
        assert_eq!(ts, "1970-01-01 00:00:00");
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
