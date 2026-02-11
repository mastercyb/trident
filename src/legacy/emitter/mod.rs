mod call;
mod expr;
mod helpers;
mod inst;
mod stmt;

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::span::Spanned;
use crate::stack::StackManager;
use crate::target::TargetConfig;
use crate::typecheck::MonoInstance;

#[allow(unused_imports)] // backends used in tests
use super::backend::{
    CairoBackend, MidenBackend, OpenVMBackend, SP1Backend, StackBackend, TritonBackend,
};

/// A deferred block to emit after the current function.
struct DeferredBlock {
    label: String,
    block: Block,
    /// If true, this is a "then" branch that must set the flag to 0 for if/else.
    clears_flag: bool,
}

/// TASM emitter — walks the AST and produces Triton Assembly.
#[allow(dead_code)] // backend field used via trait dispatch; accessors for future multi-backend.
pub struct Emitter {
    output: Vec<String>,
    label_counter: u32,
    /// Stack model: LRU-based manager with automatic RAM spill/reload.
    stack: StackManager,
    /// Blocks to emit as subroutines after the current function.
    deferred: Vec<DeferredBlock>,
    /// Struct field layouts: var_name → { field_name → (offset_from_top, field_width) }
    struct_layouts: HashMap<String, HashMap<String, (u32, u32)>>,
    /// Return widths of user-defined functions.
    fn_return_widths: HashMap<String, u32>,
    /// Event tags: event name → sequential integer tag.
    event_tags: HashMap<String, u64>,
    /// Event field names in declaration order: event name → [field_name, ...].
    event_defs: HashMap<String, Vec<String>>,
    /// Struct type definitions: struct_name → StructDef.
    struct_types: HashMap<String, StructDef>,
    /// Constants: qualified or short name → integer value.
    constants: HashMap<String, u64>,
    /// Next temporary RAM address for runtime array ops.
    temp_ram_addr: u64,
    /// Intrinsic map: function name → intrinsic TASM name.
    intrinsic_map: HashMap<String, String>,
    /// Module alias map: short name → full module name (e.g. "hash" → "std.hash").
    module_aliases: HashMap<String, String>,
    /// Monomorphized generic function instances to emit.
    mono_instances: Vec<MonoInstance>,
    /// Generic function AST definitions (name → FnDef), for emitting monomorphized copies.
    generic_fn_defs: HashMap<String, FnDef>,
    /// Current size parameter substitutions during monomorphized emission.
    current_subs: HashMap<String, u64>,
    /// Per-call-site resolutions from the type checker (consumed in AST order).
    call_resolutions: Vec<MonoInstance>,
    /// Index into call_resolutions for the next generic call.
    call_resolution_idx: usize,
    /// Active cfg flags for conditional compilation.
    cfg_flags: HashSet<String>,
    /// Target VM configuration.
    target_config: TargetConfig,
    /// Backend for instruction emission.
    backend: Box<dyn StackBackend>,
}

impl Default for Emitter {
    fn default() -> Self {
        Self::new()
    }
}

impl Emitter {
    pub(crate) fn new() -> Self {
        Self::with_backend(Box::new(TritonBackend), TargetConfig::triton())
    }

    pub(crate) fn with_backend(
        backend: Box<dyn StackBackend>,
        target_config: TargetConfig,
    ) -> Self {
        // Build a SpillFormatter from the backend's instruction methods.
        let swap_fn = {
            let swap_sample_1 = backend.inst_swap(1);
            // Extract prefix before the number: "    swap " or "    dup." etc.
            // We parse "swap 1" → prefix="swap ", or "swap.1" → prefix="swap."
            let prefix = swap_sample_1.trim_end_matches('1').to_string();
            Box::new(move |d: u32| format!("    {}{}", prefix, d)) as Box<dyn Fn(u32) -> String>
        };
        let push_fn = {
            let push_sample_1 = backend.inst_push(1);
            let prefix = push_sample_1.trim_end_matches('1').to_string();
            Box::new(move |v: u64| format!("    {}{}", prefix, v)) as Box<dyn Fn(u64) -> String>
        };
        let pop1 = format!("    {}", backend.inst_pop(1));
        let write_mem1 = format!("    {}", backend.inst_write_mem(1));
        let read_mem1 = format!("    {}", backend.inst_read_mem(1));
        let formatter = crate::stack::SpillFormatter {
            fmt_swap: swap_fn,
            fmt_push: push_fn,
            fmt_pop1: pop1,
            fmt_write_mem1: write_mem1,
            fmt_read_mem1: read_mem1,
        };
        let stack = StackManager::with_formatter(
            target_config.stack_depth,
            target_config.spill_ram_base,
            formatter,
        );
        Self {
            output: Vec::new(),
            label_counter: 0,
            stack,
            deferred: Vec::new(),
            struct_layouts: HashMap::new(),
            fn_return_widths: HashMap::new(),
            event_tags: HashMap::new(),
            event_defs: HashMap::new(),
            struct_types: HashMap::new(),
            constants: HashMap::new(),
            temp_ram_addr: target_config.spill_ram_base / 2,
            intrinsic_map: HashMap::new(),
            module_aliases: HashMap::new(),
            mono_instances: Vec::new(),
            generic_fn_defs: HashMap::new(),
            current_subs: HashMap::new(),
            call_resolutions: Vec::new(),
            call_resolution_idx: 0,
            cfg_flags: HashSet::from(["debug".to_string()]),
            target_config,
            backend,
        }
    }

