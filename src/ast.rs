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
    /// Decimal with a fixed scale S (digits after the decimal point).
    /// Represented at runtime as a scaled i64: stored = value * 10^scale.
    Decimal { scale: u32, rounding: Option<Rounding> },
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Int => write!(f, "Int"),
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

/// Expressions.
#[derive(Debug, Clone)]
pub enum Expr {
    /// An integer literal, e.g. `3`.
    IntLit(i64),
    /// A decimal literal captured EXACTLY as (unscaled_value, scale).
    /// e.g. `19.99` -> DecimalLit { unscaled: 1999, scale: 2 }.
    /// We never parse it through f64 — that is the whole point.
    DecimalLit { unscaled: i64, scale: u32 },
    /// A reference to a previously-bound name.
    Var(String),
    /// A binary operation.
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
}

/// Statements. A Burxt v0.0.1 program is just a sequence of these.
#[derive(Debug, Clone)]
pub enum Stmt {
    /// `let name: Type = value;`
    Let {
        name: String,
        declared: Type,
        value: Expr,
    },
    /// `print(expr);`
    Print(Expr),
}

/// A whole program: an ordered list of statements.
#[derive(Debug, Clone)]
pub struct Program {
    pub stmts: Vec<Stmt>,
}
