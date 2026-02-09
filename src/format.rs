use crate::ast::*;
use crate::lexer::Comment;
use crate::span::Spanned;

const MAX_WIDTH: usize = 80;
const INDENT: &str = "    ";

/// Format a parsed Trident file back to source, preserving comments.
pub fn format_file(file: &File, comments: &[Comment], source: &str) -> String {
    let mut ctx = FormatCtx::new(comments, source);
    ctx.emit_file(file);
    let mut out = ctx.output;
    // Ensure single trailing newline
    while out.ends_with("\n\n") {
        out.pop();
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

struct FormatCtx<'a> {
    output: String,
    comments: Vec<CommentEntry>,
    _source: &'a str,
}

#[derive(Clone)]
struct CommentEntry {
    text: String,
    byte_offset: u32,
    trailing: bool,
    used: bool,
}

impl<'a> FormatCtx<'a> {
    fn new(comments: &[Comment], source: &'a str) -> Self {
        let entries = comments
            .iter()
            .map(|c| CommentEntry {
                text: c.text.clone(),
                byte_offset: c.span.start,
                trailing: c.trailing,
                used: false,
            })
            .collect();
        Self {
            output: String::new(),
            comments: entries,
            _source: source,
        }
    }

    /// Emit leading comments that appear before `span_start`.
    fn emit_leading_comments(&mut self, span_start: u32, indent: &str) {
        for i in 0..self.comments.len() {
            if self.comments[i].used || self.comments[i].trailing {
                continue;
            }
            if self.comments[i].byte_offset < span_start {
                let text = self.comments[i].text.clone();
                self.comments[i].used = true;
                self.output.push_str(indent);
                self.output.push_str(&text);
                self.output.push('\n');
            }
        }
    }

    /// Emit trailing comment on the same line as the node ending at `span_end`.
    fn emit_trailing_comment(&mut self, span_end: u32) {
        for i in 0..self.comments.len() {
            if self.comments[i].used || !self.comments[i].trailing {
                continue;
            }
            if self.comments[i].byte_offset >= span_end {
                // Only take the first trailing comment right after this node
                let text = self.comments[i].text.clone();
                self.comments[i].used = true;
                self.output.push_str(" ");
                self.output.push_str(&text);
                break;
            }
        }
    }

    /// Emit any remaining unused comments (e.g., at end of file).
    fn emit_remaining_comments(&mut self, indent: &str) {
        for i in 0..self.comments.len() {
            if self.comments[i].used {
                continue;
            }
            self.comments[i].used = true;
            self.output.push_str(indent);
            self.output.push_str(&self.comments[i].text.clone());
            self.output.push('\n');
        }
    }

    fn emit_file(&mut self, file: &File) {
        // File header: program/module name
        let keyword = match file.kind {
            FileKind::Program => "program",
            FileKind::Module => "module",
        };
        self.emit_leading_comments(file.name.span.start, "");
        self.output.push_str(keyword);
        self.output.push(' ');
        self.output.push_str(&file.name.node);
        self.output.push('\n');

        // Use declarations
        for u in &file.uses {
            self.output.push('\n');
            self.emit_leading_comments(u.span.start, "");
            self.output.push_str("use ");
            self.output.push_str(&u.node.as_dotted());
            self.output.push('\n');
        }

        // I/O declarations
        for decl in &file.declarations {
            self.output.push('\n');
            match decl {
                Declaration::PubInput(ty) => {
                    self.emit_leading_comments(ty.span.start, "");
                    self.output.push_str("pub input: ");
                    self.output.push_str(&format_type(&ty.node));
                    self.output.push('\n');
                }
                Declaration::PubOutput(ty) => {
                    self.emit_leading_comments(ty.span.start, "");
                    self.output.push_str("pub output: ");
                    self.output.push_str(&format_type(&ty.node));
                    self.output.push('\n');
                }
                Declaration::SecInput(ty) => {
                    self.emit_leading_comments(ty.span.start, "");
                    self.output.push_str("sec input: ");
                    self.output.push_str(&format_type(&ty.node));
                    self.output.push('\n');
                }
                Declaration::SecRam(entries) => {
                    self.output.push_str("sec ram: {\n");
                    for (addr, ty) in entries {
                        self.output.push_str(&format!(
                            "    {}: {},\n",
                            addr,
                            format_type(&ty.node)
                        ));
                    }
                    self.output.push_str("}\n");
                }
            }
        }

        // Items
        for item in &file.items {
            self.output.push('\n');
            self.emit_item(&item, "");
        }

        // Any remaining comments at end of file
        self.emit_remaining_comments("");
    }

    fn emit_item(&mut self, item: &Spanned<Item>, indent: &str) {
        self.emit_leading_comments(item.span.start, indent);
        match &item.node {
            Item::Const(c) => self.emit_const(c, indent),
            Item::Struct(s) => self.emit_struct(s, indent),
            Item::Event(e) => self.emit_event(e, indent),
            Item::Fn(f) => self.emit_fn(f, indent),
        }
    }

    fn emit_const(&mut self, c: &ConstDef, indent: &str) {
        self.output.push_str(indent);
        if c.is_pub {
            self.output.push_str("pub ");
        }
        self.output.push_str("const ");
        self.output.push_str(&c.name.node);
        self.output.push_str(": ");
        self.output.push_str(&format_type(&c.ty.node));
        self.output.push_str(" = ");
        self.output.push_str(&format_expr(&c.value.node));
        self.output.push('\n');
    }

    fn emit_struct(&mut self, s: &StructDef, indent: &str) {
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
        self.output.push_str(indent);

        // Intrinsic attribute
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

        // Format signature
        let sig = self.format_signature(f);
        let prefix_len = indent.len()
            + if f.is_pub { 4 } else { 0 }
            + 3  // "fn "
            + f.name.node.len();

        if prefix_len + sig.len() <= MAX_WIDTH {
            self.output.push_str(&sig);
        } else {
            // Wrap params one per line
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

        // Body
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

    fn emit_block(&mut self, block: &Block, outer_indent: &str) {
        let indent = format!("{}{}", outer_indent, INDENT);
        for stmt in &block.stmts {
            self.emit_stmt(&stmt, &indent);
        }
        if let Some(tail) = &block.tail_expr {
            self.emit_leading_comments(tail.span.start, &indent);
            self.output.push_str(&indent);
            self.emit_expr_wrapped(&tail.node, &indent);
            self.output.push('\n');
        }
    }

    fn emit_stmt(&mut self, stmt: &Spanned<Stmt>, indent: &str) {
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
                    // Check if else block is a single `if` statement (else if)
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
            Stmt::Emit { event_name, fields } => {
                self.emit_event_stmt("emit", event_name, fields, indent, stmt.span.end);
            }
            Stmt::Seal { event_name, fields } => {
                self.emit_event_stmt("seal", event_name, fields, indent, stmt.span.end);
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

    /// Format an expression, wrapping long function calls.
    fn emit_expr_wrapped(&mut self, expr: &Expr, indent: &str) {
        let flat = format_expr(expr);
        let current_line_len = self.current_line_len();
        if current_line_len + flat.len() <= MAX_WIDTH {
            self.output.push_str(&flat);
        } else if let Expr::Call { path, args } = expr {
            // Try wrapping call args
            if args.is_empty() {
                self.output.push_str(&flat);
            } else {
                let arg_indent = format!("{}{}", indent, INDENT);
                self.output.push_str(&path.node.as_dotted());
                self.output.push_str("(\n");
                for (i, arg) in args.iter().enumerate() {
                    self.output.push_str(&arg_indent);
                    self.output.push_str(&format_expr(&arg.node));
                    if i + 1 < args.len() {
                        self.output.push(',');
                    }
                    self.output.push('\n');
                }
                self.output.push_str(indent);
                self.output.push(')');
            }
        } else {
            // Fall back to flat for non-call expressions
            self.output.push_str(&flat);
        }
    }

    fn current_line_len(&self) -> usize {
        match self.output.rfind('\n') {
            Some(pos) => self.output.len() - pos - 1,
            None => self.output.len(),
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

/// Format a type to string.
fn format_type(ty: &Type) -> String {
    match ty {
        Type::Field => "Field".to_string(),
        Type::XField => "XField".to_string(),
        Type::Bool => "Bool".to_string(),
        Type::U32 => "U32".to_string(),
        Type::Digest => "Digest".to_string(),
        Type::Array(inner, size) => format!("[{}; {}]", format_type(inner), size),
        Type::Tuple(elems) => {
            let inner: Vec<String> = elems.iter().map(|t| format_type(t)).collect();
            format!("({})", inner.join(", "))
        }
        Type::Named(path) => path.as_dotted(),
    }
}

/// Format an expression to a single-line string.
fn format_expr(expr: &Expr) -> String {
    match expr {
        Expr::Literal(lit) => match lit {
            Literal::Integer(n) => n.to_string(),
            Literal::Bool(b) => b.to_string(),
        },
        Expr::Var(name) => name.clone(),
        Expr::BinOp { op, lhs, rhs } => {
            let l = format_expr_precedence(&lhs.node, op, true);
            let r = format_expr_precedence(&rhs.node, op, false);
            format!("{} {} {}", l, op.as_str(), r)
        }
        Expr::Call { path, args } => {
            let args_str: Vec<String> = args.iter().map(|a| format_expr(&a.node)).collect();
            format!("{}({})", path.node.as_dotted(), args_str.join(", "))
        }
        Expr::FieldAccess { expr, field } => {
            format!("{}.{}", format_expr(&expr.node), field.node)
        }
        Expr::Index { expr, index } => {
            format!("{}[{}]", format_expr(&expr.node), format_expr(&index.node))
        }
        Expr::StructInit { path, fields } => {
            let fields_str: Vec<String> = fields
                .iter()
                .map(|(name, expr)| format!("{}: {}", name.node, format_expr(&expr.node)))
                .collect();
            format!("{} {{ {} }}", path.node.as_dotted(), fields_str.join(", "))
        }
        Expr::ArrayInit(elems) => {
            let inner: Vec<String> = elems.iter().map(|e| format_expr(&e.node)).collect();
            format!("[{}]", inner.join(", "))
        }
        Expr::Tuple(elems) => {
            let inner: Vec<String> = elems.iter().map(|e| format_expr(&e.node)).collect();
            format!("({})", inner.join(", "))
        }
    }
}

/// Format an expression with parentheses if needed for precedence.
fn format_expr_precedence(expr: &Expr, parent_op: &BinOp, _is_left: bool) -> String {
    if let Expr::BinOp { op, .. } = expr {
        if precedence(op) < precedence(parent_op) {
            return format!("({})", format_expr(expr));
        }
    }
    format_expr(expr)
}

fn precedence(op: &BinOp) -> u8 {
    match op {
        BinOp::Eq => 2,
        BinOp::Lt => 4,
        BinOp::Add => 6,
        BinOp::Mul | BinOp::XFieldMul => 8,
        BinOp::BitAnd | BinOp::BitXor => 10,
        BinOp::DivMod => 12,
    }
}

/// Format a place (l-value) to string.
fn format_place(place: &Place) -> String {
    match place {
        Place::Var(name) => name.clone(),
        Place::FieldAccess(inner, field) => {
            format!("{}.{}", format_place(&inner.node), field.node)
        }
        Place::Index(inner, index) => {
            format!(
                "{}[{}]",
                format_place(&inner.node),
                format_expr(&index.node)
            )
        }
    }
}
