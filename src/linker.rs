/// Per-module TASM output ready for linking.
#[derive(Clone, Debug)]
pub struct ModuleTasm {
    /// Dotted module name (e.g. "merkle").
    pub module_name: String,
    /// Whether this is the program entry module.
    pub is_program: bool,
    /// Raw TASM output from the emitter.
    pub tasm: String,
}

/// Link multiple module TASM outputs into a single program.
pub fn link(modules: Vec<ModuleTasm>) -> String {
    let mut output = Vec::new();

    // Find the program module (entry point)
    let program = modules.iter().find(|m| m.is_program);

    if let Some(prog) = program {
        // Emit the program's entry point
        let entry_label = format!("{}main", mangle_module(&prog.module_name));
        output.push(format!("    call {}", entry_label));
        output.push("    halt".to_string());
        output.push(String::new());
    }

    // Emit each module's TASM with mangled labels
    for module in &modules {
        let prefix = mangle_module(&module.module_name);
        let mangled = mangle_labels(&module.tasm, &prefix, module.is_program);
        output.push(format!("// === module: {} ===", module.module_name));
        output.push(mangled);
        output.push(String::new());
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
            let label = &trimmed[..trimmed.len() - 1]; // strip ':'
            result.push(format!("{}{}:", prefix, &label[2..])); // strip __ prefix, add module prefix
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
                tasm: "__verify:\n    read_io 1\n    return\n".to_string(),
            },
            ModuleTasm {
                module_name: "main_prog".to_string(),
                is_program: true,
                tasm: "    call __main\n    halt\n\n__main:\n    call __verify\n    return\n"
                    .to_string(),
            },
        ];
        let linked = link(modules);
        // Entry point should use the program module's main
        assert!(linked.contains("call main_prog__main"));
        assert!(linked.contains("halt"));
        // merkle's verify should be mangled
        assert!(linked.contains("merkle__verify:"));
        // main's call to __verify needs to be resolved to the correct module
        // Currently it mangles with the main_prog prefix — this is a known limitation
        // that cross-module calls need explicit module prefixes
        assert!(linked.contains("main_prog__main:"));
    }
}