    /// Output file extension for the current backend (e.g. ".tasm").
    #[allow(dead_code)]
    pub(crate) fn output_extension(&self) -> &str {
        self.backend.output_extension()
    }

    /// Target name from the backend (e.g. "triton").
    #[allow(dead_code)]
    pub(crate) fn target_name(&self) -> &str {
        self.backend.target_name()
    }

    /// Access the target configuration.
    #[allow(dead_code)]
    pub(crate) fn target_config(&self) -> &TargetConfig {
        &self.target_config
    }

    pub(crate) fn with_cfg_flags(mut self, flags: HashSet<String>) -> Self {
        self.cfg_flags = flags;
        self
    }

    /// Check if an item's cfg attribute is active.
    fn is_cfg_active(&self, cfg: &Option<Spanned<String>>) -> bool {
        match cfg {
            None => true,
            Some(flag) => self.cfg_flags.contains(&flag.node),
        }
    }

    /// Set a pre-built intrinsic map (from all project modules).
    pub(crate) fn with_intrinsics(mut self, map: HashMap<String, String>) -> Self {
        self.intrinsic_map = map;
        self
    }

    /// Set module alias map (short name → full dotted name).
    pub(crate) fn with_module_aliases(mut self, aliases: HashMap<String, String>) -> Self {
        self.module_aliases = aliases;
        self
    }

    /// Set external constants (from imported modules).
    pub(crate) fn with_constants(mut self, constants: HashMap<String, u64>) -> Self {
        self.constants.extend(constants);
        self
    }

    /// Set monomorphized generic function instances to emit.
    pub(crate) fn with_mono_instances(mut self, instances: Vec<MonoInstance>) -> Self {
        self.mono_instances = instances;
        self
    }

    /// Set per-call-site resolutions from the type checker.
    pub(crate) fn with_call_resolutions(mut self, resolutions: Vec<MonoInstance>) -> Self {
        self.call_resolutions = resolutions;
        self
    }

    /// Check if a top-level item's cfg is active.
    fn is_item_cfg_active(&self, item: &Item) -> bool {
        match item {
            Item::Fn(f) => self.is_cfg_active(&f.cfg),
            Item::Const(c) => self.is_cfg_active(&c.cfg),
            Item::Struct(s) => self.is_cfg_active(&s.cfg),
            Item::Event(e) => self.is_cfg_active(&e.cfg),
        }
    }

