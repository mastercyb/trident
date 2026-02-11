//! IRBuilder: lowers a type-checked AST into `Vec<IROp>`.
//!
//! This is the core of Phase 2 — it replicates the Emitter's AST-walking
//! logic but produces `Vec<IROp>` instead of `Vec<String>`. The output is
//! target-independent; a `Lowering` implementation converts it to assembly.
//!
//! Key differences from the Emitter:
//! - No `StackBackend`: instructions are IROp variants pushed directly.
//! - No `DeferredBlock`: if/else and loops use nested `Vec<IROp>` bodies
//!   inside structural `IROp::IfElse`, `IROp::IfOnly`, and `IROp::Loop`.
//! - `StackManager` spill/reload effects are parsed from their string form
//!   back into IROps via `parse_spill_effect`.

#![allow(dead_code)]

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::codegen::stack::StackManager;
use crate::span::Spanned;
use crate::stack::SpillFormatter;
use crate::target::TargetConfig;
use crate::typecheck::MonoInstance;

use super::IROp;

// ─── Type helpers (copied from emitter — private fns, not importable) ─────

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

// ─── Spill effect parser ──────────────────────────────────────────

/// Convert a SpillFormatter-produced instruction string into an IROp.
///
/// The default SpillFormatter (Triton-style) emits lines like:
///   `"    push 42"`, `"    swap 5"`, `"    pop 1"`,
///   `"    write_mem 1"`, `"    read_mem 1"`.
fn parse_spill_effect(line: &str) -> IROp {
    let trimmed = line.trim();

    if let Some(rest) = trimmed.strip_prefix("push ") {
        if let Ok(val) = rest.trim().parse::<u64>() {
            return IROp::Push(val);
        }
    }
    if let Some(rest) = trimmed.strip_prefix("swap ") {
        if let Ok(val) = rest.trim().parse::<u32>() {
            return IROp::Swap(val);
        }
    }
    if let Some(rest) = trimmed.strip_prefix("pop ") {
        if let Ok(val) = rest.trim().parse::<u32>() {
            return IROp::Pop(val);
        }
    }
    if let Some(rest) = trimmed.strip_prefix("write_mem ") {
        if let Ok(val) = rest.trim().parse::<u32>() {
            return IROp::WriteMem(val);
        }
    }
    if let Some(rest) = trimmed.strip_prefix("read_mem ") {
        if let Ok(val) = rest.trim().parse::<u32>() {
            return IROp::ReadMem(val);
        }
    }
    if trimmed.strip_prefix("dup ").is_some() {
        if let Some(rest) = trimmed.strip_prefix("dup ") {
            if let Ok(val) = rest.trim().parse::<u32>() {
                return IROp::Dup(val);
            }
        }
    }

    // Fallback: emit as raw ASM so nothing is silently lost.
    IROp::RawAsm {
        lines: vec![trimmed.to_string()],
        effect: 0,
    }
}

// ─── IRBuilder ────────────────────────────────────────────────────

/// Builds IR from a type-checked AST.
pub struct IRBuilder {
    /// Accumulated IR operations.
    ops: Vec<IROp>,
    /// Monotonic label counter.
    label_counter: u32,
    /// Stack model: LRU-based manager with automatic RAM spill/reload.
    stack: StackManager,
    /// Struct field layouts: var_name -> { field_name -> (offset_from_top, field_width) }.
    struct_layouts: HashMap<String, HashMap<String, (u32, u32)>>,
    /// Return widths of user-defined functions.
    fn_return_widths: HashMap<String, u32>,
    /// Event tags: event name -> sequential integer tag.
    event_tags: HashMap<String, u64>,
    /// Event field names in declaration order: event name -> [field_name, ...].
    event_defs: HashMap<String, Vec<String>>,
    /// Struct type definitions: struct_name -> StructDef.
    struct_types: HashMap<String, StructDef>,
    /// Constants: qualified or short name -> integer value.
    constants: HashMap<String, u64>,
    /// Next temporary RAM address for runtime array ops.
    temp_ram_addr: u64,
    /// Intrinsic map: function name -> intrinsic TASM name.
    intrinsic_map: HashMap<String, String>,
    /// Module alias map: short name -> full module name.
    module_aliases: HashMap<String, String>,
    /// Monomorphized generic function instances to emit.
    mono_instances: Vec<MonoInstance>,
    /// Generic function AST definitions (name -> FnDef).
    generic_fn_defs: HashMap<String, FnDef>,
    /// Current size parameter substitutions during monomorphized emission.
    current_subs: HashMap<String, u64>,
    /// Per-call-site resolutions from the type checker.
    call_resolutions: Vec<MonoInstance>,
    /// Index into call_resolutions for the next generic call.
    call_resolution_idx: usize,
    /// Active cfg flags for conditional compilation.
    cfg_flags: HashSet<String>,
    /// Target VM configuration.
    target_config: TargetConfig,
}

impl IRBuilder {
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

    // ── Cfg helpers ───────────────────────────────────────────────

    fn is_cfg_active(&self, cfg: &Option<Spanned<String>>) -> bool {
        match cfg {
            None => true,
            Some(flag) => self.cfg_flags.contains(&flag.node),
        }
    }

    fn is_item_cfg_active(&self, item: &Item) -> bool {
        match item {
            Item::Fn(f) => self.is_cfg_active(&f.cfg),
            Item::Const(c) => self.is_cfg_active(&c.cfg),
            Item::Struct(s) => self.is_cfg_active(&s.cfg),
            Item::Event(e) => self.is_cfg_active(&e.cfg),
        }
    }

    // ── Label generation ──────────────────────────────────────────

    fn fresh_label(&mut self, prefix: &str) -> String {
        self.label_counter += 1;
        format!("{}__{}", prefix, self.label_counter)
    }

    // ── Stack effect flushing ─────────────────────────────────────

    fn flush_stack_effects(&mut self) {
        for inst in self.stack.drain_side_effects() {
            self.ops.push(parse_spill_effect(&inst));
        }
    }

    // ── Emit helpers ──────────────────────────────────────────────

    /// Ensure stack space, flush spill effects, push the IROp, push temp to model.
    fn emit_and_push(&mut self, op: IROp, result_width: u32) {
        if result_width > 0 {
            self.stack.ensure_space(result_width);
            self.flush_stack_effects();
        }
        self.ops.push(op);
        self.stack.push_temp(result_width);
    }

    /// Push an anonymous temporary onto the stack model (no IROp emitted).
    fn push_temp(&mut self, width: u32) {
        self.stack.push_temp(width);
        self.flush_stack_effects();
    }

    /// Find depth of a named variable (may trigger reload if spilled).
    fn find_var_depth(&mut self, name: &str) -> u32 {
        let d = self.stack.find_var_depth(name);
        self.flush_stack_effects();
        d
    }

    /// Find depth and width of a named variable (may trigger reload if spilled).
    fn find_var_depth_and_width(&mut self, name: &str) -> Option<(u32, u32)> {
        let r = self.stack.find_var_depth_and_width(name);
        self.flush_stack_effects();
        r
    }

    /// Emit pop instructions in batches of up to 5.
    fn emit_pop(&mut self, n: u32) {
        let mut remaining = n;
        while remaining > 0 {
            let batch = remaining.min(5);
            self.ops.push(IROp::Pop(batch));
            remaining -= batch;
        }
    }

    /// Build a block into a separate Vec<IROp> by temporarily swapping out self.ops.
    fn build_block_as_ir(&mut self, block: &Block) -> Vec<IROp> {
        let saved_ops = std::mem::take(&mut self.ops);
        self.build_block(block);
        let nested = std::mem::take(&mut self.ops);
        self.ops = saved_ops;
        nested
    }

