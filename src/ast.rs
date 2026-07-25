//! Abstract Syntax Tree for Burxt v0.0.1.
//!
//! Deliberately tiny: only what the first vertical slice needs to prove the
//! thesis (exact decimal arithmetic). Everything here is backend-independent —
//! the lexer/parser/typechecker produce and consume these nodes, and codegen
//! reads them. No LLVM types leak in here.

/// A Burxt type as written in source.
///
/// `Decimal<S>` carries its *scale* (number of fractional digits) in the type
/// itself. This is the heart of the thesis: a money value's precision is part
/// of its type, not a runtime accident.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Int,
    /// Decimal with a fixed scale S (digits after the decimal point).
    /// Represented at runtime as a scaled i64: stored = value * 10^scale.
    Decimal { scale: u32 },
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Int => write!(f, "Int"),
            Type::Decimal { scale } => write!(f, "Decimal<{}>", scale),
        }
    }
}

/// Binary arithmetic operators supported in the first slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
}

impl std::fmt::Display for BinOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
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
