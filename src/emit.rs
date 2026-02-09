use std::collections::HashMap;

use crate::ast::*;
use crate::span::Spanned;
use crate::stack::StackManager;

/// A deferred block to emit after the current function.
struct DeferredBlock {
    label: String,
    block: Block,
    /// If true, this is a "then" branch that must set the flag to 0 for if/else.
    clears_flag: bool,
}

/// TASM emitter — walks the AST and produces Triton Assembly.
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
}

impl Default for Emitter {
    fn default() -> Self {
        Self::new()
    }
}

impl Emitter {
    pub fn new() -> Self {
        Self {
            output: Vec::new(),
            label_counter: 0,
            stack: StackManager::new(),
            deferred: Vec::new(),
            struct_layouts: HashMap::new(),
            fn_return_widths: HashMap::new(),
            event_tags: HashMap::new(),
            event_defs: HashMap::new(),
            struct_types: HashMap::new(),
            constants: HashMap::new(),
            temp_ram_addr: 1 << 29,
            intrinsic_map: HashMap::new(),
            module_aliases: HashMap::new(),
        }
    }

    /// Set a pre-built intrinsic map (from all project modules).
    pub fn with_intrinsics(mut self, map: HashMap<String, String>) -> Self {
        self.intrinsic_map = map;
        self
    }

    /// Set module alias map (short name → full dotted name).
    pub fn with_module_aliases(mut self, aliases: HashMap<String, String>) -> Self {
        self.module_aliases = aliases;
        self
    }

    /// Set external constants (from imported modules).
    pub fn with_constants(mut self, constants: HashMap<String, u64>) -> Self {
        self.constants.extend(constants);
        self
    }

