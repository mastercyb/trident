mod advanced;
mod basics;

use crate::diagnostic::Diagnostic;
use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::typecheck::{ModuleExports, TypeChecker};

pub(super) fn check(source: &str) -> Result<ModuleExports, Vec<Diagnostic>> {
    let (tokens, _, _) = Lexer::new(source, 0).tokenize();
    let file = Parser::new(tokens).parse_file().unwrap();
    TypeChecker::new().check_file(&file)
}

pub(super) fn check_err(source: &str) -> Vec<Diagnostic> {
    match check(source) {
        Ok(_) => vec![],
        Err(diags) => diags,
    }
}

pub(super) fn check_with_flags(
    source: &str,
    flags: &[&str],
) -> Result<ModuleExports, Vec<Diagnostic>> {
    let (tokens, _, _) = Lexer::new(source, 0).tokenize();
    let file = Parser::new(tokens).parse_file().unwrap();
    let flag_set: std::collections::BTreeSet<String> =
        flags.iter().map(|s| s.to_string()).collect();
    TypeChecker::new()
        .with_cfg_flags(flag_set)
        .check_file(&file)
}
