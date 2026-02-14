//! TIRBuilder: lowers a type-checked AST into `Vec<TIROp>`.
//!
//! This is the core of Phase 2 — it replicates the Emitter's AST-walking
//! logic but produces `Vec<TIROp>` instead of `Vec<String>`. The output is
//! target-independent; a `StackLowering` implementation converts it to assembly.
//!
//! Key differences from the Emitter:
//! - No `StackBackend`: instructions are TIROp variants pushed directly.
//! - No `DeferredBlock`: if/else and loops use nested `Vec<TIROp>` bodies
//!   inside structural `TIROp::IfElse`, `TIROp::IfOnly`, and `TIROp::Loop`.
//! - `StackManager` spill/reload effects are parsed from their string form
//!   back into TIROps via `parse_spill_effect`.

mod call;
mod expr;
mod helpers;
mod layout;
mod stmt;
#[cfg(test)]
mod tests;

#[allow(dead_code)]
use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::target::TargetConfig;
use crate::tir::stack::SpillFormatter;
use crate::tir::stack::StackManager;
use crate::tir::TIROp;
use crate::typecheck::MonoInstance;

use self::layout::{format_type_name, resolve_type_width, resolve_type_width_with_subs};

// ─── TIRBuilder ────────────────────────────────────────────────────

/// Builds IR from a type-checked AST.
pub struct TIRBuilder {
    /// Accumulated IR operations.
    pub(crate) ops: Vec<TIROp>,
    /// Monotonic label counter.
    pub(crate) label_counter: u32,
    /// Stack model: LRU-based manager with automatic RAM spill/reload.
    pub(crate) stack: StackManager,
    /// Struct field layouts: var_name -> { field_name -> (offset_from_top, field_width) }.
    pub(crate) struct_layouts: HashMap<String, HashMap<String, (u32, u32)>>,
    /// Return widths of user-defined functions.
    pub(crate) fn_return_widths: HashMap<String, u32>,
    /// Event tags: event name -> sequential integer tag.
    pub(crate) event_tags: HashMap<String, u64>,
    /// Event field names in declaration order: event name -> [field_name, ...].
    pub(crate) event_defs: HashMap<String, Vec<String>>,
    /// Struct type definitions: struct_name -> StructDef.
    pub(crate) struct_types: HashMap<String, StructDef>,
    /// Constants: qualified or short name -> integer value.
    pub(crate) constants: HashMap<String, u64>,
    /// Next temporary RAM address for runtime array ops.
    pub(crate) temp_ram_addr: u64,
    /// Intrinsic map: function name -> intrinsic TASM name.
    pub(crate) intrinsic_map: HashMap<String, String>,
    /// Module alias map: short name -> full module name.
    pub(crate) module_aliases: HashMap<String, String>,
    /// Monomorphized generic function instances to emit.
    pub(crate) mono_instances: Vec<MonoInstance>,
    /// Generic function AST definitions (name -> FnDef).
    pub(crate) generic_fn_defs: HashMap<String, FnDef>,
    /// Current size parameter substitutions during monomorphized emission.
    pub(crate) current_subs: HashMap<String, u64>,
    /// Per-call-site resolutions from the type checker.
    pub(crate) call_resolutions: Vec<MonoInstance>,
    /// Index into call_resolutions for the next generic call.
    pub(crate) call_resolution_idx: usize,
    /// Active cfg flags for conditional compilation.
    pub(crate) cfg_flags: HashSet<String>,
    /// Target VM configuration.
    pub(crate) target_config: TargetConfig,
}

impl TIRBuilder {
    pub fn new(target_config: TargetConfig) -> Self {
        let stack = StackManager::with_formatter(
            target_config.stack_depth,
            target_config.spill_ram_base,
            SpillFormatter::default(),
        );
        Self {
            ops: Vec::new(),
            label_counter: 0,
            stack,
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
        }
    }

    // ── Builder-pattern configuration ─────────────────────────────

    pub fn with_cfg_flags(mut self, flags: HashSet<String>) -> Self {
        self.cfg_flags = flags;
        self
    }

    pub fn with_intrinsics(mut self, map: HashMap<String, String>) -> Self {
        self.intrinsic_map = map;
        self
    }

    pub fn with_module_aliases(mut self, aliases: HashMap<String, String>) -> Self {
        self.module_aliases = aliases;
        self
    }

    pub fn with_constants(mut self, constants: HashMap<String, u64>) -> Self {
        self.constants.extend(constants);
        self
    }

    pub fn with_mono_instances(mut self, instances: Vec<MonoInstance>) -> Self {
        self.mono_instances = instances;
        self
    }

    pub fn with_call_resolutions(mut self, resolutions: Vec<MonoInstance>) -> Self {
        self.call_resolutions = resolutions;
        self
    }

