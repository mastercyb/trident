use crate::ast::*;
use crate::diagnostic::Diagnostic;
use crate::lexeme::Lexeme;
use crate::span::{Span, Spanned};

pub struct Parser {
    tokens: Vec<Spanned<Lexeme>>,
    pos: usize,
    diagnostics: Vec<Diagnostic>,
}

impl Parser {
    pub fn new(tokens: Vec<Spanned<Lexeme>>) -> Self {
        Self {
            tokens,
            pos: 0,
            diagnostics: Vec::new(),
        }
    }

    pub fn parse_file(mut self) -> Result<File, Vec<Diagnostic>> {
        let file = if self.at(&Lexeme::Program) {
            self.parse_program()
        } else if self.at(&Lexeme::Module) {
            self.parse_module()
        } else {
            self.error_at_current("expected 'program' or 'module' declaration");
            return Err(self.diagnostics);
        };

        if !self.diagnostics.is_empty() {
            return Err(self.diagnostics);
        }
        Ok(file)
    }

    fn parse_program(&mut self) -> File {
        let _start = self.current_span();
        self.expect(&Lexeme::Program);
        let name = self.expect_ident();

        let uses = self.parse_uses();
        let declarations = self.parse_declarations();
        let items = self.parse_items();

        File {
            kind: FileKind::Program,
            name,
            uses,
            declarations,
            items,
        }
    }

    fn parse_module(&mut self) -> File {
        self.expect(&Lexeme::Module);
        let name = self.expect_ident();

        // Module name can be dotted: `module std.hash`
        let mut name_str = name.node.clone();
        while self.eat(&Lexeme::Dot) {
            let part = self.expect_ident();
            name_str.push('.');
            name_str.push_str(&part.node);
        }
        let name = Spanned::new(name_str, name.span);

        let uses = self.parse_uses();
        let items = self.parse_items();

        File {
            kind: FileKind::Module,
            name,
            uses,
            declarations: Vec::new(),
            items,
        }
    }

    fn parse_declarations(&mut self) -> Vec<Declaration> {
        let mut decls = Vec::new();
        loop {
            if self.at(&Lexeme::Pub) && self.is_declaration_ahead() {
                self.advance(); // consume pub
                let kind_name = self.expect_ident();
                self.expect(&Lexeme::Colon);
                let ty = self.parse_type();
                match kind_name.node.as_str() {
                    "input" => decls.push(Declaration::PubInput(ty)),
                    "output" => decls.push(Declaration::PubOutput(ty)),
                    _ => {}
                }
            } else if self.at(&Lexeme::Sec) && self.is_sec_declaration_ahead() {
                self.advance(); // consume sec
                let kind_name = self.expect_ident();
                self.expect(&Lexeme::Colon);
                if kind_name.node == "input" {
                    let ty = self.parse_type();
                    decls.push(Declaration::SecInput(ty));
                } else if kind_name.node == "ram" {
                    // sec ram: { addr: Type, addr: Type, ... }
                    self.expect(&Lexeme::LBrace);
                    let mut entries = Vec::new();
                    while !self.at(&Lexeme::RBrace) && !self.at(&Lexeme::Eof) {
                        // Parse address (integer literal)
                        let addr_tok = self.advance();
                        let addr = if let Lexeme::Integer(n) = &addr_tok.node {
                            *n
                        } else {
                            0 // error recovery
                        };
                        self.expect(&Lexeme::Colon);
                        let ty = self.parse_type();
                        entries.push((addr, ty));
                        // Optional comma
                        if self.at(&Lexeme::Comma) {
                            self.advance();
                        }
                    }
                    self.expect(&Lexeme::RBrace);
                    decls.push(Declaration::SecRam(entries));
                }
            } else {
                break;
            }
        }
        decls
    }

    /// Check if `pub` is followed by `input` or `output` (declaration, not item).
    fn is_declaration_ahead(&self) -> bool {
        if self.pos + 1 >= self.tokens.len() {
            return false;
        }
        match &self.tokens[self.pos + 1].node {
            Lexeme::Ident(name) => name == "input" || name == "output",
            _ => false,
        }
    }

    /// Check if `sec` is followed by `input` or `ram`.
    fn is_sec_declaration_ahead(&self) -> bool {
        if self.pos + 1 >= self.tokens.len() {
            return false;
        }
        match &self.tokens[self.pos + 1].node {
            Lexeme::Ident(name) => name == "input" || name == "ram",
            _ => false,
        }
    }

