use crate::ast::*;
use crate::lexeme::Lexeme;
use crate::span::Spanned;

use super::Parser;

impl Parser {
    pub(super) fn parse_expr(&mut self) -> Spanned<Expr> {
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

            let (l_bp, r_bp) = op.binding_power();
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
                        let name = path.0.join(".");
                        Spanned::new(Expr::Var(name), start)
                    }
                } else {
                    // Variable reference
                    if path.0.len() == 1 {
                        Spanned::new(Expr::Var(path.0.into_iter().next().unwrap()), start)
                    } else {
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
        if !self.at(&Lexeme::Lt) {
            return Vec::new();
        }
        if self.pos + 2 >= self.tokens.len() {
            return Vec::new();
        }
        let after_lt = &self.tokens[self.pos + 1].node;
        let after_val = &self.tokens[self.pos + 2].node;
        let looks_generic = match after_lt {
            Lexeme::Integer(_) | Lexeme::Ident(_) => {
                matches!(
                    after_val,
                    Lexeme::Gt | Lexeme::Comma | Lexeme::Plus | Lexeme::Star
                )
            }
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

    pub(super) fn parse_struct_init_fields(&mut self) -> Vec<(Spanned<String>, Spanned<Expr>)> {
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
            Lexeme::RBrace => true,
            _ => false,
        }
    }

    pub(super) fn expr_to_place(&self, expr: &Spanned<Expr>) -> Spanned<Place> {
        match &expr.node {
            Expr::Var(name) => Spanned::new(Place::Var(name.clone()), expr.span),
            _ => Spanned::new(Place::Var("_error_".to_string()), expr.span),
        }
    }
}