    // ═══════════════════════════════════════════════════════════════
    // ── Top-level entry: build_file ───────────────────────────────
    // ═══════════════════════════════════════════════════════════════

    pub fn build_file(mut self, file: &File) -> Vec<TIROp> {
        // ── Pre-scan: collect return widths and detect generic functions ──
        for item in &file.items {
            if !self.is_item_cfg_active(&item.node) {
                continue;
            }
            if let Item::Fn(func) = &item.node {
                if !func.type_params.is_empty() {
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

        // ── Pre-scan: register return widths for monomorphized instances ──
        for inst in &self.mono_instances.clone() {
            if let Some(gdef) = self.generic_fn_defs.get(&inst.name).cloned() {
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
                self.fn_return_widths.insert(mangled, width);
            }
        }

        // ── Pre-scan: collect intrinsic mappings ──
        for item in &file.items {
            if !self.is_item_cfg_active(&item.node) {
                continue;
            }
            if let Item::Fn(func) = &item.node {
                if let Some(ref intrinsic) = func.intrinsic {
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

        // ── Pre-scan: collect struct type definitions ──
        for item in &file.items {
            if !self.is_item_cfg_active(&item.node) {
                continue;
            }
            if let Item::Struct(sdef) = &item.node {
                self.struct_types
                    .insert(sdef.name.node.clone(), sdef.clone());
            }
        }

        // ── Pre-scan: collect constant values ──
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

        // ── Pre-scan: assign sequential tags to events ──
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

        // ── Emit sec ram metadata as comments ──
        for decl in &file.declarations {
            if let Declaration::SecRam(entries) = decl {
                self.ops.push(TIROp::Comment(
                    "sec ram: prover-initialized RAM slots".to_string(),
                ));
                for (addr, ty) in entries {
                    let width = resolve_type_width(&ty.node, &self.target_config);
                    self.ops.push(TIROp::Comment(format!(
                        "ram[{}]: {} ({} field element{})",
                        addr,
                        format_type_name(&ty.node),
                        width,
                        if width == 1 { "" } else { "s" }
                    )));
                }
                // (blank line between sec_ram and functions handled by lowering)
            }
        }

        // ── Program entry point ──
        if file.kind == FileKind::Program {
            self.ops.push(TIROp::Entry("main".to_string()));
        }

        // ── Emit non-generic, non-test functions ──
        for item in &file.items {
            if !self.is_item_cfg_active(&item.node) {
                continue;
            }
            if let Item::Fn(func) = &item.node {
                if func.type_params.is_empty() && !func.is_test {
                    self.build_fn(func);
                }
            }
        }

        // ── Emit monomorphized copies of generic functions ──
        let instances = self.mono_instances.clone();
        for inst in &instances {
            if let Some(gdef) = self.generic_fn_defs.get(&inst.name).cloned() {
                self.build_mono_fn(&gdef, inst);
            }
        }

        self.ops
    }

    // ═══════════════════════════════════════════════════════════════
    // ── Function emission ─────────────────────────────────────────
    // ═══════════════════════════════════════════════════════════════

    pub(crate) fn build_fn(&mut self, func: &FnDef) {
        if func.body.is_none() {
            return;
        }

        self.ops.push(TIROp::FnStart(func.name.node.clone()));
        self.stack.clear();

        // Parameters are already on the real stack. Register them in the model.
        for param in &func.params {
            let width = resolve_type_width(&param.ty.node, &self.target_config);
            self.stack.push_named(&param.name.node, width);
            self.flush_stack_effects();
        }

        let body = func.body.as_ref().unwrap();
        self.build_block(&body.node);

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
                self.ops.push(TIROp::Swap(1));
                self.ops.push(TIROp::Pop(1));
            }
        } else if !has_return {
            self.emit_pop(total_width);
        }

        self.ops.push(TIROp::Return);
        self.ops.push(TIROp::FnEnd);
        self.stack.clear();
    }

    fn build_mono_fn(&mut self, func: &FnDef, inst: &MonoInstance) {
        if func.body.is_none() {
            return;
        }

        // Set up substitution context.
        self.current_subs.clear();
        for (param, val) in func.type_params.iter().zip(inst.size_args.iter()) {
            self.current_subs.insert(param.node.clone(), *val);
        }

        let mangled = inst.mangled_name();
        self.ops.push(TIROp::FnStart(mangled));
        self.stack.clear();

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
        self.build_block(&body.node);

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
                self.ops.push(TIROp::Swap(1));
                self.ops.push(TIROp::Pop(1));
            }
        } else if !has_return {
            self.emit_pop(total_width);
        }

        self.ops.push(TIROp::Return);
        self.ops.push(TIROp::FnEnd);
        self.stack.clear();
        self.current_subs.clear();
    }
}
