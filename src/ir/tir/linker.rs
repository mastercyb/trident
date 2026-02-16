/// Per-module TASM output ready for linking.
#[derive(Clone, Debug)]
pub(crate) struct ModuleTasm {
    /// Dotted module name (e.g. "merkle").
    pub(crate) module_name: String,
    /// Whether this is the program entry module.
    pub(crate) is_program: bool,
    /// Raw TASM output from the emitter.
    pub(crate) tasm: String,
}

/// Link multiple module TASM outputs into a single program.
/// Performs dead code elimination: only includes functions reachable
/// from the program entry point.
pub(crate) fn link(modules: Vec<ModuleTasm>) -> String {
    // First, mangle all modules and collect the full TASM.
    let mut all_lines = Vec::new();
    let mut entry_label = String::new();

    // Find program entry
    if let Some(prog) = modules.iter().find(|m| m.is_program) {
        entry_label = format!("{}main", mangle_module(&prog.module_name));
    }

    // Mangle all modules
    for module in &modules {
        let prefix = mangle_module(&module.module_name);
        let mangled = mangle_labels(&module.tasm, &prefix, module.is_program);
        for line in mangled.lines() {
            all_lines.push(line.to_string());
        }
    }

    // Build a map: label -> (start_line, end_line) and label -> [called labels]
    let mut functions: Vec<(String, usize, usize)> = Vec::new();
    let mut i = 0;
    while i < all_lines.len() {
        let trimmed = all_lines[i].trim();
        if trimmed.ends_with(':') && !trimmed.is_empty() {
            let label = trimmed.trim_end_matches(':').to_string();
            let start = i;
            i += 1;
            // Scan until next label or end
            while i < all_lines.len() {
                let t = all_lines[i].trim();
                if t.ends_with(':') && !t.is_empty() && !t.starts_with("//") {
                    break;
                }
                i += 1;
            }
            functions.push((label, start, i));
        } else {
            i += 1;
        }
    }

    // Find call targets for each function
    use std::collections::{BTreeMap, BTreeSet, VecDeque};
    let mut call_graph: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (label, start, end) in &functions {
        let mut calls = Vec::new();
        for line in &all_lines[*start..*end] {
            let t = line.trim();
            if let Some(target) = t.strip_prefix("call ") {
                calls.push(target.to_string());
            } else if t == "recurse" {
                calls.push(label.clone());
            }
        }
        call_graph.insert(label.clone(), calls);
    }

    // Build a suffix index for fuzzy label matching.
    // Cross-module calls may carry the caller's prefix (e.g. card__plumb__fn)
    // while the label is defined as plumb__fn. Build suffix → label map.
    let all_labels: BTreeSet<String> = functions.iter().map(|(l, _, _)| l.clone()).collect();
    let resolve_target = |target: &str| -> String {
        if all_labels.contains(target) {
            return target.to_string();
        }
        // Try stripping successive prefixes (before first __)
        let mut t = target;
        while let Some(pos) = t.find("__") {
            let suffix = &t[pos + 2..];
            if !suffix.is_empty() && all_labels.contains(suffix) {
                return suffix.to_string();
            }
            t = suffix;
        }
        target.to_string()
    };

    // BFS from entry label to find all reachable functions
    let mut reachable: BTreeSet<String> = BTreeSet::new();
    let mut queue: VecDeque<String> = VecDeque::new();
    queue.push_back(entry_label.clone());
    while let Some(label) = queue.pop_front() {
        if reachable.contains(&label) {
            continue;
        }
        reachable.insert(label.clone());
        if let Some(calls) = call_graph.get(&label) {
            for target in calls {
                let resolved = resolve_target(target);
                if !reachable.contains(&resolved) {
                    queue.push_back(resolved);
                }
            }
        }
    }

    // Emit only reachable functions
    let mut output = Vec::new();
    output.push(format!("    call {}", entry_label));
    output.push("    halt".to_string());

    for (label, start, end) in &functions {
        if reachable.contains(label) {
            for line in &all_lines[*start..*end] {
                output.push(line.clone());
            }
        }
    }

    output.join("\n")
}

/// Mangle all labels in a TASM block with a module prefix.
/// `__foo:` becomes `modname__foo:`
/// `call __foo` becomes `call modname__foo`
fn mangle_labels(tasm: &str, prefix: &str, is_program: bool) -> String {
    let mut result = Vec::new();

    for line in tasm.lines() {
        let trimmed = line.trim();

        // Skip the entry point wrapper (call __main / halt) — the linker handles that
        if is_program && (trimmed == "call __main" || trimmed == "halt") {
            continue;
        }

        if trimmed.is_empty() {
            result.push(String::new());
            continue;
        }

        // Label definition: `__foo:` → `prefix__foo:`
        if trimmed.ends_with(':') && trimmed.starts_with("__") {
            let label = trimmed.trim_end_matches(':');
            let body = label.strip_prefix("__").unwrap_or(label);
            result.push(format!("{}{}:", prefix, body));
            continue;
        }

        // Call instruction: `call __foo` → `call prefix__foo`
        if let Some(target) = trimmed.strip_prefix("call __") {
            result.push(format!("    call {}{}", prefix, target));
            continue;
        }

        // Everything else passes through
        result.push(line.to_string());
    }

    result.join("\n")
}

/// Convert a dotted module name to a label-safe prefix.
/// "crypto.sponge" → "crypto_sponge__"
fn mangle_module(name: &str) -> String {
    format!("{}__", name.replace('.', "_"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mangle_module() {
        assert_eq!(mangle_module("merkle"), "merkle__");
        assert_eq!(mangle_module("crypto.sponge"), "crypto_sponge__");
    }

    #[test]
    fn test_single_module_link() {
        let modules = vec![ModuleTasm {
            module_name: "test".to_string(),
            is_program: true,
            tasm: "    call __main\n    halt\n\n__main:\n    read_io 1\n    return\n".to_string(),
        }];
        let linked = link(modules);
        assert!(linked.contains("call test__main"));
        assert!(linked.contains("halt"));
        assert!(linked.contains("test__main:"));
    }

    #[test]
    fn test_multi_module_link() {
        let modules = vec![
            ModuleTasm {
                module_name: "merkle".to_string(),
                is_program: false,
                tasm: "__verify:\n    read_io 1\n    return\n__unused:\n    push 0\n    return\n"
                    .to_string(),
            },
            ModuleTasm {
                module_name: "main_prog".to_string(),
                is_program: true,
                tasm: "    call __main\n    halt\n\n__main:\n    call merkle__verify\n    return\n"
                    .to_string(),
            },
        ];
        let linked = link(modules);
        // Entry point should use the program module's main
        assert!(linked.contains("call main_prog__main"));
        assert!(linked.contains("halt"));
        // merkle's verify is called, so it should be included
        assert!(linked.contains("merkle__verify:"));
        // merkle's unused function should be eliminated by DCE
        assert!(!linked.contains("merkle__unused:"));
        assert!(linked.contains("main_prog__main:"));
    }
}
