use crate::span::Spanned;

/// A parsed `.tri` file — either a program or a library module.
#[derive(Clone, Debug)]
pub struct File {
    pub kind: FileKind,
    pub name: Spanned<String>,
    pub uses: Vec<Spanned<ModulePath>>,
    pub declarations: Vec<Declaration>,
    pub items: Vec<Spanned<Item>>,
}

/// Program I/O declarations.
#[derive(Clone, Debug)]
pub enum Declaration {
    PubInput(Spanned<Type>),
    PubOutput(Spanned<Type>),
    SecInput(Spanned<Type>),
    /// `sec ram: { addr: Type, addr: Type, ... }`
    /// Pre-initialized RAM slots (prover-supplied secret data).
    SecRam(Vec<(u64, Spanned<Type>)>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileKind {
    Program,
    Module,
}

/// A dotted module path, e.g. `std.hash` → `["std", "hash"]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModulePath(pub Vec<String>);

impl ModulePath {
    pub fn single(name: String) -> Self {
        Self(vec![name])
    }

    pub fn as_dotted(&self) -> String {
        self.0.join(".")
    }
}

impl std::fmt::Display for ModulePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_dotted())
    }
}

/// Top-level items in a module.
#[derive(Clone, Debug)]
pub enum Item {
    Const(ConstDef),
    Struct(StructDef),
    Event(EventDef),
    Fn(FnDef),
}

#[derive(Clone, Debug)]
pub struct ConstDef {
    pub is_pub: bool,
    pub cfg: Option<Spanned<String>>,
    pub name: Spanned<String>,
    pub ty: Spanned<Type>,
    pub value: Spanned<Expr>,
}

#[derive(Clone, Debug)]
pub struct StructDef {
    pub is_pub: bool,
    pub cfg: Option<Spanned<String>>,
    pub name: Spanned<String>,
    pub fields: Vec<StructField>,
}

#[derive(Clone, Debug)]
pub struct StructField {
    pub is_pub: bool,
    pub name: Spanned<String>,
    pub ty: Spanned<Type>,
}

#[derive(Clone, Debug)]
pub struct EventDef {
    pub cfg: Option<Spanned<String>>,
    pub name: Spanned<String>,
    pub fields: Vec<EventField>,
}

#[derive(Clone, Debug)]
pub struct EventField {
    pub name: Spanned<String>,
    pub ty: Spanned<Type>,
}

#[derive(Clone, Debug)]
pub struct FnDef {
    pub is_pub: bool,
    pub cfg: Option<Spanned<String>>,
    pub intrinsic: Option<Spanned<String>>,
    pub is_test: bool,
    /// Precondition annotations: `#[requires(predicate)]`.
    pub requires: Vec<Spanned<String>>,
    /// Postcondition annotations: `#[ensures(predicate)]`.
    pub ensures: Vec<Spanned<String>>,
    pub name: Spanned<String>,
    /// Size-generic parameters, e.g. `<N>` in `fn sum<N>(arr: [Field; N])`.
    pub type_params: Vec<Spanned<String>>,
    pub params: Vec<Param>,
    pub return_ty: Option<Spanned<Type>>,
    pub body: Option<Spanned<Block>>,
}

#[derive(Clone, Debug)]
pub struct Param {
    pub name: Spanned<String>,
    pub ty: Spanned<Type>,
}

/// Array size: either a compile-time literal or a generic size parameter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArraySize {
    Literal(u64),
    Param(String),
}

impl ArraySize {
    /// Return the concrete size, or `None` for unresolved params.
    pub fn as_literal(&self) -> Option<u64> {
        match self {
            ArraySize::Literal(n) => Some(*n),
            ArraySize::Param(_) => None,
        }
    }
}

impl std::fmt::Display for ArraySize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArraySize::Literal(n) => write!(f, "{}", n),
            ArraySize::Param(name) => write!(f, "{}", name),
        }
    }
}

/// Syntactic types (as written in source).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Type {
    Field,
    XField,
    Bool,
    U32,
    Digest,
    Array(Box<Type>, ArraySize),
    Tuple(Vec<Type>),
    Named(ModulePath),
}