    // ── Struct layout helpers ─────────────────────────────────────

    /// Register struct field layout from a type annotation.
    fn register_struct_layout_from_type(&mut self, var_name: &str, ty: &Type) {
        if let Type::Named(path) = ty {
            let struct_name = path.0.last().map(|s| s.as_str()).unwrap_or("");
            if let Some(sdef) = self.struct_types.get(struct_name).cloned() {
                let mut field_map = HashMap::new();
                let total: u32 = sdef
                    .fields
                    .iter()
                    .map(|f| resolve_type_width(&f.ty.node, &self.target_config))
                    .sum();
                let mut offset = 0u32;
                for sf in &sdef.fields {
                    let fw = resolve_type_width(&sf.ty.node, &self.target_config);
                    let from_top = total - offset - fw;
                    field_map.insert(sf.name.node.clone(), (from_top, fw));
                    offset += fw;
                }
                self.struct_layouts.insert(var_name.to_string(), field_map);
            }
        }
    }

    /// Look up field offset within a struct variable.
    fn find_field_offset_in_var(&self, var_name: &str, field_name: &str) -> Option<(u32, u32)> {
        if let Some(offsets) = self.struct_layouts.get(var_name) {
            return offsets.get(field_name).copied();
        }
        None
    }

    /// Resolve field offset for Expr::FieldAccess.
    fn resolve_field_offset(&self, inner: &Expr, field: &str) -> Option<(u32, u32)> {
        if let Expr::Var(name) = inner {
            return self.find_field_offset_in_var(name, field);
        }
        None
    }

    /// Compute field widths for a struct init.
    fn compute_struct_field_widths(
        &self,
        ty: &Option<Spanned<Type>>,
        fields: &[(Spanned<String>, Spanned<Expr>)],
    ) -> Vec<u32> {
        if let Some(sp_ty) = ty {
            if let Type::Named(path) = &sp_ty.node {
                if let Some(name) = path.0.last() {
                    if let Some(sdef) = self.struct_types.get(name) {
                        return sdef
                            .fields
                            .iter()
                            .map(|f| resolve_type_width(&f.ty.node, &self.target_config))
                            .collect();
                    }
                }
            }
        }
        vec![1u32; fields.len()]
    }

    // ═══════════════════════════════════════════════════════════════
    // ── Top-level entry: build_file ───────────────────────────────
    // ═══════════════════════════════════════════════════════════════

