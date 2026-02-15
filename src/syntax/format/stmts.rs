use crate::ast::*;
use crate::span::Spanned;

use super::expr::{format_expr, format_place, format_type};
use super::{FormatCtx, INDENT, MAX_WIDTH};

impl FormatCtx {
    pub(super) fn emit_stmt(&mut self, stmt: &Spanned<Stmt>, indent: &str) {
        self.emit_leading_comments(stmt.span.start, indent);
        match &stmt.node {
            Stmt::Let {
                mutable,
                pattern,
                ty,
                init,
            } => {
                self.output.push_str(indent);
                self.output.push_str("let ");
                if *mutable {
                    self.output.push_str("mut ");
                }
                match pattern {
                    Pattern::Name(name) => self.output.push_str(&name.node),
                    Pattern::Tuple(names) => {
                        self.output.push('(');
                        for (i, n) in names.iter().enumerate() {
                            if i > 0 {
                                self.output.push_str(", ");
                            }
                            self.output.push_str(&n.node);
                        }
                        self.output.push(')');
                    }
                }
                if let Some(t) = ty {
                    self.output.push_str(": ");
                    self.output.push_str(&format_type(&t.node));
                }
                self.output.push_str(" = ");
                self.emit_expr_wrapped(&init.node, indent);
                self.emit_trailing_comment(stmt.span.end);
                self.output.push('\n');
            }
            Stmt::Assign { place, value } => {
                self.output.push_str(indent);
                self.output.push_str(&format_place(&place.node));
                self.output.push_str(" = ");
                self.emit_expr_wrapped(&value.node, indent);
                self.emit_trailing_comment(stmt.span.end);
                self.output.push('\n');
            }
            Stmt::TupleAssign { names, value } => {
                self.output.push_str(indent);
                self.output.push('(');
                for (i, n) in names.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.output.push_str(&n.node);
                }
                self.output.push_str(") = ");
                self.emit_expr_wrapped(&value.node, indent);
                self.emit_trailing_comment(stmt.span.end);
                self.output.push('\n');
            }
            Stmt::If {
                cond,
                then_block,
                else_block,
            } => {
                self.output.push_str(indent);
                self.output.push_str("if ");
                self.output.push_str(&format_expr(&cond.node));
                self.output.push_str(" {\n");
                self.emit_block(&then_block.node, indent);
                if let Some(else_b) = else_block {
                    if let Some(inner_if) = as_else_if(&else_b.node) {
                        self.output.push_str(indent);
                        self.output.push_str("} else ");
                        self.emit_if_inline(inner_if, indent);
                    } else {
                        self.output.push_str(indent);
                        self.output.push_str("} else {\n");
                        self.emit_block(&else_b.node, indent);
                        self.output.push_str(indent);
                        self.output.push_str("}\n");
                    }
                } else {
                    self.output.push_str(indent);
                    self.output.push_str("}\n");
                }
            }
            Stmt::For {
                var,
                start,
                end,
                bound,
                body,
            } => {
                self.output.push_str(indent);
                self.output.push_str("for ");
                self.output.push_str(&var.node);
                self.output.push_str(" in ");
                self.output.push_str(&format_expr(&start.node));
                self.output.push_str("..");
                self.output.push_str(&format_expr(&end.node));
                if let Some(b) = bound {
                    self.output.push_str(" bounded ");
                    self.output.push_str(&b.to_string());
                }
                self.output.push_str(" {\n");
                self.emit_block(&body.node, indent);
                self.output.push_str(indent);
                self.output.push_str("}\n");
            }
            Stmt::Expr(expr) => {
                self.output.push_str(indent);
                self.emit_expr_wrapped(&expr.node, indent);
                self.emit_trailing_comment(stmt.span.end);
                self.output.push('\n');
            }
            Stmt::Return(expr) => {
                self.output.push_str(indent);
                self.output.push_str("return");
                if let Some(e) = expr {
                    self.output.push(' ');
                    self.output.push_str(&format_expr(&e.node));
                }
                self.emit_trailing_comment(stmt.span.end);
                self.output.push('\n');
            }
            Stmt::Reveal { event_name, fields } => {
                self.emit_event_stmt("reveal", event_name, fields, indent, stmt.span.end);
            }
            Stmt::Seal { event_name, fields } => {
                self.emit_event_stmt("seal", event_name, fields, indent, stmt.span.end);
            }
            Stmt::Match { expr, arms } => {
                self.output.push_str(indent);
                self.output.push_str("match ");
                self.output.push_str(&format_expr(&expr.node));
                self.output.push_str(" {\n");
                let inner = format!("{}{}", indent, INDENT);
                for arm in arms {
                    self.output.push_str(&inner);
                    match &arm.pattern.node {
                        MatchPattern::Literal(Literal::Integer(n)) => {
                            self.output.push_str(&n.to_string());
                        }
                        MatchPattern::Literal(Literal::Bool(b)) => {
                            self.output.push_str(if *b { "true" } else { "false" });
                        }
                        MatchPattern::Wildcard => {
                            self.output.push('_');
                        }
                        MatchPattern::Struct { name, fields } => {
                            self.output.push_str(&name.node);
                            self.output.push_str(" { ");
                            for (i, spf) in fields.iter().enumerate() {
                                if i > 0 {
                                    self.output.push_str(", ");
                                }
                                self.output.push_str(&spf.field_name.node);
                                match &spf.pattern.node {
                                    FieldPattern::Binding(var_name)
                                        if var_name == &spf.field_name.node =>
                                    {
                                        // Shorthand: `field` instead of `field: field`
                                    }
                                    FieldPattern::Binding(var_name) => {
                                        self.output.push_str(": ");
                                        self.output.push_str(var_name);
                                    }
                                    FieldPattern::Literal(Literal::Integer(n)) => {
                                        self.output.push_str(": ");
                                        self.output.push_str(&n.to_string());
                                    }
                                    FieldPattern::Literal(Literal::Bool(b)) => {
                                        self.output.push_str(": ");
                                        self.output.push_str(if *b { "true" } else { "false" });
                                    }
                                    FieldPattern::Wildcard => {
                                        self.output.push_str(": _");
                                    }
                                }
                            }
                            self.output.push_str(" }");
                        }
                    }
                    self.output.push_str(" => {\n");
                    self.emit_block(&arm.body.node, &inner);
                    self.output.push_str(&inner);
                    self.output.push_str("}\n");
                }
                self.output.push_str(indent);
                self.output.push_str("}\n");
            }
            Stmt::Asm {
                body,
                effect,
                target,
            } => {
                self.output.push_str(indent);
                self.output.push_str("asm");
                match (target.as_deref(), *effect != 0) {
                    (Some(tag), true) => {
                        if *effect > 0 {
                            self.output.push_str(&format!("({}, +{})", tag, effect));
                        } else {
                            self.output.push_str(&format!("({}, {})", tag, effect));
                        }
                    }
                    (Some(tag), false) => {
                        self.output.push_str(&format!("({})", tag));
                    }
                    (None, true) => {
                        if *effect > 0 {
                            self.output.push_str(&format!("(+{})", effect));
                        } else {
                            self.output.push_str(&format!("({})", effect));
                        }
                    }
                    (None, false) => {}
                }
                self.output.push_str(" {\n");
                let inner = format!("{}{}", indent, INDENT);
                for line in body.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        self.output.push('\n');
                    } else {
                        self.output.push_str(&inner);
                        self.output.push_str(trimmed);
                        self.output.push('\n');
                    }
                }
                self.output.push_str(indent);
                self.output.push_str("}\n");
            }
        }
    }

    fn emit_event_stmt(
        &mut self,
        keyword: &str,
        event_name: &Spanned<String>,
        fields: &[(Spanned<String>, Spanned<Expr>)],
        indent: &str,
        span_end: u32,
    ) {
        self.output.push_str(indent);
        self.output.push_str(keyword);
        self.output.push(' ');
        self.output.push_str(&event_name.node);
        self.output.push_str(" { ");

        let inline = self.format_event_fields_inline(fields);
        let line_len = indent.len() + keyword.len() + 1 + event_name.node.len() + 4 + inline.len();

        if line_len <= MAX_WIDTH {
            self.output.push_str(&inline);
            self.output.push_str(" }");
        } else {
            self.output.push('\n');
            let inner = format!("{}{}", indent, INDENT);
            for (name, expr) in fields {
                self.output.push_str(&inner);
                self.output.push_str(&name.node);
                self.output.push_str(": ");
                self.output.push_str(&format_expr(&expr.node));
                self.output.push_str(",\n");
            }
            self.output.push_str(indent);
            self.output.push('}');
        }
        self.emit_trailing_comment(span_end);
        self.output.push('\n');
    }

    fn format_event_fields_inline(&self, fields: &[(Spanned<String>, Spanned<Expr>)]) -> String {
        let mut s = String::new();
        for (i, (name, expr)) in fields.iter().enumerate() {
            if i > 0 {
                s.push_str(", ");
            }
            s.push_str(&name.node);
            s.push_str(": ");
            s.push_str(&format_expr(&expr.node));
        }
        s
    }

    /// Emit an if statement without the leading indent (used for `else if` chains).
    fn emit_if_inline(&mut self, stmt: &Stmt, indent: &str) {
        if let Stmt::If {
            cond,
            then_block,
            else_block,
        } = stmt
        {
            self.output.push_str("if ");
            self.output.push_str(&format_expr(&cond.node));
            self.output.push_str(" {\n");
            self.emit_block(&then_block.node, indent);
            if let Some(else_b) = else_block {
                if let Some(inner_if) = as_else_if(&else_b.node) {
                    self.output.push_str(indent);
                    self.output.push_str("} else ");
                    self.emit_if_inline(inner_if, indent);
                } else {
                    self.output.push_str(indent);
                    self.output.push_str("} else {\n");
                    self.emit_block(&else_b.node, indent);
                    self.output.push_str(indent);
                    self.output.push_str("}\n");
                }
            } else {
                self.output.push_str(indent);
                self.output.push_str("}\n");
            }
        }
    }
}

/// Check if a block is a single `if` statement (for `else if` chains).
fn as_else_if(block: &Block) -> Option<&Stmt> {
    if block.stmts.len() == 1 && block.tail_expr.is_none() {
        if let Stmt::If { .. } = &block.stmts[0].node {
            return Some(&block.stmts[0].node);
        }
    }
    None
}