    fn parse_uses(&mut self) -> Vec<Spanned<ModulePath>> {
        let mut uses = Vec::new();
        while self.at(&Lexeme::Use) {
            let start = self.current_span();
            self.advance();
            let path = self.parse_module_path();
            let span = start.merge(self.prev_span());
            uses.push(Spanned::new(path, span));
        }
        uses
    }

    fn parse_module_path(&mut self) -> ModulePath {
        let first = self.expect_ident();
        let mut parts = vec![first.node];
        while self.eat(&Lexeme::Dot) {
            if let Some(ident) = self.try_ident() {
                parts.push(ident.node);
            } else {
                break;
            }
        }
        ModulePath(parts)
    }

    fn parse_items(&mut self) -> Vec<Spanned<Item>> {
        let mut items = Vec::new();
        while !self.at(&Lexeme::Eof) {
            let start = self.current_span();

            // Handle #[attr] before pub: #[intrinsic(name)] pub fn ...
            if self.at(&Lexeme::Hash) {
                let attr = self.parse_attribute();
                let is_pub = self.eat(&Lexeme::Pub);
                let item = self.parse_fn_with_attr(is_pub, Some(attr));
                let span = start.merge(self.prev_span());
                items.push(Spanned::new(Item::Fn(item), span));
                continue;
            }

            let is_pub = self.eat(&Lexeme::Pub);

            if self.at(&Lexeme::Const) {
                let item = self.parse_const(is_pub);
                let span = start.merge(self.prev_span());
                items.push(Spanned::new(Item::Const(item), span));
            } else if self.at(&Lexeme::Struct) {
                let item = self.parse_struct(is_pub);
                let span = start.merge(self.prev_span());
                items.push(Spanned::new(Item::Struct(item), span));
            } else if self.at(&Lexeme::Event) {
                let item = self.parse_event();
                let span = start.merge(self.prev_span());
                items.push(Spanned::new(Item::Event(item), span));
            } else if self.at(&Lexeme::Fn) || self.at(&Lexeme::Hash) {
                let item = self.parse_fn(is_pub);
                let span = start.merge(self.prev_span());
                items.push(Spanned::new(Item::Fn(item), span));
            } else {
                self.error_at_current("expected item (fn, struct, event, or const)");
                self.advance(); // skip to recover
            }
        }
        items
    }

    fn parse_const(&mut self, is_pub: bool) -> ConstDef {
        self.expect(&Lexeme::Const);
        let name = self.expect_ident();
        self.expect(&Lexeme::Colon);
        let ty = self.parse_type();
        self.expect(&Lexeme::Eq);
        let value = self.parse_expr();
        ConstDef {
            is_pub,
            name,
            ty,
            value,
        }
    }

    fn parse_struct(&mut self, is_pub: bool) -> StructDef {
        self.expect(&Lexeme::Struct);
        let name = self.expect_ident();
        self.expect(&Lexeme::LBrace);
        let mut fields = Vec::new();
        while !self.at(&Lexeme::RBrace) && !self.at(&Lexeme::Eof) {
            let field_pub = self.eat(&Lexeme::Pub);
            let field_name = self.expect_ident();
            self.expect(&Lexeme::Colon);
            let field_ty = self.parse_type();
            fields.push(StructField {
                is_pub: field_pub,
                name: field_name,
                ty: field_ty,
            });
            if !self.eat(&Lexeme::Comma) {
                break;
            }
        }
        self.expect(&Lexeme::RBrace);
        StructDef {
            is_pub,
            name,
            fields,
        }
    }

    fn parse_fn(&mut self, is_pub: bool) -> FnDef {
        let intrinsic = if self.at(&Lexeme::Hash) {
            Some(self.parse_attribute())
        } else {
            None
        };
        self.parse_fn_with_attr(is_pub, intrinsic)
    }

    fn parse_fn_with_attr(&mut self, is_pub: bool, intrinsic: Option<Spanned<String>>) -> FnDef {
        self.expect(&Lexeme::Fn);
        let name = self.expect_ident();
        self.expect(&Lexeme::LParen);
        let params = self.parse_params();
        self.expect(&Lexeme::RParen);

        let return_ty = if self.eat(&Lexeme::Arrow) {
            Some(self.parse_type())
        } else {
            None
        };

        let body = if self.at(&Lexeme::LBrace) {
            Some(self.parse_block())
        } else {
            None
        };

        FnDef {
            is_pub,
            intrinsic,
            name,
            params,
            return_ty,
            body,
        }
    }

