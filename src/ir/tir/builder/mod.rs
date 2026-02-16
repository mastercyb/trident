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
mod cleanup;
mod expr;
mod helpers;
mod layout;
mod match_;
mod stmt;
#[cfg(test)]
mod tests {
    mod advanced;
    mod basics;
}

use std::collections::{BTreeMap, BTreeSet};

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
    pub(crate) struct_layouts: BTreeMap<String, BTreeMap<String, (u32, u32)>>,
    /// Return widths of user-defined functions.
    pub(crate) fn_return_widths: BTreeMap<String, u32>,
    /// Event tags: event name -> sequential integer tag.
    pub(crate) event_tags: BTreeMap<String, u64>,
    /// Event field names in declaration order: event name -> [field_name, ...].
    pub(crate) event_defs: BTreeMap<String, Vec<String>>,
    /// Struct type definitions: struct_name -> StructDef.
    pub(crate) struct_types: BTreeMap<String, StructDef>,
    /// Constants: qualified or short name -> integer value.
    pub(crate) constants: BTreeMap<String, u64>,
    /// Next temporary RAM address for runtime array ops.
    pub(crate) temp_ram_addr: u64,
    /// Intrinsic map: function name -> intrinsic TASM name.
    pub(crate) intrinsic_map: BTreeMap<String, String>,
    /// Module alias map: short name -> full module name.
    pub(crate) module_aliases: BTreeMap<String, String>,
    /// Monomorphized generic function instances to emit.
    pub(crate) mono_instances: Vec<MonoInstance>,
    /// Generic function AST definitions (name -> FnDef).
    pub(crate) generic_fn_defs: BTreeMap<String, FnDef>,
    /// Current size parameter substitutions during monomorphized emission.
    pub(crate) current_subs: BTreeMap<String, u64>,
    /// Per-call-site resolutions from the type checker.
    pub(crate) call_resolutions: Vec<MonoInstance>,
    /// Index into call_resolutions for the next generic call.
    pub(crate) call_resolution_idx: usize,
    /// Active cfg flags for conditional compilation.
    pub(crate) cfg_flags: BTreeSet<String>,
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
            struct_layouts: BTreeMap::new(),
            fn_return_widths: BTreeMap::new(),
            event_tags: BTreeMap::new(),
            event_defs: BTreeMap::new(),
            struct_types: BTreeMap::new(),
            constants: BTreeMap::new(),
            temp_ram_addr: target_config.spill_ram_base / 2,
            intrinsic_map: BTreeMap::new(),
            module_aliases: BTreeMap::new(),
            mono_instances: Vec::new(),
            generic_fn_defs: BTreeMap::new(),
            current_subs: BTreeMap::new(),
            call_resolutions: Vec::new(),
            call_resolution_idx: 0,
            cfg_flags: BTreeSet::from(["debug".to_string()]),
            target_config,
        }
    }

    // ── Builder-pattern configuration ─────────────────────────────

    pub fn with_cfg_flags(mut self, flags: BTreeSet<String>) -> Self {
        self.cfg_flags = flags;
        self
    }

    pub fn with_intrinsics(mut self, map: BTreeMap<String, String>) -> Self {
        self.intrinsic_map = map;
        self
    }

    pub fn with_module_aliases(mut self, aliases: BTreeMap<String, String>) -> Self {
        self.module_aliases = aliases;
        self
    }

    pub fn with_constants(mut self, constants: BTreeMap<String, u64>) -> Self {
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
                let mut subs = BTreeMap::new();
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
        let name = func.name.node.clone();
        let param_widths: Vec<u32> = func
            .params
            .iter()
            .map(|p| resolve_type_width(&p.ty.node, &self.target_config))
            .collect();
        let ret_width = func
            .return_ty
            .as_ref()
            .map(|t| resolve_type_width(&t.node, &self.target_config))
            .unwrap_or(0);
        self.build_fn_body(&name, func, &param_widths, ret_width);
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
        let name = inst.mangled_name();
        let param_widths: Vec<u32> = func
            .params
            .iter()
            .map(|p| {
                resolve_type_width_with_subs(&p.ty.node, &self.current_subs, &self.target_config)
            })
            .collect();
        let ret_width = func
            .return_ty
            .as_ref()
            .map(|t| resolve_type_width_with_subs(&t.node, &self.current_subs, &self.target_config))
            .unwrap_or(0);
        self.build_fn_body(&name, func, &param_widths, ret_width);
        self.current_subs.clear();
    }

    /// Detect a pass-through function: body is a single call with all
    /// params forwarded in declaration order. Works for any param width —
    /// the values are already on the stack in the correct position.
    fn detect_pass_through(&self, func: &FnDef, _param_widths: &[u32]) -> bool {
        let body = match &func.body {
            Some(b) => b,
            None => return false,
        };
        if !body.node.stmts.is_empty() {
            return false;
        }
        let tail = match &body.node.tail_expr {
            Some(t) => t,
            None => return false,
        };
        let args = match &tail.node {
            Expr::Call { args, .. } => args,
            _ => return false,
        };
        if args.len() != func.params.len() {
            return false;
        }
        for (arg, param) in args.iter().zip(func.params.iter()) {
            match &arg.node {
                Expr::Var(name) if name == &param.name.node => {}
                _ => return false,
            }
        }
        true
    }

    /// Shared body for `build_fn` and `build_mono_fn`.
    ///
    /// Emits FnStart, registers parameters, compiles the body, cleans up
    /// the stack, and emits Return + FnEnd.
    fn build_fn_body(&mut self, name: &str, func: &FnDef, param_widths: &[u32], ret_width: u32) {
        self.ops.push(TIROp::FnStart(name.to_string()));
        self.stack.clear();

        // Pass-through optimization: if the body is a single call that
        // forwards all width-1 params in order, skip variable registration
        // and emit only the call instruction.
        if self.detect_pass_through(func, param_widths) {
            let body = func.body.as_ref().unwrap();
            let tail = body.node.tail_expr.as_ref().unwrap();
            if let Expr::Call {
                path,
                generic_args,
                args,
            } = &tail.node
            {
                let call_name = path.node.as_dotted();
                self.emit_call_only(&call_name, generic_args, args.len());
            }
            self.ops.push(TIROp::Return);
            self.ops.push(TIROp::FnEnd);
            self.stack.clear();
            return;
        }

        // Parameters are already on the real stack. Register them in the model.
        for (param, &width) in func.params.iter().zip(param_widths) {
            self.stack.push_named(&param.name.node, width);
            self.flush_stack_effects();
        }

        let body = func.body.as_ref().expect("caller checked body.is_some()");
        let has_return = func.return_ty.is_some();

        if has_return && ret_width > 1 {
            // Multi-element return: build statements first, then handle
            // the tail expression specially to avoid unnecessary copies.
            for stmt in &body.node.stmts {
                self.build_stmt(&stmt.node);
            }

            if let Some(tail) = &body.node.tail_expr {
                let depth_before_tail = self.stack.stack_depth();

                // Check if tail is a simple variable reference at the top.
                let var_name = match &tail.node {
                    Expr::Var(v) => Some(v.clone()),
                    _ => None,
                };
                let var_info = var_name.as_ref().and_then(|v| {
                    self.stack.access_var(v);
                    self.flush_stack_effects();
                    self.find_var_depth_and_width(v)
                });

                if let Some((depth, width)) = var_info {
                    if width == ret_width && depth == 0 {
                        // Return variable is already at the top of the stack.
                        // Just pop dead elements below it.
                        let dead = depth_before_tail - ret_width;
                        if dead > 0 {
                            self.emit_multi_ret_cleanup(ret_width, dead);
                        }
                        // Skip building the tail expr (no dup needed).
                    } else {
                        self.build_expr(&tail.node);
                        let to_pop = self.stack.stack_depth().saturating_sub(ret_width);
                        if to_pop > 0 {
                            self.emit_multi_ret_cleanup(ret_width, to_pop);
                        }
                    }
                } else if depth_before_tail == ret_width {
                    // Stack already has exactly ret_width elements —
                    // the tail expression would just reconstruct what's
                    // already in place. Skip it entirely.
                } else {
                    self.build_expr(&tail.node);
                    let to_pop = self.stack.stack_depth().saturating_sub(ret_width);
                    if to_pop > 0 {
                        self.emit_multi_ret_cleanup(ret_width, to_pop);
                    }
                }
            }
        } else {
            // Single-element or void return: use the standard path.
            self.build_block(&body.node);
            let total_width = self.stack.stack_depth();

            if has_return && total_width > 0 {
                let to_pop = total_width.saturating_sub(ret_width);
                if to_pop > 0 && to_pop <= 15 {
                    self.ops.push(TIROp::Swap(to_pop));
                    self.emit_pop(to_pop);
                } else if to_pop > 0 {
                    for _ in 0..to_pop {
                        self.ops.push(TIROp::Swap(1));
                        self.ops.push(TIROp::Pop(1));
                    }
                }
            } else if !has_return {
                self.emit_pop(total_width);
            }
        }

        self.ops.push(TIROp::Return);
        self.ops.push(TIROp::FnEnd);
        self.stack.clear();
    }
}
