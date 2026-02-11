use std::collections::HashMap;

use super::{resolve_type_width, DeferredBlock, Emitter};
use crate::ast::*;
use crate::span::Spanned;

impl Emitter {
    pub(super) fn emit_block(&mut self, block: &Block) {
        for stmt in &block.stmts {
            self.emit_stmt(&stmt.node);
        }
        // Tail expression: emitted and left on stack as the block's return value
        if let Some(tail) = &block.tail_expr {
            self.emit_expr(&tail.node);
        }
    }

    pub(super) fn emit_stmt(&mut self, stmt: &Stmt) {
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

                    let lines = self.backend.emit_if_else(&then_label, &else_label);
                    for line in lines {
                        self.inst(&line);
                    }

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

                    let lines = self.backend.emit_if_only(&then_label);
                    for line in lines {
                        self.inst(&line);
                    }

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

                            // Branch: match arm vs continue to next pattern
                            let lines = self.backend.emit_if_else(&arm_label, &rest_label);
                            for line in lines {
                                self.inst(&line);
                            }

                            // Build arm body: pop scrutinee then run original body
                            let pop_inst = self.backend.inst_pop(1);
                            let mut arm_stmts = vec![Spanned::new(
                                Stmt::Asm {
                                    body: pop_inst,
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

                            let pop_inst = self.backend.inst_pop(1);
                            let mut arm_stmts = vec![Spanned::new(
                                Stmt::Asm {
                                    body: pop_inst,
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
                            let pop_inst = self.backend.inst_pop(1);
                            arm_stmts.push(Spanned::new(
                                Stmt::Asm {
                                    body: pop_inst,
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

    pub(super) fn emit_loop_subroutine(&mut self, label: &str, body: &Block, _var_name: &str) {
        self.emit_label(label);

        for line in self.backend.loop_check_zero() {
            self.inst(&line);
        }
        for line in self.backend.loop_decrement() {
            self.inst(&line);
        }

        // Save and restore stack model since loop body is a separate context
        let saved = self.stack.save_state();
        self.stack.clear();
        self.emit_block(body);
        self.stack.restore_state(saved);

        for line in self.backend.loop_tail() {
            self.inst(&line);
        }
        self.raw("");
    }
}