    fn parse_attribute(&mut self) -> Spanned<String> {
        let start = self.current_span();
        self.expect(&Lexeme::Hash);
        self.expect(&Lexeme::LBracket);
        let name = self.expect_ident();
        self.expect(&Lexeme::LParen);
        let value = self.expect_ident();
        self.expect(&Lexeme::RParen);
        self.expect(&Lexeme::RBracket);
        let span = start.merge(self.prev_span());
        Spanned::new(format!("{}({})", name.node, value.node), span)
    }

    fn parse_params(&mut self) -> Vec<Param> {
        let mut params = Vec::new();
        while !self.at(&Lexeme::RParen) && !self.at(&Lexeme::Eof) {
            let name = self.expect_ident();
            self.expect(&Lexeme::Colon);
            let ty = self.parse_type();
            params.push(Param { name, ty });
            if !self.eat(&Lexeme::Comma) {
                break;
            }
        }
        params
    }

    // --- Type parsing ---

    fn parse_type(&mut self) -> Spanned<Type> {
        let start = self.current_span();
        let ty = match self.peek() {
            Lexeme::FieldTy => {
                self.advance();
                Type::Field
            }
            Lexeme::XFieldTy => {
                self.advance();
                Type::XField
            }
            Lexeme::BoolTy => {
                self.advance();
                Type::Bool
            }
            Lexeme::U32Ty => {
                self.advance();
                Type::U32
            }
            Lexeme::DigestTy => {
                self.advance();
                Type::Digest
            }
            Lexeme::LBracket => {
                self.advance();
                let inner = self.parse_type();
                self.expect(&Lexeme::Semicolon);
                let size = self.expect_integer();
                self.expect(&Lexeme::RBracket);
                Type::Array(Box::new(inner.node), size)
            }
            Lexeme::LParen => {
                self.advance();
                let mut types = vec![self.parse_type().node];
                while self.eat(&Lexeme::Comma) {
                    types.push(self.parse_type().node);
                }
                self.expect(&Lexeme::RParen);
                Type::Tuple(types)
            }
            Lexeme::Ident(_) => {
                let path = self.parse_module_path();
                Type::Named(path)
            }
            _ => {
                self.error_at_current("expected type");
                Type::Field // fallback
            }
        };
        let span = start.merge(self.prev_span());
        Spanned::new(ty, span)
    }

    // --- Block and statement parsing ---

    fn parse_block(&mut self) -> Spanned<Block> {
        let start = self.current_span();
        self.expect(&Lexeme::LBrace);

        let mut stmts = Vec::new();
        let mut tail_expr = None;

        while !self.at(&Lexeme::RBrace) && !self.at(&Lexeme::Eof) {
            // Try to parse a statement
            if self.at(&Lexeme::Let) {
                stmts.push(self.parse_let_stmt());
            } else if self.at(&Lexeme::If) {
                stmts.push(self.parse_if_stmt());
            } else if self.at(&Lexeme::For) {
                stmts.push(self.parse_for_stmt());
            } else if self.at(&Lexeme::Return) {
                stmts.push(self.parse_return_stmt());
            } else if self.at(&Lexeme::Emit) {
                stmts.push(self.parse_emit_stmt());
            } else if self.at(&Lexeme::Seal) {
                stmts.push(self.parse_seal_stmt());
            } else {
                // Parse as expression statement or tail expression
                let expr = self.parse_expr();

                if self.at(&Lexeme::RBrace) {
                    // Tail expression: last expression before }, used as return value
                    tail_expr = Some(Box::new(expr));
                } else if self.eat(&Lexeme::Eq) {
                    // Assignment: expr = value or (a, b) = value
                    if let Expr::Tuple(elements) = &expr.node {
                        // Tuple assignment: (a, b) = expr
                        let names: Vec<Spanned<String>> = elements
                            .iter()
                            .map(|e| {
                                if let Expr::Var(name) = &e.node {
                                    Spanned::new(name.clone(), e.span)
                                } else {
                                    Spanned::new("_error_".to_string(), e.span)
                                }
                            })
                            .collect();
                        let value = self.parse_expr();
                        let span = expr.span.merge(value.span);
                        stmts.push(Spanned::new(Stmt::TupleAssign { names, value }, span));
                    } else {
                        let place = self.expr_to_place(&expr);
                        let value = self.parse_expr();
                        let span = expr.span.merge(value.span);
                        stmts.push(Spanned::new(Stmt::Assign { place, value }, span));
                    }
                } else {
                    let span = expr.span;
                    stmts.push(Spanned::new(Stmt::Expr(expr), span));
                }
            }
        }

        let end = self.current_span();
        self.expect(&Lexeme::RBrace);
        let span = start.merge(end);
        Spanned::new(Block { stmts, tail_expr }, span)
    }

