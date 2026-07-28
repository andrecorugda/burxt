//! Abstract Syntax Tree for Burxt v0.0.1.
//!
//! Deliberately tiny: only what the first vertical slice needs to prove the
//! thesis (exact decimal arithmetic). Everything here is backend-independent —
//! the lexer/parser/typechecker produce and consume these nodes, and codegen
//! reads them. No LLVM types leak in here.

use crate::diag::Span;

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
    /// C's `double`, at the FFI boundary only. It exists so that a crossing
    /// which would LOSE exactness can be named, and therefore refused —
    /// "a Decimal may not bind to a float" is unspellable without it. Burxt has
    /// no float type of its own and this is not one.
    CDouble,
    /// Decimal with a fixed scale S (digits after the decimal point).
    /// Represented at runtime as a scaled i64: stored = value * 10^scale.
    Decimal { scale: u32, rounding: Option<Rounding> },
    /// A struct OR enum type, by name. Typing is NOMINAL: two declarations with
    /// identical shape are different types — the name is a contract, exactly as
    /// Decimal<2> and Decimal<2, RoundHalfEven> are kept apart. Which table the
    /// name lives in (struct or enum) is the typechecker's business.
    Named(String),
    /// A growable array `[T]`, allocated in the enclosing region. Represented
    /// as { data pointer, length, capacity }. Distinct from `[T; N]`, which is
    /// fixed-size and lives on the stack.
    Slice(Box<Type>),
    /// A fixed-size stack array `[T; N]`. Arrays exist only behind bindings
    /// in this slice: indexed reads/writes and `len(a)` — never a bare value.
    Array { elem: Box<Type>, len: u32 },
    /// A generic type applied to arguments: `Option<Int>`, `Result<Int, String>`.
    /// It exists only between parsing and monomorphisation — the checker replaces every
    /// concrete one with `Named` of the instantiation's mangled name, so everything
    /// after it (layout, `match`, codegen) sees an ordinary nominal type.
    Generic { name: String, args: Vec<Type> },
    /// A type PARAMETER, inside a generic's own body and signature — the `T` of
    /// `fn largest<T>(xs: [T]) -> T`. It is not a type any value has: every one is
    /// replaced by a concrete type before codegen, one copy per instantiation.
    /// Two parameters are the same type only if they have the same name.
    /// See spec/M7-GENERICS.md.
    Param(String),
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
            Type::CDouble => write!(f, "CDouble"),
            Type::Decimal { scale, rounding: None } => write!(f, "Decimal<{}>", scale),
            Type::Decimal { scale, rounding: Some(r) } => write!(f, "Decimal<{}, {}>", scale, r),
            Type::Named(name) => write!(f, "{}", name),
            Type::Slice(elem) => write!(f, "[{}]", elem),
            Type::Array { elem, len } => write!(f, "[{}; {}]", elem, len),
            Type::Param(name) => write!(f, "{}", name),
            Type::Generic { name, args } => {
                let inner: Vec<String> = args.iter().map(|a| a.to_string()).collect();
                write!(f, "{}<{}>", name, inner.join(", "))
            }
            Type::Dyn(name) => write!(f, "dynamic {}", name),
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

