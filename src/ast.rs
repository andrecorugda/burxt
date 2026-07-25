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
    /// An immutable, NUL-terminated byte string. In the current slice every
    /// String is a literal living in .rodata — no allocation, no ownership
    /// question. Concatenation/equality arrive with the allocation story.
    String,
    /// Decimal with a fixed scale S (digits after the decimal point).
    /// Represented at runtime as a scaled i64: stored = value * 10^scale.
    Decimal { scale: u32, rounding: Option<Rounding> },
    /// A struct type, by name. Typing is NOMINAL: two structs with identical
    /// fields are different types — the name is a contract, exactly as
    /// Decimal<2> and Decimal<2, RoundHalfEven> are kept apart.
    Named(String),
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Int => write!(f, "Int"),
            Type::Bool => write!(f, "Bool"),
            Type::String => write!(f, "String"),
            Type::Decimal { scale, rounding: None } => write!(f, "Decimal<{}>", scale),
            Type::Decimal { scale, rounding: Some(r) } => write!(f, "Decimal<{}, {}>", scale, r),
            Type::Named(name) => write!(f, "{}", name),
        }
    }
}

/// Binary arithmetic operators supported in the first slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    /// A string literal, escapes already resolved by the lexer.
    StrLit(String),
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
    /// Struct construction: `LineItem { price: 19.99, qty: 3 }`.
    /// Every field must be given by name; any order.
    StructLit { name: String, fields: Vec<(String, Expr)> },
    /// Field access: `item.price` (chains for nested structs).
    Field { base: Box<Expr>, field: String },
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
    /// `name.field(.field)* = value;` — field assignment through a `let mut`
    /// binding. Mutability is per-binding, not per-field.
    AssignField { name: String, path: Vec<String>, value: Expr },
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

/// `extern fn name(params) -> ret;` — a C function Burxt may call. The name
/// is the real linker symbol (never mangled); matching the C side's actual
/// signature is the programmer's contract, as in every FFI.
#[derive(Debug, Clone)]
pub struct ExternFn {
    pub name: String,
    pub params: Vec<Param>,
    pub ret: Type,
}

/// `struct Name { field: Type, ... }` — the nominal record type and the
/// substrate for Burxt's OOP layers (methods, then interfaces).
#[derive(Debug, Clone)]
pub struct StructDef {
    pub name: String,
    pub fields: Vec<Param>,
}

/// A whole program: struct and extern declarations, function definitions,
/// and top-level statements (the implicit main). Declarations are hoisted —
/// define them in any order.
#[derive(Debug, Clone)]
pub struct Program {
    pub structs: Vec<StructDef>,
    pub externs: Vec<ExternFn>,
    pub fns: Vec<FnDef>,
    pub stmts: Vec<Stmt>,
}