    fn parse_let_stmt(&mut self) -> Spanned<Stmt> {
        let start = self.current_span();
        self.expect(&Lexeme::Let);
        let mutable = self.eat(&Lexeme::Mut);

        let pattern = if self.eat(&Lexeme::LParen) {
            // Tuple destructuring: let (a, b, ...) = ...
            let mut names = Vec::new();
            while !self.at(&Lexeme::RParen) && !self.at(&Lexeme::Eof) {
                let name = if self.at(&Lexeme::Underscore) {
                    let span = self.current_span();
                    self.advance();
                    Spanned::new("_".to_string(), span)
                } else {
                    self.expect_ident()
                };
                names.push(name);
                if !self.eat(&Lexeme::Comma) {
                    break;
                }
            }
            self.expect(&Lexeme::RParen);
            Pattern::Tuple(names)
        } else if self.at(&Lexeme::Underscore) {
            let span = self.current_span();
            self.advance();
            Pattern::Name(Spanned::new("_".to_string(), span))
        } else {
            Pattern::Name(self.expect_ident())
        };

        let ty = if self.eat(&Lexeme::Colon) {
            Some(self.parse_type())
        } else {
            None
        };

        self.expect(&Lexeme::Eq);
        let init = self.parse_expr();
        let span = start.merge(init.span);
        Spanned::new(
            Stmt::Let {
                mutable,
                pattern,
                ty,
                init,
            },
            span,
        )
    }

    fn parse_if_stmt(&mut self) -> Spanned<Stmt> {
        let start = self.current_span();
        self.expect(&Lexeme::If);
        let cond = self.parse_expr();
        let then_block = self.parse_block();
        let else_block = if self.eat(&Lexeme::Else) {
            if self.at(&Lexeme::If) {
                // `else if` — desugar to `else { if ... }`
                let inner_if = self.parse_if_stmt();
                let span = inner_if.span;
                Some(Spanned::new(
                    Block {
                        stmts: vec![inner_if],
                        tail_expr: None,
                    },
                    span,
                ))
            } else {
                Some(self.parse_block())
            }
        } else {
            None
        };
        let span = start.merge(self.prev_span());
        Spanned::new(
            Stmt::If {
                cond,
                then_block,
                else_block,
            },
            span,
        )
    }

    fn parse_for_stmt(&mut self) -> Spanned<Stmt> {
        let start = self.current_span();
        self.expect(&Lexeme::For);

        let var = if self.at(&Lexeme::Underscore) {
            let span = self.current_span();
            self.advance();
            Spanned::new("_".to_string(), span)
        } else {
            self.expect_ident()
        };

        self.expect(&Lexeme::In);
        let range_start = self.parse_expr();
        self.expect(&Lexeme::DotDot);
        let range_end = self.parse_expr();

        let bound = if self.eat(&Lexeme::Bounded) {
            Some(self.expect_integer())
        } else {
            None
        };

        let body = self.parse_block();
        let span = start.merge(self.prev_span());
        Spanned::new(
            Stmt::For {
                var,
                start: range_start,
                end: range_end,
                bound,
                body,
            },
            span,
        )
    }

    fn parse_return_stmt(&mut self) -> Spanned<Stmt> {
        let start = self.current_span();
        self.expect(&Lexeme::Return);
        let value = if !self.at(&Lexeme::RBrace) && !self.at(&Lexeme::Eof) {
            Some(self.parse_expr())
        } else {
            None
        };
        let span = start.merge(self.prev_span());
        Spanned::new(Stmt::Return(value), span)
    }

