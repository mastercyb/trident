//! TreeLowering: consumes `&[TIROp]` and produces tree-structured output.
//!
//! Each tree-machine target implements `TreeLowering` to translate
//! stack-based IR operations into combinator expressions (Nock formulas,
//! or similar tree representations).
//!
//! This is the tree-machine counterpart of:
//! - `tir::lower::StackLowering` — stack targets → assembly text
//! - `lir::lower::RegisterLowering` — register targets → machine code
//! - `kir::lower::KernelLowering` — GPU targets → kernel source

mod nock;

use crate::tir::TIROp;

pub use nock::NockLowering;

/// A Nock noun — the universal data type for tree machines.
///
/// All data in Nock is a noun: either an atom (unsigned integer)
/// or a cell (ordered pair of nouns). This recursive structure
/// is the "assembly language" of tree machines.
#[derive(Debug, Clone, PartialEq)]
pub enum Noun {
    /// An atom — an unsigned integer of arbitrary width.
    Atom(u64),
    /// A cell — an ordered pair `[head tail]`.
    Cell(Box<Noun>, Box<Noun>),
}

impl Noun {
    /// Create an atom noun.
    pub fn atom(value: u64) -> Self {
        Noun::Atom(value)
    }

    /// Create a cell noun `[a b]`.
    pub fn cell(head: Noun, tail: Noun) -> Self {
        Noun::Cell(Box::new(head), Box::new(tail))
    }

    /// Nock formula: `[0 axis]` — slot lookup in subject tree.
    pub fn slot(axis: u64) -> Self {
        Noun::cell(Noun::atom(0), Noun::atom(axis))
    }

    /// Nock formula: `[1 constant]` — produce a constant, ignore subject.
    pub fn constant(value: Noun) -> Self {
        Noun::cell(Noun::atom(1), value)
    }

    /// Nock formula: `[2 subject formula]` — evaluate formula against subject.
    pub fn evaluate(subject: Noun, formula: Noun) -> Self {
        Noun::cell(Noun::atom(2), Noun::cell(subject, formula))
    }

    /// Nock formula: `[3 noun]` — cell test (0 if cell, 1 if atom).
    pub fn cell_test(noun: Noun) -> Self {
        Noun::cell(Noun::atom(3), noun)
    }

    /// Nock formula: `[4 noun]` — increment atom.
    pub fn increment(noun: Noun) -> Self {
        Noun::cell(Noun::atom(4), noun)
    }

    /// Nock formula: `[5 a b]` — equality test.
    pub fn equals(a: Noun, b: Noun) -> Self {
        Noun::cell(Noun::atom(5), Noun::cell(a, b))
    }

    /// Nock formula: `[6 test yes no]` — conditional branch.
    pub fn branch(test: Noun, yes: Noun, no: Noun) -> Self {
        Noun::cell(Noun::atom(6), Noun::cell(test, Noun::cell(yes, no)))
    }

    /// Nock formula: `[7 a b]` — compose: evaluate b against result of a.
    pub fn compose(a: Noun, b: Noun) -> Self {
        Noun::cell(Noun::atom(7), Noun::cell(a, b))
    }

    /// Nock formula: `[8 a b]` — push: evaluate b with [result-of-a subject].
    pub fn push(a: Noun, b: Noun) -> Self {
        Noun::cell(Noun::atom(8), Noun::cell(a, b))
    }

    /// Nock formula: `[9 axis core]` — invoke: pull formula from core and eval.
    pub fn invoke(axis: u64, core: Noun) -> Self {
        Noun::cell(Noun::atom(9), Noun::cell(Noun::atom(axis), core))
    }

    /// Nock formula: `[10 [axis value] target]` — edit: replace axis in target.
    pub fn edit(axis: u64, value: Noun, target: Noun) -> Self {
        Noun::cell(
            Noun::atom(10),
            Noun::cell(Noun::cell(Noun::atom(axis), value), target),
        )
    }

    /// Nock formula: `[11 hint formula]` — hint (advisory, doesn't change result).
    pub fn hint(hint: Noun, formula: Noun) -> Self {
        Noun::cell(Noun::atom(11), Noun::cell(hint, formula))
    }
}

impl std::fmt::Display for Noun {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Noun::Atom(v) => write!(f, "{}", v),
            Noun::Cell(h, t) => write!(f, "[{} {}]", h, t),
        }
    }
}

/// Lowers TIR operations into tree-structured output for combinator VMs.
///
/// Unlike stack and register lowering, tree lowering produces a `Noun` —
/// a recursive tree structure that IS the program. There is no assembly
/// text or machine code; the program is data.
pub trait TreeLowering {
    /// The target name (e.g. "nock").
    fn target_name(&self) -> &str;

    /// Lower a sequence of TIR operations into a Nock formula (noun tree).
    fn lower(&self, ops: &[TIROp]) -> Noun;

    /// Serialize the lowered noun to bytes (.jam format for Nock).
    fn serialize(&self, noun: &Noun) -> Vec<u8>;
}

/// Create a tree-lowering backend for the given target name.
pub fn create_tree_lowering(target: &str) -> Option<Box<dyn TreeLowering>> {
    match target {
        "nock" | "nockchain" => Some(Box::new(NockLowering::new())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noun_display() {
        assert_eq!(format!("{}", Noun::atom(42)), "42");
        assert_eq!(
            format!("{}", Noun::cell(Noun::atom(1), Noun::atom(2))),
            "[1 2]"
        );
        assert_eq!(format!("{}", Noun::slot(7)), "[0 7]");
        assert_eq!(format!("{}", Noun::constant(Noun::atom(42))), "[1 42]");
    }

    #[test]
    fn test_noun_formulas() {
        // [6 [0 2] [1 1] [1 0]] — if slot 2 then 1 else 0
        let formula = Noun::branch(
            Noun::slot(2),
            Noun::constant(Noun::atom(1)),
            Noun::constant(Noun::atom(0)),
        );
        assert_eq!(format!("{}", formula), "[6 [[0 2] [[1 1] [1 0]]]]");
    }

    #[test]
    fn test_noun_compose() {
        // [7 [1 42] [4 [0 1]]] — push 42, then increment
        let formula = Noun::compose(
            Noun::constant(Noun::atom(42)),
            Noun::increment(Noun::slot(1)),
        );
        assert_eq!(format!("{}", formula), "[7 [[1 42] [4 [0 1]]]]");
    }

    #[test]
    fn test_create_tree_lowering() {
        assert!(create_tree_lowering("nock").is_some());
        assert!(create_tree_lowering("nockchain").is_some());
        assert!(create_tree_lowering("triton").is_none());
        assert!(create_tree_lowering("x86_64").is_none());
    }

    #[test]
    fn test_target_name() {
        let lowering = create_tree_lowering("nock").unwrap();
        assert_eq!(lowering.target_name(), "nock");
    }
}