    pub(crate) fn emit_file(mut self, file: &File) -> String {
        // Pre-scan: collect return widths and detect generic functions.
        for item in &file.items {
            if !self.is_item_cfg_active(&item.node) {
                continue;
            }
            if let Item::Fn(func) = &item.node {
                if !func.type_params.is_empty() {
                    // Generic function: store AST for later monomorphized emission.
                    self.generic_fn_defs
                        .insert(func.name.node.clone(), func.clone());
                } else {
                    let width = func
                        .return_ty
                        .as_ref()
                        .map(|t| resolve_type_width(&t.node, &self.target_config))
                        .unwrap_or(0);
                    self.fn_return_widths.insert(func.name.node.clone(), width);
                }
            }
        }

        // Pre-scan: register return widths for monomorphized instances.
        for inst in &self.mono_instances {
            if let Some(gdef) = self.generic_fn_defs.get(&inst.name) {
                let mut subs = HashMap::new();
                for (param, val) in gdef.type_params.iter().zip(inst.size_args.iter()) {
                    subs.insert(param.node.clone(), *val);
                }
                let width = gdef
                    .return_ty
                    .as_ref()
                    .map(|t| resolve_type_width_with_subs(&t.node, &subs, &self.target_config))
                    .unwrap_or(0);
                let mangled = inst.mangled_name();
                self.fn_return_widths.insert(mangled.clone(), width);
            }
        }

        // Pre-scan: collect intrinsic mappings.
        for item in &file.items {
            if !self.is_item_cfg_active(&item.node) {
                continue;
            }
            if let Item::Fn(func) = &item.node {
                if let Some(ref intrinsic) = func.intrinsic {
                    // Extract inner value from "intrinsic(VALUE)"
                    let intr_value = if let Some(start) = intrinsic.node.find('(') {
                        let end = intrinsic.node.rfind(')').unwrap_or(intrinsic.node.len());
                        intrinsic.node[start + 1..end].to_string()
                    } else {
                        intrinsic.node.clone()
                    };
                    self.intrinsic_map
                        .insert(func.name.node.clone(), intr_value);
                }
            }
        }

        // Pre-scan: collect struct type definitions.
        for item in &file.items {
            if !self.is_item_cfg_active(&item.node) {
                continue;
            }
            if let Item::Struct(sdef) = &item.node {
                self.struct_types
                    .insert(sdef.name.node.clone(), sdef.clone());
            }
        }

        // Pre-scan: collect constant values.
        for item in &file.items {
            if !self.is_item_cfg_active(&item.node) {
                continue;
            }
            if let Item::Const(cdef) = &item.node {
                if let Expr::Literal(Literal::Integer(val)) = &cdef.value.node {
                    self.constants.insert(cdef.name.node.clone(), *val);
                }
            }
        }

        // Pre-scan: assign sequential tags to events.
        let mut event_tag = 0u64;
        for item in &file.items {
            if !self.is_item_cfg_active(&item.node) {
                continue;
            }
            if let Item::Event(edef) = &item.node {
                self.event_tags.insert(edef.name.node.clone(), event_tag);
                let field_names: Vec<String> =
                    edef.fields.iter().map(|f| f.name.node.clone()).collect();
                self.event_defs.insert(edef.name.node.clone(), field_names);
                event_tag += 1;
            }
        }

        // Emit sec ram metadata as comments (prover pre-initializes these RAM slots)
        for decl in &file.declarations {
            if let Declaration::SecRam(entries) = decl {
                self.raw(&format!(
                    "{} sec ram: prover-initialized RAM slots",
                    self.backend.comment_prefix()
                ));
                for (addr, ty) in entries {
                    self.raw(&format!(
                        "{} ram[{}]: {} ({} field element{})",
                        self.backend.comment_prefix(),
                        addr,
                        crate::emit::format_type_name(&ty.node),
                        resolve_type_width(&ty.node, &self.target_config),
                        if resolve_type_width(&ty.node, &self.target_config) == 1 {
                            ""
                        } else {
                            "s"
                        }
                    ));
                }
                self.raw("");
            }
        }

        if file.kind == FileKind::Program {
            let main_label = self.backend.format_label("main");
            let preamble = self.backend.program_preamble(&main_label);
            for line in preamble {
                self.raw(&line);
            }
        }

        for item in &file.items {
            if !self.is_item_cfg_active(&item.node) {
                continue;
            }
            if let Item::Fn(func) = &item.node {
                if func.type_params.is_empty() && !func.is_test {
                    self.emit_fn(func);
                }
            }
        }

        // Emit monomorphized copies of generic functions.
        let instances = self.mono_instances.clone();
        for inst in &instances {
            if let Some(gdef) = self.generic_fn_defs.get(&inst.name).cloned() {
                self.emit_mono_fn(&gdef, inst);
            }
        }

        self.output.join("\n")
    }

    fn emit_fn(&mut self, func: &FnDef) {
        if func.body.is_none() {
            return;
        }

        let label = self.backend.format_label(&func.name.node);
        let prologue = self.backend.function_prologue(&label);
        for line in prologue {
            self.raw(&line);
        }
        self.stack.clear();
        self.deferred.clear();

        // Parameters are already on the real stack. Register them in the model.
        for param in &func.params {
            let width = resolve_type_width(&param.ty.node, &self.target_config);
            self.stack.push_named(&param.name.node, width);
            self.flush_stack_effects();
        }

        let body = func.body.as_ref().unwrap();
        self.emit_block(&body.node);

        // Clean up: pop everything except return value (if any).
        let has_return = func.return_ty.is_some();
        let total_width = self.stack.stack_depth();

        if has_return && total_width > 0 {
            let ret_width = func
                .return_ty
                .as_ref()
                .map(|t| resolve_type_width(&t.node, &self.target_config))
                .unwrap_or(0);
            let to_pop = total_width.saturating_sub(ret_width);
            for _ in 0..to_pop {
                self.b_swap(1);
                self.b_pop(1);
            }
        } else if !has_return {
            self.emit_pop(total_width);
        }

        let epilogue = self.backend.function_epilogue();
        for line in epilogue {
            self.inst(&line);
        }

        // Emit deferred blocks
        self.flush_deferred();
        self.stack.clear();
    }

