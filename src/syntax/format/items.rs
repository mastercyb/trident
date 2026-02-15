use crate::ast::*;
use crate::span::Spanned;

use super::expr::format_type;
use super::{FormatCtx, INDENT, MAX_WIDTH};

impl FormatCtx {
    pub(super) fn emit_item(&mut self, item: &Spanned<Item>, indent: &str) {
        self.emit_leading_comments(item.span.start, indent);
        match &item.node {
            Item::Const(c) => self.emit_const(c, indent),
            Item::Struct(s) => self.emit_struct(s, indent),
            Item::Event(e) => self.emit_event(e, indent),
            Item::Fn(f) => self.emit_fn(f, indent),
        }
    }

    fn emit_cfg_attr(&mut self, cfg: &Option<Spanned<String>>, indent: &str) {
        if let Some(flag) = cfg {
            self.output.push_str(indent);
            self.output.push_str("#[cfg(");
            self.output.push_str(&flag.node);
            self.output.push_str(")]\n");
        }
    }

    fn emit_const(&mut self, c: &ConstDef, indent: &str) {
        self.emit_cfg_attr(&c.cfg, indent);
        self.output.push_str(indent);
        if c.is_pub {
            self.output.push_str("pub ");
        }
        self.output.push_str("const ");
        self.output.push_str(&c.name.node);
        self.output.push_str(": ");
        self.output.push_str(&format_type(&c.ty.node));
        self.output.push_str(" = ");
        self.output
            .push_str(&super::expr::format_expr(&c.value.node));
        self.output.push('\n');
    }

    fn emit_struct(&mut self, s: &StructDef, indent: &str) {
        self.emit_cfg_attr(&s.cfg, indent);
        self.output.push_str(indent);
        if s.is_pub {
            self.output.push_str("pub ");
        }
        self.output.push_str("struct ");
        self.output.push_str(&s.name.node);
        self.output.push_str(" {\n");
        let inner = format!("{}{}", indent, INDENT);
        for field in &s.fields {
            self.emit_leading_comments(field.name.span.start, &inner);
            self.output.push_str(&inner);
            if field.is_pub {
                self.output.push_str("pub ");
            }
            self.output.push_str(&field.name.node);
            self.output.push_str(": ");
            self.output.push_str(&format_type(&field.ty.node));
            self.output.push_str(",\n");
        }
        self.output.push_str(indent);
        self.output.push_str("}\n");
    }

    fn emit_event(&mut self, e: &EventDef, indent: &str) {
        self.emit_cfg_attr(&e.cfg, indent);
        self.output.push_str(indent);
        self.output.push_str("event ");
        self.output.push_str(&e.name.node);
        self.output.push_str(" {\n");
        let inner = format!("{}{}", indent, INDENT);
        for field in &e.fields {
            self.emit_leading_comments(field.name.span.start, &inner);
            self.output.push_str(&inner);
            self.output.push_str(&field.name.node);
            self.output.push_str(": ");
            self.output.push_str(&format_type(&field.ty.node));
            self.output.push_str(",\n");
        }
        self.output.push_str(indent);
        self.output.push_str("}\n");
    }

    fn emit_fn(&mut self, f: &FnDef, indent: &str) {
        self.emit_cfg_attr(&f.cfg, indent);

        if f.is_test {
            self.output.push_str(indent);
            self.output.push_str("#[test]\n");
        }

        if f.is_pure {
            self.output.push_str(indent);
            self.output.push_str("#[pure]\n");
        }

        for req in &f.requires {
            self.output.push_str(indent);
            self.output.push_str("#[requires(");
            self.output.push_str(&req.node);
            self.output.push_str(")]\n");
        }
        for ens in &f.ensures {
            self.output.push_str(indent);
            self.output.push_str("#[ensures(");
            self.output.push_str(&ens.node);
            self.output.push_str(")]\n");
        }

        self.output.push_str(indent);

        if let Some(attr) = &f.intrinsic {
            self.output.push_str("#[");
            self.output.push_str(&attr.node);
            self.output.push_str("]\n");
            self.output.push_str(indent);
        }

        if f.is_pub {
            self.output.push_str("pub ");
        }
        self.output.push_str("fn ");
        self.output.push_str(&f.name.node);

        if !f.type_params.is_empty() {
            self.output.push('<');
            for (i, tp) in f.type_params.iter().enumerate() {
                if i > 0 {
                    self.output.push_str(", ");
                }
                self.output.push_str(&tp.node);
            }
            self.output.push('>');
        }

        let sig = self.format_signature(f);
        let generic_len: usize = if f.type_params.is_empty() {
            0
        } else {
            2 + f.type_params.iter().map(|tp| tp.node.len()).sum::<usize>()
                + (f.type_params.len().saturating_sub(1)) * 2
        };
        let prefix_len = indent.len()
            + if f.is_pub { 4 } else { 0 }
            + 3  // "fn "
            + f.name.node.len()
            + generic_len;

        if prefix_len + sig.len() <= MAX_WIDTH {
            self.output.push_str(&sig);
        } else {
            self.output.push_str("(\n");
            let param_indent = format!("{}{}", indent, INDENT);
            for (i, param) in f.params.iter().enumerate() {
                self.output.push_str(&param_indent);
                self.output.push_str(&param.name.node);
                self.output.push_str(": ");
                self.output.push_str(&format_type(&param.ty.node));
                if i + 1 < f.params.len() {
                    self.output.push(',');
                }
                self.output.push('\n');
            }
            self.output.push_str(indent);
            self.output.push(')');
            if let Some(ret) = &f.return_ty {
                self.output.push_str(" -> ");
                self.output.push_str(&format_type(&ret.node));
            }
        }

        match &f.body {
            Some(body) => {
                self.output.push_str(" {\n");
                self.emit_block(&body.node, indent);
                self.output.push_str(indent);
                self.output.push_str("}\n");
            }
            None => {
                self.output.push('\n');
            }
        }
    }

    fn format_signature(&self, f: &FnDef) -> String {
        let mut sig = String::from("(");
        for (i, param) in f.params.iter().enumerate() {
            if i > 0 {
                sig.push_str(", ");
            }
            sig.push_str(&param.name.node);
            sig.push_str(": ");
            sig.push_str(&format_type(&param.ty.node));
        }
        sig.push(')');
        if let Some(ret) = &f.return_ty {
            sig.push_str(" -> ");
            sig.push_str(&format_type(&ret.node));
        }
        sig
    }
}