    fn parse_event(&mut self) -> EventDef {
        self.expect(&Lexeme::Event);
        let name = self.expect_ident();
        self.expect(&Lexeme::LBrace);
        let mut fields = Vec::new();
        while !self.at(&Lexeme::RBrace) && !self.at(&Lexeme::Eof) {
            let field_name = self.expect_ident();
            self.expect(&Lexeme::Colon);
            let field_ty = self.parse_type();
            fields.push(EventField {
                name: field_name,
                ty: field_ty,
            });
            if !self.eat(&Lexeme::Comma) {
                break;
            }
        }
        self.expect(&Lexeme::RBrace);
        EventDef { name, fields }
    }

    fn parse_emit_stmt(&mut self) -> Spanned<Stmt> {
        let start = self.current_span();
        self.expect(&Lexeme::Emit);
        let event_name = self.expect_ident();
        self.expect(&Lexeme::LBrace);
        let fields = self.parse_struct_init_fields();
        self.expect(&Lexeme::RBrace);
        let span = start.merge(self.prev_span());
        Spanned::new(Stmt::Emit { event_name, fields }, span)
    }

    fn parse_seal_stmt(&mut self) -> Spanned<Stmt> {
        let start = self.current_span();
        self.expect(&Lexeme::Seal);
        let event_name = self.expect_ident();
        self.expect(&Lexeme::LBrace);
        let fields = self.parse_struct_init_fields();
        self.expect(&Lexeme::RBrace);
        let span = start.merge(self.prev_span());
        Spanned::new(Stmt::Seal { event_name, fields }, span)
    }

    // --- Expression parsing (Pratt / precedence climbing) ---

    fn parse_expr(&mut self) -> Spanned<Expr> {
        self.parse_expr_bp(0)
    }

