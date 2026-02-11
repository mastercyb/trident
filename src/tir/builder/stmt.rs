//! Block, statement, and match compilation.

use std::collections::HashMap;

use crate::ast::*;
use crate::span::Spanned;
use crate::tir::TIROp;

use super::layout::resolve_type_width;
use super::TIRBuilder;

// ─── Block and statement emission ─────────────────────────────────

impl TIRBuilder {
    pub(crate) fn build_block(&mut self, block: &Block) {
        for stmt in &block.stmts {
            self.build_stmt(&stmt.node);
        }
        if let Some(tail) = &block.tail_expr {
            self.build_expr(&tail.node);
        }
    }

    pub(crate) fn build_stmt(&mut self, stmt: &Stmt) {
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
                        self.ops.push(TIROp::Swap(depth));
                        self.ops.push(TIROp::Pop(1));
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
                    let saved = self.stack.save_state();
                    let then_body = self.build_block_as_ir(&then_block.node);
                    self.stack.restore_state(saved.clone());
                    let else_body = self.build_block_as_ir(&else_blk.node);
                    self.stack.restore_state(saved);

                    self.ops.push(TIROp::IfElse {
                        then_body,
                        else_body,
                    });
                } else {
                    let saved = self.stack.save_state();
                    let then_body = self.build_block_as_ir(&then_block.node);
                    self.stack.restore_state(saved);

                    self.ops.push(TIROp::IfOnly { then_body });
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

                self.build_expr(&end.node);

                self.ops.push(TIROp::Call(loop_label.clone()));
                self.ops.push(TIROp::Pop(1));
                self.stack.pop();

                let saved = self.stack.save_state();
                self.stack.clear();
                let body_ir = self.build_block_as_ir(&body.node);
                self.stack.restore_state(saved);

                self.ops.push(TIROp::Loop {
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
                            self.ops.push(TIROp::Swap(depth));
                            self.ops.push(TIROp::Pop(1));
                        }
                    }
                    let _ = total_width;
                }
            }

            Stmt::Expr(expr) => {
                let before = self.stack.stack_len();
                self.build_expr(&expr.node);
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

                for def_name in &decl_order {
                    if let Some((_name, val)) = fields.iter().find(|(n, _)| n.node == *def_name) {
                        self.build_expr(&val.node);
                        self.stack.pop();
                    }
                }

                self.ops.push(TIROp::Open {
                    name: event_name.node.clone(),
                    tag,
                    field_count: decl_order.len() as u32,
                });
            }

            Stmt::Asm {
                body,
                effect,
                target,
            } => {
                if let Some(tag) = target {
                    if tag != &self.target_config.name {
                        return;
                    }
                }

                self.stack.spill_all_named();
                self.flush_stack_effects();

                let lines: Vec<String> = body
                    .lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect();

                if !lines.is_empty() {
                    self.ops.push(TIROp::Asm {
                        lines,
                        effect: *effect,
                    });
                }

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
                let field_count = decl_order.len() as u32;

                // Push fields in reverse declaration order (so first declared
                // field ends up on top after all pushes).
                for def_name in decl_order.iter().rev() {
                    if let Some((_name, val)) = fields.iter().find(|(n, _)| n.node == *def_name) {
                        self.build_expr(&val.node);
                        self.stack.pop();
                    }
                }

                self.ops.push(TIROp::Seal {
                    name: event_name.node.clone(),
                    tag,
                    field_count,
                });
            }
        }
    }

    // ── Match statement ───────────────────────────────────────────

    pub(crate) fn build_match(&mut self, expr: &Spanned<Expr>, arms: &[MatchArm]) {
        self.build_expr(&expr.node);
        if let Some(top) = self.stack.last_mut() {
            top.name = Some("__match_scrutinee".to_string());
        }

        let mut deferred_subs: Vec<(String, Block, bool)> = Vec::new();

        for arm in arms {
            match &arm.pattern.node {
                MatchPattern::Literal(lit) => {
                    let _arm_label = self.fresh_label("match_arm");
                    let _rest_label = self.fresh_label("match_rest");

                    let depth = self.find_var_depth("__match_scrutinee");
                    self.ops.push(TIROp::Dup(depth));

                    match lit {
                        Literal::Integer(n) => self.ops.push(TIROp::Push(*n)),
                        Literal::Bool(b) => self.ops.push(TIROp::Push(if *b { 1 } else { 0 })),
                    }

                    self.ops.push(TIROp::Eq);

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

                    let rest_block = Block {
                        stmts: Vec::new(),
                        tail_expr: None,
                    };

                    let saved = self.stack.save_state();

                    let then_body = self.build_deferred_arm_ir(&arm_block, true);
                    self.stack.restore_state(saved.clone());

                    let else_body = self.build_deferred_arm_ir(&rest_block, false);
                    self.stack.restore_state(saved);

                    self.ops.push(TIROp::IfElse {
                        then_body,
                        else_body,
                    });
                }

                MatchPattern::Wildcard => {
                    let w_label = self.fresh_label("match_wild");
                    self.ops.push(TIROp::Call(w_label.clone()));

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
                    self.ops.push(TIROp::Call(s_label.clone()));

                    let mut arm_stmts: Vec<Spanned<Stmt>> = Vec::new();

                    arm_stmts.push(Spanned::new(
                        Stmt::Asm {
                            body: "pop 1".to_string(),
                            effect: -1,
                            target: None,
                        },
                        arm.body.span,
                    ));

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
                                FieldPattern::Wildcard => {}
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
        self.ops.push(TIROp::Pop(1));

        // Emit deferred subroutines inline.
        for (label, block, _is_literal) in deferred_subs {
            self.ops.push(TIROp::FnStart(label));
            let saved = self.stack.save_state();
            self.stack.clear();
            self.build_block(&block);
            self.stack.restore_state(saved);
            self.ops.push(TIROp::Return);
            self.ops.push(TIROp::FnEnd);
        }
    }

    /// Build a deferred match arm body into IR.
    pub(crate) fn build_deferred_arm_ir(&mut self, block: &Block, clears_flag: bool) -> Vec<TIROp> {
        let saved_ops = std::mem::take(&mut self.ops);
        if clears_flag {
            self.ops.push(TIROp::Push(0));
        }
        self.build_block(block);
        if clears_flag {
            self.ops.push(TIROp::Return);
        } else {
            self.ops.push(TIROp::Return);
        }
        let nested = std::mem::take(&mut self.ops);
        self.ops = saved_ops;
        nested
    }
}