/// One piece of an interpolated string, with its expression parsed.
#[derive(Debug, Clone)]
pub enum InterpPart {
    Lit(String),
    Expr(Expr),
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

/// An expression, plus where it came from.
///
/// Statement spans (v0.0.32) put the caret on the right line; these put it under
/// the right *sub-expression*, and they are what makes hover possible at all —
/// answering "what is the type here?" means knowing which expression `here` is.
#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

/// Expressions.
#[derive(Debug, Clone)]
pub enum ExprKind {
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
    /// An interpolated string: `"total: {amount}"`. Currently valid only as a
    /// direct argument to `print`, because producing a String VALUE would need
    /// allocation (M1) — printing the pieces in order needs none.
    InterpStr(Vec<InterpPart>),
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
    /// `e?` — the value if the enum's success variant, or an immediate return of the
    /// failure from the enclosing function. See spec/M8-ERRORS.md §1a.
    Try(Box<Expr>),
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
    /// Indexed read `a[i]` or `s.field[i]`. The base is a PLACE — a binding,
    /// possibly reached through fields — because an element read needs an
    /// address to GEP from.
    Index { base: Box<Expr>, index: Box<Expr> },
}

/// Statements. A Burxt v0.0.1 program is just a sequence of these.
/// One `requires` or `ensures` clause.
///
/// `text` is the clause exactly as written, kept so a failure can QUOTE it: a
/// message that says "precondition violated" makes the reader go and find which
/// one, and there is usually more than one.
#[derive(Debug, Clone)]
pub struct Contract {
    pub cond: Expr,
    pub text: String,
    pub span: Span,
}

/// A statement, plus where it came from.
///
/// The position lives here rather than inside every variant because a statement
/// is the granularity an editor underlines and a person reads. Expressions get
/// their own spans when something needs finer aim than "this line".
#[derive(Debug, Clone)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum StmtKind {
    /// `let name: Type = value;` — or `let mut name: ...` to allow
    /// reassignment. Immutable is the default; mutation is opt-in and visible.
    ///
    /// `declared` is `None` when the annotation was left off and the type comes
    /// from the initializer instead (`let count = 0;`). Only the annotation is
    /// optional: a binding still has exactly one type, fixed where it is bound.
    /// See spec/M10-ERGONOMICS.md §1.
    Let {
        name: String,
        mutable: bool,
        declared: Option<Type>,
        value: Expr,
    },
    /// `name = value;` — only valid for a `let mut` binding, and the value's
    /// type must match the declaration exactly.
    Assign { name: String, value: Expr },
    /// `name.field(.field)* = value;` — field assignment through a `let mut`
    /// binding. Mutability is per-binding, not per-field.
    AssignField { name: String, path: Vec<String>, value: Expr },
    /// `name.field(.field)*[index] = value;` — element assignment through a
    /// field path, which is what an arena needs: a struct holding the storage
    /// plus a mutating method that writes into it.
    AssignFieldIndex { name: String, path: Vec<String>, index: Expr, value: Expr },
    /// `name[index] = value;` — element assignment through a `let mut`
    /// binding, bounds-checked like every indexed access.
    AssignIndex { name: String, index: Expr, value: Expr },
    /// A call kept for its side effect, its result discarded: `f();` or
    /// `acct.deposit(10.00);`. The only expressions worth writing as a bare
    /// statement are ones with an effect — calls and mutating methods — so
    /// this is not a general expression-statement; the parser only builds it
    /// from a call or method-call shape.
    ExprStmt(Expr),
    /// `region name { .. }` — a named allocation scope. Everything allocated
    /// inside is released as a unit in O(1) when it ends. The name exists so
    /// escape errors can say which region a value would outlive.
    Region { name: String, body: Vec<Stmt> },
    /// `for name in iterable { body }` — iterate an array's elements.
    ///
    /// A real statement rather than a parser desugar. The first version WAS a desugar
    /// into `let mut i = 0; while i < len(xs) { ... }`, which worked in stage-0 and was
    /// impossible in stage-1: stage-1 names every binding by its span in the source, and
    /// a synthesized index has no span. Rather than have the two compilers implement one
    /// construct two ways, both check it directly — which also means the errors talk about
    /// `for` instead of about a `len` call the author never wrote.
    /// See spec/M10-ERGONOMICS.md §1b.
    For { name: String, iterable: Expr, body: Vec<Stmt> },
    /// `match value { Variant => { .. } .. }` — must cover every variant.
    Match { value: Expr, arms: Vec<MatchArm> },
    /// `while cond { ... }` — the condition must be a Bool; braces required.
    While { cond: Expr, body: Vec<Stmt> },
    /// `print(expr);`
    Print(Expr),
    /// `return expr;` — only valid inside a function.
    Return(Expr),
    /// `break;` — leave the enclosing loop.
    Break,
    /// `continue;` — jump to the enclosing loop's next test.
    Continue,
    /// `return tail f(args);` — a call the compiler must turn into a real tail
    /// call (constant stack) or refuse to compile. Never a silent difference
    /// between "optimized" and "hoped for".
    TailReturn(Expr),
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
    /// How this value is encoded to cross a foreign boundary, when it is not
    /// something C can hold directly. Only ever `Some` on an `extern fn`
    /// parameter: a Burxt-to-Burxt call has no encoding question.
    pub marshal: Option<Marshal>,
}

/// A declared, exactness-preserving encoding for crossing the C boundary.
/// Declared on the SIGNATURE, not applied at the call site, so the scale is part
/// of the contract instead of being lost in an `Int`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marshal {
    /// `Decimal<S> as scaled` — C receives the exact unscaled integer. Nothing
    /// is converted and nothing rounds; the scale lives in the declared type.
    Scaled,
}

impl std::fmt::Display for Marshal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Marshal::Scaled => write!(f, "scaled"),
        }
    }
}

/// One type parameter of a generic: its name, and the trait a value of it must satisfy.
///
/// A parameter with **no bound** can only be stored, copied, passed and returned — the
/// signature is the contract, so anything more has to be written in it. A bound may be one
/// of the two the language ships (`Ordered`, `Equatable`) or any declared trait, in which
/// case the parameter's methods are that trait's. See spec/M7-GENERICS.md Decision 2.
#[derive(Debug, Clone)]
pub struct TypeParam {
    pub name: String,
    pub bound: Option<String>,
}