    fn parse_expr_bp(&mut self, min_bp: u8) -> Spanned<Expr> {
        let mut lhs = self.parse_primary();

        // Apply postfix operators: .field and [index]
        lhs = self.parse_postfix(lhs);

        loop {
            let op = match self.peek() {
                Lexeme::EqEq => BinOp::Eq,
                Lexeme::Lt => BinOp::Lt,
                Lexeme::Plus => BinOp::Add,
                Lexeme::Star => BinOp::Mul,
                Lexeme::StarDot => BinOp::XFieldMul,
                Lexeme::Amp => BinOp::BitAnd,
                Lexeme::Caret => BinOp::BitXor,
                Lexeme::SlashPercent => BinOp::DivMod,
                _ => break,
            };

            let (l_bp, r_bp) = op_binding_power(op);
            if l_bp < min_bp {
                break;
            }

            self.advance(); // consume operator
            let rhs = self.parse_expr_bp(r_bp);
            let span = lhs.span.merge(rhs.span);
            lhs = Spanned::new(
                Expr::BinOp {
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
                span,
            );
        }

        lhs
    }

    /// Parse postfix operations: .field, [index], .method() chains
    fn parse_postfix(&mut self, mut expr: Spanned<Expr>) -> Spanned<Expr> {
        loop {
            if self.at(&Lexeme::LBracket) {
                // Index access: expr[idx]
                self.advance();
                let index = self.parse_expr();
                self.expect(&Lexeme::RBracket);
                let span = expr.span.merge(self.prev_span());
                expr = Spanned::new(
                    Expr::Index {
                        expr: Box::new(expr),
                        index: Box::new(index),
                    },
                    span,
                );
            } else {
                break;
            }
        }
        expr
    }

    fn parse_primary(&mut self) -> Spanned<Expr> {
        let start = self.current_span();

        match self.peek().clone() {
            Lexeme::Integer(n) => {
                self.advance();
                Spanned::new(Expr::Literal(Literal::Integer(n)), start)
            }
            Lexeme::True => {
                self.advance();
                Spanned::new(Expr::Literal(Literal::Bool(true)), start)
            }
            Lexeme::False => {
                self.advance();
                Spanned::new(Expr::Literal(Literal::Bool(false)), start)
            }
            Lexeme::LParen => {
                self.advance();
                let first = self.parse_expr();
                if self.eat(&Lexeme::Comma) {
                    // Tuple
                    let mut elements = vec![first];
                    elements.push(self.parse_expr());
                    while self.eat(&Lexeme::Comma) {
                        if self.at(&Lexeme::RParen) {
                            break;
                        }
                        elements.push(self.parse_expr());
                    }
                    self.expect(&Lexeme::RParen);
                    let span = start.merge(self.prev_span());
                    Spanned::new(Expr::Tuple(elements), span)
                } else {
                    // Parenthesized expression
                    self.expect(&Lexeme::RParen);
                    first
                }
            }
            Lexeme::LBracket => {
                self.advance();
                let mut elements = Vec::new();
                while !self.at(&Lexeme::RBracket) && !self.at(&Lexeme::Eof) {
                    elements.push(self.parse_expr());
                    if !self.eat(&Lexeme::Comma) {
                        break;
                    }
                }
                self.expect(&Lexeme::RBracket);
                let span = start.merge(self.prev_span());
                Spanned::new(Expr::ArrayInit(elements), span)
            }
            Lexeme::Ident(_) => {
                let path = self.parse_module_path();

                if self.at(&Lexeme::LParen) {
                    // Function call
                    self.advance();
                    let args = self.parse_call_args();
                    self.expect(&Lexeme::RParen);
                    let span = start.merge(self.prev_span());
                    Spanned::new(
                        Expr::Call {
                            path: Spanned::new(path, start),
                            args,
                        },
                        span,
                    )
                } else if self.at(&Lexeme::LBrace) && !path.0.is_empty() {
                    // Could be struct init — but only if it looks like one
                    // Struct names start with uppercase; lowercase idents are variables
                    let first_char = path.0.last().unwrap().chars().next().unwrap_or('a');
                    if first_char.is_uppercase() && self.is_struct_init_ahead() {
                        self.advance(); // consume {
                        let fields = self.parse_struct_init_fields();
                        self.expect(&Lexeme::RBrace);
                        let span = start.merge(self.prev_span());
                        Spanned::new(
                            Expr::StructInit {
                                path: Spanned::new(path, start),
                                fields,
                            },
                            span,
                        )
                    } else {
                        // Just a variable reference
                        let name = path.0.join(".");
                        Spanned::new(Expr::Var(name), start)
                    }
                } else {
                    // Variable reference
                    if path.0.len() == 1 {
                        Spanned::new(Expr::Var(path.0.into_iter().next().unwrap()), start)
                    } else {
                        // Dotted path as variable (field access chain)
                        let name = path.0.join(".");
                        Spanned::new(Expr::Var(name), start)
                    }
                }
            }
            _ => {
                self.error_at_current("expected expression");
                self.advance();
                Spanned::new(Expr::Literal(Literal::Integer(0)), start)
            }
        }
    }

    fn parse_call_args(&mut self) -> Vec<Spanned<Expr>> {
        let mut args = Vec::new();
        while !self.at(&Lexeme::RParen) && !self.at(&Lexeme::Eof) {
            args.push(self.parse_expr());
            if !self.eat(&Lexeme::Comma) {
                break;
            }
        }
        args
    }

    fn parse_struct_init_fields(&mut self) -> Vec<(Spanned<String>, Spanned<Expr>)> {
        let mut fields = Vec::new();
        while !self.at(&Lexeme::RBrace) && !self.at(&Lexeme::Eof) {
            let name = self.expect_ident();
            if self.eat(&Lexeme::Colon) {
                let value = self.parse_expr();
                fields.push((name, value));
            } else {
                // Shorthand: `{ name }` means `{ name: name }`
                let value = Spanned::new(Expr::Var(name.node.clone()), name.span);
                fields.push((name, value));
            }
            if !self.eat(&Lexeme::Comma) {
                break;
            }
        }
        fields
    }

    fn is_struct_init_ahead(&self) -> bool {
        // Check if after `{` we have `ident :` or `ident ,` or `ident }`
        if self.pos + 2 >= self.tokens.len() {
            return false;
        }
        if self.tokens[self.pos].node != Lexeme::LBrace {
            return false;
        }
        match &self.tokens[self.pos + 1].node {
            Lexeme::Ident(_) => {
                matches!(
                    &self.tokens[self.pos + 2].node,
                    Lexeme::Colon | Lexeme::Comma | Lexeme::RBrace
                )
            }
            Lexeme::RBrace => true, // empty struct init `Foo {}`
            _ => false,
        }
    }

    fn expr_to_place(&self, expr: &Spanned<Expr>) -> Spanned<Place> {
        match &expr.node {
            Expr::Var(name) => Spanned::new(Place::Var(name.clone()), expr.span),
            _ => {
                // For now, only support simple variable assignments
                Spanned::new(Place::Var("_error_".to_string()), expr.span)
            }
        }
    }

    // --- Utility methods ---

    fn peek(&self) -> &Lexeme {
        &self.tokens[self.pos].node
    }

    fn current_span(&self) -> Span {
        self.tokens[self.pos].span
    }

    fn prev_span(&self) -> Span {
        if self.pos > 0 {
            self.tokens[self.pos - 1].span
        } else {
            self.current_span()
        }
    }

    fn advance(&mut self) -> &Spanned<Lexeme> {
        let tok = &self.tokens[self.pos];
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        tok
    }

    fn at(&self, token: &Lexeme) -> bool {
        std::mem::discriminant(self.peek()) == std::mem::discriminant(token)
    }

    fn eat(&mut self, token: &Lexeme) -> bool {
        if self.at(token) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, token: &Lexeme) -> Span {
        if self.at(token) {
            let span = self.current_span();
            self.advance();
            span
        } else {
            self.error_at_current(&format!("expected {}", token.description()));
            self.current_span()
        }
    }

    fn expect_ident(&mut self) -> Spanned<String> {
        if let Lexeme::Ident(name) = self.peek().clone() {
            let span = self.current_span();
            self.advance();
            Spanned::new(name, span)
        } else {
            self.error_at_current("expected identifier");
            Spanned::new("_error_".to_string(), self.current_span())
        }
    }

    fn try_ident(&mut self) -> Option<Spanned<String>> {
        if let Lexeme::Ident(name) = self.peek().clone() {
            let span = self.current_span();
            self.advance();
            Some(Spanned::new(name, span))
        } else {
            None
        }
    }

    fn expect_integer(&mut self) -> u64 {
        if let Lexeme::Integer(n) = self.peek() {
            let n = *n;
            self.advance();
            n
        } else {
            self.error_at_current("expected integer literal");
            0
        }
    }

    fn error_at_current(&mut self, msg: &str) {
        self.diagnostics
            .push(Diagnostic::error(msg.to_string(), self.current_span()));
    }
}

/// Returns (left binding power, right binding power) for a binary operator.
/// Higher binding power = higher precedence.
fn op_binding_power(op: BinOp) -> (u8, u8) {
    match op {
        BinOp::Eq => (2, 3),                       // non-associative (low precedence)
        BinOp::Lt => (4, 5),                       // non-associative
        BinOp::Add => (6, 7),                      // left-associative
        BinOp::Mul | BinOp::XFieldMul => (8, 9),   // left-associative
        BinOp::BitAnd | BinOp::BitXor => (10, 11), // left-associative
        BinOp::DivMod => (12, 13),                 // non-associative
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

    fn parse(source: &str) -> File {
        let (tokens, _comments, lex_diags) = Lexer::new(source, 0).tokenize();
        assert!(lex_diags.is_empty(), "lex errors: {:?}", lex_diags);
        Parser::new(tokens).parse_file().unwrap()
    }

    #[test]
    fn test_minimal_program() {
        let file = parse("program test\n\nfn main() {\n}");
        assert_eq!(file.kind, FileKind::Program);
        assert_eq!(file.name.node, "test");
        assert_eq!(file.items.len(), 1);
    }

    #[test]
    fn test_function_with_params() {
        let file = parse("program test\n\nfn add(a: Field, b: Field) -> Field {\n    a + b\n}");
        assert_eq!(file.items.len(), 1);
        if let Item::Fn(f) = &file.items[0].node {
            assert_eq!(f.name.node, "add");
            assert_eq!(f.params.len(), 2);
            assert!(f.return_ty.is_some());
        } else {
            panic!("expected function");
        }
    }

    #[test]
    fn test_let_binding() {
        let file = parse("program test\n\nfn main() {\n    let a: Field = 42\n}");
        if let Item::Fn(f) = &file.items[0].node {
            let block = f.body.as_ref().unwrap();
            assert_eq!(block.node.stmts.len(), 1);
        }
    }

    #[test]
    fn test_function_call() {
        let file = parse("program test\n\nfn main() {\n    let a: Field = pub_read()\n}");
        if let Item::Fn(f) = &file.items[0].node {
            let block = f.body.as_ref().unwrap();
            if let Stmt::Let { init, .. } = &block.node.stmts[0].node {
                assert!(matches!(init.node, Expr::Call { .. }));
            }
        }
    }

    #[test]
    fn test_binary_expr() {
        let file = parse("program test\n\nfn main() {\n    let c: Field = a + b * c\n}");
        if let Item::Fn(f) = &file.items[0].node {
            let block = f.body.as_ref().unwrap();
            if let Stmt::Let { init, .. } = &block.node.stmts[0].node {
                // Should be Add(a, Mul(b, c)) due to precedence
                if let Expr::BinOp { op, .. } = &init.node {
                    assert_eq!(*op, BinOp::Add);
                } else {
                    panic!("expected binary op");
                }
            }
        }
    }

    #[test]
    fn test_module() {
        let file = parse("module merkle\n\npub fn verify(root: Digest) {\n}");
        assert_eq!(file.kind, FileKind::Module);
        assert_eq!(file.name.node, "merkle");
        if let Item::Fn(f) = &file.items[0].node {
            assert!(f.is_pub);
        }
    }

    #[test]
    fn test_program_declarations() {
        let file = parse("program test\n\npub input: [Field; 3]\npub output: Field\nsec input: [Field; 5]\n\nfn main() {\n}");
        assert_eq!(file.declarations.len(), 3);
        assert!(matches!(file.declarations[0], Declaration::PubInput(_)));
        assert!(matches!(file.declarations[1], Declaration::PubOutput(_)));
        assert!(matches!(file.declarations[2], Declaration::SecInput(_)));
    }

    #[test]
    fn test_sec_ram_declaration() {
        let file = parse(
            "program test\n\nsec ram: {\n    17: Field,\n    42: Field,\n}\n\nfn main() {\n}",
        );
        assert_eq!(file.declarations.len(), 1);
        if let Declaration::SecRam(entries) = &file.declarations[0] {
            assert_eq!(entries.len(), 2);
            assert_eq!(entries[0].0, 17);
            assert_eq!(entries[1].0, 42);
        } else {
            panic!("expected SecRam declaration");
        }
    }

    #[test]
    fn test_tuple_destructure_let() {
        let file = parse("program test\nfn main() {\n    let (a, b): (Field, Field) = (pub_read(), pub_read())\n}");
        if let Item::Fn(f) = &file.items[0].node {
            let block = f.body.as_ref().unwrap();
            assert_eq!(block.node.stmts.len(), 1);
            if let Stmt::Let {
                pattern: Pattern::Tuple(names),
                ..
            } = &block.node.stmts[0].node
            {
                assert_eq!(names.len(), 2);
                assert_eq!(names[0].node, "a");
                assert_eq!(names[1].node, "b");
            } else {
                panic!("expected tuple destructuring let");
            }
        }
    }

    #[test]
    fn test_event_declaration() {
        let file = parse("program test\nevent Transfer {\n    from: Field,\n    to: Field,\n    amount: Field,\n}\nfn main() {\n}");
        assert_eq!(file.items.len(), 2); // event + fn
        if let Item::Event(e) = &file.items[0].node {
            assert_eq!(e.name.node, "Transfer");
            assert_eq!(e.fields.len(), 3);
            assert_eq!(e.fields[0].name.node, "from");
            assert_eq!(e.fields[1].name.node, "to");
            assert_eq!(e.fields[2].name.node, "amount");
        } else {
            panic!("expected event declaration");
        }
    }

    #[test]
    fn test_emit_statement() {
        let file = parse("program test\nevent Ev { x: Field }\nfn main() {\n    let a: Field = pub_read()\n    emit Ev { x: a }\n}");
        if let Item::Fn(f) = &file.items[1].node {
            let block = f.body.as_ref().unwrap();
            assert_eq!(block.node.stmts.len(), 2);
            if let Stmt::Emit { event_name, fields } = &block.node.stmts[1].node {
                assert_eq!(event_name.node, "Ev");
                assert_eq!(fields.len(), 1);
                assert_eq!(fields[0].0.node, "x");
            } else {
                panic!("expected emit statement");
            }
        }
    }

    #[test]
    fn test_seal_statement() {
        let file = parse("program test\nevent Ev { x: Field, y: Field }\nfn main() {\n    seal Ev { x: pub_read(), y: pub_read() }\n}");
        if let Item::Fn(f) = &file.items[1].node {
            let block = f.body.as_ref().unwrap();
            assert_eq!(block.node.stmts.len(), 1);
            if let Stmt::Seal { event_name, fields } = &block.node.stmts[0].node {
                assert_eq!(event_name.node, "Ev");
                assert_eq!(fields.len(), 2);
            } else {
                panic!("expected seal statement");
            }
        }
    }
}
