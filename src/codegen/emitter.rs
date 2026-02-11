use std::collections::{HashMap, HashSet};

use super::stack::StackManager;
use crate::ast::*;
use crate::span::Spanned;
use crate::target::TargetConfig;
use crate::typecheck::MonoInstance;

pub(crate) use super::backend::create_backend;
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
                // Strip the leading __ from mangled_name for fn_return_widths lookup
                let base = mangled.trim_start_matches("__");
                self.fn_return_widths.insert(base.to_string(), width);
                // Also register under full mangled name
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
                self.raw("// sec ram: prover-initialized RAM slots");
                for (addr, ty) in entries {
                    self.raw(&format!(
                        "// ram[{}]: {} ({} field element{})",
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
            let call_inst = self.backend.inst_call("__main");
            let halt_inst = self.backend.inst_halt();
            self.raw(&format!("    {}", call_inst));
            self.raw(&format!("    {}", halt_inst));
            self.raw("");
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

        let label = if func.name.node == "main" {
            "__main".to_string()
        } else {
            format!("__{}", func.name.node)
        };

        self.emit_label(&label);
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

        self.b_return();
        self.raw("");

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

        let label = inst.mangled_name();
        self.emit_label(&label);
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

        self.b_return();
        self.raw("");

        self.flush_deferred();
        self.stack.clear();
        self.current_subs.clear();
    }

    fn flush_deferred(&mut self) {
        while !self.deferred.is_empty() {
            let deferred = std::mem::take(&mut self.deferred);
            for block in deferred {
                self.emit_label(&block.label);
                if block.clears_flag {
                    self.b_pop(1);
                }
                self.emit_block(&block.block);
                if block.clears_flag {
                    self.b_push(0);
                }
                self.b_return();
                self.raw("");
            }
        }
    }

    fn emit_block(&mut self, block: &Block) {
        for stmt in &block.stmts {
            self.emit_stmt(&stmt.node);
        }
        // Tail expression: emitted and left on stack as the block's return value
        if let Some(tail) = &block.tail_expr {
            self.emit_expr(&tail.node);
        }
    }

    fn emit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let {
                pattern, init, ty, ..
            } => {
                // emit_expr pushes an anonymous temp onto the stack model
                self.emit_expr(&init.node);

                match pattern {
                    Pattern::Name(name) => {
                        // Rename the top temp to the variable name
                        if name.node != "_" {
                            if let Some(top) = self.stack.last_mut() {
                                top.name = Some(name.node.clone());
                            }
                            // If type is an array, record elem_width
                            if let Some(sp_ty) = ty {
                                if let Type::Array(inner_ty, _) = &sp_ty.node {
                                    let ew = resolve_type_width(inner_ty, &self.target_config);
                                    if let Some(top) = self.stack.last_mut() {
                                        top.elem_width = Some(ew);
                                    }
                                }
                            }
                            // Record struct field layout from type annotation or struct init
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
                                // Resolve struct layout from type annotation
                                // (covers function returns, divine, etc.)
                                self.register_struct_layout_from_type(&name.node, &sp_ty.node);
                            }
                        }
                    }
                    Pattern::Tuple(names) => {
                        // The init expression pushed a single temp with combined width.
                        // Split it into individual named entries.
                        let top = self.stack.pop();
                        if let Some(entry) = top {
                            let total_width = entry.width;
                            let n = names.len() as u32;
                            let elem_width = if n > 0 { total_width / n } else { 1 };

                            // Push individual entries for each name (first name = deepest)
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
                    self.emit_expr(&value.node);
                    // New value is the anonymous temp on top.
                    // Old value is below it. Find it (accounting for the temp).
                    let depth = self.find_var_depth(name);
                    if depth <= 15 {
                        self.b_swap(depth);
                        self.b_pop(1);
                    }
                    // Pop the temp and update the model: the variable's value was swapped
                    self.stack.pop(); // remove the anonymous temp
                }
            }
            Stmt::If {
                cond,
                then_block,
                else_block,
            } => {
                self.emit_expr(&cond.node);
                self.stack.pop(); // cond consumed by skiz

                if let Some(else_blk) = else_block {
                    let then_label = self.fresh_label("then");
                    let else_label = self.fresh_label("else");

                    self.b_push(1);
                    self.b_swap(1);
                    self.b_skiz();
                    self.b_call(&then_label);
                    self.b_skiz();
                    self.b_call(&else_label);

                    self.deferred.push(DeferredBlock {
                        label: then_label,
                        block: then_block.node.clone(),
                        clears_flag: true,
                    });
                    self.deferred.push(DeferredBlock {
                        label: else_label,
                        block: else_blk.node.clone(),
                        clears_flag: false,
                    });
                } else {
                    let then_label = self.fresh_label("then");
                    self.b_skiz();
                    self.b_call(&then_label);

                    self.deferred.push(DeferredBlock {
                        label: then_label,
                        block: then_block.node.clone(),
                        clears_flag: false,
                    });
                }
            }
            Stmt::For {
                var,
                start: _,
                end,
                body,
                ..
            } => {
                let loop_label = self.fresh_label("loop");

                self.emit_expr(&end.node);
                // counter is now on top as a temp

                self.b_call(&loop_label);
                self.b_pop(1);
                self.stack.pop(); // counter consumed

                let loop_body = &body.node;
                self.emit_loop_subroutine(&loop_label, loop_body, &var.node);
            }
            Stmt::TupleAssign { names, value } => {
                // Evaluate the RHS expression (pushes a tuple temp)
                self.emit_expr(&value.node);
                let top = self.stack.pop();
                if let Some(entry) = top {
                    let total_width = entry.width;
                    let n = names.len() as u32;
                    let elem_width = if n > 0 { total_width / n } else { 1 };

                    // For each name in the tuple, swap the new value into the old variable's position
                    for name in names.iter().rev() {
                        let depth = self.find_var_depth(&name.node);
                        if elem_width == 1 {
                            self.b_swap(depth);
                            self.b_pop(1);
                        }
                    }
                }
            }
            Stmt::Expr(expr) => {
                let before = self.stack.stack_len();
                self.emit_expr(&expr.node);
                // Pop any new entries produced by this expression
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
                    self.emit_expr(&val.node);
                }
            }
            Stmt::Emit { event_name, fields } => {
                let tag = self.event_tags.get(&event_name.node).copied().unwrap_or(0);
                let decl_order = self
                    .event_defs
                    .get(&event_name.node)
                    .cloned()
                    .unwrap_or_default();

                // Push tag and write it
                self.b_push(tag);
                self.b_write_io(1);

                // Emit each field in declaration order, write one at a time
                for def_name in &decl_order {
                    if let Some((_name, val)) = fields.iter().find(|(n, _)| n.node == *def_name) {
                        self.emit_expr(&val.node);
                        self.stack.pop(); // consumed by write_io
                        self.b_write_io(1);
                    }
                }
            }
            Stmt::Asm {
                body,
                effect,
                target,
            } => {
                // Skip asm blocks tagged for a different target
                if let Some(tag) = target {
                    if tag != &self.target_config.name {
                        return;
                    }
                }

                // Spill all named variables to RAM to isolate asm from managed stack
                self.stack.spill_all_named();
                self.flush_stack_effects();

                // Emit each non-empty, non-comment line as a raw instruction
                for line in body.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    self.inst(trimmed);
                }

                // Adjust stack model by declared net effect
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
                // Emit scrutinee value onto the stack
                self.emit_expr(&expr.node);
                // The scrutinee is now the top anonymous temp on the stack.
                // Rename it so we can track it.
                if let Some(top) = self.stack.last_mut() {
                    top.name = Some("__match_scrutinee".to_string());
                }

                // Collect arm info: (label, body_clone, is_literal)
                let mut deferred_arms: Vec<(String, Block, bool)> = Vec::new();

                for arm in arms {
                    match &arm.pattern.node {
                        MatchPattern::Literal(lit) => {
                            let arm_label = self.fresh_label("match_arm");
                            let rest_label = self.fresh_label("match_rest");

                            // dup the scrutinee for comparison
                            let depth = self.find_var_depth("__match_scrutinee");
                            self.b_dup(depth);

                            // push the pattern value
                            match lit {
                                Literal::Integer(n) => {
                                    self.b_push(*n);
                                }
                                Literal::Bool(b) => {
                                    self.b_push(if *b { 1 } else { 0 });
                                }
                            }

                            // eq → produces bool on stack
                            self.b_eq();

                            // Use the flag pattern: push 1, swap, skiz call arm, skiz call rest
                            self.b_push(1);
                            self.b_swap(1);
                            self.b_skiz();
                            self.b_call(&arm_label);
                            self.b_skiz();
                            self.b_call(&rest_label);

                            // Build arm body: pop scrutinee then run original body
                            let mut arm_stmts = vec![Spanned::new(
                                Stmt::Asm {
                                    body: "pop 1".to_string(),
                                    effect: -1,
                                    target: None,
                                },
                                arm.body.span,
                            )];
                            arm_stmts.extend(arm.body.node.stmts.clone());
                            deferred_arms.push((
                                arm_label,
                                Block {
                                    stmts: arm_stmts,
                                    tail_expr: arm.body.node.tail_expr.clone(),
                                },
                                true,
                            ));

                            // rest_label continues to next arm check — empty block
                            self.deferred.push(DeferredBlock {
                                label: rest_label,
                                block: Block {
                                    stmts: Vec::new(),
                                    tail_expr: None,
                                },
                                clears_flag: false,
                            });
                        }
                        MatchPattern::Wildcard => {
                            let w_label = self.fresh_label("match_wild");
                            self.b_call(&w_label);

                            let mut arm_stmts = vec![Spanned::new(
                                Stmt::Asm {
                                    body: "pop 1".to_string(),
                                    effect: -1,
                                    target: None,
                                },
                                arm.body.span,
                            )];
                            arm_stmts.extend(arm.body.node.stmts.clone());
                            deferred_arms.push((
                                w_label,
                                Block {
                                    stmts: arm_stmts,
                                    tail_expr: arm.body.node.tail_expr.clone(),
                                },
                                false,
                            ));
                        }
                        MatchPattern::Struct { name, fields } => {
                            // Struct pattern: unconditionally enter this arm (type
                            // checker guarantees the scrutinee matches the struct).
                            // Inside the arm, we decompose the struct and bind/check
                            // each field.
                            let s_label = self.fresh_label("match_struct");
                            self.b_call(&s_label);

                            // Build the arm body: first pop the 1-wide scrutinee
                            // placeholder, then emit inline asm to set up field
                            // bindings. The struct scrutinee occupies width field
                            // elements on the stack — the match setup pushed a
                            // 1-wide placeholder, but the actual struct is wider.
                            // We'll handle field extraction in the deferred block
                            // by looking up the struct type.
                            let mut arm_stmts: Vec<Spanned<Stmt>> = Vec::new();

                            // Pop the 1-wide scrutinee placeholder
                            arm_stmts.push(Spanned::new(
                                Stmt::Asm {
                                    body: "pop 1".to_string(),
                                    effect: -1,
                                    target: None,
                                },
                                arm.body.span,
                            ));

                            // Now emit field assertions and let-bindings.
                            // The struct is on the stack. We need to:
                            // 1. For literal fields: assert the field equals the literal
                            // 2. For binding fields: introduce a let binding
                            // 3. For wildcard fields: nothing
                            //
                            // We synthesize `let` statements for bindings and
                            // assert statements for literals using the struct's
                            // field access expression.
                            if let Some(sdef) = self.struct_types.get(&name.node).cloned() {
                                // The scrutinee expression — we need to reference
                                // the original scrutinee variable. Since we're inside
                                // a subroutine call, the scrutinee is gone. Instead,
                                // we divine each field's value and constrain it.
                                //
                                // Actually, the match emitter puts the scrutinee on
                                // the stack as __match_scrutinee. After calling the
                                // arm subroutine, the scrutinee is still available
                                // to the caller but not inside the callee.
                                //
                                // Simplest correct approach: emit the struct pattern
                                // as a wildcard (unconditional) and generate let
                                // bindings + assertions as synthesized statements
                                // that reference the scrutinee expression.
                                for spf in fields {
                                    let field_name = &spf.field_name.node;
                                    // Build expr: scrutinee_expr.field_name
                                    let access_expr = Expr::FieldAccess {
                                        expr: Box::new(expr.clone()),
                                        field: spf.field_name.clone(),
                                    };
                                    let access_spanned =
                                        Spanned::new(access_expr, spf.field_name.span);

                                    match &spf.pattern.node {
                                        FieldPattern::Binding(var_name) => {
                                            // Synthesize: let var_name = scrutinee.field
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
                                            // Synthesize: assert_eq(scrutinee.field, literal)
                                            let lit_expr = Spanned::new(
                                                Expr::Literal(lit.clone()),
                                                spf.pattern.span,
                                            );
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
                                                            ModulePath::single(
                                                                "assert".to_string(),
                                                            ),
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
                                            // No action needed
                                        }
                                    }
                                }
                            }

                            arm_stmts.extend(arm.body.node.stmts.clone());
                            deferred_arms.push((
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

                // Pop the scrutinee after match completes
                self.stack.pop(); // remove scrutinee from model
                self.b_pop(1);

                // Emit deferred blocks for each arm
                for (label, block, is_literal) in deferred_arms {
                    self.deferred.push(DeferredBlock {
                        label,
                        block,
                        clears_flag: is_literal,
                    });
                }
            }
            Stmt::Seal { event_name, fields } => {
                let tag = self.event_tags.get(&event_name.node).copied().unwrap_or(0);
                let decl_order = self
                    .event_defs
                    .get(&event_name.node)
                    .cloned()
                    .unwrap_or_default();
                let num_fields = decl_order.len();

                // Build 10-element hash input: tag, field0, field1, ..., 0-padding
                // Triton hash consumes 10 elements, produces 5 (Digest)
                // Stack order: push in reverse so first element is deepest

                // Push zero padding first (deepest)
                let padding = 9usize.saturating_sub(num_fields); // 10 elements minus 1 tag minus fields
                for _ in 0..padding {
                    self.b_push(0);
                }

                // Push fields in reverse declaration order
                for def_name in decl_order.iter().rev() {
                    if let Some((_name, val)) = fields.iter().find(|(n, _)| n.node == *def_name) {
                        self.emit_expr(&val.node);
                        self.stack.pop(); // will be consumed by hash
                    }
                }

                // Push tag (will be on top, consumed first by hash)
                self.b_push(tag);

                // Hash: consumes 10, produces 5 (Digest)
                self.b_hash();

                // Write the 5-element digest commitment
                self.b_write_io(5);
            }
        }
    }

    fn emit_loop_subroutine(&mut self, label: &str, body: &Block, _var_name: &str) {
        self.emit_label(label);
        self.b_dup(0);
        self.b_push(0);
        self.b_eq();
        self.b_skiz();
        self.b_return();
        self.b_push_neg_one();
        self.b_add();

        // Save and restore stack model since loop body is a separate context
        let saved = self.stack.save_state();
        self.stack.clear();
        self.emit_block(body);
        self.stack.restore_state(saved);

        self.b_recurse();
        self.raw("");
    }

    /// Emit an expression. Always pushes exactly one anonymous entry onto the model.
    fn emit_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Literal(Literal::Integer(n)) => {
                let s = self.backend.inst_push(*n as u64);
                self.emit_and_push(&s, 1);
            }
            Expr::Literal(Literal::Bool(b)) => {
                let s = self.backend.inst_push(if *b { 1 } else { 0 });
                self.emit_and_push(&s, 1);
            }
            Expr::Var(name) => {
                if name.contains('.') {
                    // Dotted name: could be field access (var.field) or module constant
                    let dot_pos = name.rfind('.').unwrap();
                    let prefix = &name[..dot_pos];
                    let suffix = &name[dot_pos + 1..];
                    // Check if prefix is a variable on stack
                    let var_depth_info = self.find_var_depth_and_width(prefix);
                    if let Some((base_depth, _var_width)) = var_depth_info {
                        // Field access on struct variable
                        let field_offset = self.find_field_offset_in_var(prefix, suffix);
                        if let Some((offset_from_top, field_width)) = field_offset {
                            let real_depth = base_depth + offset_from_top;
                            self.stack.ensure_space(field_width);
                            self.flush_stack_effects();
                            for _ in 0..field_width {
                                self.b_dup(real_depth + field_width - 1);
                            }
                            self.stack.push_temp(field_width);
                        } else {
                            let depth = base_depth;
                            let s = self.backend.inst_dup(depth);
                            self.emit_and_push(&s, 1);
                        }
                    } else {
                        // Module constant — look up value
                        if let Some(&val) = self.constants.get(name) {
                            let s = self.backend.inst_push(val);
                            self.emit_and_push(&s, 1);
                        } else if let Some(&val) = self.constants.get(suffix) {
                            let s = self.backend.inst_push(val);
                            self.emit_and_push(&s, 1);
                        } else {
                            self.inst(&format!("// ERROR: unresolved constant '{}'", name));
                            let s = self.backend.inst_push(0);
                            self.emit_and_push(&s, 1);
                        }
                    }
                } else {
                    // Ensure variable is on stack (reload if spilled)
                    self.stack.access_var(name);
                    self.flush_stack_effects();

                    // Get the variable's width from the stack model
                    let var_info = self.stack.find_var_depth_and_width(name);
                    self.flush_stack_effects();

                    if let Some((_depth, width)) = var_info {
                        // Ensure space for the dup copies
                        self.stack.ensure_space(width);
                        self.flush_stack_effects();
                        // Recompute depth after potential spill
                        let depth = self.stack.find_var_depth(name);
                        self.flush_stack_effects();

                        if depth + width - 1 <= 15 {
                            // dup (depth + width - 1) repeated `width` times
                            // copies the variable's elements bottom-to-top
                            for _ in 0..width {
                                self.b_dup(depth + width - 1);
                            }
                        } else {
                            // Too deep — force spill of other variables
                            self.stack.ensure_space(width);
                            self.flush_stack_effects();
                            self.stack.access_var(name);
                            self.flush_stack_effects();
                            let depth2 = self.stack.find_var_depth(name);
                            self.flush_stack_effects();
                            if depth2 + width - 1 <= 15 {
                                for _ in 0..width {
                                    self.b_dup(depth2 + width - 1);
                                }
                            } else {
                                self.inst(&format!(
                                    "// BUG: variable '{}' unreachable (depth {}+{}), aborting",
                                    name, depth2, width
                                ));
                                self.b_push(0);
                                self.b_assert(); // halt: stack depth exceeded
                            }
                        }
                        self.stack.push_temp(width);
                    } else {
                        // Variable not found — fallback
                        self.b_dup(0);
                        self.stack.push_temp(1);
                    }
                }
            }
            Expr::BinOp { op, lhs, rhs } => {
                self.emit_expr(&lhs.node); // pushes temp for lhs
                self.emit_expr(&rhs.node); // pushes temp for rhs
                match op {
                    BinOp::Add => self.b_add(),
                    BinOp::Mul => self.b_mul(),
                    BinOp::Eq => self.b_eq(),
                    BinOp::Lt => self.b_lt(),
                    BinOp::BitAnd => self.b_and(),
                    BinOp::BitXor => self.b_xor(),
                    BinOp::DivMod => self.b_div_mod(),
                    BinOp::XFieldMul => self.b_xb_mul(),
                }
                // Pop both temps, push result
                self.stack.pop(); // rhs temp
                self.stack.pop(); // lhs temp
                let result_width = match op {
                    BinOp::DivMod => 2,
                    BinOp::XFieldMul => 3,
                    _ => 1,
                };
                // BinOp consumes 2 and produces result_width. Net change is usually ≤0,
                // so no spill needed. But handle it correctly anyway.
                self.stack.push_temp(result_width);
                self.flush_stack_effects();
            }
            Expr::Call {
                path,
                generic_args,
                args,
            } => {
                let fn_name = path.node.as_dotted();
                self.emit_call(&fn_name, generic_args, args);
            }
            Expr::Tuple(elements) => {
                for elem in elements {
                    self.emit_expr(&elem.node);
                }
                // Merge all element temps into one tuple temp
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
                    self.emit_expr(&elem.node);
                }
                let n = elements.len();
                let mut total_width = 0u32;
                for _ in 0..n {
                    if let Some(e) = self.stack.pop() {
                        total_width += e.width;
                    }
                }
                self.stack.push_temp(total_width);
                // Record element width for index operations
                if n > 0 {
                    if let Some(top) = self.stack.last_mut() {
                        top.elem_width = Some(total_width / n as u32);
                    }
                }
                self.flush_stack_effects();
            }
            Expr::FieldAccess { expr: inner, field } => {
                // Emit the struct value onto the stack, then extract the field
                self.emit_expr(&inner.node);
                let inner_entry = self.stack.last().cloned();
                if let Some(entry) = inner_entry {
                    let struct_width = entry.width;
                    let field_offset = self.resolve_field_offset(&inner.node, &field.node);
                    if let Some((offset, field_width)) = field_offset {
                        // Dup the field from within the struct block on top of stack
                        for i in 0..field_width {
                            self.b_dup(offset + (field_width - 1 - i));
                        }
                        // Pop the struct temp, push field temp
                        self.stack.pop();
                        for _ in 0..field_width {
                            self.b_swap(field_width + struct_width - 1);
                        }
                        self.emit_pop(struct_width);
                        self.stack.push_temp(field_width);
                        self.flush_stack_effects();
                    } else {
                        // No layout from variable — search struct_types
                        // Collect field info first to avoid borrow conflict
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
                                self.b_dup(from_top + (fw - 1 - i));
                            }
                            self.stack.pop();
                            for _ in 0..fw {
                                self.b_swap(fw + struct_width - 1);
                            }
                            self.emit_pop(struct_width);
                            self.stack.push_temp(fw);
                            self.flush_stack_effects();
                        } else {
                            self.inst(&format!("// ERROR: unresolved field '{}'", field.node));
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
            Expr::Index { expr: inner, index } => {
                // For constant index on arrays, compute the offset
                self.emit_expr(&inner.node);
                let inner_entry = self.stack.last().cloned();
                if let Expr::Literal(Literal::Integer(idx)) = &index.node {
                    let idx = *idx as u32;
                    if let Some(entry) = inner_entry {
                        let array_width = entry.width;
                        let elem_width = entry.elem_width.unwrap_or(1);
                        let base_offset = array_width - (idx + 1) * elem_width;
                        // Dup elem_width elements from within the array
                        for i in 0..elem_width {
                            self.b_dup(base_offset + (elem_width - 1 - i));
                        }
                        // Pop the array, push the element
                        self.stack.pop();
                        for _ in 0..elem_width {
                            self.b_swap(elem_width + array_width - 1);
                        }
                        self.emit_pop(array_width);
                        self.stack.push_temp(elem_width);
                        self.flush_stack_effects();
                    } else {
                        self.stack.push_temp(1);
                        self.flush_stack_effects();
                    }
                } else {
                    // Runtime index — use RAM-based access
                    self.emit_expr(&index.node);
                    let _idx_entry = self.stack.pop();
                    let arr_entry = self.stack.pop();

                    if let Some(arr) = arr_entry {
                        let array_width = arr.width;
                        let elem_width = arr.elem_width.unwrap_or(1);
                        let base = self.temp_ram_addr;
                        self.temp_ram_addr += array_width as u64;

                        // Store array elements to RAM
                        // Stack has: [... array_elems index]
                        // Save index, store array, restore index
                        self.b_swap(1); // move index below top array elem
                        for i in 0..array_width {
                            let addr = base + i as u64;
                            self.b_push(addr);
                            self.b_swap(1);
                            self.b_write_mem(1);
                            self.b_pop(1);
                            if i + 1 < array_width {
                                self.b_swap(1); // bring next array elem to top
                            }
                        }
                        // Stack now has: [... index]

                        // Compute target address: base + idx * elem_width
                        if elem_width > 1 {
                            self.b_push(elem_width as u64);
                            self.b_mul();
                        }
                        self.b_push(base);
                        self.b_add();

                        // Read elem_width elements from computed address
                        for i in 0..elem_width {
                            self.b_dup(0);
                            if i > 0 {
                                self.b_push(i as u64);
                                self.b_add();
                            }
                            self.b_read_mem(1);
                            self.b_pop(1);
                            self.b_swap(1);
                        }
                        self.b_pop(1); // pop address

                        self.stack.push_temp(elem_width);
                        self.flush_stack_effects();
                    } else {
                        self.stack.push_temp(1);
                        self.flush_stack_effects();
                    }
                }
            }
            Expr::StructInit { path: _, fields } => {
                let mut total_width = 0u32;
                for (_name, val) in fields {
                    self.emit_expr(&val.node);
                    if let Some(e) = self.stack.pop() {
                        total_width += e.width;
                    }
                }
                self.stack.push_temp(total_width);
                self.flush_stack_effects();
            }
        }
    }

    fn emit_call(
        &mut self,
        name: &str,
        generic_args: &[Spanned<ArraySize>],
        args: &[Spanned<Expr>],
    ) {
        // Evaluate arguments — each pushes a temp
        for arg in args {
            self.emit_expr(&arg.node);
        }

        // Pop all arg temps from the model
        let arg_count = args.len();
        for _ in 0..arg_count {
            self.stack.pop();
        }

        // Resolve intrinsic name: check if this function has an #[intrinsic] mapping.
        // For cross-module calls like "std_hash.tip5", extract the short name "tip5".
        let resolved_name = self.intrinsic_map.get(name).cloned().or_else(|| {
            // Cross-module: "module.func" → look up "func"
            name.rsplit('.')
                .next()
                .and_then(|short| self.intrinsic_map.get(short).cloned())
        });
        let effective_name = resolved_name.as_deref().unwrap_or(name);

        // Emit the instruction and push result temp
        match effective_name {
            // I/O
            "pub_read" => {
                let s = self.backend.inst_read_io(1);
                self.emit_and_push(&s, 1);
            }
            "pub_read2" => {
                let s = self.backend.inst_read_io(2);
                self.emit_and_push(&s, 2);
            }
            "pub_read3" => {
                let s = self.backend.inst_read_io(3);
                self.emit_and_push(&s, 3);
            }
            "pub_read4" => {
                let s = self.backend.inst_read_io(4);
                self.emit_and_push(&s, 4);
            }
            "pub_read5" => {
                let s = self.backend.inst_read_io(5);
                self.emit_and_push(&s, 5);
            }
            "pub_write" => {
                self.b_write_io(1);
                self.push_temp(0);
            }
            "pub_write2" => {
                self.b_write_io(2);
                self.push_temp(0);
            }
            "pub_write3" => {
                self.b_write_io(3);
                self.push_temp(0);
            }
            "pub_write4" => {
                self.b_write_io(4);
                self.push_temp(0);
            }
            "pub_write5" => {
                self.b_write_io(5);
                self.push_temp(0);
            }

            // Non-deterministic input
            "divine" => {
                let s = self.backend.inst_divine(1);
                self.emit_and_push(&s, 1);
            }
            "divine3" => {
                let s = self.backend.inst_divine(3);
                self.emit_and_push(&s, 3);
            }
            "divine5" => {
                let s = self.backend.inst_divine(5);
                self.emit_and_push(&s, 5);
            }

            // Assertions — consume arg, produce nothing
            "assert" => {
                self.b_assert();
                self.push_temp(0);
            }
            "assert_eq" => {
                self.b_eq();
                self.b_assert();
                self.push_temp(0);
            }
            "assert_digest" => {
                self.b_assert_vector();
                self.b_pop(5);
                self.push_temp(0);
            }

            // Field operations
            "field_add" => {
                self.b_add();
                self.push_temp(1);
            }
            "field_mul" => {
                self.b_mul();
                self.push_temp(1);
            }
            "inv" => {
                self.b_invert();
                self.push_temp(1);
            }
            "neg" => {
                self.b_push_neg_one();
                self.b_mul();
                self.push_temp(1);
            }
            "sub" => {
                self.b_push_neg_one();
                self.b_mul();
                self.b_add();
                self.push_temp(1);
            }

            // U32 operations
            "split" => {
                self.b_split();
                self.push_temp(2);
            }
            "log2" => {
                self.b_log2();
                self.push_temp(1);
            }
            "pow" => {
                self.b_pow();
                self.push_temp(1);
            }
            "popcount" => {
                self.b_pop_count();
                self.push_temp(1);
            }

            // Hash operations
            "hash" => {
                self.b_hash();
                self.push_temp(5);
            }
            "sponge_init" => {
                self.b_sponge_init();
                self.push_temp(0);
            }
            "sponge_absorb" => {
                self.b_sponge_absorb();
                self.push_temp(0);
            }
            "sponge_squeeze" => {
                let s = self.backend.inst_sponge_squeeze().to_string();
                self.emit_and_push(&s, 10);
            }
            "sponge_absorb_mem" => {
                self.b_sponge_absorb_mem();
                self.push_temp(0);
            }

            // Merkle
            "merkle_step" => {
                let s = self.backend.inst_merkle_step().to_string();
                self.emit_and_push(&s, 6);
            }
            "merkle_step_mem" => {
                let s = self.backend.inst_merkle_step_mem().to_string();
                self.emit_and_push(&s, 7);
            }

            // RAM
            "ram_read" => {
                self.b_read_mem(1);
                self.b_pop(1);
                self.push_temp(1);
            }
            "ram_write" => {
                self.b_write_mem(1);
                self.b_pop(1);
                self.push_temp(0);
            }
            "ram_read_block" => {
                // Read 5 consecutive elements (Digest-sized block)
                self.b_read_mem(5);
                self.b_pop(1);
                self.push_temp(5);
            }
            "ram_write_block" => {
                // Write 5 consecutive elements (Digest-sized block)
                self.b_write_mem(5);
                self.b_pop(1);
                self.push_temp(0);
            }

            // Conversion
            "as_u32" => {
                self.b_split();
                self.b_pop(1);
                self.push_temp(1);
            }
            "as_field" => {
                self.push_temp(1);
            }

            // XField
            "xfield" => {
                self.push_temp(3);
            }
            "xinvert" => {
                self.b_x_invert();
                self.push_temp(3);
            }
            "xx_dot_step" => {
                let s = self.backend.inst_xx_dot_step().to_string();
                self.emit_and_push(&s, 5);
            }
            "xb_dot_step" => {
                let s = self.backend.inst_xb_dot_step().to_string();
                self.emit_and_push(&s, 5);
            }

            // User-defined function
            _ => {
                // Check if this is a generic function call.
                let is_generic = self.generic_fn_defs.contains_key(name);

                let (call_label, base_name) = if is_generic {
                    // Resolve size args: explicit from call site, current_subs
                    // for calls inside generic bodies, or call_resolutions
                    // from the type checker for inferred calls.
                    let size_args: Vec<u64> = if !generic_args.is_empty() {
                        generic_args
                            .iter()
                            .map(|ga| ga.node.eval(&self.current_subs))
                            .collect()
                    } else if !self.current_subs.is_empty() {
                        // Inside a monomorphized body: resolve through current_subs.
                        if let Some(gdef) = self.generic_fn_defs.get(name) {
                            gdef.type_params
                                .iter()
                                .map(|p| self.current_subs.get(&p.node).copied().unwrap_or(0))
                                .collect()
                        } else {
                            vec![]
                        }
                    } else {
                        // Inferred call: consume from call_resolutions.
                        let idx = self.call_resolution_idx;
                        if idx < self.call_resolutions.len()
                            && self.call_resolutions[idx].name == name
                        {
                            self.call_resolution_idx += 1;
                            self.call_resolutions[idx].size_args.clone()
                        } else {
                            // Fallback: search for a matching resolution.
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
                    let mangled = inst.mangled_name();
                    let base = mangled.clone();
                    (mangled, base)
                } else if name.contains('.') {
                    // Cross-module call: "merkle.verify" → "call merkle__verify"
                    let parts: Vec<&str> = name.rsplitn(2, '.').collect();
                    let fn_name = parts[0];
                    let short_module = parts[1];
                    let full_module = self
                        .module_aliases
                        .get(short_module)
                        .map(|s| s.as_str())
                        .unwrap_or(short_module);
                    let mangled = full_module.replace('.', "_");
                    (format!("{}__{}", mangled, fn_name), fn_name.to_string())
                } else {
                    (format!("__{}", name), name.to_string())
                };
                let ret_width = self.fn_return_widths.get(&base_name).copied().unwrap_or(0);
                let call_inst = self.backend.inst_call(&call_label);
                if ret_width > 0 {
                    self.emit_and_push(&call_inst, ret_width);
                } else {
                    // Void function — emit call but don't push a stack entry
                    self.b_call(&call_label);
                    self.push_temp(0);
                }
            }
        }
    }

    // --- Stack/output helpers ---

    /// Drain any TASM instructions generated by stack spill/reload operations
    /// and append them to the output.
    fn flush_stack_effects(&mut self) {
        for inst in self.stack.drain_side_effects() {
            self.output.push(inst);
        }
    }

    /// Ensure stack space, flush spill effects, emit instruction, push temp to model.
    /// This is the correct ordering: spill BEFORE the physical push instruction.
    fn emit_and_push(&mut self, instruction: &str, result_width: u32) {
        if result_width > 0 {
            self.stack.ensure_space(result_width);
            self.flush_stack_effects();
        }
        self.inst(instruction);
        self.stack.push_temp(result_width);
        // push_temp's internal ensure_space is a no-op (space already ensured)
    }

    /// Push an anonymous temporary onto the stack model.
    /// For operations where the physical push already happened (e.g. assertions
    /// that consume a value and produce nothing — width 0).
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
    /// Returns (offset_from_top_of_struct, field_width).
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
        // Try to resolve from struct type definition
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
        // Fallback: assume each field is width 1
        vec![1u32; fields.len()]
    }

    fn emit_pop(&mut self, n: u32) {
        let mut remaining = n;
        while remaining > 0 {
            let batch = remaining.min(5);
            self.b_pop(batch);
            remaining -= batch;
        }
    }

    fn fresh_label(&mut self, prefix: &str) -> String {
        self.label_counter += 1;
        format!("__{}__{}", prefix, self.label_counter)
    }

    // ── Backend-delegating instruction helpers ──────────────────────
    fn b_push(&mut self, value: u64) {
        let s = self.backend.inst_push(value);
        self.inst(&s);
    }
    fn b_pop(&mut self, count: u32) {
        let s = self.backend.inst_pop(count);
        self.inst(&s);
    }
    fn b_dup(&mut self, depth: u32) {
        let s = self.backend.inst_dup(depth);
        self.inst(&s);
    }
    fn b_swap(&mut self, depth: u32) {
        let s = self.backend.inst_swap(depth);
        self.inst(&s);
    }
    fn b_push_neg_one(&mut self) {
        self.inst(self.backend.inst_push_neg_one());
    }
    fn b_add(&mut self) {
        self.inst(self.backend.inst_add());
    }
    fn b_mul(&mut self) {
        self.inst(self.backend.inst_mul());
    }
    fn b_eq(&mut self) {
        self.inst(self.backend.inst_eq());
    }
    fn b_lt(&mut self) {
        self.inst(self.backend.inst_lt());
    }
    fn b_and(&mut self) {
        self.inst(self.backend.inst_and());
    }
    fn b_xor(&mut self) {
        self.inst(self.backend.inst_xor());
    }
    fn b_div_mod(&mut self) {
        self.inst(self.backend.inst_div_mod());
    }
    fn b_xb_mul(&mut self) {
        self.inst(self.backend.inst_xb_mul());
    }
    fn b_invert(&mut self) {
        self.inst(self.backend.inst_invert());
    }
    fn b_x_invert(&mut self) {
        self.inst(self.backend.inst_x_invert());
    }
    fn b_split(&mut self) {
        self.inst(self.backend.inst_split());
    }
    fn b_log2(&mut self) {
        self.inst(self.backend.inst_log2());
    }
    fn b_pow(&mut self) {
        self.inst(self.backend.inst_pow());
    }
    fn b_pop_count(&mut self) {
        self.inst(self.backend.inst_pop_count());
    }
    fn b_skiz(&mut self) {
        self.inst(self.backend.inst_skiz());
    }
    fn b_assert(&mut self) {
        self.inst(self.backend.inst_assert());
    }
    fn b_assert_vector(&mut self) {
        self.inst(self.backend.inst_assert_vector());
    }
    fn b_hash(&mut self) {
        self.inst(self.backend.inst_hash());
    }
    fn b_sponge_init(&mut self) {
        self.inst(self.backend.inst_sponge_init());
    }
    fn b_sponge_absorb(&mut self) {
        self.inst(self.backend.inst_sponge_absorb());
    }
    #[allow(dead_code)]
    fn b_sponge_squeeze(&mut self) {
        self.inst(self.backend.inst_sponge_squeeze());
    }
    fn b_sponge_absorb_mem(&mut self) {
        self.inst(self.backend.inst_sponge_absorb_mem());
    }
    #[allow(dead_code)]
    fn b_merkle_step(&mut self) {
        self.inst(self.backend.inst_merkle_step());
    }
    #[allow(dead_code)]
    fn b_merkle_step_mem(&mut self) {
        self.inst(self.backend.inst_merkle_step_mem());
    }
    fn b_call(&mut self, label: &str) {
        let s = self.backend.inst_call(label);
        self.inst(&s);
    }
    fn b_return(&mut self) {
        self.inst(self.backend.inst_return());
    }
    fn b_recurse(&mut self) {
        self.inst(self.backend.inst_recurse());
    }
    #[allow(dead_code)]
    fn b_read_io(&mut self, count: u32) {
        let s = self.backend.inst_read_io(count);
        self.inst(&s);
    }
    fn b_write_io(&mut self, count: u32) {
        let s = self.backend.inst_write_io(count);
        self.inst(&s);
    }
    #[allow(dead_code)]
    fn b_divine(&mut self, count: u32) {
        let s = self.backend.inst_divine(count);
        self.inst(&s);
    }
    fn b_read_mem(&mut self, count: u32) {
        let s = self.backend.inst_read_mem(count);
        self.inst(&s);
    }
    fn b_write_mem(&mut self, count: u32) {
        let s = self.backend.inst_write_mem(count);
        self.inst(&s);
    }
    #[allow(dead_code)]
    fn b_xx_dot_step(&mut self) {
        self.inst(self.backend.inst_xx_dot_step());
    }
    #[allow(dead_code)]
    fn b_xb_dot_step(&mut self) {
        self.inst(self.backend.inst_xb_dot_step());
    }

    // ── Low-level output helpers ──────────────────────────────────

    fn inst(&mut self, instruction: &str) {
        self.output.push(format!("    {}", instruction));
    }

    fn raw(&mut self, line: &str) {
        self.output.push(line.to_string());
    }

    fn emit_label(&mut self, label: &str) {
        self.output.push(format!("{}:", label));
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
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    fn compile(source: &str) -> String {
        let (tokens, _, _) = Lexer::new(source, 0).tokenize();
        let file = Parser::new(tokens).parse_file().unwrap();
        Emitter::new().emit_file(&file)
    }

    #[test]
    fn test_minimal_program() {
        let tasm = compile("program test\nfn main() {\n}");
        assert!(tasm.contains("call __main"));
        assert!(tasm.contains("halt"));
        assert!(tasm.contains("__main:"));
        assert!(tasm.contains("return"));
    }

    #[test]
    fn test_pub_read_write() {
        let tasm = compile(
            "program test\nfn main() {\n    let a: Field = pub_read()\n    pub_write(a)\n}",
        );
        assert!(tasm.contains("read_io 1"));
        assert!(tasm.contains("dup 0")); // access a
        assert!(tasm.contains("write_io 1"));
    }

    #[test]
    fn test_field_arithmetic_stack_correctness() {
        let tasm = compile(
            "program test\nfn main() {\n    let a: Field = pub_read()\n    let b: Field = pub_read()\n    let c: Field = a + b\n    pub_write(c)\n}",
        );
        let lines: Vec<&str> = tasm.lines().collect();
        let read_io_count = lines.iter().filter(|l| l.contains("read_io 1")).count();
        assert_eq!(read_io_count, 2);
        assert!(tasm.contains("add"));
        assert!(tasm.contains("write_io 1"));
    }

    #[test]
    fn test_assert_eq() {
        let tasm = compile(
            "program test\nfn main() {\n    let a: Field = pub_read()\n    let b: Field = divine()\n    assert(a == b)\n}",
        );
        assert!(tasm.contains("read_io 1"));
        assert!(tasm.contains("divine 1"));
        assert!(tasm.contains("eq"));
        assert!(tasm.contains("assert"));
    }

    #[test]
    fn test_sum_check_program() {
        let tasm = compile(
            "program sum_check\nfn main() {\n    let a: Field = pub_read()\n    let b: Field = pub_read()\n    let sum: Field = a + b\n    pub_write(sum)\n    let expected: Field = divine()\n    assert(sum == expected)\n}",
        );
        eprintln!("=== TASM output ===\n{}", tasm);
        assert!(tasm.contains("read_io 1"));
        assert!(tasm.contains("add"));
        assert!(tasm.contains("write_io 1"));
        assert!(tasm.contains("divine 1"));
        assert!(tasm.contains("eq"));
        assert!(tasm.contains("assert"));
    }

    #[test]
    fn test_user_function_call() {
        let tasm = compile(
            "program test\nfn add(a: Field, b: Field) -> Field {\n    a + b\n}\nfn main() {\n    let x: Field = pub_read()\n    let y: Field = pub_read()\n    let z: Field = add(x, y)\n    pub_write(z)\n}",
        );
        assert!(tasm.contains("call __add"));
        assert!(tasm.contains("__add:"));
    }

    #[test]
    fn test_function_return_via_tail_expr() {
        let tasm = compile(
            "program test\nfn double(x: Field) -> Field {\n    x + x\n}\nfn main() {\n    let a: Field = pub_read()\n    let b: Field = double(a)\n    pub_write(b)\n}",
        );
        assert!(tasm.contains("__double:"));
        assert!(tasm.contains("add"));
        assert!(tasm.contains("swap 1"));
    }

    #[test]
    fn test_cross_module_call_emission() {
        let tasm = compile(
            "program test\nfn main() {\n    let a: Field = pub_read()\n    let b: Field = helpers.double(a)\n    pub_write(b)\n}",
        );
        assert!(tasm.contains("call helpers__double"));
    }

    #[test]
    fn test_struct_init_emission() {
        let tasm = compile(
            "program test\nstruct Point {\n    x: Field,\n    y: Field,\n}\nfn main() {\n    let a: Field = pub_read()\n    let b: Field = pub_read()\n    let p: Point = Point { x: a, y: b }\n    pub_write(p.x)\n}",
        );
        eprintln!("=== struct TASM ===\n{}", tasm);
        assert!(tasm.contains("read_io 1"));
        assert!(tasm.contains("write_io 1"));
    }

    #[test]
    fn test_array_index_emission() {
        let tasm = compile(
            "program test\nfn main() {\n    let a: Field = pub_read()\n    let b: Field = pub_read()\n    let c: Field = pub_read()\n    let arr: [Field; 3] = [a, b, c]\n    pub_write(arr[0])\n}",
        );
        eprintln!("=== array TASM ===\n{}", tasm);
        assert!(tasm.contains("read_io 1"));
        assert!(tasm.contains("write_io 1"));
    }

    #[test]
    fn test_module_no_entry_wrapper() {
        let (tokens, _, _) = Lexer::new(
            "module helpers\npub fn add(a: Field, b: Field) -> Field {\n    a + b\n}",
            0,
        )
        .tokenize();
        let file = Parser::new(tokens).parse_file().unwrap();
        let tasm = Emitter::new().emit_file(&file);
        let first_line = tasm.lines().next().unwrap_or("").trim();
        assert_ne!(first_line, "call __main");
        assert!(!tasm.starts_with("    call __main"));
    }

    #[test]
    fn test_digest_variable_access() {
        // Digest variables (width 5) should dup all 5 elements
        let tasm = compile(
            "program test\nfn main() {\n    let d: Digest = divine5()\n    let e: Digest = pub_read5()\n    assert_digest(d, e)\n}",
        );
        eprintln!("=== digest TASM ===\n{}", tasm);
        assert!(tasm.contains("divine 5"));
        assert!(tasm.contains("read_io 5"));
        // Accessing d (width 5) should produce 5 dup instructions, not 1
        let dup_count = tasm.lines().filter(|l| l.trim().starts_with("dup")).count();
        assert!(
            dup_count >= 10,
            "expected at least 10 dups for two Digest variable accesses, got {}",
            dup_count
        );
        assert!(tasm.contains("assert_vector"));
    }

    #[test]
    fn test_user_fn_returning_digest() {
        // User function returning Digest should have correct return width
        let tasm = compile(
            "program test\nfn make_digest() -> Digest {\n    divine5()\n}\nfn main() {\n    let d: Digest = make_digest()\n    let e: Digest = divine5()\n    assert_digest(d, e)\n}",
        );
        eprintln!("=== fn-digest TASM ===\n{}", tasm);
        assert!(tasm.contains("call __make_digest"));
        assert!(tasm.contains("assert_vector"));
    }

    #[test]
    fn test_spill_with_many_variables() {
        // Create a program with >16 live Field variables to trigger stack spilling
        let mut src = String::from("program test\nfn main() {\n");
        for i in 0..18 {
            src.push_str(&format!("    let v{}: Field = pub_read()\n", i));
        }
        // Access an early variable after many others to trigger reload
        src.push_str("    pub_write(v0)\n");
        src.push_str("}\n");

        let tasm = compile(&src);
        eprintln!("=== spill TASM ===\n{}", tasm);

        // Should contain spill instructions (write_mem to high RAM addresses)
        assert!(
            tasm.contains("write_mem"),
            "expected spill write_mem instructions"
        );
        // The output should still have all 18 read_io instructions
        let read_count = tasm.lines().filter(|l| l.contains("read_io 1")).count();
        assert_eq!(read_count, 18);
        // And the final write_io
        assert!(tasm.contains("write_io 1"));
    }

    #[test]
    fn test_emit_tasm() {
        let tasm = compile(
            "program test\nevent Transfer { from: Field, to: Field, amount: Field }\nfn main() {\n    let a: Field = pub_read()\n    let b: Field = pub_read()\n    let c: Field = pub_read()\n    emit Transfer { from: a, to: b, amount: c }\n}",
        );
        eprintln!("=== emit TASM ===\n{}", tasm);
        // Tag (push 0) + write_io 1 for tag + 3 field write_io 1s = 4 write_io 1s total
        let write_io_1_count = tasm.lines().filter(|l| l.trim() == "write_io 1").count();
        assert!(
            write_io_1_count >= 4,
            "expected at least 4 write_io 1 for emit, got {}",
            write_io_1_count
        );
        // No hash instruction for open emit
        let hash_in_main = tasm
            .lines()
            .skip_while(|l| !l.contains("__main:"))
            .take_while(|l| !l.trim().starts_with("return"))
            .filter(|l| l.trim() == "hash")
            .count();
        assert_eq!(hash_in_main, 0, "open emit should not hash");
    }

    #[test]
    fn test_seal_tasm() {
        let tasm = compile(
            "program test\nevent Nullifier { id: Field, nonce: Field }\nfn main() {\n    let x: Field = pub_read()\n    let y: Field = pub_read()\n    seal Nullifier { id: x, nonce: y }\n}",
        );
        eprintln!("=== seal TASM ===\n{}", tasm);
        // Seal should produce hash + write_io 5
        assert!(tasm.contains("hash"), "seal should contain hash");
        assert!(
            tasm.contains("write_io 5"),
            "seal should write_io 5 for digest"
        );
    }

    #[test]
    fn test_multi_width_array_element() {
        // Array of Digest (width 5 per element)
        let tasm = compile(
            "program test\nfn main() {\n    let a: Digest = divine5()\n    let b: Digest = divine5()\n    let arr: [Digest; 2] = [a, b]\n    let first: Digest = arr[0]\n    let second: Digest = arr[1]\n    assert_digest(first, second)\n}",
        );
        eprintln!("=== multi-width array TASM ===\n{}", tasm);
        assert!(!tasm.contains("ERROR"), "should not have errors");
        assert!(tasm.contains("assert_vector"));
    }

    #[test]
    fn test_runtime_array_index() {
        // Access array element with a runtime-computed index
        let tasm = compile(
            "program test\nfn main() {\n    let a: Field = pub_read()\n    let b: Field = pub_read()\n    let c: Field = pub_read()\n    let arr: [Field; 3] = [a, b, c]\n    let idx: Field = pub_read()\n    let val: Field = arr[idx]\n    pub_write(val)\n}",
        );
        eprintln!("=== runtime index TASM ===\n{}", tasm);
        assert!(!tasm.contains("ERROR"), "should not have errors");
        // Runtime indexing uses RAM: write_mem to store, read_mem to load
        assert!(tasm.contains("write_mem"));
        assert!(tasm.contains("read_mem"));
    }

    #[test]
    fn test_deep_variable_access_spill() {
        // Access a variable when the stack is deeply loaded (>16 elements)
        // The stack manager should spill/reload automatically
        let tasm = compile(
            "program test\nfn main() {\n    let a: Field = pub_read()\n    let b: Field = pub_read()\n    let c: Field = pub_read()\n    let d: Field = pub_read()\n    let e: Field = pub_read()\n    let f: Field = pub_read()\n    let g: Field = pub_read()\n    let h: Field = pub_read()\n    let i: Field = pub_read()\n    let j: Field = pub_read()\n    let k: Field = pub_read()\n    let l: Field = pub_read()\n    let m: Field = pub_read()\n    let n: Field = pub_read()\n    let o: Field = pub_read()\n    let p: Field = pub_read()\n    let q: Field = pub_read()\n    pub_write(a)\n    pub_write(q)\n}",
        );
        eprintln!("=== deep access TASM ===\n{}", tasm);
        assert!(
            !tasm.contains("ERROR"),
            "deep variable should be accessible via spill/reload"
        );
        // Should have spill instructions (write_mem for eviction)
        assert!(tasm.contains("write_mem"), "expected spill to RAM");
    }

    #[test]
    fn test_struct_field_from_fn_return() {
        // Struct field access on a value returned from a function call
        let tasm = compile(
            "program test\nstruct Point {\n    x: Field,\n    y: Field,\n}\nfn make_point(a: Field, b: Field) -> Point {\n    Point { x: a, y: b }\n}\nfn main() {\n    let a: Field = pub_read()\n    let b: Field = pub_read()\n    let p: Point = make_point(a, b)\n    pub_write(p.x)\n    pub_write(p.y)\n}",
        );
        eprintln!("=== struct fn return TASM ===\n{}", tasm);
        assert!(!tasm.contains("ERROR"), "should not have unresolved field");
        assert!(tasm.contains("write_io 1"));
    }

    #[test]
    fn test_sec_ram_emission() {
        // sec ram declarations should produce metadata comments in TASM
        let tasm = compile(
            "program test\n\nsec ram: {\n    17: Field,\n    42: Field,\n}\n\nfn main() {\n    let v: Field = ram_read(17)\n    pub_write(v)\n}",
        );
        eprintln!("=== sec ram TASM ===\n{}", tasm);
        assert!(tasm.contains("sec ram"), "should have sec ram comment");
        assert!(tasm.contains("ram[17]"), "should document address 17");
        assert!(tasm.contains("ram[42]"), "should document address 42");
    }

    #[test]
    fn test_digest_destructuring() {
        // Decompose a Digest into 5 individual Field variables
        let tasm = compile(
            "program test\nfn main() {\n    let d: Digest = divine5()\n    let (f0, f1, f2, f3, f4) = d\n    pub_write(f0)\n    pub_write(f4)\n}",
        );
        eprintln!("=== digest destructure TASM ===\n{}", tasm);
        assert!(tasm.contains("divine 5"));
        // After destructuring, each field should be accessible as width-1 var
        assert!(tasm.contains("write_io 1"));
    }

    #[test]
    fn test_digest_destructure_and_pass_to_hash() {
        // Decompose a Digest, then pass fields to hash()
        let tasm = compile(
            "program test\nfn main() {\n    let d: Digest = divine5()\n    let (f0, f1, f2, f3, f4) = d\n    let h: Digest = hash(f0, f1, f2, f3, f4, 0, 0, 0, 0, 0)\n    let e: Digest = divine5()\n    assert_digest(h, e)\n}",
        );
        eprintln!("=== digest decompose+hash TASM ===\n{}", tasm);
        assert!(tasm.contains("divine 5"));
        assert!(tasm.contains("hash"));
        assert!(tasm.contains("assert_vector"));
    }

    #[test]
    fn test_asm_block_emits_raw_tasm() {
        let tasm = compile(
            "program test\nfn main() {\n    asm(+1) { push 42 }\n    asm(-1) { write_io 1 }\n}",
        );
        eprintln!("=== asm TASM ===\n{}", tasm);
        assert!(tasm.contains("push 42"), "raw asm should appear in output");
        assert!(
            tasm.contains("write_io 1"),
            "raw asm should appear in output"
        );
    }

    #[test]
    fn test_asm_block_with_negative_push() {
        // TASM allows `push -1` but Trident doesn't have negative literals
        let tasm = compile("program test\nfn main() {\n    asm { push -1\nadd }\n}");
        eprintln!("=== asm negative TASM ===\n{}", tasm);
        assert!(tasm.contains("push -1"));
        assert!(tasm.contains("add"));
    }

    #[test]
    fn test_asm_spills_variables_before_block() {
        // Variables should be spilled to RAM before asm block executes
        let mut src = String::from("program test\nfn main() {\n");
        for i in 0..5 {
            src.push_str(&format!("    let v{}: Field = pub_read()\n", i));
        }
        src.push_str("    asm { push 99 }\n");
        src.push_str("}\n");

        let tasm = compile(&src);
        eprintln!("=== asm spill TASM ===\n{}", tasm);
        // The asm instruction should be present
        assert!(tasm.contains("push 99"));
        // Variables should be spilled before the asm block
        assert!(tasm.contains("write_mem"), "expected spill before asm");
    }

    #[test]
    fn test_asm_net_zero_effect() {
        // asm with net-zero effect: stack model unchanged
        let tasm = compile(
            "program test\nfn main() {\n    let x: Field = pub_read()\n    asm { dup 0\npop 1 }\n    pub_write(x)\n}",
        );
        eprintln!("=== asm zero-effect TASM ===\n{}", tasm);
        assert!(tasm.contains("dup 0"));
        assert!(!tasm.contains("ERROR"));
    }

    // --- Size-generic function emission tests ---

    /// Full pipeline: parse → typecheck → emit (needed for generic functions).
    fn compile_full(source: &str) -> String {
        crate::compile(source, "test.tri").expect("compilation should succeed")
    }

    #[test]
    fn test_generic_fn_emits_mangled_label() {
        let tasm = compile_full(
            "program test\nfn first<N>(arr: [Field; N]) -> Field {\n    arr[0]\n}\nfn main() {\n    let a: [Field; 3] = [1, 2, 3]\n    let s: Field = first<3>(a)\n    pub_write(s)\n}",
        );
        eprintln!("=== generic TASM ===\n{}", tasm);
        // Should have mangled label for first with N=3
        assert!(
            tasm.contains("__first__N3:"),
            "should emit mangled label __first__N3"
        );
        assert!(
            tasm.contains("call __first__N3"),
            "should call mangled label"
        );
    }

    #[test]
    fn test_generic_fn_two_instantiations() {
        let tasm = compile_full(
            "program test\nfn first<N>(arr: [Field; N]) -> Field {\n    arr[0]\n}\nfn main() {\n    let a: [Field; 3] = [1, 2, 3]\n    let b: [Field; 5] = [1, 2, 3, 4, 5]\n    let x: Field = first<3>(a)\n    let y: Field = first<5>(b)\n    pub_write(x + y)\n}",
        );
        eprintln!("=== two instantiations TASM ===\n{}", tasm);
        assert!(tasm.contains("__first__N3:"), "should emit first<3>");
        assert!(tasm.contains("__first__N5:"), "should emit first<5>");
        assert!(tasm.contains("call __first__N3"), "should call first<3>");
        assert!(tasm.contains("call __first__N5"), "should call first<5>");
    }

    #[test]
    fn test_generic_fn_inferred_emission() {
        let tasm = compile_full(
            "program test\nfn first<N>(arr: [Field; N]) -> Field {\n    arr[0]\n}\nfn main() {\n    let a: [Field; 3] = [1, 2, 3]\n    let s: Field = first(a)\n    pub_write(s)\n}",
        );
        eprintln!("=== inferred generic TASM ===\n{}", tasm);
        // Inferred N=3 from [Field; 3]
        assert!(
            tasm.contains("__first__N3:"),
            "should emit __first__N3 via inference"
        );
        assert!(tasm.contains("call __first__N3"), "should call __first__N3");
    }

    #[test]
    fn test_generic_fn_not_emitted_as_regular() {
        let tasm = compile_full(
            "program test\nfn first<N>(arr: [Field; N]) -> Field {\n    arr[0]\n}\nfn main() {\n    let a: [Field; 3] = [1, 2, 3]\n    let s: Field = first<3>(a)\n    pub_write(s)\n}",
        );
        // Generic function should NOT be emitted with the un-mangled label
        assert!(
            !tasm.contains("\n__first:"),
            "generic fn should not emit un-mangled label"
        );
    }

    // ─── Cross-Target Tests ────────────────────────────────────────

    fn compile_with_target(source: &str, target_name: &str) -> String {
        let config = TargetConfig::resolve(target_name).unwrap_or_else(|_| TargetConfig::triton());
        let backend = create_backend(target_name);
        let (tokens, _, _) = Lexer::new(source, 0).tokenize();
        let file = Parser::new(tokens).parse_file().unwrap();
        Emitter::with_backend(backend, config).emit_file(&file)
    }

    #[test]
    fn test_backend_factory_triton() {
        let backend = create_backend("triton");
        assert_eq!(backend.target_name(), "triton");
        assert_eq!(backend.output_extension(), ".tasm");
    }

    #[test]
    fn test_backend_factory_miden() {
        let backend = create_backend("miden");
        assert_eq!(backend.target_name(), "miden");
        assert_eq!(backend.output_extension(), ".masm");
    }

    #[test]
    fn test_backend_factory_openvm() {
        let backend = create_backend("openvm");
        assert_eq!(backend.target_name(), "openvm");
        assert_eq!(backend.output_extension(), ".S");
    }

    #[test]
    fn test_backend_factory_sp1() {
        let backend = create_backend("sp1");
        assert_eq!(backend.target_name(), "sp1");
        assert_eq!(backend.output_extension(), ".S");
    }

    #[test]
    fn test_backend_factory_cairo() {
        let backend = create_backend("cairo");
        assert_eq!(backend.target_name(), "cairo");
        assert_eq!(backend.output_extension(), ".sierra");
    }

    #[test]
    fn test_backend_factory_unknown_falls_back() {
        let backend = create_backend("unknown");
        assert_eq!(backend.target_name(), "triton");
    }

    #[test]
    fn test_triton_instructions() {
        let b = TritonBackend;
        assert_eq!(b.inst_push(42), "push 42");
        assert_eq!(b.inst_pop(1), "pop 1");
        assert_eq!(b.inst_dup(0), "dup 0");
        assert_eq!(b.inst_swap(1), "swap 1");
        assert_eq!(b.inst_add(), "add");
        assert_eq!(b.inst_mul(), "mul");
        assert_eq!(b.inst_call("foo"), "call foo");
        assert_eq!(b.inst_return(), "return");
    }

    #[test]
    fn test_miden_instructions() {
        let b = MidenBackend;
        assert_eq!(b.inst_push(42), "push.42");
        assert_eq!(b.inst_pop(1), "drop");
        assert_eq!(b.inst_dup(0), "dup.0");
        assert!(b.inst_swap(3).contains("movup"));
        assert_eq!(b.inst_add(), "add");
        assert_eq!(b.inst_mul(), "mul");
        assert_eq!(b.inst_call("foo"), "exec.foo");
        assert_eq!(b.inst_return(), "end");
    }

    #[test]
    fn test_openvm_instructions() {
        let b = OpenVMBackend;
        assert!(b.inst_push(42).contains("li"));
        assert!(b.inst_add().contains("add"));
        assert!(b.inst_call("foo").contains("jal"));
    }

    #[test]
    fn test_sp1_instructions() {
        let b = SP1Backend;
        assert!(b.inst_push(42).contains("li"));
        assert!(b.inst_add().contains("add"));
        assert_eq!(b.inst_return(), "ret");
    }

    #[test]
    fn test_cairo_instructions() {
        let b = CairoBackend;
        assert!(b.inst_push(42).contains("felt252_const<42>"));
        assert!(b.inst_add().contains("felt252_add"));
        assert!(b.inst_mul().contains("felt252_mul"));
        assert!(b.inst_call("foo").contains("function_call"));
        assert_eq!(b.inst_return(), "return([0])");
    }

    #[test]
    fn test_compile_minimal_triton() {
        let out = compile_with_target("program test\nfn main() {\n}", "triton");
        assert!(out.contains("call __main"));
    }

    #[test]
    fn test_compile_minimal_miden() {
        let out = compile_with_target("program test\nfn main() {\n}", "miden");
        assert!(out.contains("exec.__main"));
    }

    #[test]
    fn test_compile_minimal_openvm() {
        let out = compile_with_target("program test\nfn main() {\n}", "openvm");
        assert!(out.contains("jal ra, __main"));
    }

    #[test]
    fn test_compile_minimal_sp1() {
        let out = compile_with_target("program test\nfn main() {\n}", "sp1");
        assert!(out.contains("jal ra, __main"));
    }

    #[test]
    fn test_compile_minimal_cairo() {
        let out = compile_with_target("program test\nfn main() {\n}", "cairo");
        assert!(out.contains("function_call<__main>"));
    }

    #[test]
    fn test_all_targets_produce_output() {
        let source = "program test\nfn main() {\n  let x: Field = 42\n}";
        for target in &["triton", "miden", "openvm", "sp1", "cairo"] {
            let out = compile_with_target(source, target);
            assert!(!out.is_empty(), "target {} produced empty output", target);
        }
    }

    // ─── Cross-Target Integration Tests ─────────────────────────────

    #[test]
    fn test_cross_target_arithmetic() {
        let source = "program test\nfn main() {\n  let a: Field = 10\n  let b: Field = 20\n  let c: Field = a + b\n  let d: Field = a * b\n  let e: Field = d + c\n  pub_write(e)\n}";
        for target in &["triton", "miden", "openvm", "sp1", "cairo"] {
            let out = compile_with_target(source, target);
            assert!(!out.is_empty(), "{}: empty output for arithmetic", target);
            // Each target should have its add and mul instructions
            match *target {
                "triton" => {
                    assert!(out.contains("add"), "{}: missing add", target);
                    assert!(out.contains("mul"), "{}: missing mul", target);
                }
                "miden" => {
                    assert!(out.contains("add"), "{}: missing add", target);
                    assert!(out.contains("mul"), "{}: missing mul", target);
                }
                "openvm" | "sp1" => {
                    assert!(out.contains("add"), "{}: missing add", target);
                    assert!(out.contains("mul"), "{}: missing mul", target);
                }
                "cairo" => {
                    assert!(out.contains("felt252_add"), "{}: missing add", target);
                    assert!(out.contains("felt252_mul"), "{}: missing mul", target);
                }
                _ => {}
            }
        }
    }

    #[test]
    fn test_cross_target_control_flow() {
        let source = "program test\nfn main() {\n  let x: Field = pub_read()\n  if x == 0 {\n    pub_write(1)\n  } else {\n    pub_write(2)\n  }\n}";
        for target in &["triton", "miden", "openvm", "sp1", "cairo"] {
            let out = compile_with_target(source, target);
            assert!(!out.is_empty(), "{}: empty output for control flow", target);
            // All targets should have labels for branching
            assert!(
                out.contains("__main"),
                "{}: missing main label in control flow",
                target
            );
        }
    }

    #[test]
    fn test_cross_target_function_calls() {
        let source = "program test\nfn add_one(x: Field) -> Field {\n  x + 1\n}\nfn main() {\n  let r: Field = add_one(41)\n  pub_write(r)\n}";
        for target in &["triton", "miden", "openvm", "sp1", "cairo"] {
            let out = compile_with_target(source, target);
            assert!(!out.is_empty(), "{}: empty for function calls", target);
            // Should have both main and add_one labels
            assert!(out.contains("__main"), "{}: missing __main", target);
            assert!(out.contains("__add_one"), "{}: missing __add_one", target);
        }
    }

    #[test]
    fn test_cross_target_loops() {
        let source = "program test\nfn main() {\n  let n: Field = 5\n  let mut sum: Field = 0\n  for i in 0..n bounded 10 {\n    sum = sum + i\n  }\n  pub_write(sum)\n}";
        for target in &["triton", "miden", "openvm", "sp1", "cairo"] {
            let out = compile_with_target(source, target);
            assert!(!out.is_empty(), "{}: empty for loops", target);
            // Loops desugar to labels and jumps in all targets
            assert!(
                out.contains("__main"),
                "{}: missing main in loop test",
                target
            );
        }
    }

    #[test]
    fn test_cross_target_io() {
        let source = "program test\nfn main() {\n  let x: Field = pub_read()\n  let y: Field = pub_read()\n  pub_write(x + y)\n}";
        for target in &["triton", "miden", "openvm", "sp1", "cairo"] {
            let out = compile_with_target(source, target);
            assert!(!out.is_empty(), "{}: empty for IO", target);
            match *target {
                "triton" => {
                    assert!(out.contains("read_io"), "{}: missing read_io", target);
                    assert!(out.contains("write_io"), "{}: missing write_io", target);
                }
                "miden" => {
                    assert!(
                        out.contains("sdepth") && out.contains("drop"),
                        "{}: missing miden IO pattern",
                        target
                    );
                }
                "openvm" | "sp1" => {
                    assert!(out.contains("ecall"), "{}: missing ecall for IO", target);
                }
                "cairo" => {
                    assert!(
                        out.contains("input") || out.contains("output"),
                        "{}: missing cairo IO",
                        target
                    );
                }
                _ => {}
            }
        }
    }

    #[test]
    fn test_cross_target_events() {
        let source = "program test\nevent Transfer {\n  amount: Field,\n}\nfn main() {\n  emit Transfer { amount: 100 }\n}";
        for target in &["triton", "miden", "openvm", "sp1", "cairo"] {
            let out = compile_with_target(source, target);
            assert!(!out.is_empty(), "{}: empty for events", target);
        }
    }

    #[test]
    fn test_cross_target_multiple_functions() {
        let source = "program test\nfn double(x: Field) -> Field {\n  x * 2\n}\nfn triple(x: Field) -> Field {\n  x * 3\n}\nfn main() {\n  let a: Field = double(5)\n  let b: Field = triple(5)\n  pub_write(a + b)\n}";
        for target in &["triton", "miden", "openvm", "sp1", "cairo"] {
            let out = compile_with_target(source, target);
            assert!(out.contains("__double"), "{}: missing __double", target);
            assert!(out.contains("__triple"), "{}: missing __triple", target);
            assert!(out.contains("__main"), "{}: missing __main", target);
        }
    }

    #[test]
    fn test_cross_target_u32_operations() {
        let source = "program test\nfn main() {\n  let a: U32 = as_u32(10)\n  let b: U32 = as_u32(20)\n  if a < b {\n    pub_write(1)\n  } else {\n    pub_write(0)\n  }\n}";
        for target in &["triton", "miden", "openvm", "sp1", "cairo"] {
            let out = compile_with_target(source, target);
            assert!(!out.is_empty(), "{}: empty for U32 ops", target);
        }
    }

    #[test]
    fn test_cross_target_output_size_comparison() {
        // Benchmark: same program compiled to all targets — compare sizes
        let source = "program test\nfn fib(n: Field) -> Field {\n  let mut a: Field = 0\n  let mut b: Field = 1\n  for i in 0..n bounded 20 {\n    let t: Field = b\n    b = a + b\n    a = t\n  }\n  a\n}\nfn main() {\n  let r: Field = fib(10)\n  pub_write(r)\n}";
        let mut sizes: Vec<(&str, usize)> = Vec::new();
        for target in &["triton", "miden", "openvm", "sp1", "cairo"] {
            let out = compile_with_target(source, target);
            assert!(!out.is_empty(), "{}: empty for fib benchmark", target);
            sizes.push((target, out.len()));
        }
        // All targets should produce non-trivial output
        for (target, size) in &sizes {
            assert!(*size > 50, "{}: output too small ({})", target, size);
        }
        // Sanity: outputs should differ between target families
        let triton_size = sizes[0].1;
        let cairo_size = sizes[4].1;
        assert_ne!(
            triton_size, cairo_size,
            "triton and cairo should produce different-sized output"
        );
    }

    #[test]
    fn test_cross_target_nested_calls() {
        let source = "program test\nfn inc(x: Field) -> Field {\n  x + 1\n}\nfn add_two(x: Field) -> Field {\n  inc(inc(x))\n}\nfn main() {\n  pub_write(add_two(40))\n}";
        for target in &["triton", "miden", "openvm", "sp1", "cairo"] {
            let out = compile_with_target(source, target);
            assert!(out.contains("__inc"), "{}: missing __inc", target);
            assert!(out.contains("__add_two"), "{}: missing __add_two", target);
        }
    }

    #[test]
    fn test_cross_target_struct() {
        let source = "program test\nstruct Point {\n  x: Field,\n  y: Field,\n}\nfn origin() -> Point {\n  Point { x: 0, y: 0 }\n}\nfn main() {\n  let p: Point = origin()\n  pub_write(p.x)\n  pub_write(p.y)\n}";
        for target in &["triton", "miden", "openvm", "sp1", "cairo"] {
            let out = compile_with_target(source, target);
            assert!(!out.is_empty(), "{}: empty for struct test", target);
            assert!(out.contains("__origin"), "{}: missing __origin", target);
        }
    }

    #[test]
    fn test_cross_target_mutable_variables() {
        let source = "program test\nfn main() {\n  let mut x: Field = 0\n  x = x + 1\n  x = x + 2\n  x = x + 3\n  pub_write(x)\n}";
        for target in &["triton", "miden", "openvm", "sp1", "cairo"] {
            let out = compile_with_target(source, target);
            assert!(!out.is_empty(), "{}: empty for mutable vars", target);
        }
    }

    #[test]
    fn test_cross_target_divine() {
        let source = "program test\nfn main() {\n  let secret: Field = divine()\n  let d: Digest = divine5()\n  let (a, b, c, e, f) = d\n  pub_write(secret + a)\n}";
        for target in &["triton", "miden", "openvm", "sp1", "cairo"] {
            let out = compile_with_target(source, target);
            assert!(!out.is_empty(), "{}: empty for divine test", target);
        }
    }

    #[test]
    fn test_cross_target_hash() {
        let source = "program test\nfn main() {\n  let d: Digest = hash(1, 2, 3, 4, 5, 6, 7, 8, 9, 0)\n  let (a, b, c, e, f) = d\n  pub_write(a)\n}";
        for target in &["triton", "miden", "openvm", "sp1", "cairo"] {
            let out = compile_with_target(source, target);
            assert!(!out.is_empty(), "{}: empty for hash test", target);
        }
    }

    #[test]
    fn test_cross_target_seal() {
        let source = "program test\nevent Secret {\n  val: Field,\n}\nfn main() {\n  seal Secret { val: 42 }\n}";
        for target in &["triton", "miden", "openvm", "sp1", "cairo"] {
            let out = compile_with_target(source, target);
            assert!(!out.is_empty(), "{}: empty for seal test", target);
        }
    }
}