/// `fn name(params) -> ret { body }`. Every function returns a value, and the
/// typechecker proves it returns on every path.
#[derive(Debug, Clone)]
pub struct FnDef {
    pub name: String,
    /// `fn largest<T: Ordered, U>(...)` — what this function is generic over, in order.
    /// Empty for the overwhelming majority of functions.
    pub type_params: Vec<TypeParam>,
    pub params: Vec<Param>,
    pub ret: Type,
    /// Declared `allocates`: builds values in the CALLER's region, so it may
    /// allocate without opening one and may return what it built. One bit, not a
    /// lifetime — there is no name and no scope relation to unify.
    pub allocates: bool,
    /// Declared `pure`: the result depends only on the arguments. No I/O, no FFI,
    /// and no calls to functions that do not make the same promise.
    pub is_pure: bool,
    /// Preconditions, checked on entry in the order written.
    pub requires: Vec<Contract>,
    /// Postconditions, checked before every return. `result` is in scope.
    pub ensures: Vec<Contract>,
    /// A termination measure: an Int that must strictly shrink at every recursive
    /// call and never be negative. One says the answer is right; this says an
    /// answer arrives.
    pub decreases: Option<Contract>,
    pub body: Vec<Stmt>,
    /// Where this item was written, for errors about the item itself.
    pub span: Span,
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
    /// Declared `allocates`: builds values in the CALLER's region, exactly as on a
    /// free function. The M1a spec deferred this with the trigger "a required
    /// program needs an allocating method" — `examples/symbols.bx` was it.
    pub allocates: bool,
    /// Preconditions and postconditions, exactly as on a free function. A
    /// mutating method is where contracts get interesting: `old(...)` in an
    /// `ensures` clause can compare the state after against the state before.
    pub requires: Vec<Contract>,
    pub ensures: Vec<Contract>,
    pub body: Vec<Stmt>,
    /// Where this item was written, for errors about the item itself.
    pub span: Span,
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
    /// Where this item was written, for errors about the item itself.
    pub span: Span,
}

/// `impl Trait for Type { <methods> }` — satisfaction is EXPLICIT and nominal:
/// Burxt never auto-satisfies a trait because method shapes happen to match,
/// so conformance is a deliberate, greppable declaration.
#[derive(Debug, Clone)]
pub struct ImplBlock {
    pub trait_name: String,
    pub type_name: String,
    pub methods: Vec<MethodDef>,
    /// Where this item was written, for errors about the item itself.
    pub span: Span,
}

/// `extern fn name(params) -> ret;` — a C function Burxt may call. The name
/// is the real linker symbol (never mangled); matching the C side's actual
/// signature is the programmer's contract, as in every FFI.
#[derive(Debug, Clone)]
pub struct ExternFn {
    pub name: String,
    pub params: Vec<Param>,
    pub ret: Type,
    /// Where this item was written, for errors about the item itself.
    pub span: Span,
}

/// One variant of an enum: a name plus zero or more positional payload types.
#[derive(Debug, Clone)]
pub struct Variant {
    pub name: String,
    pub payload: Vec<Type>,
}

/// `enum Name { Unit, WithPayload(Int), ... }` — a sum type. Nominal, hoisted.
#[derive(Debug, Clone)]
pub struct EnumDef {
    pub name: String,
    /// `enum Option<T> { ... }` — what this enum is generic over. Empty for the
    /// overwhelming majority. See spec/M7-GENERICS.md.
    pub type_params: Vec<TypeParam>,
    pub variants: Vec<Variant>,
    /// Where this item was written, for errors about the item itself.
    pub span: Span,
}

/// One arm of a `match`: an unqualified variant name, names for its payload,
/// and the block to run.
#[derive(Debug, Clone)]
pub struct MatchArm {
    pub variant: String,
    pub bindings: Vec<String>,
    pub body: Vec<Stmt>,
}

/// `struct Name { field: Type, ... }` — the nominal record type and the
/// substrate for Burxt's OOP layers (methods, then interfaces).
#[derive(Debug, Clone)]
pub struct StructDef {
    pub name: String,
    pub fields: Vec<Param>,
    /// Where this item was written, for errors about the item itself.
    pub span: Span,
}

/// A whole program: struct and extern declarations, function definitions,
/// and top-level statements (the implicit main). Declarations are hoisted —
/// define them in any order.
#[derive(Debug, Clone)]
pub struct Program {
    pub structs: Vec<StructDef>,
    pub enums: Vec<EnumDef>,
    pub traits: Vec<TraitDef>,
    pub impls: Vec<ImplBlock>,
    pub externs: Vec<ExternFn>,
    pub fns: Vec<FnDef>,
    pub methods: Vec<MethodDef>,
    pub stmts: Vec<Stmt>,
}
