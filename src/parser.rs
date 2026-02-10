use crate::ast::*;
use crate::diagnostic::Diagnostic;
use crate::lexeme::Lexeme;
use crate::span::{Span, Spanned};

const MAX_NESTING_DEPTH: u32 = 256;

pub(crate) struct Parser {
    tokens: Vec<Spanned<Lexeme>>,
    pos: usize,
    diagnostics: Vec<Diagnostic>,
    depth: u32,
}

impl Parser {
    pub(crate) fn new(tokens: Vec<Spanned<Lexeme>>) -> Self {
        Self {
            tokens,
            pos: 0,
            diagnostics: Vec::new(),
            depth: 0,
        }
    }

    fn enter_nesting(&mut self) -> bool {
        self.depth += 1;
        if self.depth > MAX_NESTING_DEPTH {
            self.error_with_help(
                "nesting depth exceeded (maximum 256 levels)",
                "simplify your program by extracting deeply nested code into functions",
            );
            return false;
        }
        true
    }

    fn exit_nesting(&mut self) {
        self.depth -= 1;
    }

    pub(crate) fn parse_file(mut self) -> Result<File, Vec<Diagnostic>> {
        let file = if self.at(&Lexeme::Program) {
            self.parse_program()
        } else if self.at(&Lexeme::Module) {
            self.parse_module()
        } else {
            self.error_with_help(
                "expected 'program' or 'module' declaration at the start of file",
                "every .tri file must begin with `program <name>` or `module <name>`",
            );
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

            // Parse attributes: #[cfg(flag)], #[intrinsic(name)], #[test],
            // #[requires(pred)], #[ensures(pred)]
            let mut cfg_attr: Option<Spanned<String>> = None;
            let mut intrinsic_attr: Option<Spanned<String>> = None;
            let mut is_test = false;
            let mut is_pure = false;
            let mut requires_attrs: Vec<Spanned<String>> = Vec::new();
            let mut ensures_attrs: Vec<Spanned<String>> = Vec::new();
            while self.at(&Lexeme::Hash) {
                let attr = self.parse_attribute();
                if attr.node.starts_with("cfg(") {
                    // Extract flag name from "cfg(flag)"
                    let flag = attr.node[4..attr.node.len() - 1].to_string();
                    cfg_attr = Some(Spanned::new(flag, attr.span));
                } else if attr.node.starts_with("intrinsic(") {
                    intrinsic_attr = Some(attr);
                } else if attr.node.starts_with("requires(") {
                    let pred = attr.node[9..attr.node.len() - 1].to_string();
                    requires_attrs.push(Spanned::new(pred, attr.span));
                } else if attr.node.starts_with("ensures(") {
                    let pred = attr.node[8..attr.node.len() - 1].to_string();
                    ensures_attrs.push(Spanned::new(pred, attr.span));
                } else if attr.node == "test" {
                    is_test = true;
                } else if attr.node == "pure" {
                    is_pure = true;
                } else {
                    self.error_at_current(
                        "unknown attribute; expected cfg, intrinsic, test, pure, requires, or ensures",
                    );
                }
            }

            let is_pub = self.eat(&Lexeme::Pub);

            if self.at(&Lexeme::Const) {
                if intrinsic_attr.is_some() {
                    self.error_at_current("#[intrinsic] is only allowed on functions");
                }
                if is_test {
                    self.error_at_current("#[test] is only allowed on functions");
                }
                if is_pure {
                    self.error_at_current("#[pure] is only allowed on functions");
                }
                if !requires_attrs.is_empty() || !ensures_attrs.is_empty() {
                    self.error_at_current(
                        "#[requires] and #[ensures] are only allowed on functions",
                    );
                }
                let item = self.parse_const(is_pub, cfg_attr);
                let span = start.merge(self.prev_span());
                items.push(Spanned::new(Item::Const(item), span));
            } else if self.at(&Lexeme::Struct) {
                if intrinsic_attr.is_some() {
                    self.error_at_current("#[intrinsic] is only allowed on functions");
                }
                if is_test {
                    self.error_at_current("#[test] is only allowed on functions");
                }
                if is_pure {
                    self.error_at_current("#[pure] is only allowed on functions");
                }
                if !requires_attrs.is_empty() || !ensures_attrs.is_empty() {
                    self.error_at_current(
                        "#[requires] and #[ensures] are only allowed on functions",
                    );
                }
                let item = self.parse_struct(is_pub, cfg_attr);
                let span = start.merge(self.prev_span());
                items.push(Spanned::new(Item::Struct(item), span));
            } else if self.at(&Lexeme::Event) {
                if intrinsic_attr.is_some() {
                    self.error_at_current("#[intrinsic] is only allowed on functions");
                }
                if is_test {
                    self.error_at_current("#[test] is only allowed on functions");
                }
                if is_pure {
                    self.error_at_current("#[pure] is only allowed on functions");
                }
                if !requires_attrs.is_empty() || !ensures_attrs.is_empty() {
                    self.error_at_current(
                        "#[requires] and #[ensures] are only allowed on functions",
                    );
                }
                let item = self.parse_event(cfg_attr);
                let span = start.merge(self.prev_span());
                items.push(Spanned::new(Item::Event(item), span));
            } else if self.at(&Lexeme::Fn) || self.at(&Lexeme::Hash) {
                let item = self.parse_fn_with_attr(
                    is_pub,
                    cfg_attr,
                    intrinsic_attr,
                    is_test,
                    is_pure,
                    requires_attrs,
                    ensures_attrs,
                );
                let span = start.merge(self.prev_span());
                items.push(Spanned::new(Item::Fn(item), span));
            } else {
                self.error_with_help(
                    "expected item (fn, struct, event, or const)",
                    "top-level items must be function, struct, event, or const definitions",
                );
                self.advance(); // skip to recover
            }
        }
        items
    }

