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
    /// C's 32-bit int. Exists ONLY in extern fn signatures, so FFI is honest
    /// about width: returns are sign-extended, arguments are range-checked at
    /// runtime (a value that doesn't fit is a loud error, never a silent wrap).
    /// In Burxt code the value is always an Int.
    CInt,
    /// Decimal with a fixed scale S (digits after the decimal point).
    /// Represented at runtime as a scaled i64: stored = value * 10^scale.
    Decimal { scale: u32, rounding: Option<Rounding> },
    /// A struct type, by name. Typing is NOMINAL: two structs with identical
    /// fields are different types — the name is a contract, exactly as
    /// Decimal<2> and Decimal<2, RoundHalfEven> are kept apart.
    Named(String),
    /// A fixed-size stack array `[T; N]`. Arrays exist only behind bindings
    /// in this slice: indexed reads/writes and `len(a)` — never a bare value.
    Array { elem: Box<Type>, len: u32 },
    /// `dyn Trait` — a trait object: the ONLY thing that triggers dynamic
    /// dispatch. Represented as a fat pointer (data pointer, vtable pointer);
    /// the vtable lives outside the data, which is why the A4.5 layout
    /// guarantee means becoming a trait object never moves a field.
    Dyn(String),
}

impl Type {
    /// "a" or "an", so error messages read as English ("an Int", "a Bool").
    pub fn article(&self) -> &'static str {
        match self {
            Type::Int => "an",
            _ => "a",
        }
    }
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Int => write!(f, "Int"),
            Type::Bool => write!(f, "Bool"),
            Type::String => write!(f, "String"),
            Type::CInt => write!(f, "CInt"),
            Type::Decimal { scale, rounding: None } => write!(f, "Decimal<{}>", scale),
            Type::Decimal { scale, rounding: Some(r) } => write!(f, "Decimal<{}, {}>", scale, r),
            Type::Named(name) => write!(f, "{}", name),
            Type::Array { elem, len } => write!(f, "[{}; {}]", elem, len),
            Type::Dyn(name) => write!(f, "dyn {}", name),
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

/// Short-circuiting boolean operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicalOp {
    And,
    Or,
}

impl std::fmt::Display for LogicalOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogicalOp::And => write!(f, "&&"),
            LogicalOp::Or => write!(f, "||"),
        }
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
    /// Unary negation, e.g. `-19.99`. Overflow-checked like every subtraction.
    Neg(Box<Expr>),
    /// Logical not: `!ok`. Bool only — there is no truthiness to negate.
    Not(Box<Expr>),
    /// `&&` / `||`. Kept separate from BinOp because they SHORT-CIRCUIT: the
    /// right side is not evaluated when the left already decides the answer.
    /// That is observable behavior, so it is part of the language, not an
    /// optimization.
    Logical { op: LogicalOp, lhs: Box<Expr>, rhs: Box<Expr> },
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
    /// Method call: `item.total()`.
    MethodCall { base: Box<Expr>, method: String, args: Vec<Expr> },
    /// An array literal `[10.00, 5.99, 4.01]` — only valid as a `let`
    /// initializer with a declared array type.
    ArrayLit(Vec<Expr>),
    /// Indexed read `a[i]`. The base is a binding NAME, not an expression —
    /// arrays only live behind bindings in this slice.
    Index { name: String, index: Box<Expr> },
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
    /// `name[index] = value;` — element assignment through a `let mut`
    /// binding, bounds-checked like every indexed access.
    AssignIndex { name: String, index: Expr, value: Expr },
    /// A call kept for its side effect, its result discarded: `f();` or
    /// `acct.deposit(10.00);`. The only expressions worth writing as a bare
    /// statement are ones with an effect — calls and mutating methods — so
    /// this is not a general expression-statement; the parser only builds it
    /// from a call or method-call shape.
    ExprStmt(Expr),
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

/// `fn (self: Type) name(params) -> ret { body }` — a method: a function in
/// the receiver type's namespace. `fn (mut self: Type) ...` declares a
/// MUTATING method, callable only through a `let mut` binding; the receiver
/// is then passed as a true reference, not a value copy (see the aggregate
/// ABI — this is the one place Burxt passes an aggregate by address on
/// purpose, mirroring the existing field-assignment mutability rule).
#[derive(Debug, Clone)]
pub struct MethodDef {
    pub receiver: String,
    pub receiver_mut: bool,
    pub name: String,
    pub params: Vec<Param>,
    pub ret: Type,
    pub body: Vec<Stmt>,
}

/// One method signature inside a `trait` declaration: a name, a receiver form
/// and a type — no body, no fields, no state. Traits declare signatures only.
#[derive(Debug, Clone)]
pub struct TraitSig {
    pub name: String,
    pub receiver_mut: bool,
    pub params: Vec<Param>,
    pub ret: Type,
}

/// `trait Name { fn m(self) -> T ... }` — a named set of method signatures a
/// type can promise to satisfy. That is the whole concept.
#[derive(Debug, Clone)]
pub struct TraitDef {
    pub name: String,
    pub methods: Vec<TraitSig>,
}

/// `impl Trait for Type { <methods> }` — satisfaction is EXPLICIT and nominal:
/// Burxt never auto-satisfies a trait because method shapes happen to match,
/// so conformance is a deliberate, greppable declaration.
#[derive(Debug, Clone)]
pub struct ImplBlock {
    pub trait_name: String,
    pub type_name: String,
    pub methods: Vec<MethodDef>,
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
    pub traits: Vec<TraitDef>,
    pub impls: Vec<ImplBlock>,
    pub externs: Vec<ExternFn>,
    pub fns: Vec<FnDef>,
    pub methods: Vec<MethodDef>,
    pub stmts: Vec<Stmt>,
}
