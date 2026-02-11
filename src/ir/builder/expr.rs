//! Expression emission: build_expr, build_var_expr, build_field_access, build_index.

use crate::ast::*;
use crate::ir::IROp;
use crate::span::Spanned;

use super::layout::resolve_type_width;
use super::IRBuilder;

impl IRBuilder {
    pub(crate) fn build_expr(&mut self, expr: &Expr) {
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

    pub(crate) fn build_var_expr(&mut self, name: &str) {
        if name.contains('.') {
            let dot_pos = name.rfind('.').unwrap();
            let prefix = &name[..dot_pos];
            let suffix = &name[dot_pos + 1..];
            let var_depth_info = self.find_var_depth_and_width(prefix);
            if let Some((base_depth, _var_width)) = var_depth_info {
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

    pub(crate) fn build_field_access(&mut self, inner: &Spanned<Expr>, field: &Spanned<String>) {
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

    pub(crate) fn build_index(&mut self, inner: &Spanned<Expr>, index: &Spanned<Expr>) {
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
}