    fn parse_const(&mut self, is_pub: bool, cfg: Option<Spanned<String>>) -> ConstDef {
        self.expect(&Lexeme::Const);
        let name = self.expect_ident();
        self.expect(&Lexeme::Colon);
        let ty = self.parse_type();
        self.expect(&Lexeme::Eq);
        let value = self.parse_expr();
        ConstDef {
            is_pub,
            cfg,
            name,
            ty,
            value,
        }
    }

    fn parse_struct(&mut self, is_pub: bool, cfg: Option<Spanned<String>>) -> StructDef {
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
            cfg,
            name,
            fields,
        }
    }

    fn parse_fn_with_attr(
        &mut self,
        is_pub: bool,
        cfg: Option<Spanned<String>>,
        intrinsic: Option<Spanned<String>>,
        is_test: bool,
        is_pure: bool,
        requires: Vec<Spanned<String>>,
        ensures: Vec<Spanned<String>>,
    ) -> FnDef {
        self.expect(&Lexeme::Fn);
        let name = self.expect_ident();

        // Parse optional size-generic parameters: fn name<N, M>(...)
        let type_params = self.parse_type_params();

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
            cfg,
            intrinsic,
            is_test,
            is_pure,
            requires,
            ensures,
            name,
            type_params,
            params,
            return_ty,
            body,
        }
    }

    /// Parse `<N, M>` size-generic parameter list (if present).
    fn parse_type_params(&mut self) -> Vec<Spanned<String>> {
        if !self.eat(&Lexeme::Lt) {
            return Vec::new();
        }
        let mut params = Vec::new();
        while !self.at(&Lexeme::Gt) && !self.at(&Lexeme::Eof) {
            params.push(self.expect_ident());
            if !self.eat(&Lexeme::Comma) {
                break;
            }
        }
        self.expect(&Lexeme::Gt);
        params
    }

    fn parse_attribute(&mut self) -> Spanned<String> {
        let start = self.current_span();
        self.expect(&Lexeme::Hash);
        self.expect(&Lexeme::LBracket);
        let name = self.expect_ident();
        if self.at(&Lexeme::LParen) {
            self.expect(&Lexeme::LParen);
            // Collect everything between ( and ) as raw text, handling nesting.
            // For simple attributes like cfg(flag) or intrinsic(name), this is
            // just an identifier. For requires/ensures predicates like
            // requires(x + y == z), this collects the full expression text.
            let mut depth = 1u32;
            let mut parts = Vec::new();
            while depth > 0 && !self.at(&Lexeme::Eof) {
                if self.at(&Lexeme::LParen) {
                    depth += 1;
                    parts.push("(".to_string());
                    self.advance();
                } else if self.at(&Lexeme::RParen) {
                    depth -= 1;
                    if depth > 0 {
                        parts.push(")".to_string());
                        self.advance();
                    }
                    // Don't advance the final RParen — we consume it below
                } else {
                    parts.push(self.current_lexeme_text());
                    self.advance();
                }
            }
            self.expect(&Lexeme::RParen);
            self.expect(&Lexeme::RBracket);
            let value = parts.join(" ");
            let span = start.merge(self.prev_span());
            Spanned::new(format!("{}({})", name.node, value.trim()), span)
        } else {
            self.expect(&Lexeme::RBracket);
            let span = start.merge(self.prev_span());
            Spanned::new(name.node, span)
        }
    }

    /// Get the text representation of the current token for attribute parsing.
    fn current_lexeme_text(&self) -> String {
        match self.peek() {
            Lexeme::Ident(s) => s.clone(),
            Lexeme::Integer(n) => n.to_string(),
            Lexeme::Plus => "+".to_string(),
            Lexeme::Star => "*".to_string(),
            Lexeme::Eq => "=".to_string(),
            Lexeme::EqEq => "==".to_string(),
            Lexeme::Lt => "<".to_string(),
            Lexeme::Gt => ">".to_string(),
            Lexeme::Amp => "&".to_string(),
            Lexeme::Caret => "^".to_string(),
            Lexeme::Dot => ".".to_string(),
            Lexeme::Comma => ",".to_string(),
            Lexeme::Colon => ":".to_string(),
            Lexeme::Arrow => "->".to_string(),
            Lexeme::LBracket => "[".to_string(),
            Lexeme::RBracket => "]".to_string(),
            Lexeme::Hash => "#".to_string(),
            other => format!("{:?}", other),
        }
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
                let size = self.parse_array_size_expr();
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
                self.error_with_help(
                    "expected type",
                    "valid types are: Field, XField, Bool, U32, Digest, [T; N], (T, U), or a struct name",
                );
                Type::Field // fallback
            }
        };
        let span = start.merge(self.prev_span());
        Spanned::new(ty, span)
    }

    // --- Array size expression parsing (compile-time arithmetic) ---

    /// Parse a compile-time size expression: `N`, `3`, `M + N`, `N * 2`, `M + N * 2`.
    /// Precedence: `*` binds tighter than `+`.
    fn parse_array_size_expr(&mut self) -> ArraySize {
        let mut left = self.parse_array_size_mul();
        while self.at(&Lexeme::Plus) {
            self.advance();
            let right = self.parse_array_size_mul();
            left = ArraySize::Add(Box::new(left), Box::new(right));
        }
        left
    }

    fn parse_array_size_mul(&mut self) -> ArraySize {
        let mut left = self.parse_array_size_atom();
        while self.at(&Lexeme::Star) {
            self.advance();
            let right = self.parse_array_size_atom();
            left = ArraySize::Mul(Box::new(left), Box::new(right));
        }
        left
    }

    fn parse_array_size_atom(&mut self) -> ArraySize {
        if let Lexeme::Integer(n) = self.peek() {
            let n = *n;
            self.advance();
            ArraySize::Literal(n)
        } else if let Lexeme::Ident(_) = self.peek() {
            let ident = self.expect_ident();
            ArraySize::Param(ident.node)
        } else if self.at(&Lexeme::LParen) {
            self.advance();
            let inner = self.parse_array_size_expr();
            self.expect(&Lexeme::RParen);
            inner
        } else {
            self.error_with_help(
                "expected array size (integer literal or size parameter name)",
                "array sizes are written as `N`, `3`, `M + N`, or `N * 2`",
            );
            ArraySize::Literal(0)
        }
    }

    // --- Block and statement parsing ---

    fn parse_block(&mut self) -> Spanned<Block> {
        if !self.enter_nesting() {
            let span = self.current_span();
            // Skip to EOF to abort parsing entirely — the nesting
            // depth error has already been recorded.
            while !self.at(&Lexeme::Eof) {
                self.advance();
            }
            return Spanned::new(
                Block {
                    stmts: Vec::new(),
                    tail_expr: None,
                },
                span,
            );
        }

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
            } else if self.at(&Lexeme::Match) {
                stmts.push(self.parse_match_stmt());
            } else if matches!(self.peek(), Lexeme::AsmBlock { .. }) {
                let start = self.current_span();
                let tok = self.advance().clone();
                if let Lexeme::AsmBlock {
                    body,
                    effect,
                    target,
                } = &tok.node
                {
                    let span = start.merge(tok.span);
                    stmts.push(Spanned::new(
                        Stmt::Asm {
                            body: body.clone(),
                            effect: *effect,
                            target: target.clone(),
                        },
                        span,
                    ));
                }
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
        self.exit_nesting();
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

    fn parse_event(&mut self, cfg: Option<Spanned<String>>) -> EventDef {
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
        EventDef { cfg, name, fields }
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

    fn parse_match_stmt(&mut self) -> Spanned<Stmt> {
        let start = self.current_span();
        self.expect(&Lexeme::Match);
        let expr = self.parse_expr();
        self.expect(&Lexeme::LBrace);

        let mut arms = Vec::new();
        while !self.at(&Lexeme::RBrace) && !self.at(&Lexeme::Eof) {
            let pat_start = self.current_span();
            let pattern = if self.at(&Lexeme::Underscore)
                || matches!(self.peek(), Lexeme::Ident(s) if s == "_")
            {
                self.advance();
                MatchPattern::Wildcard
            } else if let Lexeme::Integer(n) = self.peek().clone() {
                self.advance();
                MatchPattern::Literal(Literal::Integer(n))
            } else if self.at(&Lexeme::True) {
                self.advance();
                MatchPattern::Literal(Literal::Bool(true))
            } else if self.at(&Lexeme::False) {
                self.advance();
                MatchPattern::Literal(Literal::Bool(false))
            } else if matches!(self.peek(), Lexeme::Ident(_))
                && matches!(self.tokens[self.pos + 1].node, Lexeme::LBrace)
            {
                // Struct pattern: `Name { field, field: value, ... }`
                self.parse_struct_match_pattern()
            } else {
                self.error_with_help(
                    "expected match pattern (integer, true, false, StructName { ... }, or _)",
                    "match arms use literal patterns like `0 =>`, `true =>`, struct patterns like `Point { x, y } =>`, or wildcard `_ =>`",
                );
                self.advance();
                MatchPattern::Wildcard
            };
            let pat_span = pat_start.merge(self.prev_span());

            self.expect(&Lexeme::FatArrow);
            let body = self.parse_block();

            arms.push(MatchArm {
                pattern: Spanned::new(pattern, pat_span),
                body,
            });

            // Optional comma between arms
            self.eat(&Lexeme::Comma);
        }

        self.expect(&Lexeme::RBrace);
        let span = start.merge(self.prev_span());
        Spanned::new(Stmt::Match { expr, arms }, span)
    }

    /// Parse a struct destructuring pattern: `Point { x, y: 0, z: _ }`.
    fn parse_struct_match_pattern(&mut self) -> MatchPattern {
        let name = self.expect_ident();
        self.expect(&Lexeme::LBrace);

        let mut fields = Vec::new();
        while !self.at(&Lexeme::RBrace) && !self.at(&Lexeme::Eof) {
            let field_name = self.expect_ident();

            let pattern = if self.eat(&Lexeme::Colon) {
                // Explicit pattern: `field: value`
                let pat_start = self.current_span();
                let pat = if self.at(&Lexeme::Underscore)
                    || matches!(self.peek(), Lexeme::Ident(s) if s == "_")
                {
                    self.advance();
                    FieldPattern::Wildcard
                } else if let Lexeme::Integer(n) = self.peek().clone() {
                    self.advance();
                    FieldPattern::Literal(Literal::Integer(n))
                } else if self.at(&Lexeme::True) {
                    self.advance();
                    FieldPattern::Literal(Literal::Bool(true))
                } else if self.at(&Lexeme::False) {
                    self.advance();
                    FieldPattern::Literal(Literal::Bool(false))
                } else if matches!(self.peek(), Lexeme::Ident(_)) {
                    let binding = self.expect_ident();
                    FieldPattern::Binding(binding.node)
                } else {
                    self.error_with_help(
                        "expected field pattern (identifier, literal, or _)",
                        "use `field: var` to bind, `field: 0` to match, or `field: _` to ignore",
                    );
                    self.advance();
                    FieldPattern::Wildcard
                };
                let pat_span = pat_start.merge(self.prev_span());
                Spanned::new(pat, pat_span)
            } else {
                // Shorthand: `field` is the same as `field: field`
                let span = field_name.span;
                Spanned::new(FieldPattern::Binding(field_name.node.clone()), span)
            };

            fields.push(StructPatternField {
                field_name,
                pattern,
            });

            if !self.eat(&Lexeme::Comma) {
                break;
            }
        }

        self.expect(&Lexeme::RBrace);
        MatchPattern::Struct { name, fields }
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

                // Check for generic args: name<3>(...) or name<N>(...)
                let generic_args = self.parse_call_generic_args();

                if self.at(&Lexeme::LParen) {
                    // Function call
                    self.advance();
                    let args = self.parse_call_args();
                    self.expect(&Lexeme::RParen);
                    let span = start.merge(self.prev_span());
                    Spanned::new(
                        Expr::Call {
                            path: Spanned::new(path, start),
                            generic_args,
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
                self.error_with_help(
                    &format!("expected expression, found {}", self.peek().description()),
                    "expressions include literals (42, true), variables, function calls, and operators",
                );
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

    /// Parse optional `<3, 5>` generic size arguments at a call site.
    /// Only consumed if `<` is followed by integer/ident and then `>` or `,`.
    fn parse_call_generic_args(&mut self) -> Vec<Spanned<ArraySize>> {
        // Only try if next token is `<` and the token after is int/ident
        // (disambiguates from `a < b` comparison)
        if !self.at(&Lexeme::Lt) {
            return Vec::new();
        }
        // Lookahead: after `<`, expect int or ident, then `,` or `>`
        if self.pos + 2 >= self.tokens.len() {
            return Vec::new();
        }
        let after_lt = &self.tokens[self.pos + 1].node;
        let after_val = &self.tokens[self.pos + 2].node;
        let looks_generic = match after_lt {
            Lexeme::Integer(_) | Lexeme::Ident(_) => {
                // `<N>`, `<N,`, `<N +`, `<N *` all look like generic args
                matches!(
                    after_val,
                    Lexeme::Gt | Lexeme::Comma | Lexeme::Plus | Lexeme::Star
                )
            }
            // `<(M + N) * 2>` — parenthesized size expression
            Lexeme::LParen => true,
            _ => false,
        };
        if !looks_generic {
            return Vec::new();
        }

        self.advance(); // consume <
        let mut args = Vec::new();
        while !self.at(&Lexeme::Gt) && !self.at(&Lexeme::Eof) {
            let start = self.current_span();
            let size = self.parse_array_size_expr();
            let span = start.merge(self.prev_span());
            args.push(Spanned::new(size, span));
            if !self.eat(&Lexeme::Comma) {
                break;
            }
        }
        self.expect(&Lexeme::Gt);
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
            self.error_at_current(&format!(
                "expected {}, found {}",
                token.description(),
                self.peek().description()
            ));
            self.current_span()
        }
    }

    fn expect_ident(&mut self) -> Spanned<String> {
        if let Lexeme::Ident(name) = self.peek().clone() {
            let span = self.current_span();
            self.advance();
            Spanned::new(name, span)
        } else {
            self.error_at_current(&format!(
                "expected identifier, found {}",
                self.peek().description()
            ));
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
            self.error_at_current(&format!(
                "expected integer literal, found {}",
                self.peek().description()
            ));
            0
        }
    }

    fn error_at_current(&mut self, msg: &str) {
        self.diagnostics
            .push(Diagnostic::error(msg.to_string(), self.current_span()));
    }

    fn error_with_help(&mut self, msg: &str, help: &str) {
        self.diagnostics.push(
            Diagnostic::error(msg.to_string(), self.current_span()).with_help(help.to_string()),
        );
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

    #[test]
    fn test_asm_basic() {
        let file = parse("program test\nfn main() {\n    asm { dup 0\n    add }\n}");
        if let Item::Fn(f) = &file.items[0].node {
            let block = f.body.as_ref().unwrap();
            assert_eq!(block.node.stmts.len(), 1);
            if let Stmt::Asm {
                body,
                effect,
                target,
            } = &block.node.stmts[0].node
            {
                assert!(body.contains("dup 0"));
                assert!(body.contains("add"));
                assert_eq!(*effect, 0);
                assert_eq!(*target, None);
            } else {
                panic!("expected asm statement");
            }
        }
    }

    #[test]
    fn test_asm_with_effect() {
        let file = parse("program test\nfn main() {\n    asm(+1) { push 42 }\n}");
        if let Item::Fn(f) = &file.items[0].node {
            let block = f.body.as_ref().unwrap();
            if let Stmt::Asm { effect, .. } = &block.node.stmts[0].node {
                assert_eq!(*effect, 1);
            } else {
                panic!("expected asm statement");
            }
        }
    }

    #[test]
    fn test_asm_between_statements() {
        // pub_write(x) is the last expr before }, so it becomes tail_expr
        let file = parse("program test\nfn main() {\n    let x: Field = pub_read()\n    asm { dup 0\nadd }\n    pub_write(x)\n}");
        if let Item::Fn(f) = &file.items[0].node {
            let block = f.body.as_ref().unwrap();
            assert_eq!(block.node.stmts.len(), 2);
            assert!(matches!(&block.node.stmts[0].node, Stmt::Let { .. }));
            assert!(matches!(&block.node.stmts[1].node, Stmt::Asm { .. }));
            assert!(block.node.tail_expr.is_some(), "pub_write(x) is tail expr");
        }
    }

    // --- cfg attribute parsing ---

    #[test]
    fn test_cfg_on_fn() {
        let file = parse("program test\n#[cfg(debug)]\nfn check() {}");
        if let Item::Fn(f) = &file.items[0].node {
            assert_eq!(f.cfg.as_ref().unwrap().node, "debug");
            assert_eq!(f.name.node, "check");
        } else {
            panic!("expected fn");
        }
    }

    #[test]
    fn test_cfg_on_const() {
        let file = parse("program test\n#[cfg(release)]\nconst X: Field = 0");
        if let Item::Const(c) = &file.items[0].node {
            assert_eq!(c.cfg.as_ref().unwrap().node, "release");
            assert_eq!(c.name.node, "X");
        } else {
            panic!("expected const");
        }
    }

    #[test]
    fn test_cfg_on_struct() {
        let file = parse("program test\n#[cfg(debug)]\nstruct Dbg { val: Field }");
        if let Item::Struct(s) = &file.items[0].node {
            assert_eq!(s.cfg.as_ref().unwrap().node, "debug");
            assert_eq!(s.name.node, "Dbg");
        } else {
            panic!("expected struct");
        }
    }

    #[test]
    fn test_cfg_on_pub_fn() {
        let file = parse("program test\n#[cfg(release)]\npub fn fast() {}");
        if let Item::Fn(f) = &file.items[0].node {
            assert_eq!(f.cfg.as_ref().unwrap().node, "release");
            assert!(f.is_pub);
        } else {
            panic!("expected fn");
        }
    }

    #[test]
    fn test_cfg_with_intrinsic() {
        let file = parse("module std.test\n#[cfg(debug)]\n#[intrinsic(add)]\npub fn add(a: Field, b: Field) -> Field");
        if let Item::Fn(f) = &file.items[0].node {
            assert_eq!(f.cfg.as_ref().unwrap().node, "debug");
            assert!(f.intrinsic.is_some());
        } else {
            panic!("expected fn");
        }
    }

    #[test]
    fn test_no_cfg() {
        let file = parse("program test\nfn main() {}");
        if let Item::Fn(f) = &file.items[0].node {
            assert!(f.cfg.is_none());
        } else {
            panic!("expected fn");
        }
    }

    // --- match statement parsing ---

    #[test]
    fn test_match_basic() {
        let file = parse("program test\nfn main() {\n    let x: Field = pub_read()\n    match x {\n        0 => { pub_write(0) }\n        1 => { pub_write(1) }\n        _ => { pub_write(2) }\n    }\n}");
        if let Item::Fn(f) = &file.items[0].node {
            let block = f.body.as_ref().unwrap();
            assert_eq!(block.node.stmts.len(), 2);
            if let Stmt::Match { arms, .. } = &block.node.stmts[1].node {
                assert_eq!(arms.len(), 3);
                assert!(matches!(
                    arms[0].pattern.node,
                    MatchPattern::Literal(Literal::Integer(0))
                ));
                assert!(matches!(
                    arms[1].pattern.node,
                    MatchPattern::Literal(Literal::Integer(1))
                ));
                assert!(matches!(arms[2].pattern.node, MatchPattern::Wildcard));
            } else {
                panic!("expected match statement");
            }
        }
    }

    #[test]
    fn test_match_bool_patterns() {
        let file = parse("program test\nfn main() {\n    let b: Bool = true\n    match b {\n        true => { pub_write(1) }\n        false => { pub_write(0) }\n    }\n}");
        if let Item::Fn(f) = &file.items[0].node {
            let block = f.body.as_ref().unwrap();
            if let Stmt::Match { arms, .. } = &block.node.stmts[1].node {
                assert_eq!(arms.len(), 2);
                assert!(matches!(
                    arms[0].pattern.node,
                    MatchPattern::Literal(Literal::Bool(true))
                ));
                assert!(matches!(
                    arms[1].pattern.node,
                    MatchPattern::Literal(Literal::Bool(false))
                ));
            } else {
                panic!("expected match statement");
            }
        }
    }

    #[test]
    fn test_match_wildcard_only() {
        let file = parse("program test\nfn main() {\n    match pub_read() {\n        _ => { pub_write(0) }\n    }\n}");
        if let Item::Fn(f) = &file.items[0].node {
            let block = f.body.as_ref().unwrap();
            if let Stmt::Match { arms, .. } = &block.node.stmts[0].node {
                assert_eq!(arms.len(), 1);
                assert!(matches!(arms[0].pattern.node, MatchPattern::Wildcard));
            } else {
                panic!("expected match statement");
            }
        }
    }

    #[test]
    fn test_match_struct_pattern() {
        let file = parse(
            "program test\nstruct Point { x: Field, y: Field }\nfn main() {\n    let p = Point { x: 1, y: 2 }\n    match p {\n        Point { x, y } => { pub_write(x) }\n    }\n}",
        );
        if let Item::Fn(f) = &file.items[1].node {
            let block = f.body.as_ref().unwrap();
            if let Stmt::Match { arms, .. } = &block.node.stmts[1].node {
                assert_eq!(arms.len(), 1);
                if let MatchPattern::Struct { name, fields } = &arms[0].pattern.node {
                    assert_eq!(name.node, "Point");
                    assert_eq!(fields.len(), 2);
                    assert_eq!(fields[0].field_name.node, "x");
                    assert_eq!(fields[1].field_name.node, "y");
                    assert!(
                        matches!(fields[0].pattern.node, FieldPattern::Binding(ref v) if v == "x")
                    );
                    assert!(
                        matches!(fields[1].pattern.node, FieldPattern::Binding(ref v) if v == "y")
                    );
                } else {
                    panic!("expected struct pattern");
                }
            } else {
                panic!("expected match statement");
            }
        }
    }

    #[test]
    fn test_match_struct_pattern_with_literals() {
        let file = parse(
            "program test\nstruct Pair { a: Field, b: Field }\nfn main() {\n    let p = Pair { a: 1, b: 2 }\n    match p {\n        Pair { a: 0, b } => { pub_write(b) }\n        _ => { pub_write(0) }\n    }\n}",
        );
        if let Item::Fn(f) = &file.items[1].node {
            let block = f.body.as_ref().unwrap();
            if let Stmt::Match { arms, .. } = &block.node.stmts[1].node {
                assert_eq!(arms.len(), 2);
                if let MatchPattern::Struct { fields, .. } = &arms[0].pattern.node {
                    assert!(matches!(
                        fields[0].pattern.node,
                        FieldPattern::Literal(Literal::Integer(0))
                    ));
                    assert!(
                        matches!(fields[1].pattern.node, FieldPattern::Binding(ref v) if v == "b")
                    );
                } else {
                    panic!("expected struct pattern");
                }
                assert!(matches!(arms[1].pattern.node, MatchPattern::Wildcard));
            } else {
                panic!("expected match statement");
            }
        }
    }

    #[test]
    fn test_match_struct_pattern_with_wildcard_field() {
        let file = parse(
            "program test\nstruct Pair { a: Field, b: Field }\nfn main() {\n    let p = Pair { a: 1, b: 2 }\n    match p {\n        Pair { a: _, b } => { pub_write(b) }\n    }\n}",
        );
        if let Item::Fn(f) = &file.items[1].node {
            let block = f.body.as_ref().unwrap();
            if let Stmt::Match { arms, .. } = &block.node.stmts[1].node {
                if let MatchPattern::Struct { fields, .. } = &arms[0].pattern.node {
                    assert!(matches!(fields[0].pattern.node, FieldPattern::Wildcard));
                } else {
                    panic!("expected struct pattern");
                }
            } else {
                panic!("expected match statement");
            }
        }
    }

    // --- #[test] attribute parsing ---

    #[test]
    fn test_test_attribute_on_fn() {
        let file =
            parse("program test\n#[test]\nfn check_math() {\n    assert(1 == 1)\n}\nfn main() {}");
        // First item should be the test function, second should be main
        assert_eq!(file.items.len(), 2);
        if let Item::Fn(f) = &file.items[0].node {
            assert!(f.is_test, "function should be marked as test");
            assert_eq!(f.name.node, "check_math");
        } else {
            panic!("expected test function");
        }
        if let Item::Fn(f) = &file.items[1].node {
            assert!(!f.is_test, "main should not be marked as test");
            assert_eq!(f.name.node, "main");
        } else {
            panic!("expected main function");
        }
    }

    #[test]
    fn test_test_attribute_with_cfg() {
        let file = parse("program test\n#[cfg(debug)]\n#[test]\nfn debug_check() {}\nfn main() {}");
        if let Item::Fn(f) = &file.items[0].node {
            assert!(f.is_test, "function should be marked as test");
            assert_eq!(f.cfg.as_ref().unwrap().node, "debug");
            assert_eq!(f.name.node, "debug_check");
        } else {
            panic!("expected test function");
        }
    }

    #[test]
    fn test_no_test_attribute() {
        let file = parse("program test\nfn main() {}");
        if let Item::Fn(f) = &file.items[0].node {
            assert!(!f.is_test, "main should not be marked as test");
        } else {
            panic!("expected function");
        }
    }

    #[test]
    fn test_no_arg_attribute_format() {
        // Verify the parse_attribute handles #[test] (no args) correctly
        let file = parse("program test\n#[test]\nfn t() {}\nfn main() {}");
        if let Item::Fn(f) = &file.items[0].node {
            assert!(f.is_test);
            assert!(f.intrinsic.is_none());
        } else {
            panic!("expected function");
        }
    }

    // --- Error path tests ---

    fn parse_err(source: &str) -> Vec<Diagnostic> {
        let (tokens, _comments, lex_diags) = Lexer::new(source, 0).tokenize();
        if !lex_diags.is_empty() {
            return lex_diags;
        }
        match Parser::new(tokens).parse_file() {
            Ok(_) => vec![],
            Err(diags) => diags,
        }
    }

    #[test]
    fn test_error_missing_program_or_module() {
        let diags = parse_err("fn main() {}");
        assert!(!diags.is_empty(), "should error on missing program/module");
        assert!(
            diags[0].message.contains("expected 'program' or 'module'"),
            "should say what was expected, got: {}",
            diags[0].message
        );
        assert!(
            diags[0].help.is_some(),
            "should have help text for program/module declaration"
        );
    }

    #[test]
    fn test_error_missing_closing_brace() {
        let diags = parse_err("program test\nfn main() {");
        assert!(!diags.is_empty(), "should error on missing closing brace");
        assert!(
            diags[0].message.contains("expected '}'"),
            "should expect closing brace, got: {}",
            diags[0].message
        );
    }

    #[test]
    fn test_error_unexpected_token_in_expr() {
        let diags = parse_err("program test\nfn main() {\n    let x: Field = }\n}");
        assert!(
            !diags.is_empty(),
            "should error on unexpected token in expression"
        );
        assert!(
            diags[0].message.contains("expected expression"),
            "should say 'expected expression', got: {}",
            diags[0].message
        );
        assert!(
            diags[0].help.is_some(),
            "expression error should have help text"
        );
    }

    #[test]
    fn test_error_missing_fn_body() {
        // A function with a body that has no closing brace produces a parse error
        let diags = parse_err("program test\nfn main() {\n    let x: Field = 1");
        assert!(!diags.is_empty(), "should error on unclosed function body");
        let has_relevant_error = diags.iter().any(|d| d.message.contains("expected"));
        assert!(
            has_relevant_error,
            "should have an 'expected' error, got: {:?}",
            diags.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_error_invalid_type() {
        let diags = parse_err("program test\nfn main() {\n    let x: 42 = 0\n}");
        assert!(!diags.is_empty(), "should error on invalid type");
        assert!(
            diags[0].message.contains("expected type"),
            "should say 'expected type', got: {}",
            diags[0].message
        );
        assert!(
            diags[0].help.as_deref().unwrap().contains("Field"),
            "help should list valid types"
        );
    }

    #[test]
    fn test_error_missing_arrow_in_return_type() {
        // Missing -> before return type: `fn foo() Field {}`
        // This parses as: fn foo() followed by item "Field" which isn't valid
        let diags = parse_err("program test\nfn foo() Field {}");
        assert!(
            !diags.is_empty(),
            "should error when return type arrow is missing"
        );
    }

    #[test]
    fn test_error_expected_token_shows_found() {
        // When expecting '(' but finding something else, error should show what was found
        let diags = parse_err("program test\nfn main {}");
        assert!(!diags.is_empty());
        let msg = &diags[0].message;
        assert!(
            msg.contains("expected") && msg.contains("found"),
            "error should show both expected and found tokens, got: {}",
            msg
        );
    }

    #[test]
    fn test_error_expected_item() {
        let diags = parse_err("program test\n42");
        assert!(
            !diags.is_empty(),
            "should error on bare integer at top level"
        );
        assert!(
            diags[0].message.contains("expected item"),
            "should say 'expected item', got: {}",
            diags[0].message
        );
        assert!(
            diags[0].help.is_some(),
            "expected item error should have help text"
        );
    }

    // --- Const generic expression parsing ---

    #[test]
    fn test_parse_array_size_add() {
        let file = parse("program test\nfn f(a: [Field; M + N]) {}");
        let func = match &file.items[0].node {
            Item::Fn(f) => f,
            _ => panic!("expected fn"),
        };
        match &func.params[0].ty.node {
            Type::Array(_, size) => {
                assert_eq!(format!("{}", size), "M + N");
            }
            other => panic!("expected array type, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_array_size_mul() {
        let file = parse("program test\nfn f(a: [Field; N * 2]) {}");
        let func = match &file.items[0].node {
            Item::Fn(f) => f,
            _ => panic!("expected fn"),
        };
        match &func.params[0].ty.node {
            Type::Array(_, size) => {
                assert_eq!(format!("{}", size), "N * 2");
            }
            other => panic!("expected array type, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_array_size_precedence() {
        // M + N * 2 should parse as M + (N * 2), not (M + N) * 2
        let file = parse("program test\nfn f(a: [Field; M + N * 2]) {}");
        let func = match &file.items[0].node {
            Item::Fn(f) => f,
            _ => panic!("expected fn"),
        };
        match &func.params[0].ty.node {
            Type::Array(_, size) => {
                assert_eq!(format!("{}", size), "M + N * 2");
                // Verify structure: Add(Param("M"), Mul(Param("N"), Literal(2)))
                match size {
                    ArraySize::Add(a, b) => {
                        assert!(matches!(a.as_ref(), ArraySize::Param(n) if n == "M"));
                        assert!(matches!(b.as_ref(), ArraySize::Mul(..)));
                    }
                    other => panic!("expected Add, got {:?}", other),
                }
            }
            other => panic!("expected array type, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_array_size_parenthesized() {
        let file = parse("program test\nfn f(a: [Field; (M + N) * 2]) {}");
        let func = match &file.items[0].node {
            Item::Fn(f) => f,
            _ => panic!("expected fn"),
        };
        match &func.params[0].ty.node {
            Type::Array(_, size) => {
                assert_eq!(format!("{}", size), "(M + N) * 2");
                // Verify structure: Mul(Add(Param("M"), Param("N")), Literal(2))
                match size {
                    ArraySize::Mul(a, b) => {
                        assert!(matches!(a.as_ref(), ArraySize::Add(..)));
                        assert!(matches!(b.as_ref(), ArraySize::Literal(2)));
                    }
                    other => panic!("expected Mul, got {:?}", other),
                }
            }
            other => panic!("expected array type, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_generic_call_size_expr() {
        let file = parse("program test\nfn f() { g<M + N>() }");
        let func = match &file.items[0].node {
            Item::Fn(f) => f,
            _ => panic!("expected fn"),
        };
        let body = func.body.as_ref().unwrap();
        // g<M + N>() is parsed as a tail expression (last expr before })
        let tail = body
            .node
            .tail_expr
            .as_ref()
            .expect("expected tail expression");
        match &tail.node {
            Expr::Call { generic_args, .. } => {
                assert_eq!(generic_args.len(), 1);
                assert_eq!(format!("{}", generic_args[0].node), "M + N");
            }
            other => panic!("expected Call, got {:?}", other),
        }
    }
}