    pub fn emit_file(mut self, file: &File) -> String {
        // Pre-scan: collect return widths for all user-defined functions.
        for item in &file.items {
            if let Item::Fn(func) = &item.node {
                let width = func
                    .return_ty
                    .as_ref()
                    .map(|t| resolve_type_width(&t.node))
                    .unwrap_or(0);
                self.fn_return_widths.insert(func.name.node.clone(), width);
            }
        }

        // Pre-scan: collect intrinsic mappings.
        for item in &file.items {
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
            if let Item::Struct(sdef) = &item.node {
                self.struct_types
                    .insert(sdef.name.node.clone(), sdef.clone());
            }
        }

        // Pre-scan: collect constant values.
        for item in &file.items {
            if let Item::Const(cdef) = &item.node {
                if let Expr::Literal(Literal::Integer(val)) = &cdef.value.node {
                    self.constants.insert(cdef.name.node.clone(), *val);
                }
            }
        }

        // Pre-scan: assign sequential tags to events.
        let mut event_tag = 0u64;
        for item in &file.items {
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
                        resolve_type_width(&ty.node),
                        if resolve_type_width(&ty.node) == 1 {
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
            self.raw("    call __main");
            self.raw("    halt");
            self.raw("");
        }

        for item in &file.items {
            if let Item::Fn(func) = &item.node {
                self.emit_fn(func);
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
            let width = resolve_type_width(&param.ty.node);
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
                .map(|t| resolve_type_width(&t.node))
                .unwrap_or(0);
            let to_pop = total_width.saturating_sub(ret_width);
            for _ in 0..to_pop {
                self.inst("swap 1");
                self.inst("pop 1");
            }
        } else if !has_return {
            self.emit_pop(total_width);
        }

        self.inst("return");
        self.raw("");

        // Emit deferred blocks
        self.flush_deferred();
        self.stack.clear();
    }

    fn flush_deferred(&mut self) {
        while !self.deferred.is_empty() {
            let deferred = std::mem::take(&mut self.deferred);
            for block in deferred {
                self.emit_label(&block.label);
                if block.clears_flag {
                    self.inst("pop 1");
                }
                self.emit_block(&block.block);
                if block.clears_flag {
                    self.inst("push 0");
                }
                self.inst("return");
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
                                    let ew = resolve_type_width(inner_ty);
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
                        self.inst(&format!("swap {}", depth));
                        self.inst("pop 1");
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

                    self.inst("push 1");
                    self.inst("swap 1");
                    self.inst("skiz");
                    self.inst(&format!("call {}", then_label));
                    self.inst("skiz");
                    self.inst(&format!("call {}", else_label));

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
                    self.inst("skiz");
                    self.inst(&format!("call {}", then_label));

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

                self.inst(&format!("call {}", loop_label));
                self.inst("pop 1");
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
                            self.inst(&format!("swap {}", depth));
                            self.inst("pop 1");
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
                self.inst(&format!("push {}", tag));
                self.inst("write_io 1");

                // Emit each field in declaration order, write one at a time
                for def_name in &decl_order {
                    if let Some((_name, val)) = fields.iter().find(|(n, _)| n.node == *def_name) {
                        self.emit_expr(&val.node);
                        self.stack.pop(); // consumed by write_io
                        self.inst("write_io 1");
                    }
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
                let padding = 10 - 1 - num_fields; // 1 for tag
                for _ in 0..padding {
                    self.inst("push 0");
                }

                // Push fields in reverse declaration order
                for def_name in decl_order.iter().rev() {
                    if let Some((_name, val)) = fields.iter().find(|(n, _)| n.node == *def_name) {
                        self.emit_expr(&val.node);
                        self.stack.pop(); // will be consumed by hash
                    }
                }

                // Push tag (will be on top, consumed first by hash)
                self.inst(&format!("push {}", tag));

                // Hash: consumes 10, produces 5 (Digest)
                self.inst("hash");

                // Write the 5-element digest commitment
                self.inst("write_io 5");
            }
        }
    }

    fn emit_loop_subroutine(&mut self, label: &str, body: &Block, _var_name: &str) {
        self.emit_label(label);
        self.inst("dup 0");
        self.inst("push 0");
        self.inst("eq");
        self.inst("skiz");
        self.inst("return");
        self.inst("push -1");
        self.inst("add");

        // Save and restore stack model since loop body is a separate context
        let saved = self.stack.save_state();
        self.stack.clear();
        self.emit_block(body);
        self.stack.restore_state(saved);

        self.inst("recurse");
        self.raw("");
    }

    /// Emit an expression. Always pushes exactly one anonymous entry onto the model.
    fn emit_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Literal(Literal::Integer(n)) => {
                self.emit_and_push(&format!("push {}", n), 1);
            }
            Expr::Literal(Literal::Bool(b)) => {
                self.emit_and_push(&format!("push {}", if *b { 1 } else { 0 }), 1);
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
                                self.inst(&format!("dup {}", real_depth + field_width - 1));
                            }
                            self.stack.push_temp(field_width);
                        } else {
                            let depth = base_depth;
                            self.emit_and_push(&format!("dup {}", depth), 1);
                        }
                    } else {
                        // Module constant — look up value
                        if let Some(&val) = self.constants.get(name) {
                            self.emit_and_push(&format!("push {}", val), 1);
                        } else if let Some(&val) = self.constants.get(suffix) {
                            self.emit_and_push(&format!("push {}", val), 1);
                        } else {
                            self.inst(&format!("// ERROR: unresolved constant '{}'", name));
                            self.emit_and_push("push 0", 1);
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
                                self.inst(&format!("dup {}", depth + width - 1));
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
                                    self.inst(&format!("dup {}", depth2 + width - 1));
                                }
                            } else {
                                self.inst(&format!(
                                    "// ERROR: variable '{}' unreachable \
                                     (depth {}+{})",
                                    name, depth2, width
                                ));
                            }
                        }
                        self.stack.push_temp(width);
                    } else {
                        // Variable not found — fallback
                        self.inst("dup 0");
                        self.stack.push_temp(1);
                    }
                }
            }
            Expr::BinOp { op, lhs, rhs } => {
                self.emit_expr(&lhs.node); // pushes temp for lhs
                self.emit_expr(&rhs.node); // pushes temp for rhs
                match op {
                    BinOp::Add => self.inst("add"),
                    BinOp::Mul => self.inst("mul"),
                    BinOp::Eq => self.inst("eq"),
                    BinOp::Lt => self.inst("lt"),
                    BinOp::BitAnd => self.inst("and"),
                    BinOp::BitXor => self.inst("xor"),
                    BinOp::DivMod => self.inst("div_mod"),
                    BinOp::XFieldMul => self.inst("xb_mul"),
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
            Expr::Call { path, args } => {
                let fn_name = path.node.as_dotted();
                self.emit_call(&fn_name, args);
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
                            self.inst(&format!("dup {}", offset + (field_width - 1 - i)));
                        }
                        // Pop the struct temp, push field temp
                        self.stack.pop();
                        for _ in 0..field_width {
                            self.inst(&format!("swap {}", field_width + struct_width - 1));
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
                                .map(|f| resolve_type_width(&f.ty.node))
                                .sum();
                            if total != struct_width {
                                continue;
                            }
                            let mut off = 0u32;
                            for sf in &sdef.fields {
                                let fw = resolve_type_width(&sf.ty.node);
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
                                self.inst(&format!("dup {}", from_top + (fw - 1 - i)));
                            }
                            self.stack.pop();
                            for _ in 0..fw {
                                self.inst(&format!("swap {}", fw + struct_width - 1));
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
                            self.inst(&format!("dup {}", base_offset + (elem_width - 1 - i)));
                        }
                        // Pop the array, push the element
                        self.stack.pop();
                        for _ in 0..elem_width {
                            self.inst(&format!("swap {}", elem_width + array_width - 1));
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
                        self.inst("swap 1"); // move index below top array elem
                        for i in 0..array_width {
                            let addr = base + i as u64;
                            self.inst(&format!("push {}", addr));
                            self.inst("swap 1");
                            self.inst("write_mem 1");
                            self.inst("pop 1");
                            if i + 1 < array_width {
                                self.inst("swap 1"); // bring next array elem to top
                            }
                        }
                        // Stack now has: [... index]

                        // Compute target address: base + idx * elem_width
                        if elem_width > 1 {
                            self.inst(&format!("push {}", elem_width));
                            self.inst("mul");
                        }
                        self.inst(&format!("push {}", base));
                        self.inst("add");

                        // Read elem_width elements from computed address
                        for i in 0..elem_width {
                            self.inst("dup 0");
                            if i > 0 {
                                self.inst(&format!("push {}", i));
                                self.inst("add");
                            }
                            self.inst("read_mem 1");
                            self.inst("pop 1");
                            self.inst("swap 1");
                        }
                        self.inst("pop 1"); // pop address

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

    fn emit_call(&mut self, name: &str, args: &[Spanned<Expr>]) {
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
                self.emit_and_push("read_io 1", 1);
            }
            "pub_read2" => {
                self.emit_and_push("read_io 2", 2);
            }
            "pub_read3" => {
                self.emit_and_push("read_io 3", 3);
            }
            "pub_read4" => {
                self.emit_and_push("read_io 4", 4);
            }
            "pub_read5" => {
                self.emit_and_push("read_io 5", 5);
            }
            "pub_write" => {
                self.inst("write_io 1");
                self.push_temp(0);
            }
            "pub_write2" => {
                self.inst("write_io 2");
                self.push_temp(0);
            }
            "pub_write3" => {
                self.inst("write_io 3");
                self.push_temp(0);
            }
            "pub_write4" => {
                self.inst("write_io 4");
                self.push_temp(0);
            }
            "pub_write5" => {
                self.inst("write_io 5");
                self.push_temp(0);
            }

            // Non-deterministic input
            "divine" => {
                self.emit_and_push("divine 1", 1);
            }
            "divine3" => {
                self.emit_and_push("divine 3", 3);
            }
            "divine5" => {
                self.emit_and_push("divine 5", 5);
            }

            // Assertions — consume arg, produce nothing
            "assert" => {
                self.inst("assert");
                self.push_temp(0);
            }
            "assert_eq" => {
                self.inst("eq");
                self.inst("assert");
                self.push_temp(0);
            }
            "assert_digest" => {
                self.inst("assert_vector");
                self.inst("pop 5");
                self.push_temp(0);
            }

            // Field operations
            "field_add" => {
                self.inst("add");
                self.push_temp(1);
            }
            "field_mul" => {
                self.inst("mul");
                self.push_temp(1);
            }
            "inv" => {
                self.inst("invert");
                self.push_temp(1);
            }
            "neg" => {
                self.inst("push -1");
                self.inst("mul");
                self.push_temp(1);
            }
            "sub" => {
                self.inst("push -1");
                self.inst("mul");
                self.inst("add");
                self.push_temp(1);
            }

            // U32 operations
            "split" => {
                self.inst("split");
                self.push_temp(2);
            }
            "log2" => {
                self.inst("log_2_floor");
                self.push_temp(1);
            }
            "pow" => {
                self.inst("pow");
                self.push_temp(1);
            }
            "popcount" => {
                self.inst("pop_count");
                self.push_temp(1);
            }

            // Hash operations
            "hash" => {
                self.inst("hash");
                self.push_temp(5);
            }
            "sponge_init" => {
                self.inst("sponge_init");
                self.push_temp(0);
            }
            "sponge_absorb" => {
                self.inst("sponge_absorb");
                self.push_temp(0);
            }
            "sponge_squeeze" => {
                self.emit_and_push("sponge_squeeze", 10);
            }
            "sponge_absorb_mem" => {
                self.inst("sponge_absorb_mem");
                self.push_temp(0);
            }

            // Merkle
            "merkle_step" => {
                self.emit_and_push("merkle_step", 6);
            }
            "merkle_step_mem" => {
                self.emit_and_push("merkle_step_mem", 7);
            }

            // RAM
            "ram_read" => {
                self.inst("read_mem 1");
                self.inst("pop 1");
                self.push_temp(1);
            }
            "ram_write" => {
                self.inst("write_mem 1");
                self.inst("pop 1");
                self.push_temp(0);
            }
            "ram_read_block" => {
                // Read 5 consecutive elements (Digest-sized block)
                self.inst("read_mem 5");
                self.inst("pop 1");
                self.push_temp(5);
            }
            "ram_write_block" => {
                // Write 5 consecutive elements (Digest-sized block)
                self.inst("write_mem 5");
                self.inst("pop 1");
                self.push_temp(0);
            }

            // Conversion
            "as_u32" => {
                self.inst("split");
                self.inst("pop 1");
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
                self.inst("x_invert");
                self.push_temp(3);
            }
            "xx_dot_step" => {
                self.emit_and_push("xx_dot_step", 5);
            }
            "xb_dot_step" => {
                self.emit_and_push("xb_dot_step", 5);
            }

            // User-defined function
            _ => {
                let (call_inst, base_name) = if name.contains('.') {
                    // Cross-module call: "merkle.verify" → "call merkle__verify"
                    // Resolve module aliases: "hash" → "std.hash" → "std_hash"
                    let parts: Vec<&str> = name.rsplitn(2, '.').collect();
                    let fn_name = parts[0];
                    let short_module = parts[1];
                    let full_module = self
                        .module_aliases
                        .get(short_module)
                        .map(|s| s.as_str())
                        .unwrap_or(short_module);
                    let mangled = full_module.replace('.', "_");
                    (
                        format!("call {}__{}", mangled, fn_name),
                        fn_name.to_string(),
                    )
                } else {
                    (format!("call __{}", name), name.to_string())
                };
                let ret_width = self.fn_return_widths.get(&base_name).copied().unwrap_or(0);
                if ret_width > 0 {
                    self.emit_and_push(&call_inst, ret_width);
                } else {
                    // Void function — emit call but don't push a stack entry
                    self.inst(&call_inst);
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
                    .map(|f| resolve_type_width(&f.ty.node))
                    .sum();
                let mut offset = 0u32;
                for sf in &sdef.fields {
                    let fw = resolve_type_width(&sf.ty.node);
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
                            .map(|f| resolve_type_width(&f.ty.node))
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
            self.inst(&format!("pop {}", batch));
            remaining -= batch;
        }
    }

    fn fresh_label(&mut self, prefix: &str) -> String {
        self.label_counter += 1;
        format!("__{}__{}", prefix, self.label_counter)
    }

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

fn resolve_type_width(ty: &Type) -> u32 {
    match ty {
        Type::Field | Type::Bool | Type::U32 => 1,
        Type::XField => 3,
        Type::Digest => 5,
        Type::Array(inner, n) => resolve_type_width(inner) * (*n as u32),
        Type::Tuple(elems) => elems.iter().map(resolve_type_width).sum(),
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
}
