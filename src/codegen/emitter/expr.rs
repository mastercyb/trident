use super::{resolve_type_width, Emitter};
use crate::ast::*;

impl Emitter {
    pub(super) fn emit_expr(&mut self, expr: &Expr) {
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
                            self.inst(&format!(
                                "{} ERROR: unresolved constant '{}'",
                                self.backend.comment_prefix(),
                                name
                            ));
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
                                    "{} BUG: variable '{}' unreachable (depth {}+{}), aborting",
                                    self.backend.comment_prefix(),
                                    name,
                                    depth2,
                                    width
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
                            self.inst(&format!(
                                "{} ERROR: unresolved field '{}'",
                                self.backend.comment_prefix(),
                                field.node
                            ));
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
}