    /// Emit a monomorphized copy of a generic function with concrete size substitutions.
    fn emit_mono_fn(&mut self, func: &FnDef, inst: &MonoInstance) {
        if func.body.is_none() {
            return;
        }

        // Set up substitution context.
        self.current_subs.clear();
        for (param, val) in func.type_params.iter().zip(inst.size_args.iter()) {
            self.current_subs.insert(param.node.clone(), *val);
        }

        let label = self.backend.format_label(&inst.mangled_name());
        let prologue = self.backend.function_prologue(&label);
        for line in prologue {
            self.raw(&line);
        }
        self.stack.clear();
        self.deferred.clear();

        // Parameters with substituted widths.
        for param in &func.params {
            let width = resolve_type_width_with_subs(
                &param.ty.node,
                &self.current_subs,
                &self.target_config,
            );
            self.stack.push_named(&param.name.node, width);
            self.flush_stack_effects();
        }

        let body = func.body.as_ref().unwrap();
        self.emit_block(&body.node);

        // Clean up: pop everything except return value.
        let has_return = func.return_ty.is_some();
        let total_width = self.stack.stack_depth();

        if has_return && total_width > 0 {
            let ret_width = func
                .return_ty
                .as_ref()
                .map(|t| {
                    resolve_type_width_with_subs(&t.node, &self.current_subs, &self.target_config)
                })
                .unwrap_or(0);
            let to_pop = total_width.saturating_sub(ret_width);
            for _ in 0..to_pop {
                self.b_swap(1);
                self.b_pop(1);
            }
        } else if !has_return {
            self.emit_pop(total_width);
        }

        let epilogue = self.backend.function_epilogue();
        for line in epilogue {
            self.inst(&line);
        }

        self.flush_deferred();
        self.stack.clear();
        self.current_subs.clear();
    }

    fn flush_deferred(&mut self) {
        while !self.deferred.is_empty() {
            let deferred = std::mem::take(&mut self.deferred);
            for block in deferred {
                self.emit_label(&block.label);
                let prologue = self.backend.deferred_block_prologue(block.clears_flag);
                for line in prologue {
                    self.inst(&line);
                }
                self.emit_block(&block.block);
                let epilogue = self.backend.deferred_block_epilogue(block.clears_flag);
                for line in epilogue {
                    self.inst(&line);
                }
                self.raw("");
            }
        }
    }

    fn fresh_label(&mut self, prefix: &str) -> String {
        self.label_counter += 1;
        self.backend
            .format_label(&format!("{}__{}", prefix, self.label_counter))
    }

    // ── Low-level output helpers ──────────────────────────────────

    fn inst(&mut self, instruction: &str) {
        self.output.push(format!(
            "{}{}",
            self.backend.instruction_indent(),
            instruction
        ));
    }

    fn raw(&mut self, line: &str) {
        self.output.push(line.to_string());
    }

    fn emit_label(&mut self, label: &str) {
        self.output.push(self.backend.emit_label_def(label));
    }
}

fn format_type_name(ty: &Type) -> String {
    match ty {
        Type::Field => "Field".to_string(),
        Type::Bool => "Bool".to_string(),
        Type::U32 => "U32".to_string(),
        Type::XField => "XField".to_string(),
        Type::Digest => "Digest".to_string(),
        Type::Array(inner, n) => format!("[{}; {}]", format_type_name(inner), n),
        Type::Tuple(elems) => {
            let parts: Vec<_> = elems.iter().map(format_type_name).collect();
            format!("({})", parts.join(", "))
        }
        Type::Named(path) => path.0.join("."),
    }
}

fn resolve_type_width(ty: &Type, tc: &TargetConfig) -> u32 {
    match ty {
        Type::Field | Type::Bool | Type::U32 => 1,
        Type::XField => tc.xfield_width,
        Type::Digest => tc.digest_width,
        Type::Array(inner, n) => {
            let size = n.as_literal().unwrap_or(0);
            resolve_type_width(inner, tc) * (size as u32)
        }
        Type::Tuple(elems) => elems.iter().map(|t| resolve_type_width(t, tc)).sum(),
        Type::Named(_) => 1,
    }
}

/// Like `resolve_type_width` but substitutes size parameters with concrete values.
fn resolve_type_width_with_subs(ty: &Type, subs: &HashMap<String, u64>, tc: &TargetConfig) -> u32 {
    match ty {
        Type::Field | Type::Bool | Type::U32 => 1,
        Type::XField => tc.xfield_width,
        Type::Digest => tc.digest_width,
        Type::Array(inner, n) => {
            let size = n.eval(subs);
            resolve_type_width_with_subs(inner, subs, tc) * (size as u32)
        }
        Type::Tuple(elems) => elems
            .iter()
            .map(|t| resolve_type_width_with_subs(t, subs, tc))
            .sum(),
        Type::Named(_) => 1,
    }
}

#[cfg(test)]
mod tests;