    pub fn build_file(mut self, file: &File) -> Vec<IROp> {
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
                self.ops.push(IROp::Comment(
                    "sec ram: prover-initialized RAM slots".to_string(),
                ));
                for (addr, ty) in entries {
                    let width = resolve_type_width(&ty.node, &self.target_config);
                    self.ops.push(IROp::Comment(format!(
                        "ram[{}]: {} ({} field element{})",
                        addr,
                        format_type_name(&ty.node),
                        width,
                        if width == 1 { "" } else { "s" }
                    )));
                }
                self.ops.push(IROp::BlankLine);
            }
        }

        // ── Program preamble ──
        if file.kind == FileKind::Program {
            self.ops.push(IROp::Preamble("main".to_string()));
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

    fn build_fn(&mut self, func: &FnDef) {
        if func.body.is_none() {
            return;
        }

        self.ops.push(IROp::FnStart(func.name.node.clone()));
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
                self.ops.push(IROp::Swap(1));
                self.ops.push(IROp::Pop(1));
            }
        } else if !has_return {
            self.emit_pop(total_width);
        }

        self.ops.push(IROp::Return);
        self.ops.push(IROp::FnEnd);
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
        self.ops.push(IROp::FnStart(mangled));
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
                self.ops.push(IROp::Swap(1));
                self.ops.push(IROp::Pop(1));
            }
        } else if !has_return {
            self.emit_pop(total_width);
        }

        self.ops.push(IROp::Return);
        self.ops.push(IROp::FnEnd);
        self.stack.clear();
        self.current_subs.clear();
    }

    // ═══════════════════════════════════════════════════════════════
    // ── Block and statement emission ──────────────────────────────
    // ═══════════════════════════════════════════════════════════════

    fn build_block(&mut self, block: &Block) {
        for stmt in &block.stmts {
            self.build_stmt(&stmt.node);
        }
        if let Some(tail) = &block.tail_expr {
            self.build_expr(&tail.node);
        }
    }

    fn build_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let {
                pattern, init, ty, ..
            } => {
                self.build_expr(&init.node);

                match pattern {
                    Pattern::Name(name) => {
                        if name.node != "_" {
                            if let Some(top) = self.stack.last_mut() {
                                top.name = Some(name.node.clone());
                            }
                            // If type is an array, record elem_width.
                            if let Some(sp_ty) = ty {
                                if let Type::Array(inner_ty, _) = &sp_ty.node {
                                    let ew = resolve_type_width(inner_ty, &self.target_config);
                                    if let Some(top) = self.stack.last_mut() {
                                        top.elem_width = Some(ew);
                                    }
                                }
                            }
                            // Record struct field layout from struct init.
                            if let Expr::StructInit { fields, .. } = &init.node {
                                let mut field_map = HashMap::new();
                                let widths = self.compute_struct_field_widths(ty, fields);
                                let total: u32 = widths.iter().sum();
                                let mut offset = 0u32;
                                for (i, (fname, _)) in fields.iter().enumerate() {
                                    let fw = widths.get(i).copied().unwrap_or(1);
                                    let from_top = total - offset - fw;
                                    field_map.insert(fname.node.clone(), (from_top, fw));
                                    offset += fw;
                                }
                                self.struct_layouts.insert(name.node.clone(), field_map);
                            } else if let Some(sp_ty) = ty {
                                self.register_struct_layout_from_type(&name.node, &sp_ty.node);
                            }
                        }
                    }
                    Pattern::Tuple(names) => {
                        let top = self.stack.pop();
                        if let Some(entry) = top {
                            let total_width = entry.width;
                            let n = names.len() as u32;
                            let elem_width = if n > 0 { total_width / n } else { 1 };

                            for name in names.iter() {
                                let var_name = if name.node == "_" {
                                    "__anon"
                                } else {
                                    &name.node
                                };
                                self.stack.push_named(var_name, elem_width);
                                self.flush_stack_effects();
                            }
                        }
                    }
                }
            }

            Stmt::Assign { place, value } => {
                if let Place::Var(name) = &place.node {
                    self.build_expr(&value.node);
                    let depth = self.find_var_depth(name);
                    if depth <= 15 {
                        self.ops.push(IROp::Swap(depth));
                        self.ops.push(IROp::Pop(1));
                    }
                    self.stack.pop();
                }
            }

            Stmt::If {
                cond,
                then_block,
                else_block,
            } => {
                self.build_expr(&cond.node);
                self.stack.pop(); // cond consumed

                if let Some(else_blk) = else_block {
                    // Save and restore stack around each branch.
                    let saved = self.stack.save_state();
                    let then_body = self.build_block_as_ir(&then_block.node);
                    self.stack.restore_state(saved.clone());
                    let else_body = self.build_block_as_ir(&else_blk.node);
                    self.stack.restore_state(saved);

                    self.ops.push(IROp::IfElse {
                        then_body,
                        else_body,
                    });
                } else {
                    let saved = self.stack.save_state();
                    let then_body = self.build_block_as_ir(&then_block.node);
                    self.stack.restore_state(saved);

                    self.ops.push(IROp::IfOnly { then_body });
                }
            }

            Stmt::For {
                var: _,
                start: _,
                end,
                body,
                ..
            } => {
                let loop_label = self.fresh_label("loop");

                // Emit the counter expression.
                self.build_expr(&end.node);
                // counter is on top as a temp

                // Call the loop subroutine and pop the counter afterwards.
                self.ops.push(IROp::Call(loop_label.clone()));
                self.ops.push(IROp::Pop(1));
                self.stack.pop(); // counter consumed

                // Build the loop body as a nested IR block.
                let saved = self.stack.save_state();
                self.stack.clear();
                let body_ir = self.build_block_as_ir(&body.node);
                self.stack.restore_state(saved);

                // Emit the loop subroutine inline (matches Emitter's deferred flush).
                self.ops.push(IROp::Loop {
                    label: loop_label,
                    body: body_ir,
                });
            }

            Stmt::TupleAssign { names, value } => {
                self.build_expr(&value.node);
                let top = self.stack.pop();
                if let Some(entry) = top {
                    let total_width = entry.width;
                    let n = names.len() as u32;
                    let elem_width = if n > 0 { total_width / n } else { 1 };

                    for name in names.iter().rev() {
                        let depth = self.find_var_depth(&name.node);
                        if elem_width == 1 {
                            self.ops.push(IROp::Swap(depth));
                            self.ops.push(IROp::Pop(1));
                        }
                    }
                    let _ = total_width; // suppress unused warning
                }
            }

            Stmt::Expr(expr) => {
                let before = self.stack.stack_len();
                self.build_expr(&expr.node);
                // Pop any new entries produced by this expression.
                while self.stack.stack_len() > before {
                    if let Some(top) = self.stack.last() {
                        let w = top.width;
                        if w > 0 {
                            self.emit_pop(w);
                        }
                    }
                    self.stack.pop();
                }
            }

            Stmt::Return(value) => {
                if let Some(val) = value {
                    self.build_expr(&val.node);
                }
            }

            Stmt::Emit { event_name, fields } => {
                let tag = self.event_tags.get(&event_name.node).copied().unwrap_or(0);
                let decl_order = self
                    .event_defs
                    .get(&event_name.node)
                    .cloned()
                    .unwrap_or_default();

                // Push tag and write it.
                self.ops.push(IROp::Push(tag));
                self.ops.push(IROp::WriteIo(1));

                // Emit each field in declaration order, write one at a time.
                for def_name in &decl_order {
                    if let Some((_name, val)) = fields.iter().find(|(n, _)| n.node == *def_name) {
                        self.build_expr(&val.node);
                        self.stack.pop(); // consumed by write_io
                        self.ops.push(IROp::WriteIo(1));
                    }
                }
            }

            Stmt::Asm {
                body,
                effect,
                target,
            } => {
                // Skip asm blocks tagged for a different target.
                if let Some(tag) = target {
                    if tag != &self.target_config.name {
                        return;
                    }
                }

                // Spill all named variables to RAM to isolate asm from managed stack.
                self.stack.spill_all_named();
                self.flush_stack_effects();

                // Collect non-empty lines.
                let lines: Vec<String> = body
                    .lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect();

                if !lines.is_empty() {
                    self.ops.push(IROp::RawAsm {
                        lines,
                        effect: *effect,
                    });
                }

                // Adjust stack model by declared net effect.
                if *effect > 0 {
                    for _ in 0..*effect {
                        self.stack.push_temp(1);
                    }
                } else if *effect < 0 {
                    for _ in 0..effect.unsigned_abs() {
                        self.stack.pop();
                    }
                }
            }

            Stmt::Match { expr, arms } => {
                self.build_match(expr, arms);
            }

            Stmt::Seal { event_name, fields } => {
                let tag = self.event_tags.get(&event_name.node).copied().unwrap_or(0);
                let decl_order = self
                    .event_defs
                    .get(&event_name.node)
                    .cloned()
                    .unwrap_or_default();
                let num_fields = decl_order.len();

                // Build 10-element hash input: tag, field0, field1, ..., 0-padding.
                let padding = 9usize.saturating_sub(num_fields);
                for _ in 0..padding {
                    self.ops.push(IROp::Push(0));
                }

                // Push fields in reverse declaration order.
                for def_name in decl_order.iter().rev() {
                    if let Some((_name, val)) = fields.iter().find(|(n, _)| n.node == *def_name) {
                        self.build_expr(&val.node);
                        self.stack.pop(); // consumed by hash
                    }
                }

                // Push tag.
                self.ops.push(IROp::Push(tag));

                // Hash: consumes 10, produces 5 (Digest).
                self.ops.push(IROp::Hash);

                // Write the 5-element digest commitment.
                self.ops.push(IROp::WriteIo(5));
            }
        }
    }

    // ── Match statement ───────────────────────────────────────────

    fn build_match(&mut self, expr: &Spanned<Expr>, arms: &[MatchArm]) {
        // Emit scrutinee value onto the stack.
        self.build_expr(&expr.node);
        if let Some(top) = self.stack.last_mut() {
            top.name = Some("__match_scrutinee".to_string());
        }

        // We'll collect deferred arm subroutines to emit after the match logic.
        let mut deferred_subs: Vec<(String, Block, bool)> = Vec::new();

        for arm in arms {
            match &arm.pattern.node {
                MatchPattern::Literal(lit) => {
                    let _arm_label = self.fresh_label("match_arm");
                    let _rest_label = self.fresh_label("match_rest");

                    // Dup the scrutinee for comparison.
                    let depth = self.find_var_depth("__match_scrutinee");
                    self.ops.push(IROp::Dup(depth));

                    // Push the pattern value.
                    match lit {
                        Literal::Integer(n) => self.ops.push(IROp::Push(*n)),
                        Literal::Bool(b) => self.ops.push(IROp::Push(if *b { 1 } else { 0 })),
                    }

                    // eq produces bool on stack.
                    self.ops.push(IROp::Eq);

                    // Build arm body: pop scrutinee then run original body.
                    let mut arm_stmts = vec![Spanned::new(
                        Stmt::Asm {
                            body: "pop 1".to_string(),
                            effect: -1,
                            target: None,
                        },
                        arm.body.span,
                    )];
                    arm_stmts.extend(arm.body.node.stmts.clone());

                    let arm_block = Block {
                        stmts: arm_stmts,
                        tail_expr: arm.body.node.tail_expr.clone(),
                    };

                    // Rest block is empty (continues to next arm).
                    let rest_block = Block {
                        stmts: Vec::new(),
                        tail_expr: None,
                    };

                    // Build both branches as nested IR.
                    let saved = self.stack.save_state();

                    // Arm body needs to see the scrutinee for the pop.
                    let then_body = self.build_deferred_arm_ir(&arm_block, true);
                    self.stack.restore_state(saved.clone());

                    let else_body = self.build_deferred_arm_ir(&rest_block, false);
                    self.stack.restore_state(saved);

                    self.ops.push(IROp::IfElse {
                        then_body,
                        else_body,
                    });
                }

                MatchPattern::Wildcard => {
                    // Wildcard: call a subroutine unconditionally.
                    let w_label = self.fresh_label("match_wild");
                    self.ops.push(IROp::Call(w_label.clone()));

                    let mut arm_stmts = vec![Spanned::new(
                        Stmt::Asm {
                            body: "pop 1".to_string(),
                            effect: -1,
                            target: None,
                        },
                        arm.body.span,
                    )];
                    arm_stmts.extend(arm.body.node.stmts.clone());
                    deferred_subs.push((
                        w_label,
                        Block {
                            stmts: arm_stmts,
                            tail_expr: arm.body.node.tail_expr.clone(),
                        },
                        false,
                    ));
                }

                MatchPattern::Struct { name, fields } => {
                    let s_label = self.fresh_label("match_struct");
                    self.ops.push(IROp::Call(s_label.clone()));

                    let mut arm_stmts: Vec<Spanned<Stmt>> = Vec::new();

                    // Pop the 1-wide scrutinee placeholder.
                    arm_stmts.push(Spanned::new(
                        Stmt::Asm {
                            body: "pop 1".to_string(),
                            effect: -1,
                            target: None,
                        },
                        arm.body.span,
                    ));

                    // Emit field assertions and let-bindings.
                    if let Some(sdef) = self.struct_types.get(&name.node).cloned() {
                        for spf in fields {
                            let field_name = &spf.field_name.node;
                            let access_expr = Expr::FieldAccess {
                                expr: Box::new(expr.clone()),
                                field: spf.field_name.clone(),
                            };
                            let access_spanned = Spanned::new(access_expr, spf.field_name.span);

                            match &spf.pattern.node {
                                FieldPattern::Binding(var_name) => {
                                    let field_ty = sdef
                                        .fields
                                        .iter()
                                        .find(|f| f.name.node == *field_name)
                                        .map(|f| f.ty.clone());
                                    arm_stmts.push(Spanned::new(
                                        Stmt::Let {
                                            mutable: false,
                                            pattern: Pattern::Name(Spanned::new(
                                                var_name.clone(),
                                                spf.pattern.span,
                                            )),
                                            ty: field_ty,
                                            init: access_spanned,
                                        },
                                        spf.field_name.span,
                                    ));
                                }
                                FieldPattern::Literal(lit) => {
                                    let lit_expr =
                                        Spanned::new(Expr::Literal(lit.clone()), spf.pattern.span);
                                    let eq_expr = Spanned::new(
                                        Expr::BinOp {
                                            op: BinOp::Eq,
                                            lhs: Box::new(access_spanned),
                                            rhs: Box::new(lit_expr),
                                        },
                                        spf.pattern.span,
                                    );
                                    arm_stmts.push(Spanned::new(
                                        Stmt::Expr(Spanned::new(
                                            Expr::Call {
                                                path: Spanned::new(
                                                    ModulePath::single("assert".to_string()),
                                                    spf.pattern.span,
                                                ),
                                                generic_args: vec![],
                                                args: vec![eq_expr],
                                            },
                                            spf.pattern.span,
                                        )),
                                        spf.pattern.span,
                                    ));
                                }
                                FieldPattern::Wildcard => {
                                    // No action needed.
                                }
                            }
                        }
                    }

                    arm_stmts.extend(arm.body.node.stmts.clone());
                    deferred_subs.push((
                        s_label,
                        Block {
                            stmts: arm_stmts,
                            tail_expr: arm.body.node.tail_expr.clone(),
                        },
                        false,
                    ));
                }
            }
        }

        // Pop the scrutinee after match completes.
        self.stack.pop();
        self.ops.push(IROp::Pop(1));

        // Emit deferred subroutines (wildcard / struct arms) inline.
        for (label, block, _is_literal) in deferred_subs {
            self.ops.push(IROp::Label(label));
            let saved = self.stack.save_state();
            self.stack.clear();
            self.build_block(&block);
            self.stack.restore_state(saved);
            self.ops.push(IROp::Return);
            self.ops.push(IROp::BlankLine);
        }
    }

    /// Build a deferred match arm body into IR. If `clears_flag` is true,
    /// emit Push(0) at the start (Triton if/else flag clearing).
    fn build_deferred_arm_ir(&mut self, block: &Block, clears_flag: bool) -> Vec<IROp> {
        let saved_ops = std::mem::take(&mut self.ops);
        if clears_flag {
            self.ops.push(IROp::Push(0));
        }
        self.build_block(block);
        if clears_flag {
            // Deferred block epilogue when clearing flag: recurse.
            self.ops.push(IROp::Return);
        } else {
            self.ops.push(IROp::Return);
        }
        let nested = std::mem::take(&mut self.ops);
        self.ops = saved_ops;
        nested
    }

    // ═══════════════════════════════════════════════════════════════
    // ── Expression emission ───────────────────────────────────────
    // ═══════════════════════════════════════════════════════════════

    fn build_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Literal(Literal::Integer(n)) => {
                self.emit_and_push(IROp::Push(*n), 1);
            }
            Expr::Literal(Literal::Bool(b)) => {
                self.emit_and_push(IROp::Push(if *b { 1 } else { 0 }), 1);
            }

            Expr::Var(name) => {
                self.build_var_expr(name);
            }

            Expr::BinOp { op, lhs, rhs } => {
                self.build_expr(&lhs.node);
                self.build_expr(&rhs.node);
                match op {
                    BinOp::Add => self.ops.push(IROp::Add),
                    BinOp::Mul => self.ops.push(IROp::Mul),
                    BinOp::Eq => self.ops.push(IROp::Eq),
                    BinOp::Lt => self.ops.push(IROp::Lt),
                    BinOp::BitAnd => self.ops.push(IROp::And),
                    BinOp::BitXor => self.ops.push(IROp::Xor),
                    BinOp::DivMod => self.ops.push(IROp::DivMod),
                    BinOp::XFieldMul => self.ops.push(IROp::XbMul),
                }
                self.stack.pop(); // rhs temp
                self.stack.pop(); // lhs temp
                let result_width = match op {
                    BinOp::DivMod => 2,
                    BinOp::XFieldMul => 3,
                    _ => 1,
                };
                self.stack.push_temp(result_width);
                self.flush_stack_effects();
            }

            Expr::Call {
                path,
                generic_args,
                args,
            } => {
                let fn_name = path.node.as_dotted();
                self.build_call(&fn_name, generic_args, args);
            }

            Expr::Tuple(elements) => {
                for elem in elements {
                    self.build_expr(&elem.node);
                }
                let n = elements.len();
                let mut total_width = 0u32;
                for _ in 0..n {
                    if let Some(e) = self.stack.pop() {
                        total_width += e.width;
                    }
                }
                self.stack.push_temp(total_width);
                self.flush_stack_effects();
            }

            Expr::ArrayInit(elements) => {
                for elem in elements {
                    self.build_expr(&elem.node);
                }
                let n = elements.len();
                let mut total_width = 0u32;
                for _ in 0..n {
                    if let Some(e) = self.stack.pop() {
                        total_width += e.width;
                    }
                }
                self.stack.push_temp(total_width);
                if n > 0 {
                    if let Some(top) = self.stack.last_mut() {
                        top.elem_width = Some(total_width / n as u32);
                    }
                }
                self.flush_stack_effects();
            }

            Expr::FieldAccess { expr: inner, field } => {
                self.build_field_access(inner, field);
            }

            Expr::Index { expr: inner, index } => {
                self.build_index(inner, index);
            }

            Expr::StructInit { path: _, fields } => {
                let mut total_width = 0u32;
                for (_name, val) in fields {
                    self.build_expr(&val.node);
                    if let Some(e) = self.stack.pop() {
                        total_width += e.width;
                    }
                }
                self.stack.push_temp(total_width);
                self.flush_stack_effects();
            }
        }
    }

    // ── Var expression (dotted and simple) ────────────────────────

    fn build_var_expr(&mut self, name: &str) {
        if name.contains('.') {
            let dot_pos = name.rfind('.').unwrap();
            let prefix = &name[..dot_pos];
            let suffix = &name[dot_pos + 1..];
            let var_depth_info = self.find_var_depth_and_width(prefix);
            if let Some((base_depth, _var_width)) = var_depth_info {
                // Field access on struct variable.
                let field_offset = self.find_field_offset_in_var(prefix, suffix);
                if let Some((offset_from_top, field_width)) = field_offset {
                    let real_depth = base_depth + offset_from_top;
                    self.stack.ensure_space(field_width);
                    self.flush_stack_effects();
                    for _ in 0..field_width {
                        self.ops.push(IROp::Dup(real_depth + field_width - 1));
                    }
                    self.stack.push_temp(field_width);
                } else {
                    let depth = base_depth;
                    self.emit_and_push(IROp::Dup(depth), 1);
                }
            } else {
                // Module constant.
                if let Some(&val) = self.constants.get(name) {
                    self.emit_and_push(IROp::Push(val), 1);
                } else if let Some(&val) = self.constants.get(suffix) {
                    self.emit_and_push(IROp::Push(val), 1);
                } else {
                    self.ops.push(IROp::Comment(format!(
                        "ERROR: unresolved constant '{}'",
                        name
                    )));
                    self.emit_and_push(IROp::Push(0), 1);
                }
            }
        } else {
            // Ensure variable is on stack (reload if spilled).
            self.stack.access_var(name);
            self.flush_stack_effects();

            let var_info = self.stack.find_var_depth_and_width(name);
            self.flush_stack_effects();

            if let Some((_depth, width)) = var_info {
                self.stack.ensure_space(width);
                self.flush_stack_effects();
                let depth = self.stack.find_var_depth(name);
                self.flush_stack_effects();

                if depth + width - 1 <= 15 {
                    for _ in 0..width {
                        self.ops.push(IROp::Dup(depth + width - 1));
                    }
                } else {
                    // Too deep — force spill of other variables.
                    self.stack.ensure_space(width);
                    self.flush_stack_effects();
                    self.stack.access_var(name);
                    self.flush_stack_effects();
                    let depth2 = self.stack.find_var_depth(name);
                    self.flush_stack_effects();
                    if depth2 + width - 1 <= 15 {
                        for _ in 0..width {
                            self.ops.push(IROp::Dup(depth2 + width - 1));
                        }
                    } else {
                        self.ops.push(IROp::Comment(format!(
                            "BUG: variable '{}' unreachable (depth {}+{}), aborting",
                            name, depth2, width
                        )));
                        self.ops.push(IROp::Push(0));
                        self.ops.push(IROp::Assert);
                    }
                }
                self.stack.push_temp(width);
            } else {
                // Variable not found — fallback.
                self.ops.push(IROp::Dup(0));
                self.stack.push_temp(1);
            }
        }
    }

    // ── Field access ──────────────────────────────────────────────

    fn build_field_access(&mut self, inner: &Spanned<Expr>, field: &Spanned<String>) {
        self.build_expr(&inner.node);
        let inner_entry = self.stack.last().cloned();
        if let Some(entry) = inner_entry {
            let struct_width = entry.width;
            let field_offset = self.resolve_field_offset(&inner.node, &field.node);
            if let Some((offset, field_width)) = field_offset {
                for i in 0..field_width {
                    self.ops.push(IROp::Dup(offset + (field_width - 1 - i)));
                }
                self.stack.pop();
                for _ in 0..field_width {
                    self.ops.push(IROp::Swap(field_width + struct_width - 1));
                }
                self.emit_pop(struct_width);
                self.stack.push_temp(field_width);
                self.flush_stack_effects();
            } else {
                // No layout from variable — search struct_types.
                let mut found: Option<(u32, u32)> = None;
                for sdef in self.struct_types.values() {
                    let total: u32 = sdef
                        .fields
                        .iter()
                        .map(|f| resolve_type_width(&f.ty.node, &self.target_config))
                        .sum();
                    if total != struct_width {
                        continue;
                    }
                    let mut off = 0u32;
                    for sf in &sdef.fields {
                        let fw = resolve_type_width(&sf.ty.node, &self.target_config);
                        if sf.name.node == field.node {
                            found = Some((total - off - fw, fw));
                            break;
                        }
                        off += fw;
                    }
                    if found.is_some() {
                        break;
                    }
                }
                if let Some((from_top, fw)) = found {
                    for i in 0..fw {
                        self.ops.push(IROp::Dup(from_top + (fw - 1 - i)));
                    }
                    self.stack.pop();
                    for _ in 0..fw {
                        self.ops.push(IROp::Swap(fw + struct_width - 1));
                    }
                    self.emit_pop(struct_width);
                    self.stack.push_temp(fw);
                    self.flush_stack_effects();
                } else {
                    self.ops.push(IROp::Comment(format!(
                        "ERROR: unresolved field '{}'",
                        field.node
                    )));
                    self.stack.pop();
                    self.stack.push_temp(1);
                    self.flush_stack_effects();
                }
            }
        } else {
            self.stack.push_temp(1);
            self.flush_stack_effects();
        }
    }

    // ── Index expression ──────────────────────────────────────────

    fn build_index(&mut self, inner: &Spanned<Expr>, index: &Spanned<Expr>) {
        self.build_expr(&inner.node);
        let inner_entry = self.stack.last().cloned();

        if let Expr::Literal(Literal::Integer(idx)) = &index.node {
            // Constant index.
            let idx = *idx as u32;
            if let Some(entry) = inner_entry {
                let array_width = entry.width;
                let elem_width = entry.elem_width.unwrap_or(1);
                let base_offset = array_width - (idx + 1) * elem_width;
                for i in 0..elem_width {
                    self.ops.push(IROp::Dup(base_offset + (elem_width - 1 - i)));
                }
                self.stack.pop();
                for _ in 0..elem_width {
                    self.ops.push(IROp::Swap(elem_width + array_width - 1));
                }
                self.emit_pop(array_width);
                self.stack.push_temp(elem_width);
                self.flush_stack_effects();
            } else {
                self.stack.push_temp(1);
                self.flush_stack_effects();
            }
        } else {
            // Runtime index — use RAM-based access.
            self.build_expr(&index.node);
            let _idx_entry = self.stack.pop();
            let arr_entry = self.stack.pop();

            if let Some(arr) = arr_entry {
                let array_width = arr.width;
                let elem_width = arr.elem_width.unwrap_or(1);
                let base = self.temp_ram_addr;
                self.temp_ram_addr += array_width as u64;

                // Store array elements to RAM.
                self.ops.push(IROp::Swap(1));
                for i in 0..array_width {
                    let addr = base + i as u64;
                    self.ops.push(IROp::Push(addr));
                    self.ops.push(IROp::Swap(1));
                    self.ops.push(IROp::WriteMem(1));
                    self.ops.push(IROp::Pop(1));
                    if i + 1 < array_width {
                        self.ops.push(IROp::Swap(1));
                    }
                }

                // Compute target address: base + idx * elem_width.
                if elem_width > 1 {
                    self.ops.push(IROp::Push(elem_width as u64));
                    self.ops.push(IROp::Mul);
                }
                self.ops.push(IROp::Push(base));
                self.ops.push(IROp::Add);

                // Read elem_width elements from computed address.
                for i in 0..elem_width {
                    self.ops.push(IROp::Dup(0));
                    if i > 0 {
                        self.ops.push(IROp::Push(i as u64));
                        self.ops.push(IROp::Add);
                    }
                    self.ops.push(IROp::ReadMem(1));
                    self.ops.push(IROp::Pop(1));
                    self.ops.push(IROp::Swap(1));
                }
                self.ops.push(IROp::Pop(1)); // pop address

                self.stack.push_temp(elem_width);
                self.flush_stack_effects();
            } else {
                self.stack.push_temp(1);
                self.flush_stack_effects();
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // ── Call emission ─────────────────────────────────────────────
    // ═══════════════════════════════════════════════════════════════

    fn build_call(
        &mut self,
        name: &str,
        generic_args: &[Spanned<ArraySize>],
        args: &[Spanned<Expr>],
    ) {
        // Evaluate arguments — each pushes a temp.
        for arg in args {
            self.build_expr(&arg.node);
        }

        // Pop all arg temps from the model.
        let arg_count = args.len();
        for _ in 0..arg_count {
            self.stack.pop();
        }

        // Resolve intrinsic name.
        let resolved_name = self.intrinsic_map.get(name).cloned().or_else(|| {
            name.rsplit('.')
                .next()
                .and_then(|short| self.intrinsic_map.get(short).cloned())
        });
        let effective_name = resolved_name.as_deref().unwrap_or(name);

        match effective_name {
            // ── I/O ──
            "pub_read" => {
                self.emit_and_push(IROp::ReadIo(1), 1);
            }
            "pub_read2" => {
                self.emit_and_push(IROp::ReadIo(2), 2);
            }
            "pub_read3" => {
                self.emit_and_push(IROp::ReadIo(3), 3);
            }
            "pub_read4" => {
                self.emit_and_push(IROp::ReadIo(4), 4);
            }
            "pub_read5" => {
                self.emit_and_push(IROp::ReadIo(5), 5);
            }
            "pub_write" => {
                self.ops.push(IROp::WriteIo(1));
                self.push_temp(0);
            }
            "pub_write2" => {
                self.ops.push(IROp::WriteIo(2));
                self.push_temp(0);
            }
            "pub_write3" => {
                self.ops.push(IROp::WriteIo(3));
                self.push_temp(0);
            }
            "pub_write4" => {
                self.ops.push(IROp::WriteIo(4));
                self.push_temp(0);
            }
            "pub_write5" => {
                self.ops.push(IROp::WriteIo(5));
                self.push_temp(0);
            }

            // ── Non-deterministic input ──
            "divine" => {
                self.emit_and_push(IROp::Divine(1), 1);
            }
            "divine3" => {
                self.emit_and_push(IROp::Divine(3), 3);
            }
            "divine5" => {
                self.emit_and_push(IROp::Divine(5), 5);
            }

            // ── Assertions ──
            "assert" => {
                self.ops.push(IROp::Assert);
                self.push_temp(0);
            }
            "assert_eq" => {
                self.ops.push(IROp::Eq);
                self.ops.push(IROp::Assert);
                self.push_temp(0);
            }
            "assert_digest" => {
                self.ops.push(IROp::AssertVector);
                self.ops.push(IROp::Pop(5));
                self.push_temp(0);
            }

            // ── Field operations ──
            "field_add" => {
                self.ops.push(IROp::Add);
                self.push_temp(1);
            }
            "field_mul" => {
                self.ops.push(IROp::Mul);
                self.push_temp(1);
            }
            "inv" => {
                self.ops.push(IROp::Invert);
                self.push_temp(1);
            }
            "neg" => {
                self.ops.push(IROp::PushNegOne);
                self.ops.push(IROp::Mul);
                self.push_temp(1);
            }
            "sub" => {
                self.ops.push(IROp::PushNegOne);
                self.ops.push(IROp::Mul);
                self.ops.push(IROp::Add);
                self.push_temp(1);
            }

            // ── U32 operations ──
            "split" => {
                self.ops.push(IROp::Split);
                self.push_temp(2);
            }
            "log2" => {
                self.ops.push(IROp::Log2);
                self.push_temp(1);
            }
            "pow" => {
                self.ops.push(IROp::Pow);
                self.push_temp(1);
            }
            "popcount" => {
                self.ops.push(IROp::PopCount);
                self.push_temp(1);
            }

            // ── Hash operations ──
            "hash" => {
                self.ops.push(IROp::Hash);
                self.push_temp(5);
            }
            "sponge_init" => {
                self.ops.push(IROp::SpongeInit);
                self.push_temp(0);
            }
            "sponge_absorb" => {
                self.ops.push(IROp::SpongeAbsorb);
                self.push_temp(0);
            }
            "sponge_squeeze" => {
                self.emit_and_push(IROp::SpongeSqueeze, 10);
            }
            "sponge_absorb_mem" => {
                self.ops.push(IROp::SpongeAbsorbMem);
                self.push_temp(0);
            }

            // ── Merkle ──
            "merkle_step" => {
                self.emit_and_push(IROp::MerkleStep, 6);
            }
            "merkle_step_mem" => {
                self.emit_and_push(IROp::MerkleStepMem, 7);
            }

            // ── RAM ──
            "ram_read" => {
                self.ops.push(IROp::ReadMem(1));
                self.ops.push(IROp::Pop(1));
                self.push_temp(1);
            }
            "ram_write" => {
                self.ops.push(IROp::WriteMem(1));
                self.ops.push(IROp::Pop(1));
                self.push_temp(0);
            }
            "ram_read_block" => {
                self.ops.push(IROp::ReadMem(5));
                self.ops.push(IROp::Pop(1));
                self.push_temp(5);
            }
            "ram_write_block" => {
                self.ops.push(IROp::WriteMem(5));
                self.ops.push(IROp::Pop(1));
                self.push_temp(0);
            }

            // ── Conversion ──
            "as_u32" => {
                self.ops.push(IROp::Split);
                self.ops.push(IROp::Pop(1));
                self.push_temp(1);
            }
            "as_field" => {
                self.push_temp(1);
            }

            // ── XField ──
            "xfield" => {
                self.push_temp(3);
            }
            "xinvert" => {
                self.ops.push(IROp::XInvert);
                self.push_temp(3);
            }
            "xx_dot_step" => {
                self.emit_and_push(IROp::XxDotStep, 5);
            }
            "xb_dot_step" => {
                self.emit_and_push(IROp::XbDotStep, 5);
            }

            // ── User-defined function ──
            _ => {
                self.build_user_call(name, generic_args);
            }
        }
    }

    /// Emit a call to a user-defined (non-intrinsic) function.
    fn build_user_call(&mut self, name: &str, generic_args: &[Spanned<ArraySize>]) {
        let is_generic = self.generic_fn_defs.contains_key(name);

        let (call_label, base_name) = if is_generic {
            let size_args: Vec<u64> = if !generic_args.is_empty() {
                generic_args
                    .iter()
                    .map(|ga| ga.node.eval(&self.current_subs))
                    .collect()
            } else if !self.current_subs.is_empty() {
                if let Some(gdef) = self.generic_fn_defs.get(name) {
                    gdef.type_params
                        .iter()
                        .map(|p| self.current_subs.get(&p.node).copied().unwrap_or(0))
                        .collect()
                } else {
                    vec![]
                }
            } else {
                let idx = self.call_resolution_idx;
                if idx < self.call_resolutions.len() && self.call_resolutions[idx].name == name {
                    self.call_resolution_idx += 1;
                    self.call_resolutions[idx].size_args.clone()
                } else {
                    let mut found = vec![];
                    for (i, res) in self.call_resolutions.iter().enumerate() {
                        if i >= self.call_resolution_idx && res.name == name {
                            self.call_resolution_idx = i + 1;
                            found = res.size_args.clone();
                            break;
                        }
                    }
                    found
                }
            };
            let inst = MonoInstance {
                name: name.to_string(),
                size_args,
            };
            let base = inst.mangled_name();
            (base.clone(), base)
        } else if name.contains('.') {
            // Cross-module call.
            let parts: Vec<&str> = name.rsplitn(2, '.').collect();
            let fn_name = parts[0];
            let short_module = parts[1];
            let full_module = self
                .module_aliases
                .get(short_module)
                .map(|s| s.as_str())
                .unwrap_or(short_module);
            let mangled = full_module.replace('.', "_");
            let base = format!("{}__{}", mangled, fn_name);
            (base, fn_name.to_string())
        } else {
            (name.to_string(), name.to_string())
        };

        let ret_width = self.fn_return_widths.get(&base_name).copied().unwrap_or(0);
        if ret_width > 0 {
            self.emit_and_push(IROp::Call(call_label), ret_width);
        } else {
            self.ops.push(IROp::Call(call_label));
            self.push_temp(0);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// ── Tests ─────────────────────────────────────────────────────────
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::{Span, Spanned};

    fn dummy_span() -> Span {
        Span::dummy()
    }

    fn sp<T>(node: T) -> Spanned<T> {
        Spanned::new(node, dummy_span())
    }

    fn minimal_program(items: Vec<Item>) -> File {
        File {
            kind: FileKind::Program,
            name: sp("test".to_string()),
            uses: vec![],
            declarations: vec![],
            items: items.into_iter().map(|i| sp(i)).collect(),
        }
    }

    fn make_builder() -> IRBuilder {
        IRBuilder::new(TargetConfig::triton())
    }

    // ── Test: minimal program produces Preamble + FnStart + FnEnd ──

    #[test]
    fn test_minimal_program() {
        let file = minimal_program(vec![Item::Fn(FnDef {
            is_pub: false,
            cfg: None,
            intrinsic: None,
            is_test: false,
            is_pure: false,
            requires: vec![],
            ensures: vec![],
            name: sp("main".to_string()),
            type_params: vec![],
            params: vec![],
            return_ty: None,
            body: Some(sp(Block {
                stmts: vec![],
                tail_expr: None,
            })),
        })]);

        let ops = make_builder().build_file(&file);

        // Should have Preamble, FnStart, Return, FnEnd.
        assert!(
            ops.iter().any(|op| matches!(op, IROp::Preamble(_))),
            "expected Preamble op"
        );
        assert!(
            ops.iter()
                .any(|op| matches!(op, IROp::FnStart(n) if n == "main")),
            "expected FnStart(main)"
        );
        assert!(
            ops.iter().any(|op| matches!(op, IROp::Return)),
            "expected Return"
        );
        assert!(
            ops.iter().any(|op| matches!(op, IROp::FnEnd)),
            "expected FnEnd"
        );
    }

    // ── Test: if/else produces IROp::IfElse ──

    #[test]
    fn test_if_else_produces_structural_op() {
        let file = minimal_program(vec![Item::Fn(FnDef {
            is_pub: false,
            cfg: None,
            intrinsic: None,
            is_test: false,
            is_pure: false,
            requires: vec![],
            ensures: vec![],
            name: sp("main".to_string()),
            type_params: vec![],
            params: vec![],
            return_ty: None,
            body: Some(sp(Block {
                stmts: vec![sp(Stmt::If {
                    cond: sp(Expr::Literal(Literal::Bool(true))),
                    then_block: sp(Block {
                        stmts: vec![sp(Stmt::Expr(sp(Expr::Call {
                            path: sp(ModulePath::single("pub_write".to_string())),
                            generic_args: vec![],
                            args: vec![sp(Expr::Literal(Literal::Integer(1)))],
                        })))],
                        tail_expr: None,
                    }),
                    else_block: Some(sp(Block {
                        stmts: vec![sp(Stmt::Expr(sp(Expr::Call {
                            path: sp(ModulePath::single("pub_write".to_string())),
                            generic_args: vec![],
                            args: vec![sp(Expr::Literal(Literal::Integer(0)))],
                        })))],
                        tail_expr: None,
                    })),
                })],
                tail_expr: None,
            })),
        })]);

        let ops = make_builder().build_file(&file);

        let has_if_else = ops.iter().any(|op| matches!(op, IROp::IfElse { .. }));
        assert!(has_if_else, "expected IROp::IfElse in output");
    }

    // ── Test: for loop produces IROp::Loop ──

    #[test]
    fn test_for_loop_produces_loop_op() {
        let file = minimal_program(vec![Item::Fn(FnDef {
            is_pub: false,
            cfg: None,
            intrinsic: None,
            is_test: false,
            is_pure: false,
            requires: vec![],
            ensures: vec![],
            name: sp("main".to_string()),
            type_params: vec![],
            params: vec![],
            return_ty: None,
            body: Some(sp(Block {
                stmts: vec![sp(Stmt::For {
                    var: sp("i".to_string()),
                    start: sp(Expr::Literal(Literal::Integer(0))),
                    end: sp(Expr::Literal(Literal::Integer(5))),
                    bound: Some(5),
                    body: sp(Block {
                        stmts: vec![],
                        tail_expr: None,
                    }),
                })],
                tail_expr: None,
            })),
        })]);

        let ops = make_builder().build_file(&file);

        let has_loop = ops.iter().any(|op| matches!(op, IROp::Loop { .. }));
        assert!(has_loop, "expected IROp::Loop in output");
    }

    // ── Test: arithmetic produces the right instruction sequence ──

    #[test]
    fn test_arithmetic_sequence() {
        // fn main() -> Field { 2 + 3 * 4 }
        // Parser would give us: BinOp(Add, Lit(2), BinOp(Mul, Lit(3), Lit(4)))
        let file = minimal_program(vec![Item::Fn(FnDef {
            is_pub: false,
            cfg: None,
            intrinsic: None,
            is_test: false,
            is_pure: false,
            requires: vec![],
            ensures: vec![],
            name: sp("main".to_string()),
            type_params: vec![],
            params: vec![],
            return_ty: Some(sp(Type::Field)),
            body: Some(sp(Block {
                stmts: vec![],
                tail_expr: Some(Box::new(sp(Expr::BinOp {
                    op: BinOp::Add,
                    lhs: Box::new(sp(Expr::Literal(Literal::Integer(2)))),
                    rhs: Box::new(sp(Expr::BinOp {
                        op: BinOp::Mul,
                        lhs: Box::new(sp(Expr::Literal(Literal::Integer(3)))),
                        rhs: Box::new(sp(Expr::Literal(Literal::Integer(4)))),
                    })),
                }))),
            })),
        })]);

        let ops = make_builder().build_file(&file);

        // Expect: Push(2), Push(3), Push(4), Mul, Add somewhere in the ops.
        let flat: Vec<String> = ops.iter().map(|op| format!("{}", op)).collect();
        let joined = flat.join(" | ");

        assert!(
            joined.contains("push 2"),
            "expected push 2, got: {}",
            joined
        );
        assert!(
            joined.contains("push 3"),
            "expected push 3, got: {}",
            joined
        );
        assert!(
            joined.contains("push 4"),
            "expected push 4, got: {}",
            joined
        );
        assert!(joined.contains("mul"), "expected mul, got: {}", joined);
        assert!(joined.contains("add"), "expected add, got: {}", joined);

        // Verify ordering: push 3 before push 4 before mul.
        let push3_pos = flat.iter().position(|s| s == "push 3").unwrap();
        let push4_pos = flat.iter().position(|s| s == "push 4").unwrap();
        let mul_pos = flat.iter().position(|s| s == "mul").unwrap();
        let push2_pos = flat.iter().position(|s| s == "push 2").unwrap();
        let add_pos = flat.iter().position(|s| s == "add").unwrap();

        assert!(push3_pos < push4_pos, "push 3 should precede push 4");
        assert!(push4_pos < mul_pos, "push 4 should precede mul");
        assert!(push2_pos < add_pos, "push 2 should precede add");
        assert!(mul_pos < add_pos, "mul should precede add");
    }

    // ── Test: parse_spill_effect ──

    #[test]
    fn test_parse_spill_effect() {
        assert!(matches!(parse_spill_effect("    push 42"), IROp::Push(42)));
        assert!(matches!(parse_spill_effect("    swap 5"), IROp::Swap(5)));
        assert!(matches!(parse_spill_effect("    pop 1"), IROp::Pop(1)));
        assert!(matches!(
            parse_spill_effect("    write_mem 1"),
            IROp::WriteMem(1)
        ));
        assert!(matches!(
            parse_spill_effect("    read_mem 1"),
            IROp::ReadMem(1)
        ));
        assert!(matches!(parse_spill_effect("  dup 3"), IROp::Dup(3)));
    }

    // ── Test: module (not program) omits preamble ──

    #[test]
    fn test_module_no_preamble() {
        let file = File {
            kind: FileKind::Module,
            name: sp("mylib".to_string()),
            uses: vec![],
            declarations: vec![],
            items: vec![sp(Item::Fn(FnDef {
                is_pub: true,
                cfg: None,
                intrinsic: None,
                is_test: false,
                is_pure: false,
                requires: vec![],
                ensures: vec![],
                name: sp("helper".to_string()),
                type_params: vec![],
                params: vec![],
                return_ty: None,
                body: Some(sp(Block {
                    stmts: vec![],
                    tail_expr: None,
                })),
            }))],
        };

        let ops = make_builder().build_file(&file);

        assert!(
            !ops.iter().any(|op| matches!(op, IROp::Preamble(_))),
            "module should not produce Preamble"
        );
        assert!(
            ops.iter()
                .any(|op| matches!(op, IROp::FnStart(n) if n == "helper")),
            "expected FnStart(helper)"
        );
    }

    // ── Test: if-only (no else) produces IfOnly ──

    #[test]
    fn test_if_only_produces_structural_op() {
        let file = minimal_program(vec![Item::Fn(FnDef {
            is_pub: false,
            cfg: None,
            intrinsic: None,
            is_test: false,
            is_pure: false,
            requires: vec![],
            ensures: vec![],
            name: sp("main".to_string()),
            type_params: vec![],
            params: vec![],
            return_ty: None,
            body: Some(sp(Block {
                stmts: vec![sp(Stmt::If {
                    cond: sp(Expr::Literal(Literal::Bool(true))),
                    then_block: sp(Block {
                        stmts: vec![],
                        tail_expr: None,
                    }),
                    else_block: None,
                })],
                tail_expr: None,
            })),
        })]);

        let ops = make_builder().build_file(&file);
        let has_if_only = ops.iter().any(|op| matches!(op, IROp::IfOnly { .. }));
        assert!(has_if_only, "expected IROp::IfOnly in output");
    }

    // ── Test: let binding + variable reference ──

    #[test]
    fn test_let_and_var_ref() {
        // fn main() -> Field { let x: Field = 42; x }
        let file = minimal_program(vec![Item::Fn(FnDef {
            is_pub: false,
            cfg: None,
            intrinsic: None,
            is_test: false,
            is_pure: false,
            requires: vec![],
            ensures: vec![],
            name: sp("main".to_string()),
            type_params: vec![],
            params: vec![],
            return_ty: Some(sp(Type::Field)),
            body: Some(sp(Block {
                stmts: vec![sp(Stmt::Let {
                    mutable: false,
                    pattern: Pattern::Name(sp("x".to_string())),
                    ty: Some(sp(Type::Field)),
                    init: sp(Expr::Literal(Literal::Integer(42))),
                })],
                tail_expr: Some(Box::new(sp(Expr::Var("x".to_string())))),
            })),
        })]);

        let ops = make_builder().build_file(&file);

        // Should push 42 and then dup it for the tail expression.
        let flat: Vec<String> = ops.iter().map(|op| format!("{}", op)).collect();
        assert!(flat.contains(&"push 42".to_string()), "expected push 42");
        assert!(
            flat.contains(&"dup 0".to_string()),
            "expected dup 0 for variable reference"
        );
    }

    // ── Test: intrinsic call dispatch ──

    #[test]
    fn test_intrinsic_pub_read_write() {
        // fn main() { pub_write(pub_read()) }
        let file = minimal_program(vec![Item::Fn(FnDef {
            is_pub: false,
            cfg: None,
            intrinsic: None,
            is_test: false,
            is_pure: false,
            requires: vec![],
            ensures: vec![],
            name: sp("main".to_string()),
            type_params: vec![],
            params: vec![],
            return_ty: None,
            body: Some(sp(Block {
                stmts: vec![sp(Stmt::Expr(sp(Expr::Call {
                    path: sp(ModulePath::single("pub_write".to_string())),
                    generic_args: vec![],
                    args: vec![sp(Expr::Call {
                        path: sp(ModulePath::single("pub_read".to_string())),
                        generic_args: vec![],
                        args: vec![],
                    })],
                })))],
                tail_expr: None,
            })),
        })]);

        let ops = make_builder().build_file(&file);

        let has_read = ops.iter().any(|op| matches!(op, IROp::ReadIo(1)));
        let has_write = ops.iter().any(|op| matches!(op, IROp::WriteIo(1)));
        assert!(has_read, "expected ReadIo(1)");
        assert!(has_write, "expected WriteIo(1)");
    }

    // ── Test: IfElse has non-empty nested bodies ──

    #[test]
    fn test_if_else_nested_bodies_have_content() {
        let file = minimal_program(vec![Item::Fn(FnDef {
            is_pub: false,
            cfg: None,
            intrinsic: None,
            is_test: false,
            is_pure: false,
            requires: vec![],
            ensures: vec![],
            name: sp("main".to_string()),
            type_params: vec![],
            params: vec![],
            return_ty: None,
            body: Some(sp(Block {
                stmts: vec![sp(Stmt::If {
                    cond: sp(Expr::Literal(Literal::Bool(true))),
                    then_block: sp(Block {
                        stmts: vec![sp(Stmt::Expr(sp(Expr::Call {
                            path: sp(ModulePath::single("pub_write".to_string())),
                            generic_args: vec![],
                            args: vec![sp(Expr::Literal(Literal::Integer(1)))],
                        })))],
                        tail_expr: None,
                    }),
                    else_block: Some(sp(Block {
                        stmts: vec![sp(Stmt::Expr(sp(Expr::Call {
                            path: sp(ModulePath::single("pub_write".to_string())),
                            generic_args: vec![],
                            args: vec![sp(Expr::Literal(Literal::Integer(0)))],
                        })))],
                        tail_expr: None,
                    })),
                })],
                tail_expr: None,
            })),
        })]);

        let ops = make_builder().build_file(&file);

        for op in &ops {
            if let IROp::IfElse {
                then_body,
                else_body,
            } = op
            {
                assert!(!then_body.is_empty(), "then_body should not be empty");
                assert!(!else_body.is_empty(), "else_body should not be empty");

                // then_body should contain Push(1) and WriteIo(1).
                let then_has_push1 = then_body.iter().any(|o| matches!(o, IROp::Push(1)));
                let then_has_write = then_body.iter().any(|o| matches!(o, IROp::WriteIo(1)));
                assert!(then_has_push1, "then_body should have Push(1)");
                assert!(then_has_write, "then_body should have WriteIo(1)");

                // else_body should contain Push(0) and WriteIo(1).
                let else_has_push0 = else_body.iter().any(|o| matches!(o, IROp::Push(0)));
                let else_has_write = else_body.iter().any(|o| matches!(o, IROp::WriteIo(1)));
                assert!(else_has_push0, "else_body should have Push(0)");
                assert!(else_has_write, "else_body should have WriteIo(1)");

                return;
            }
        }
        panic!("no IfElse op found");
    }
}
