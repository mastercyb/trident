use std::path::{Path, PathBuf};
use std::process;

use clap::Args;

use super::resolve_tri_files;

#[derive(Args)]
pub struct FmtArgs {
    /// Input .tri file or directory (defaults to current directory)
    pub input: Option<PathBuf>,
    /// Check formatting without modifying (exit 1 if unformatted)
    #[arg(long)]
    pub check: bool,
}

pub fn cmd_fmt(args: FmtArgs) {
    let FmtArgs { input, check } = args;
    let input = input.unwrap_or_else(|| PathBuf::from("."));
    let files = resolve_tri_files(&input);

    if files.is_empty() {
        eprintln!("No .tri files found in '{}'", input.display());
        return;
    }

    let mut any_unformatted = false;
    for file in &files {
        match format_single_file(file, check) {
            Ok(changed) if changed => any_unformatted = true,
            Err(msg) => eprintln!("error: {}", msg),
            _ => {}
        }
    }

    if check && any_unformatted {
        process::exit(1);
    }
}

/// Format a single .tri file. Returns Ok(true) if the file was changed/would be changed.
fn format_single_file(path: &Path, check: bool) -> Result<bool, String> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read '{}': {}", path.display(), e))?;
    let filename = path.to_string_lossy().to_string();
    let formatted = trident::format_source(&source, &filename)
        .map_err(|_| format!("cannot format '{}' (parse errors)", path.display()))?;

    if formatted == source {
        if check {
            eprintln!("OK: {}", path.display());
        } else {
            eprintln!("Already formatted: {}", path.display());
        }
        return Ok(false);
    }

    if check {
        eprintln!("would reformat: {}", path.display());
        return Ok(true);
    }

    std::fs::write(path, &formatted)
        .map_err(|e| format!("cannot write '{}': {}", path.display(), e))?;
    eprintln!("Formatted: {}", path.display());
    Ok(true)
}
