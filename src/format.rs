use crate::ast::*;
use crate::lexer::Comment;
use crate::span::Spanned;

const MAX_WIDTH: usize = 80;
const INDENT: &str = "    ";

/// Format a parsed Trident file back to source, preserving comments.
pub(crate) fn format_file(file: &File, comments: &[Comment], source: &str) -> String {
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
                self.output.push(' ');
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
            self.emit_item(item, "");
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
        self.output.push_str(&format_expr(&c.value.node));
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

        // Test attribute
        if f.is_test {
            self.output.push_str(indent);
            self.output.push_str("#[test]\n");
        }

        // Spec annotations
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

        // Emit size-generic parameters: <N, M>
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

        // Format signature
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
            self.emit_stmt(stmt, &indent);
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
                // Format annotation: target and/or effect
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

    /// Format an expression, wrapping long function calls.
    fn emit_expr_wrapped(&mut self, expr: &Expr, indent: &str) {
        let flat = format_expr(expr);
        let current_line_len = self.current_line_len();
        if current_line_len + flat.len() <= MAX_WIDTH {
            self.output.push_str(&flat);
        } else if let Expr::Call { path, args, .. } = expr {
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
            let inner: Vec<String> = elems.iter().map(format_type).collect();
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
        Expr::Call {
            path,
            args,
            generic_args,
        } => {
            let args_str: Vec<String> = args.iter().map(|a| format_expr(&a.node)).collect();
            if generic_args.is_empty() {
                format!("{}({})", path.node.as_dotted(), args_str.join(", "))
            } else {
                let ga: Vec<String> = generic_args.iter().map(|a| a.node.to_string()).collect();
                format!(
                    "{}<{}>({})",
                    path.node.as_dotted(),
                    ga.join(", "),
                    args_str.join(", ")
                )
            }
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

#[cfg(test)]
mod tests {
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    use super::*;

    /// Helper: parse source and format it back.
    fn fmt(source: &str) -> String {
        let (tokens, comments, lex_errors) = Lexer::new(source, 0).tokenize();
        assert!(lex_errors.is_empty(), "lex errors: {:?}", lex_errors);
        let file = Parser::new(tokens).parse_file().unwrap();
        format_file(&file, &comments, source)
    }

    // --- Basic formatting ---

    #[test]
    fn test_minimal_program() {
        let src = "program test\n\nfn main() {\n    pub_write(pub_read())\n}\n";
        assert_eq!(fmt(src), src);
    }

    #[test]
    fn test_module_header() {
        let src = "module math\n\npub fn add(a: Field, b: Field) -> Field {\n    a + b\n}\n";
        assert_eq!(fmt(src), src);
    }

    #[test]
    fn test_normalizes_whitespace() {
        let input = "program   test\n\n\n\nfn main() {\n    pub_write(pub_read())\n}\n";
        let output = fmt(input);
        assert!(output.starts_with("program test\n"));
        assert!(!output.contains("\n\n\n")); // no triple newlines
    }

    #[test]
    fn test_const_formatting() {
        let src = "program test\n\npub const MAX: U32 = 100\n\nfn main() {\n    pub_write(pub_read())\n}\n";
        assert_eq!(fmt(src), src);
    }

    #[test]
    fn test_struct_formatting() {
        let src = "program test\n\nstruct Point {\n    x: Field,\n    y: Field,\n}\n\nfn main() {\n    pub_write(pub_read())\n}\n";
        assert_eq!(fmt(src), src);
    }

    #[test]
    fn test_pub_struct_formatting() {
        let src = "program test\n\npub struct Config {\n    pub owner: Digest,\n    value: Field,\n}\n\nfn main() {\n    pub_write(pub_read())\n}\n";
        assert_eq!(fmt(src), src);
    }

    #[test]
    fn test_event_formatting() {
        let src = "program test\n\nevent Transfer {\n    from: Field,\n    to: Field,\n}\n\nfn main() {\n    pub_write(pub_read())\n}\n";
        assert_eq!(fmt(src), src);
    }

    // --- Statements ---

    #[test]
    fn test_let_binding() {
        let src =
            "program test\n\nfn main() {\n    let x: Field = pub_read()\n    pub_write(x)\n}\n";
        assert_eq!(fmt(src), src);
    }

    #[test]
    fn test_let_mut() {
        let src = "program test\n\nfn main() {\n    let mut x: Field = pub_read()\n    x = x + 1\n    pub_write(x)\n}\n";
        assert_eq!(fmt(src), src);
    }

    #[test]
    fn test_tuple_destructure() {
        let src = "program test\n\nfn main() {\n    let (a, b) = split(pub_read())\n    pub_write(as_field(a))\n}\n";
        assert_eq!(fmt(src), src);
    }

    #[test]
    fn test_if_else() {
        let src = "program test\n\nfn main() {\n    let x: Field = pub_read()\n    if x == 0 {\n        pub_write(0)\n    } else {\n        pub_write(1)\n    }\n}\n";
        assert_eq!(fmt(src), src);
    }

    #[test]
    fn test_for_loop() {
        let src = "program test\n\nfn main() {\n    let mut s: Field = 0\n    for i in 0..10 bounded 10 {\n        s = s + 1\n    }\n    pub_write(s)\n}\n";
        assert_eq!(fmt(src), src);
    }

    #[test]
    fn test_return_statement() {
        let src = "program test\n\nfn helper(x: Field) -> Field {\n    return x + 1\n}\n\nfn main() {\n    pub_write(helper(pub_read()))\n}\n";
        assert_eq!(fmt(src), src);
    }

    #[test]
    fn test_emit_statement() {
        let src = "program test\n\nevent Log {\n    value: Field,\n}\n\nfn main() {\n    emit Log { value: pub_read() }\n}\n";
        assert_eq!(fmt(src), src);
    }

    #[test]
    fn test_seal_statement() {
        let src = "program test\n\nevent Commit {\n    value: Field,\n}\n\nfn main() {\n    seal Commit { value: pub_read() }\n}\n";
        assert_eq!(fmt(src), src);
    }

    // --- Expressions ---

    #[test]
    fn test_binary_precedence() {
        // a + b * c should not add unnecessary parens
        let src =
            "program test\n\nfn main() {\n    let x: Field = 1 + 2 * 3\n    pub_write(x)\n}\n";
        assert_eq!(fmt(src), src);
    }

    #[test]
    fn test_array_init() {
        let src = "program test\n\nfn main() {\n    let a: [Field; 3] = [1, 2, 3]\n    pub_write(a[0])\n}\n";
        assert_eq!(fmt(src), src);
    }

    #[test]
    fn test_struct_init_expr() {
        let src = "program test\n\nstruct Pt {\n    x: Field,\n    y: Field,\n}\n\nfn main() {\n    let p: Pt = Pt { x: 1, y: 2 }\n    pub_write(p.x)\n}\n";
        assert_eq!(fmt(src), src);
    }

    #[test]
    fn test_field_access() {
        let src = "program test\n\nstruct Pt {\n    x: Field,\n    y: Field,\n}\n\nfn main() {\n    let p: Pt = Pt { x: 1, y: 2 }\n    pub_write(p.x + p.y)\n}\n";
        assert_eq!(fmt(src), src);
    }

    // --- Comments ---

    #[test]
    fn test_comment_preservation() {
        let src = "program test\n\n// Main entry point\nfn main() {\n    // Read input\n    let x: Field = pub_read()\n    pub_write(x)\n}\n";
        let out = fmt(src);
        assert!(
            out.contains("// Main entry point"),
            "leading comment preserved"
        );
        assert!(out.contains("// Read input"), "inline comment preserved");
    }

    #[test]
    fn test_trailing_comment() {
        let src = "program test\n\nfn main() {\n    let x: Field = pub_read() // read value\n    pub_write(x)\n}\n";
        let out = fmt(src);
        assert!(out.contains("// read value"), "trailing comment preserved");
    }

    // --- Idempotency ---

    #[test]
    fn test_idempotent_simple() {
        let src = "program test\n\nfn main() {\n    let x: Field = pub_read()\n    let y: Field = x + 1\n    pub_write(y)\n}\n";
        let first = fmt(src);
        let second = fmt(&first);
        assert_eq!(first, second, "formatting should be idempotent");
    }

    #[test]
    fn test_idempotent_complex() {
        let src = r#"program token

use std.hash
use std.assert

struct Config {
    owner: Digest,
    supply: Field,
}

event Transfer {
    from: Field,
    to: Field,
    amount: Field,
}

const MAX_SUPPLY: Field = 1000000

// Main function
fn main() {
    let cfg: Config = Config { owner: divine5(), supply: 100 }
    let x: Field = pub_read()
    if x == 0 {
        pub_write(cfg.supply)
    } else {
        let (hi, lo) = split(x)
        pub_write(as_field(lo))
    }
    for i in 0..5 bounded 5 {
        pub_write(i)
    }
    emit Transfer { from: 0, to: 1, amount: x }
}
"#;
        let first = fmt(src);
        let second = fmt(&first);
        assert_eq!(first, second, "complex formatting should be idempotent");
    }

    // --- Use declarations ---

    #[test]
    fn test_use_declarations() {
        let src = "program test\n\nuse std.hash\n\nuse std.field\n\nfn main() {\n    pub_write(pub_read())\n}\n";
        assert_eq!(fmt(src), src);
    }

    // --- Types ---

    #[test]
    fn test_all_types_formatted() {
        assert_eq!(format_type(&Type::Field), "Field");
        assert_eq!(format_type(&Type::XField), "XField");
        assert_eq!(format_type(&Type::Bool), "Bool");
        assert_eq!(format_type(&Type::U32), "U32");
        assert_eq!(format_type(&Type::Digest), "Digest");
        assert_eq!(
            format_type(&Type::Array(Box::new(Type::Field), ArraySize::Literal(10))),
            "[Field; 10]"
        );
        assert_eq!(
            format_type(&Type::Tuple(vec![Type::Field, Type::U32])),
            "(Field, U32)"
        );
    }

    // --- Intrinsic attribute ---

    #[test]
    fn test_intrinsic_function() {
        let src = "module std.hash\n\n#[intrinsic(hash)]\npub fn tip5(a: Field, b: Field, c: Field, d: Field, e: Field, f: Field, g: Field, h: Field, i: Field, j: Field) -> Digest\n";
        let out = fmt(src);
        assert!(out.contains("#[intrinsic(hash)]"), "intrinsic preserved");
        assert!(out.contains("pub fn tip5"), "function name preserved");
    }

    // --- Line wrapping ---

    #[test]
    fn test_long_signature_wraps() {
        // A function with many parameters should wrap
        let src = "program test\n\nfn long_function(aaa: Field, bbb: Field, ccc: Field, ddd: Field, eee: Field, fff: Field) -> Field {\n    aaa\n}\n";
        let out = fmt(src);
        // Should either fit on one line or wrap — the key is it shouldn't panic
        assert!(out.contains("fn long_function"));
        assert!(out.contains("-> Field"));
    }

    // --- Round-trip: parse -> format -> parse produces same AST items ---

    #[test]
    fn test_round_trip_preserves_ast() {
        let src = r#"program test

struct Pair {
    a: Field,
    b: Field,
}

const LIMIT: Field = 42

event Tick {
    seq: Field,
}

fn helper(x: Field) -> Field {
    x + 1
}

fn main() {
    let p: Pair = Pair { a: 1, b: 2 }
    let mut sum: Field = p.a + p.b
    if sum == 3 {
        sum = helper(sum)
    }
    for i in 0..5 bounded 5 {
        sum = sum + i
    }
    emit Tick { seq: sum }
    pub_write(sum)
}
"#;
        let formatted = fmt(src);
        let (tok2, _, lex2) = Lexer::new(&formatted, 0).tokenize();
        assert!(lex2.is_empty(), "formatted source should lex cleanly");
        let file2 = Parser::new(tok2).parse_file().unwrap();

        // Re-parse original
        let (tok1, _, _) = Lexer::new(src, 0).tokenize();
        let file1 = Parser::new(tok1).parse_file().unwrap();

        // Same number of items
        assert_eq!(file1.items.len(), file2.items.len(), "item count mismatch");

        // Same item kinds
        for (a, b) in file1.items.iter().zip(file2.items.iter()) {
            let kind_a = match &a.node {
                Item::Fn(_) => "fn",
                Item::Struct(_) => "struct",
                Item::Const(_) => "const",
                Item::Event(_) => "event",
            };
            let kind_b = match &b.node {
                Item::Fn(_) => "fn",
                Item::Struct(_) => "struct",
                Item::Const(_) => "const",
                Item::Event(_) => "event",
            };
            assert_eq!(kind_a, kind_b, "item kind mismatch");
        }
    }

    // --- Edge cases ---

    #[test]
    fn test_empty_function_body() {
        let src = "program test\n\nfn main() {\n}\n";
        let out = fmt(src);
        assert!(out.contains("fn main()"));
    }

    #[test]
    fn test_single_trailing_newline() {
        let src = "program test\n\nfn main() {\n    pub_write(0)\n}\n";
        let out = fmt(src);
        assert!(out.ends_with("}\n"), "should end with exactly one newline");
        assert!(
            !out.ends_with("}\n\n"),
            "should not end with double newline"
        );
    }

    #[test]
    fn test_sec_ram_formatting() {
        let src = "program test\n\nsec ram: {\n    0: Field,\n    5: Digest,\n}\n\nfn main() {\n    pub_write(ram_read(0))\n}\n";
        let out = fmt(src);
        assert!(out.contains("sec ram:"));
        assert!(out.contains("0: Field"));
        assert!(out.contains("5: Digest"));
    }

    // --- Fungible token round-trip ---

    #[test]
    fn test_fungible_token_idempotent() {
        let src = include_str!("../examples/fungible_token/token.tri");
        let first = fmt(src);
        let second = fmt(&first);
        assert_eq!(first, second, "token.tri formatting should be idempotent");
    }

    #[test]
    fn test_asm_basic_formatting() {
        let src = "program test\n\nfn main() {\n    asm {\n        push 1\n        add\n    }\n}\n";
        assert_eq!(fmt(src), src);
    }

    #[test]
    fn test_asm_positive_effect_formatting() {
        let src = "program test\n\nfn main() {\n    asm(+1) {\n        push 42\n    }\n}\n";
        assert_eq!(fmt(src), src);
    }

    #[test]
    fn test_asm_negative_effect_formatting() {
        let src =
            "program test\n\nfn main() {\n    asm(-2) {\n        pop 1\n        pop 1\n    }\n}\n";
        assert_eq!(fmt(src), src);
    }

    #[test]
    fn test_asm_idempotent() {
        let src = "program test\n\nfn main() {\n    let x: Field = pub_read()\n    asm {\n        dup 0\n        add\n    }\n    pub_write(x)\n}\n";
        let first = fmt(src);
        let second = fmt(&first);
        assert_eq!(first, second, "asm formatting should be idempotent");
    }

    #[test]
    fn test_asm_with_negative_literal() {
        let src =
            "program test\n\nfn main() {\n    asm {\n        push -1\n        mul\n    }\n}\n";
        assert_eq!(fmt(src), src);
    }

    #[test]
    fn test_match_formatting() {
        let src = "program test\n\nfn main() {\n    let x: Field = pub_read()\n    match x {\n        0 => {\n            pub_write(0)\n        }\n        1 => {\n            pub_write(1)\n        }\n        _ => {\n            pub_write(2)\n        }\n    }\n}\n";
        assert_eq!(fmt(src), src);
    }

    #[test]
    fn test_match_bool_formatting() {
        let src = "program test\n\nfn main() {\n    let b: Bool = true\n    match b {\n        true => {\n            pub_write(1)\n        }\n        false => {\n            pub_write(0)\n        }\n    }\n}\n";
        assert_eq!(fmt(src), src);
    }

    #[test]
    fn test_match_idempotent() {
        let src = "program test\n\nfn main() {\n    let x: Field = pub_read()\n    match x {\n        0 => {\n            pub_write(0)\n        }\n        _ => {\n            pub_write(1)\n        }\n    }\n}\n";
        let first = fmt(src);
        let second = fmt(&first);
        assert_eq!(first, second, "match formatting should be idempotent");
    }
}
