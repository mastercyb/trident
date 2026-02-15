mod expr;
mod items;
mod stmts;

#[cfg(test)]
mod tests;

use crate::ast::*;
use crate::lexer::Comment;

pub(crate) use expr::{format_expr, format_type};

const MAX_WIDTH: usize = 80;
const INDENT: &str = "    ";

/// Format a parsed Trident file back to source, preserving comments.
pub(crate) fn format_file(file: &File, comments: &[Comment]) -> String {
    let mut ctx = FormatCtx::new(comments);
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

pub(super) struct FormatCtx {
    pub(super) output: String,
    pub(super) comments: Vec<CommentEntry>,
}

#[derive(Clone)]
pub(super) struct CommentEntry {
    pub(super) text: String,
    pub(super) byte_offset: u32,
    pub(super) trailing: bool,
    pub(super) used: bool,
}

impl FormatCtx {
    fn new(comments: &[Comment]) -> Self {
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
        }
    }

    /// Emit leading comments that appear before `span_start`.
    pub(super) fn emit_leading_comments(&mut self, span_start: u32, indent: &str) {
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
    pub(super) fn emit_trailing_comment(&mut self, span_end: u32) {
        for i in 0..self.comments.len() {
            if self.comments[i].used || !self.comments[i].trailing {
                continue;
            }
            if self.comments[i].byte_offset >= span_end {
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
        let keyword = match file.kind {
            FileKind::Program => "program",
            FileKind::Module => "module",
        };
        self.emit_leading_comments(file.name.span.start, "");
        self.output.push_str(keyword);
        self.output.push(' ');
        self.output.push_str(&file.name.node);
        self.output.push('\n');

        for u in &file.uses {
            self.output.push('\n');
            self.emit_leading_comments(u.span.start, "");
            self.output.push_str("use ");
            self.output.push_str(&u.node.as_dotted());
            self.output.push('\n');
        }

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

        for item in &file.items {
            self.output.push('\n');
            self.emit_item(item, "");
        }

        self.emit_remaining_comments("");
    }

    pub(super) fn emit_block(&mut self, block: &Block, outer_indent: &str) {
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

    /// Format an expression, wrapping long function calls.
    pub(super) fn emit_expr_wrapped(&mut self, expr: &Expr, indent: &str) {
        let flat = format_expr(expr);
        let current_line_len = self.current_line_len();
        if current_line_len + flat.len() <= MAX_WIDTH {
            self.output.push_str(&flat);
        } else if let Expr::Call { path, args, .. } = expr {
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
            self.output.push_str(&flat);
        }
    }

    pub(super) fn current_line_len(&self) -> usize {
        match self.output.rfind('\n') {
            Some(pos) => self.output.len() - pos - 1,
            None => self.output.len(),
        }
    }
}
