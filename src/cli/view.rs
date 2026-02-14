use std::path::PathBuf;
use std::process;

use super::{load_and_parse, resolve_input};

pub fn cmd_view(name: String, input: Option<PathBuf>, full: bool) {
    let input =
        input.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let ri = resolve_input(&input);
    let (_, file) = load_and_parse(&ri.entry);
    let filename = ri.entry.to_string_lossy().to_string();

    let fn_hashes = trident::hash::hash_file(&file);

    // Try to find the function: by hash prefix or by name
    let (fn_name, func) = if trident::view::looks_like_hash(&name) {
        if let Some((found_name, found_func)) =
            trident::view::find_function_by_hash(&file, &fn_hashes, &name)
        {
            (found_name, found_func.clone())
        } else if let Some(found_func) = trident::view::find_function(&file, &name) {
            (name.clone(), found_func.clone())
        } else {
            eprintln!("error: no function matching '{}' found", name);
            process::exit(1);
        }
    } else if let Some(found_func) = trident::view::find_function(&file, &name) {
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

    let formatted = trident::view::format_function(&func);

    if let Some(hash) = fn_hashes.get(&fn_name) {
        if full {
            eprintln!("Hash: {}", hash.to_hex());
        } else {
            eprintln!("Hash: {}", hash);
        }
    }

    print!("{}", formatted);
}
