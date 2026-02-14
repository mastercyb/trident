use std::path::PathBuf;

use super::{load_and_parse, resolve_input};

pub fn cmd_hash(input: PathBuf, full: bool) {
    let ri = resolve_input(&input);
    let (_, file) = load_and_parse(&ri.entry);

    let fn_hashes = trident::hash::hash_file(&file);
    let file_hash = trident::hash::hash_file_content(&file);

    if full {
        eprintln!("File: {} {}", file_hash.to_hex(), ri.entry.display());
    } else {
        eprintln!("File: {} {}", file_hash, ri.entry.display());
    }

    let mut sorted: Vec<_> = fn_hashes.iter().collect();
    sorted.sort_by_key(|(name, _)| (*name).clone());
    for (name, hash) in sorted {
        if full {
            println!("  {} {}", hash.to_hex(), name);
        } else {
            println!("  {} {}", hash, name);
        }
    }
}
