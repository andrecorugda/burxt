//! Abstract Syntax Tree for Burxt v0.0.1.
//!
//! Deliberately tiny: only what the first vertical slice needs to prove the
//! thesis (exact decimal arithmetic). Everything here is backend-independent —
//! the lexer/parser/typechecker produce and consume these nodes, and codegen
//! reads them. No LLVM types leak in here.

/// A rounding contract: how a decimal result returns to its declared scale
/// when arithmetic (multiplication, division) produces extra digits.
/// Naming reads as plain English inside the type: `Decimal<2, RoundHalfEven>`
/// = "two decimal places, rounding half to even".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Rounding {
    /// Ties go to the even neighbor (banker's rounding): 0.105 -> 0.10.
    HalfEven,
    /// Ties go away from zero (commercial rounding): 0.105 -> 0.11.
    HalfUp,
}

impl std::fmt::Display for Rounding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Rounding::HalfEven => write!(f, "RoundHalfEven"),
            Rounding::HalfUp => write!(f, "RoundHalfUp"),
        }
    }
}

/// A Burxt type as written in source.
///
/// `Decimal<S>` carries its *scale* (number of fractional digits) in the type
/// itself. This is the heart of the thesis: a money value's precision is part
/// of its type, not a runtime accident.
///
/// `Decimal<S, R>` additionally carries a rounding contract `R`. Without one,
/// only exact arithmetic (+, -, * Int) is allowed; multiplication and division
/// of decimals — which must round — are compile errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Int,
    Bool,
    /// Decimal with a fixed scale S (digits after the decimal point).
    /// Represented at runtime as a scaled i64: stored = value * 10^scale.
    Decimal { scale: u32, rounding: Option<Rounding> },
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Int => write!(f, "Int"),
            Type::Bool => write!(f, "Bool"),
            Type::Decimal { scale, rounding: None } => write!(f, "Decimal<{}>", scale),
            Type::Decimal { scale, rounding: Some(r) } => write!(f, "Decimal<{}, {}>", scale, r),
        }
    }
}

/// Binary arithmetic operators supported in the first slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
}

impl std::fmt::Display for BinOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
        };
        write!(f, "{}", s)
    }
}

/// Comparison operators. Comparisons are always exact (scaled integers compare
/// directly) and always produce a Bool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl std::fmt::Display for CmpOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            CmpOp::Eq => "==",
            CmpOp::Ne => "!=",
            CmpOp::Lt => "<",
            CmpOp::Le => "<=",
            CmpOp::Gt => ">",
            CmpOp::Ge => ">=",
        };
        write!(f, "{}", s)
    }
}

/// Expressions.
#[derive(Debug, Clone)]
pub enum Expr {
    /// An integer literal, e.g. `3`.
    IntLit(i64),
    /// A decimal literal captured EXACTLY as (unscaled_value, scale).
    /// e.g. `19.99` -> DecimalLit { unscaled: 1999, scale: 2 }.
    /// We never parse it through f64 — that is the whole point.
    DecimalLit { unscaled: i64, scale: u32 },
    /// `true` or `false`.
    BoolLit(bool),
    /// A reference to a previously-bound name.
    Var(String),
    /// A binary operation.
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    /// A comparison, e.g. `balance >= 0.00`. Produces a Bool.
    Compare {
        op: CmpOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    /// A function call, e.g. `total(19.99, 3)`.
    Call { name: String, args: Vec<Expr> },
}

/// Statements. A Burxt v0.0.1 program is just a sequence of these.
#[derive(Debug, Clone)]
pub enum Stmt {
    /// `let name: Type = value;` — or `let mut name: ...` to allow
    /// reassignment. Immutable is the default; mutation is opt-in and visible.
    Let {
        name: String,
        mutable: bool,
        declared: Type,
        value: Expr,
    },
    /// `name = value;` — only valid for a `let mut` binding, and the value's
    /// type must match the declaration exactly.
    Assign { name: String, value: Expr },
    /// `while cond { ... }` — the condition must be a Bool; braces required.
    While { cond: Expr, body: Vec<Stmt> },
    /// `print(expr);`
    Print(Expr),
    /// `return expr;` — only valid inside a function.
    Return(Expr),
    /// `if cond { ... } else { ... }` — the condition must be a Bool, and the
    /// braces are required. `else if` chains nest inside `else_block`.
    If {
        cond: Expr,
        then_block: Vec<Stmt>,
        else_block: Option<Vec<Stmt>>,
    },
}

/// One typed function parameter: `price: Decimal<2>`.
#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: Type,
}

/// `fn name(params) -> ret { body }`. Every function returns a value, and the
/// typechecker proves it returns on every path.
#[derive(Debug, Clone)]
pub struct FnDef {
    pub name: String,
    pub params: Vec<Param>,
    pub ret: Type,
    pub body: Vec<Stmt>,
}

/// A whole program: function definitions plus top-level statements (the
/// implicit main). Functions are hoisted — they may be defined after the
/// code that calls them, and may call each other.
#[derive(Debug, Clone)]
pub struct Program {
    pub fns: Vec<FnDef>,
    pub stmts: Vec<Stmt>,
}