/// A block of statements with an optional trailing expression.
#[derive(Clone, Debug)]
pub struct Block {
    pub stmts: Vec<Spanned<Stmt>>,
    pub tail_expr: Option<Box<Spanned<Expr>>>,
}

/// A binding pattern for `let` statements.
#[derive(Clone, Debug)]
pub enum Pattern {
    /// Single name: `let x = ...`
    Name(Spanned<String>),
    /// Tuple destructure: `let (a, b) = ...`
    Tuple(Vec<Spanned<String>>),
}

/// A pattern in a match arm.
#[derive(Clone, Debug)]
pub enum MatchPattern {
    /// Integer or boolean literal: `0`, `42`, `true`, `false`.
    Literal(Literal),
    /// Wildcard: `_`.
    Wildcard,
}

/// A single arm in a match statement.
#[derive(Clone, Debug)]
pub struct MatchArm {
    pub pattern: Spanned<MatchPattern>,
    pub body: Spanned<Block>,
}

/// Statements.
#[derive(Clone, Debug)]
pub enum Stmt {
    Let {
        mutable: bool,
        pattern: Pattern,
        ty: Option<Spanned<Type>>,
        init: Spanned<Expr>,
    },
    Assign {
        place: Spanned<Place>,
        value: Spanned<Expr>,
    },
    TupleAssign {
        names: Vec<Spanned<String>>,
        value: Spanned<Expr>,
    },
    If {
        cond: Spanned<Expr>,
        then_block: Spanned<Block>,
        else_block: Option<Spanned<Block>>,
    },
    For {
        var: Spanned<String>,
        start: Spanned<Expr>,
        end: Spanned<Expr>,
        bound: Option<u64>,
        body: Spanned<Block>,
    },
    Expr(Spanned<Expr>),
    Return(Option<Spanned<Expr>>),
    Emit {
        event_name: Spanned<String>,
        fields: Vec<(Spanned<String>, Spanned<Expr>)>,
    },
    Seal {
        event_name: Spanned<String>,
        fields: Vec<(Spanned<String>, Spanned<Expr>)>,
    },
    Asm {
        body: String,
        effect: i32,
        target: Option<String>,
    },
    Match {
        expr: Spanned<Expr>,
        arms: Vec<MatchArm>,
    },
}

/// L-value places (can appear on left side of assignment).
#[derive(Clone, Debug)]
pub enum Place {
    Var(String),
    FieldAccess(Box<Spanned<Place>>, Spanned<String>),
    Index(Box<Spanned<Place>>, Box<Spanned<Expr>>),
}

/// Expressions.
#[derive(Clone, Debug)]
pub enum Expr {
    Literal(Literal),
    Var(String),
    BinOp {
        op: BinOp,
        lhs: Box<Spanned<Expr>>,
        rhs: Box<Spanned<Expr>>,
    },
    Call {
        path: Spanned<ModulePath>,
        /// Explicit size-generic arguments, e.g. `sum<3>(...)`.
        generic_args: Vec<Spanned<ArraySize>>,
        args: Vec<Spanned<Expr>>,
    },
    FieldAccess {
        expr: Box<Spanned<Expr>>,
        field: Spanned<String>,
    },
    Index {
        expr: Box<Spanned<Expr>>,
        index: Box<Spanned<Expr>>,
    },
    StructInit {
        path: Spanned<ModulePath>,
        fields: Vec<(Spanned<String>, Spanned<Expr>)>,
    },
    ArrayInit(Vec<Spanned<Expr>>),
    Tuple(Vec<Spanned<Expr>>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Literal {
    Integer(u64),
    Bool(bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,       // +
    Mul,       // *
    Eq,        // ==
    Lt,        // <
    BitAnd,    // &
    BitXor,    // ^
    DivMod,    // /%
    XFieldMul, // *.
}

impl BinOp {
    pub fn as_str(&self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Mul => "*",
            BinOp::Eq => "==",
            BinOp::Lt => "<",
            BinOp::BitAnd => "&",
            BinOp::BitXor => "^",
            BinOp::DivMod => "/%",
            BinOp::XFieldMul => "*.",
        }
    }
}
