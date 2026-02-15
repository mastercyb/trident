//! Documentation generation for Trident projects.
//!
//! Produces markdown documentation listing all public functions, structs,
//! constants, and events with their type signatures and cost annotations.

use std::path::Path;

use crate::ast;
use crate::ast::FileKind;
use crate::cost;
use crate::diagnostic::Diagnostic;
use crate::pipeline::PreparedProject;
use crate::target::TargetConfig;
use crate::CompileOptions;

/// Generate markdown documentation for a Trident project.
///
/// Resolves all modules, parses and type-checks them, computes cost analysis,
/// and produces a markdown document listing all public functions, structs,
/// constants, and events with their type signatures and cost annotations.
pub(crate) fn generate_docs(
    entry_path: &Path,
    options: &CompileOptions,
) -> Result<String, Vec<Diagnostic>> {
    let project = PreparedProject::build(entry_path, options)?;

    // Compute cost analysis per module
    let mut module_costs: Vec<Option<cost::ProgramCost>> = Vec::new();
    for pm in &project.modules {
        let pc = cost::CostAnalyzer::for_target(&options.target_config.name).analyze_file(&pm.file);
        module_costs.push(Some(pc));
    }

    // Determine the program name from the entry module
    let program_name = project
        .program_module()
        .map(|m| m.file.name.node.clone())
        .unwrap_or_else(|| "project".to_string());

    let mut doc = String::new();
    doc.push_str(&format!("# {}\n", program_name));

    // --- Functions ---
    let mut fn_entries: Vec<String> = Vec::new();
    for (i, pm) in project.modules.iter().enumerate() {
        let module_name = &pm.file.name.node;
        let costs = module_costs[i].as_ref();
        for item in &pm.file.items {
            if let ast::Item::Fn(func) = &item.node {
                // Skip test functions, intrinsic-only, and non-pub functions in modules
                if func.is_test {
                    continue;
                }
                if pm.file.kind == FileKind::Module && !func.is_pub {
                    continue;
                }
                // Skip cfg-excluded items
                if let Some(ref cfg) = func.cfg {
                    if !options.cfg_flags.contains(&cfg.node) {
                        continue;
                    }
                }

                let sig = format_fn_signature(func);
                let fn_cost =
                    costs.and_then(|pc| pc.functions.iter().find(|f| f.name == func.name.node));

                let mut entry = format!("### `{}`\n", sig);
                if let Some(fc) = fn_cost {
                    let c = &fc.cost;
                    let sn = costs.unwrap().short_names();
                    entry.push_str(&format!(
                        "**Cost:** cc={}, hash={}, u32={} | dominant: {}\n",
                        c.get(0),
                        c.get(1),
                        c.get(2),
                        c.dominant_table(&sn)
                    ));
                }
                entry.push_str(&format!("**Module:** {}\n", module_name));
                fn_entries.push(entry);
            }
        }
    }

    if !fn_entries.is_empty() {
        doc.push_str("\n## Functions\n\n");
        for entry in &fn_entries {
            doc.push_str(entry);
            doc.push('\n');
        }
    }

    // --- Structs ---
    let mut struct_entries: Vec<String> = Vec::new();
    for pm in project.modules.iter() {
        for item in &pm.file.items {
            if let ast::Item::Struct(sdef) = &item.node {
                if pm.file.kind == FileKind::Module && !sdef.is_pub {
                    continue;
                }
                if let Some(ref cfg) = sdef.cfg {
                    if !options.cfg_flags.contains(&cfg.node) {
                        continue;
                    }
                }

                let mut entry = format!("### `struct {}`\n", sdef.name.node);
                entry.push_str("| Field | Type | Width |\n");
                entry.push_str("|-------|------|-------|\n");
                let mut total_width: u32 = 0;
                for field in &sdef.fields {
                    let ty_str = format_ast_type(&field.ty.node);
                    let width = ast_type_width(&field.ty.node, &options.target_config);
                    total_width += width;
                    entry.push_str(&format!(
                        "| {} | {} | {} |\n",
                        field.name.node, ty_str, width
                    ));
                }
                entry.push_str(&format!("Total width: {} field elements\n", total_width));
                struct_entries.push(entry);
            }
        }
    }

    if !struct_entries.is_empty() {
        doc.push_str("\n## Structs\n\n");
        for entry in &struct_entries {
            doc.push_str(entry);
            doc.push('\n');
        }
    }

    // --- Constants ---
    let mut const_entries: Vec<(String, String, String)> = Vec::new(); // (name, type, value)
    for pm in project.modules.iter() {
        for item in &pm.file.items {
            if let ast::Item::Const(cdef) = &item.node {
                if pm.file.kind == FileKind::Module && !cdef.is_pub {
                    continue;
                }
                if let Some(ref cfg) = cdef.cfg {
                    if !options.cfg_flags.contains(&cfg.node) {
                        continue;
                    }
                }
                let ty_str = format_ast_type(&cdef.ty.node);
                let val_str = format_const_value(&cdef.value.node);
                const_entries.push((cdef.name.node.clone(), ty_str, val_str));
            }
        }
    }

    if !const_entries.is_empty() {
        doc.push_str("\n## Constants\n\n");
        doc.push_str("| Name | Type | Value |\n");
        doc.push_str("|------|------|-------|\n");
        for (name, ty, val) in &const_entries {
            doc.push_str(&format!("| {} | {} | {} |\n", name, ty, val));
        }
        doc.push('\n');
    }

    // --- Events ---
    let mut event_entries: Vec<String> = Vec::new();
    for pm in project.modules.iter() {
        for item in &pm.file.items {
            if let ast::Item::Event(edef) = &item.node {
                if let Some(ref cfg) = edef.cfg {
                    if !options.cfg_flags.contains(&cfg.node) {
                        continue;
                    }
                }
                let mut entry = format!("### `event {}`\n", edef.name.node);
                entry.push_str("| Field | Type |\n");
                entry.push_str("|-------|------|\n");
                for field in &edef.fields {
                    let ty_str = format_ast_type(&field.ty.node);
                    entry.push_str(&format!("| {} | {} |\n", field.name.node, ty_str));
                }
                event_entries.push(entry);
            }
        }
    }

    if !event_entries.is_empty() {
        doc.push_str("\n## Events\n\n");
        for entry in &event_entries {
            doc.push_str(entry);
            doc.push('\n');
        }
    }

    // --- Cost Summary ---
    // Aggregate costs across all modules — use the program module's cost if it exists,
    // otherwise sum all module costs.
    let program_cost_idx = project
        .modules
        .iter()
        .enumerate()
        .find(|(_, m)| m.file.kind == FileKind::Program)
        .map(|(i, _)| i);

    let program_cost = program_cost_idx.and_then(|i| module_costs[i].as_ref());

    let total_cost = if let Some(pc) = program_cost {
        pc.total.clone()
    } else {
        // Sum across all modules
        let mut total = cost::TableCost::ZERO;
        for pc in module_costs.iter().flatten() {
            total = total.add(&pc.total);
        }
        total
    };

    let padded_height = if let Some(pc) = program_cost {
        pc.padded_height
    } else {
        cost::next_power_of_two(total_cost.max_height())
    };

    doc.push_str("\n## Cost Summary\n\n");
    doc.push_str("| Table | Height |\n");
    doc.push_str("|-------|--------|\n");
    doc.push_str(&format!("| Processor | {} |\n", total_cost.get(0)));
    doc.push_str(&format!("| Hash | {} |\n", total_cost.get(1)));
    doc.push_str(&format!("| U32 | {} |\n", total_cost.get(2)));
    doc.push_str(&format!("| Padded | {} |\n", padded_height));

    Ok(doc)
}

