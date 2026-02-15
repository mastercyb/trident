use crate::ast::*;
use crate::lexeme::Lexeme;
use crate::span::Spanned;

use super::Parser;

impl Parser {
    pub(super) fn parse_program(&mut self) -> File {
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

    pub(super) fn parse_module(&mut self) -> File {
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

    pub(super) fn parse_items(&mut self) -> Vec<Spanned<Item>> {
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
                self.reject_fn_only_attrs(
                    &intrinsic_attr,
                    is_test,
                    is_pure,
                    &requires_attrs,
                    &ensures_attrs,
                );
                let item = self.parse_const(is_pub, cfg_attr);
                let span = start.merge(self.prev_span());
                items.push(Spanned::new(Item::Const(item), span));
            } else if self.at(&Lexeme::Struct) {
                self.reject_fn_only_attrs(
                    &intrinsic_attr,
                    is_test,
                    is_pure,
                    &requires_attrs,
                    &ensures_attrs,
                );
                let item = self.parse_struct(is_pub, cfg_attr);
                let span = start.merge(self.prev_span());
                items.push(Spanned::new(Item::Struct(item), span));
            } else if self.at(&Lexeme::Event) {
                self.reject_fn_only_attrs(
                    &intrinsic_attr,
                    is_test,
                    is_pure,
                    &requires_attrs,
                    &ensures_attrs,
                );
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

    fn reject_fn_only_attrs(
        &mut self,
        intrinsic: &Option<Spanned<String>>,
        is_test: bool,
        is_pure: bool,
        requires: &[Spanned<String>],
        ensures: &[Spanned<String>],
    ) {
        if intrinsic.is_some() {
            self.error_at_current("#[intrinsic] is only allowed on functions");
        }
        if is_test {
            self.error_at_current("#[test] is only allowed on functions");
        }
        if is_pure {
            self.error_at_current("#[pure] is only allowed on functions");
        }
        if !requires.is_empty() || !ensures.is_empty() {
            self.error_at_current("#[requires] and #[ensures] are only allowed on functions");
        }
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

    pub(super) fn parse_event(&mut self, cfg: Option<Spanned<String>>) -> EventDef {
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
}
