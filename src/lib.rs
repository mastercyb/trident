pub mod api;
pub mod ast;
pub mod config;
pub mod cost;
pub mod deploy;
pub mod diagnostic;
pub mod field;
pub mod gpu;
pub mod ir;
pub mod lsp;
pub mod package;
pub mod runtime;
pub mod syntax;
pub mod typecheck;
pub mod verify;

// Re-exports — moved modules keep their old `crate::X` paths
pub(crate) use api::pipeline;
pub use ir::tir;
pub use syntax::span;
pub use typecheck::types;

// Re-exports — preserves `trident::X` paths used by CLI and tests
pub use config::project;
pub use config::resolve;
pub use config::scaffold;
pub use config::target;
pub use package::cache;
pub use package::hash;
pub use package::manifest;
pub use package::poseidon2;
pub use package::registry;
pub use package::store;
pub use syntax::format;
pub use syntax::lexeme;
pub use syntax::lexer;
pub use syntax::parser;
pub use verify::equiv;
pub use verify::report;
pub use verify::smt;
pub use verify::solve;
pub use verify::sym;
pub use verify::synthesize;

// Re-export public API — preserves `trident::compile()` etc.
pub use api::*;

use diagnostic::{render_diagnostics, Diagnostic};
use lexer::Lexer;
use parser::Parser;

pub(crate) fn parse_source(source: &str, filename: &str) -> Result<ast::File, Vec<Diagnostic>> {
    let (tokens, _comments, lex_errors) = Lexer::new(source, 0).tokenize();
    if !lex_errors.is_empty() {
        render_diagnostics(&lex_errors, filename, source);
        return Err(lex_errors);
    }

    match Parser::new_with_source(tokens, source).parse_file() {
        Ok(file) => Ok(file),
        Err(errors) => {
            render_diagnostics(&errors, filename, source);
            Err(errors)
        }
    }
}

pub fn parse_source_silent(source: &str, _filename: &str) -> Result<ast::File, Vec<Diagnostic>> {
    let (tokens, _comments, lex_errors) = Lexer::new(source, 0).tokenize();
    if !lex_errors.is_empty() {
        return Err(lex_errors);
    }
    Parser::new_with_source(tokens, source).parse_file()
}