/// Format an AST type for documentation display.
pub(crate) fn format_ast_type(ty: &ast::Type) -> String {
    match ty {
        ast::Type::Field => "Field".to_string(),
        ast::Type::XField => "XField".to_string(),
        ast::Type::Bool => "Bool".to_string(),
        ast::Type::U32 => "U32".to_string(),
        ast::Type::Digest => "Digest".to_string(),
        ast::Type::Array(inner, size) => format!("[{}; {}]", format_ast_type(inner), size),
        ast::Type::Tuple(elems) => {
            let parts: Vec<_> = elems.iter().map(format_ast_type).collect();
            format!("({})", parts.join(", "))
        }
        ast::Type::Named(path) => path.as_dotted(),
    }
}

/// Compute the width in field elements for an AST type (best-effort).
pub(crate) fn ast_type_width(ty: &ast::Type, config: &TargetConfig) -> u32 {
    match ty {
        ast::Type::Field | ast::Type::Bool | ast::Type::U32 => 1,
        ast::Type::XField => config.xfield_width,
        ast::Type::Digest => config.digest_width,
        ast::Type::Array(inner, size) => {
            let inner_w = ast_type_width(inner, config);
            let n = size.as_literal().unwrap_or(1) as u32;
            inner_w * n
        }
        ast::Type::Tuple(elems) => elems.iter().map(|e| ast_type_width(e, config)).sum(),
        ast::Type::Named(_) => 1, // unknown, default to 1
    }
}

/// Format a function signature for documentation.
pub(crate) fn format_fn_signature(func: &ast::FnDef) -> String {
    let mut sig = String::from("fn ");
    sig.push_str(&func.name.node);

    // Type params
    if !func.type_params.is_empty() {
        let params: Vec<_> = func.type_params.iter().map(|p| p.node.clone()).collect();
        sig.push_str(&format!("<{}>", params.join(", ")));
    }

    sig.push('(');
    let params: Vec<String> = func
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name.node, format_ast_type(&p.ty.node)))
        .collect();
    sig.push_str(&params.join(", "));
    sig.push(')');

    if let Some(ref ret) = func.return_ty {
        sig.push_str(&format!(" -> {}", format_ast_type(&ret.node)));
    }

    sig
}

/// Format a constant value expression for documentation.
pub(crate) fn format_const_value(expr: &ast::Expr) -> String {
    match expr {
        ast::Expr::Literal(ast::Literal::Integer(n)) => n.to_string(),
        ast::Expr::Literal(ast::Literal::Bool(b)) => b.to_string(),
        _ => "...".to_string(),
    }
}
