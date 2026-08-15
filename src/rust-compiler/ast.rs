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
    /// A SIZED C integer at the FFI boundary: `i32` `u8` `u32` `u64`. Roadmap A7.
    ///
    /// **ONE variant carrying two numbers, not four variants.** `u8` versus `u32` differs only in
    /// the bit count, and no `match` arm anywhere cares about the spelling: `llvm_type` wants the
    /// bits, the range check wants the bounds, `layout_of` wants bits/8. Four variants would have
    /// been four arms at each of those sites, all saying the same thing with different numbers.
    /// `Decimal { scale, rounding }` is the precedent — a family of types held as its parameters.
    ///
    /// **Boundary-only, exactly like `CInt`**, and `validate_type` is where that is enforced: a
    /// width may appear in an `external function` signature and nowhere else. That is what keeps it
    /// out of the layout walk, out of `review`, and out of the language server — a width is never
    /// the type of a Burxt binding, so nothing downstream of the boundary can meet one.
    ///
    /// **`u64` above `Int`'s maximum is a real limit, and the range check NAMES it** rather than
    /// pretending: a Burxt `Int` is a signed i64, so a `u64` value above `i64::MAX` has no Int to
    /// land in. The upper bound checked for `u64` is therefore the SIGNED maximum, and the runtime
    /// message says so instead of claiming a range the language cannot hold.
    Width { bits: u32, signed: bool },
    /// An opaque pointer C handed back: a `FILE*`, a `DIR*`, a socket, a `char*`.
    ///
    /// Burxt treats it as a value it may MOVE but never look inside. Exactly two things can be
    /// done with one — `c_is_null(p)` asks whether the call failed, and `c_string_at(p)` copies
    /// NUL-terminated bytes into a Burxt String. No arithmetic, no indexing, no printing, not even
    /// `==`. So the pointer never becomes something the language has to reason about the lifetime
    /// of: it is a token to hand back to C, and the only way through the wall is a COPY.
    ///
    /// Printing is refused for a reason that is the thesis rather than caution: an address differs
    /// between runs, so a program that printed one would not be reproducible.
    CPointer,
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
    Generic { name: String, arguments: Vec<Type> },
    /// `(Int, String)` — a tuple: a class whose fields have positions instead of names.
    ///
    /// **It exists only between parsing and `expand`, exactly as `Generic` above does, and that
    /// is the whole design.** `expand` registers `(Int, String)` in `made_records` as an ordinary
    /// nominal class with the fields `0: Int, 1: String` and hands back `Named("(Int, String)")`.
    /// After that no rule in this compiler knows tuples exist: layout, sret, `byval`, copying,
    /// `may_be_region_storage`, `==` and the language server all see a class they already handle.
    ///
    /// **Measured before it was written, because the alternative was a new aggregate kind.**
    /// `Type::Named` is matched at 36 sites in `typeck.rs` and 19 in `codegen.rs`; a tuple that
    /// survived to codegen would need an arm beside most of them. `Type::Generic`, which dies at
    /// `expand`, is matched at 19 and 3 — and the 3 are monomorphisation plumbing a tuple never
    /// reaches. The typed AST is already POSITIONAL — `TypedExprKind::StructLit { fields }` and
    /// `Field { index }` — so a tuple lowers to the nodes codegen has emitted since v0.0.1 and
    /// `codegen.rs` needed no change at all.
    ///
    /// **The instantiation's symbol is the tuple's own spelling, `(Int, String)`.** A mangled
    /// `Tuple$Int$String` would be a name the reader never wrote, which is exactly what
    /// `declared_name` and `show` exist to prevent — and unlike a generic, where the reader at
    /// least wrote `Wrapper`, a tuple has no written name to fall back to. `(`, `,` and a space
    /// cannot occur in a declared class name, so the symbol cannot collide with one: the same
    /// argument `mangle` makes for `$`. It also means the messages that DON'T route through
    /// `show` — and there are several — print the right thing anyway.
    Tuple(Vec<Type>),
    /// A type PARAMETER, inside a generic's own body and signature — the `T` of
    /// `fn largest<T>(xs: [T]) -> T`. It is not a type any value has: every one is
    /// replaced by a concrete type before codegen, one copy per instantiation.
    /// Two parameters are the same type only if they have the same name.
    /// See spec/1.0/M7-GENERICS.md.
    Param(String),
    /// `dyn Trait` — an interface object: the ONLY thing that triggers dynamic
    /// dispatch. Represented as a fat pointer (data pointer, vtable pointer);
    /// the vtable lives outside the data, which is why the A4.5 layout
    /// guarantee means becoming an interface object never moves a field.
    Dyn(String),
    /// `dynamic Mapper<Int>` — an interface object at a generic instantiation. Roadmap A9.
    ///
    /// **It exists only between parsing and `expand`**, exactly as `Generic` and `Tuple` above
    /// do. `expand` makes the instantiation — substituting `Int` for `T` through every method
    /// signature, registering the result under the mangled name — and hands back
    /// `Dyn("Mapper$Int")`. After that, no rule in this compiler knows a generic interface
    /// exists: the vtable, the conformance check, the method lookup and `may_be_region_storage`
    /// all see the `Dyn` they have handled since v0.0.14, and two instantiations are two names.
    ///
    /// **Why this is a variant and not just `Generic`.** `dynamic` is sugar — a bare `Mapper<Int>`
    /// means the same thing, by the same v0.0.155 rule that makes a bare `Tax` mean `dynamic Tax`
    /// — so desugaring it to `Generic` in the parser was the obvious move. It is wrong, and
    /// measurably: `dynamic Holder<Int>` where `Holder` is a generic CLASS would then expand to
    /// the ordinary record `Holder$Int` and be **silently accepted**, while `dynamic Holder`
    /// without arguments is refused today with "unknown interface `Holder`". A refusal at one
    /// spelling and acceptance at another is a bug, not a design. Keeping the two apart until
    /// `expand` is what lets the generic case give the same refusal the bare case gives —
    /// see `tests/fail/dynamic_on_a_generic_class.bx`, which is that exact program.
    DynGeneric { name: String, arguments: Vec<Type> },
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
            // Spelled back exactly as it was written, which is what makes `review` need no change:
            // a parameter going `CInt` -> `u8` changes `Promise.shape`, and `review` renders that
            // from this `Display`. A width is carried by the shape already.
            Type::Width { bits, signed } => {
                write!(f, "{}{}", if *signed { "i" } else { "u" }, bits)
            }
            Type::CPointer => write!(f, "CPointer"),
            Type::CDouble => write!(f, "CDouble"),
            Type::Decimal { scale, rounding: None } => write!(f, "Decimal<{}>", scale),
            Type::Decimal { scale, rounding: Some(r) } => write!(f, "Decimal<{}, {}>", scale, r),
            Type::Named(name) => write!(f, "{}", name),
            Type::Slice(elem) => write!(f, "[{}]", elem),
            Type::Array { elem, len } => write!(f, "[{}; {}]", elem, len),
            Type::Param(name) => write!(f, "{}", name),
            Type::Tuple(elements) => {
                let inner: Vec<String> = elements.iter().map(|e| e.to_string()).collect();
                write!(f, "({})", inner.join(", "))
            }
            Type::Generic { name, arguments } => {
                let inner: Vec<String> = arguments.iter().map(|a| a.to_string()).collect();
                write!(f, "{}<{}>", name, inner.join(", "))
            }
            Type::Dyn(name) => write!(f, "dynamic {}", name),
            Type::DynGeneric { name, arguments } => {
                let inner: Vec<String> = arguments.iter().map(|a| a.to_string()).collect();
                write!(f, "dynamic {}<{}>", name, inner.join(", "))
            }
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
    /// failure from the enclosing function. See spec/1.0/M8-ERRORS.md §1a.
    Try(Box<Expr>),
    /// A comparison, e.g. `balance >= 0.00`. Produces a Bool.
    Compare {
        op: CmpOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    /// A function call, e.g. `total(19.99, 3)`.
    Call { name: String, arguments: Vec<Expr> },
    /// Struct construction: `LineItem { price: 19.99, qty: 3 }`.
    /// Every field must be given by name; any order.
    StructLit { name: String, fields: Vec<(String, Expr)> },
    /// Field access: `item.price` (chains for nested structs).
    Field { base: Box<Expr>, field: String },
    /// Method call: `item.total()`.
    MethodCall { base: Box<Expr>, method: String, arguments: Vec<Expr> },
    /// A tuple literal: `(1, "a")`. Two or more elements — one is a parenthesised
    /// expression and zero is nothing at all.
    ///
    /// The ONE new expression kind A8 costs. The checker types the elements, builds
    /// `Type::Tuple`, expands it to the anonymous class, and emits an ordinary
    /// `TypedExprKind::StructLit` — so codegen never learns the word.
    TupleLit(Vec<Expr>),
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
    /// See spec/1.0/M10-ERGONOMICS.md §1.
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
    /// See spec/1.0/M10-ERGONOMICS.md §1b.
    For { name: String, iterable: Expr, body: Vec<Stmt> },
    /// `for name in start..end { body }` — count up, end EXCLUSIVE.
    ///
    /// ---- the five decisions, and what each costs -----------------------------------
    ///
    /// 1. EXCLUSIVE, and there is no inclusive form at all. `0..3` is 0, 1, 2. Three
    ///    reasons in order of weight: (a) the idiom this replaces is
    ///    `while i < len(xs)`, and `0..len(xs)` is the same bound written the same way,
    ///    where an inclusive range would need `0..=len(xs) - 1` and put an arithmetic
    ///    correction into the most-written line in the language; (b) half-open ranges
    ///    compose — `a..b` and `b..c` tile `a..c` with no overlap and no gap, which is
    ///    why `substring(s, from, LENGTH)` and every slice API in the codebase are
    ///    already half-open; (c) two forms differing by ONE character, where that
    ///    character changes the number of iterations, is exactly the class of defect a
    ///    reviewer's eye slides over. The cost is named: a loop that genuinely wants to
    ///    touch `n` writes `0..n + 1`, which is one visible `+ 1` instead of an
    ///    invisible `=`. `..=` and `...` are refused in the LEXER with that sentence.
    ///
    /// 2. A range is NOT a value. `let r = 0..3;` is refused. A range as a value wants
    ///    an iterator protocol — that is roadmap A11 — and the half version buildable
    ///    today would be a two-field record with no `next`, no laziness and no way to
    ///    pass it anywhere useful. Then A11 would have to either keep it or break it.
    ///    So the only place `..` may appear is between the bounds of a `for`, and the
    ///    parser says so by name. The cost: no `for i in r`, no range parameter, and no
    ///    `range(n)` replacement in `lib/` until A11.
    ///
    /// 3. A real statement kind, not a parser desugar into `while` — the same decision
    ///    `StmtKind::For` records above, for the same reason. A desugar would make
    ///    `for i in 0..xs` complain about comparing an Int with an array, in a `while`
    ///    the author never wrote. Lowering happens in codegen, to exactly the shape of
    ///    the hand-written `while i < n`: two stack slots, no allocation, no iterator.
    ///
    /// 4. The bounds are evaluated ONCE, before the loop. This is a real difference from
    ///    `for x in xs`, which re-reads the array's length header every pass and so SEES
    ///    a `push` from inside the body. `for i in 0..len(xs)` snapshots the length
    ///    instead. Both are defensible; what is not defensible is leaving it unwritten.
    ///    Once is the right answer here because the end may be any expression — a call,
    ///    a field path, arithmetic — and re-evaluating a call per pass is the cost
    ///    `StmtKind::For` refuses outright by demanding a name.
    ///
    /// 5. Reversed literal bounds are refused; reversed computed bounds run zero times.
    ///    `for i in 3..0` can only be a mistake, both values are known at compile time,
    ///    and refusing costs nothing. `for i in a..b` with `a > b` runs zero times,
    ///    because the lowering is `i = a; while i < b`, which is what every count-up
    ///    loop in `lib/` already does — and `for i in 0..len(xs)` over an empty array
    ///    MUST run zero times rather than trap. The asymmetry is deliberate: a compiler
    ///    refuses what it can see, and a range it cannot see is not an error.
    ///    `0..0` is allowed and runs zero times; only strictly-decreasing is refused.
    ForRange { name: String, start: Expr, end: Expr, body: Vec<Stmt> },
    /// `match value { Variant => { .. } .. }` — must cover every variant.
    Match { value: Expr, arms: Vec<MatchArm> },
    /// `while cond { ... }` — the condition must be a Bool; braces required.
    While { cond: Expr, body: Vec<Stmt> },
    /// `print(expr);`
    /// `print(x)` and `print_error(x)` — one statement with a destination, not two statements.
    ///
    /// One variant on purpose: the per-type formatting is the part that must never fork. Two
    /// statements would mean two formatters, and the first time one of them learned about a new type
    /// the other would print something different for the same value.
    Print { value: Expr, to_stderr: bool },
    /// `return expr;` — only valid inside a function.
    Return(Expr),
    /// `break;` — leave the enclosing loop.
    Break,
    /// `continue;` — jump to the enclosing loop's next test.
    Continue,
    /// `return tail f(arguments);` — a call the compiler must turn into a real tail
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
    /// Where the TYPE was written, not where the declaration starts. B17.
    ///
    /// `function scaled(n: CInt)` is refused, and stage-0 used to draw the caret at column 1 — the
    /// `function` keyword — because `validate_type` reports a string and the caller attached the
    /// nearest span it had, which was the whole declaration. Stage-1 named the offending token
    /// instead, so the two compilers refused the same program and pointed at different places.
    ///
    /// The span is not cosmetic: it is where the editor draws the squiggle and what the language
    /// server returns. A caret on `function` tells a reader to look at the wrong thing.
    pub ty_span: Span,
    /// `mutable xs: [Int]` — the callee may modify the CALLER's value, and the signature says so.
    ///
    /// The mechanism is the one `mutable self` has always used: an aggregate parameter is normally
    /// `byval`, so LLVM copies it and a callee writing to it changes its own copy. A `mutable` one is
    /// a plain pointer to the caller's storage instead — same ABI decision, same soundness argument,
    /// already proven by every method that mutates.
    ///
    /// Only aggregates may be `mutable`, and that is a rule about MEANING rather than a limitation.
    /// On a scalar the word would have to mean "you get your own copy to change", which is a fact
    /// about the body and not about the call — so one word would mean two different things depending
    /// on the type, decided silently. A local copy is written `let mutable n: Int = parameter;`,
    /// which says where the copy is.
    pub writable: bool,
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

/// One type parameter of a generic: its name, and the interface a value of it must satisfy.
///
/// A parameter with **no bound** can only be stored, copied, passed and returned — the
/// signature is the contract, so anything more has to be written in it. A bound may be one
/// of the two the language ships (`Ordered`, `Equatable`) or any declared trait, in which
/// case the parameter's methods are that trait's. See spec/1.0/M7-GENERICS.md Decision 2.
#[derive(Debug, Clone)]
pub struct TypeParam {
    pub name: String,
    pub bound: Option<String>,
}

/// `fn name(parameters) -> ret { body }`. Every function returns a value, and the
/// typechecker proves it returns on every path.
#[derive(Debug, Clone)]
pub struct FnDef {
    /// Declared `public`, so a package that DEPENDS on this one may reach it. C2.
    ///
    /// The boundary is the package and not the file, because `use` concatenates every source into
    /// one buffer (M6 Decision 5) — there is no file boundary at runtime for anything to be private
    /// across. Inside a package everything stays visible, which is why adding this changed no
    /// existing program.
    ///
    /// Note the asymmetry with `private_fields`, which is deliberate rather than an oversight: a
    /// field opts OUT of being seen by the class's users, and a declaration opts IN to being seen
    /// by another package. Two boundaries, two defaults, and the default is the safe one at each —
    /// a field is part of a class you are already holding, and a package's declaration is not.
    pub public: bool,
    pub name: String,
    /// `fn largest<T: Ordered, U>(...)` — what this function is generic over, in order.
    /// Empty for the overwhelming majority of functions.
    pub type_parameters: Vec<TypeParam>,
    pub parameters: Vec<Param>,
    pub ret: Type,
    /// Declared `allocates`: builds values in the CALLER's region, so it may
    /// allocate without opening one and may return what it built. One bit, not a
    /// lifetime — there is no name and no scope relation to unify.
    pub allocates: bool,
    /// Declared `allocates nothing` — a CHECKED CLAIM rather than a permission.
    ///
    /// The mirror image of `allocates`, and the reason it is worth having is that the two say
    /// opposite things about who is trusted. `allocates` is the programmer telling the compiler
    /// something it will verify anyway; since v0.0.142 it is inferred, so writing it adds nothing.
    /// `allocates nothing` is the programmer asking the compiler to *hold them to it* — the useful
    /// direction, because a function that quietly starts allocating is how a constant-memory loop
    /// stops being one.
    ///
    /// Transitive, because the inference is: a function that calls something that allocates does
    /// allocate, and a claim that stopped at the first call would be worth nothing.
    pub allocates_nothing: bool,
    /// Declared `pure`: the result depends only on the arguments. No I/O, no FFI,
    /// and no calls to functions that do not make the same promise.
    pub is_pure: bool,
    /// Declared `touches ...`. Optional and inferred: the compiler works out what a body reaches
    /// and refuses a declaration that claims LESS than that. Over-declaring is allowed, because
    /// it is a promise the function keeps.
    pub touches: Vec<Effect>,
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

/// `fn (self: Type) name(parameters) -> ret { body }` — a method: a function in
/// the receiver type's namespace. `fn (mut self: Type) ...` declares a
/// MUTATING method, callable only through a `let mut` binding; the receiver
/// is then passed as a true reference, not a value copy (see the aggregate
/// ABI — this is the one place Burxt passes an aggregate by address on
/// purpose, mirroring the existing field-assignment mutability rule).
#[derive(Debug, Clone)]
pub struct MethodDef {
    pub receiver: String,
    /// Declared `touches ...`, inferred and verified exactly as on a free function.
    pub touches: Vec<Effect>,
    /// Declared `private` inside its class body: callable only from that class's own methods.
    /// Always false for a method written outside a class, where there is no boundary to be
    /// private from.
    pub private: bool,
    /// Declared `pure`: the answer depends only on the arguments, and the receiver IS an
    /// argument — so `self.x` is readable and nothing else about the program may be.
    ///
    /// **This field is why A4 cost more on the Rust side than the Burxt one, which is the
    /// reverse of the usual direction and worth saying at the site.** Normally stage-0 has the
    /// machinery and stage-1 is catching up. Here stage-1's method item already carried the bit —
    /// its markers ride in one flags word, and `flag_pure` read it for free — while stage-0 had
    /// no place to put it at all, because `pure` had been refused in the PARSER since the marker
    /// existed. A refusal in the parser leaves no room in the tree, so nothing downstream could
    /// have asked the question even if it wanted to.
    ///
    /// It is also what `burxt review` reads to notice a method that has STOPPED being pure. Both
    /// reviewers wrote `is_pure: false` for every method until v0.0.247, which was correct while
    /// the marker was unspellable and became an under-report the moment it was not — and an
    /// under-reporting semver gate is worse than none, because someone relies on it.
    pub is_pure: bool,
    /// `function (mutable self: Stack<T>) push_one(...)` — the receiver's type arguments,
    /// which for a method are always the class's own parameter NAMES. A method may use the
    /// parameters of the type it is on and declare none of its own, per
    /// spec/1.0/M7-GENERICS.md Decision 3 — so these are names, not types.
    pub receiver_arguments: Vec<String>,
    pub receiver_mut: bool,
    pub name: String,
    pub parameters: Vec<Param>,
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

/// One method signature inside a `interface` declaration: a name, a receiver form
/// and a type — no body, no fields, no state. Interfaces declare signatures only.
#[derive(Debug, Clone)]
pub struct InterfaceSig {
    pub name: String,
    pub receiver_mut: bool,
    pub parameters: Vec<Param>,
    pub ret: Type,
}

/// `trait Name { fn m(self) -> T ... }` — a named set of method signatures a
/// type can promise to satisfy. That is the whole concept.
#[derive(Debug, Clone)]
pub struct InterfaceDef {
    /// Declared `public`, so a package that DEPENDS on this one may reach it. C2.
    ///
    /// The boundary is the package and not the file, because `use` concatenates every source into
    /// one buffer (M6 Decision 5) — there is no file boundary at runtime for anything to be private
    /// across. Inside a package everything stays visible, which is why adding this changed no
    /// existing program.
    ///
    /// Note the asymmetry with `private_fields`, which is deliberate rather than an oversight: a
    /// field opts OUT of being seen by the class's users, and a declaration opts IN to being seen
    /// by another package. Two boundaries, two defaults, and the default is the safe one at each —
    /// a field is part of a class you are already holding, and a package's declaration is not.
    pub public: bool,
    pub name: String,
    /// `interface Mapper<T> { function apply(self, x: T) -> T }` — what this interface is
    /// generic over, in order. Empty for the overwhelming majority. Roadmap A9.
    ///
    /// An instantiation is MONOMORPHISED, exactly as a generic class is: `expand` turns
    /// `Mapper<Int>` into `Dyn("Mapper$Int")` and registers a signature set under that
    /// mangled name. After that pass no rule in the checker knows a generic interface
    /// exists — the vtable, the method lookup and the conformance check all key off the
    /// interface NAME they already keyed off, and two instantiations are two names.
    pub type_parameters: Vec<TypeParam>,
    pub methods: Vec<InterfaceSig>,
    /// Where this item was written, for errors about the item itself.
    pub span: Span,
}

/// `impl Trait for Type { <methods> }` — satisfaction is EXPLICIT and nominal:
/// Burxt never auto-satisfies an interface because method shapes happen to match,
/// so conformance is a deliberate, greppable declaration.
#[derive(Debug, Clone)]
pub struct ImplBlock {
    pub interface_name: String,
    /// `implement Mapper<Int> for Doubler` — the arguments the interface is being
    /// implemented AT. Empty for a non-generic interface, which is almost all of them.
    ///
    /// Kept beside `interface_name` rather than mangled into it by the parser, because the
    /// parser does not know which names are generic and a message must be able to say
    /// `Mapper<Int>` — the name the author wrote. `check_program_inner` resolves the pair
    /// to the mangled symbol once `expand` has made the instantiation.
    pub interface_arguments: Vec<Type>,
    pub type_name: String,
    pub methods: Vec<MethodDef>,
    /// Synthesized from `class X implements Y`, rather than written as a standalone
    /// `implement Y for X { ... }` block.
    ///
    /// When true, `methods` is EMPTY and the class's own methods are what satisfy the
    /// interface — so conformance is checked against the method table rather than against a
    /// list this block carries. The standalone form stays legal, because it is the only way
    /// to add an interface to a class declared somewhere else.
    pub declared_on_class: bool,
    /// Where this item was written, for errors about the item itself.
    pub span: Span,
}

/// `extern fn name(parameters) -> ret;` — a C function Burxt may call. The name
/// is the real linker symbol (never mangled); matching the C side's actual
/// signature is the programmer's contract, as in every FFI.
#[derive(Debug, Clone)]
pub struct ExternFn {
    pub name: String,
    pub parameters: Vec<Param>,
    pub ret: Type,
    /// Declared `touches ...`. REQUIRED reasoning lives at this boundary: there is no body to
    /// infer from, so whatever a C function reaches, only the declaration can say.
    pub touches: Vec<Effect>,
    /// Where this item was written, for errors about the item itself.
    pub span: Span,
}

/// What a function reaches outside itself.
///
/// A closed vocabulary, and it has to be: two libraries spelling the same thing `network` and
/// `net` would make the whole point — a reviewer scanning for what a change can reach — useless.
///
/// The list is what the language can actually DISTINGUISH plus what the FFI boundary needs to be
/// able to say. `system`, `time` and `getchar` are all just `external function` today, so nothing
/// below the boundary could derive `commands` from `clock`; the programmer declares it there, and
/// everything above it is inferred. Exactly the shape `allocates` took — declared where there is
/// no body, worked out where there is.
///
/// `print` is deliberately NOT here. It would be on almost every function, and an annotation that
/// is on everything tells a reviewer nothing — the lesson `allocates` taught in v0.0.142.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Effect {
    /// `read_file`, `write_file`, `write_bytes`, and anything at the boundary that says so.
    Files,
    /// Runs another program. The one that can do anything, so it is named on its own.
    Commands,
    /// The clock, a random source — anything that answers differently for the same arguments.
    Clock,
    /// Reads stdin, arguments, the environment.
    Input,
    /// Speaks to something over a network.
    Network,
    /// Asks a language model. Kept apart from `network` because the rule that matters is about
    /// this one: an LLM may decide what to DO, never what a NUMBER is.
    Model,
}

impl Effect {
    pub fn parse(word: &str) -> Option<Effect> {
        Some(match word {
            "files" => Effect::Files,
            "commands" => Effect::Commands,
            "clock" => Effect::Clock,
            "input" => Effect::Input,
            "network" => Effect::Network,
            "model" => Effect::Model,
            _ => return None,
        })
    }

    pub fn all() -> &'static str {
        "files, commands, clock, input, network, model"
    }
}

impl std::fmt::Display for Effect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Effect::Files => "files",
            Effect::Commands => "commands",
            Effect::Clock => "clock",
            Effect::Input => "input",
            Effect::Network => "network",
            Effect::Model => "model",
        })
    }
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
    /// Declared `public`, so a package that DEPENDS on this one may reach it. C2.
    ///
    /// The boundary is the package and not the file, because `use` concatenates every source into
    /// one buffer (M6 Decision 5) — there is no file boundary at runtime for anything to be private
    /// across. Inside a package everything stays visible, which is why adding this changed no
    /// existing program.
    ///
    /// Note the asymmetry with `private_fields`, which is deliberate rather than an oversight: a
    /// field opts OUT of being seen by the class's users, and a declaration opts IN to being seen
    /// by another package. Two boundaries, two defaults, and the default is the safe one at each —
    /// a field is part of a class you are already holding, and a package's declaration is not.
    pub public: bool,
    pub name: String,
    /// `enum Option<T> { ... }` — what this enum is generic over. Empty for the
    /// overwhelming majority. See spec/1.0/M7-GENERICS.md.
    pub type_parameters: Vec<TypeParam>,
    pub variants: Vec<Variant>,
    /// Where this item was written, for errors about the item itself.
    pub span: Span,
}

/// One arm of a `match`: an unqualified variant name, names for its payload,
/// and the block to run.
#[derive(Debug, Clone)]
pub struct MatchArm {
    /// The variant name for an enum arm, the literal's text for a scalar arm, and `_` for a
    /// wildcard. Kept as a String because every message quotes it, and `_` was already the
    /// wildcard's spelling before scalar matching existed.
    pub variant: String,
    pub bindings: Vec<String>,
    pub body: Vec<Stmt>,
    /// Set when the pattern is a LITERAL rather than a variant name — `match status { 200 => ... }`.
    ///
    /// A scalar `match` is desugared to an `if / else if` chain by the checker, so nothing below
    /// it and nothing in either backend learns a new statement kind. The comparison is the
    /// ordinary `==`, which is already correct for an Int and already uses `burxt.streq` for a
    /// String — so there is no new branching to get wrong, which matters more here than a switch
    /// table would.
    pub literal: Option<MatchLiteral>,
}

/// A literal in a `match` pattern. Only the types `==` accepts, because the desugaring IS `==`.
#[derive(Debug, Clone, PartialEq)]
pub enum MatchLiteral {
    Int(i64),
    Text(String),
    Truth(bool),
}

/// `struct Name { field: Type, ... }` — the nominal class type and the
/// substrate for Burxt's OOP layers (methods, then interfaces).
#[derive(Debug, Clone)]
pub struct StructDef {
    /// Declared `public`, so a package that DEPENDS on this one may reach it. C2.
    ///
    /// The boundary is the package and not the file, because `use` concatenates every source into
    /// one buffer (M6 Decision 5) — there is no file boundary at runtime for anything to be private
    /// across. Inside a package everything stays visible, which is why adding this changed no
    /// existing program.
    ///
    /// Note the asymmetry with `private_fields`, which is deliberate rather than an oversight: a
    /// field opts OUT of being seen by the class's users, and a declaration opts IN to being seen
    /// by another package. Two boundaries, two defaults, and the default is the safe one at each —
    /// a field is part of a class you are already holding, and a package's declaration is not.
    pub public: bool,
    pub name: String,
    /// `record List<T> { items: [T] }` — what this record is generic over. Empty for the
    /// overwhelming majority. See spec/1.0/M7-GENERICS.md.
    pub type_parameters: Vec<TypeParam>,
    pub fields: Vec<Param>,
    /// Field names declared `private`: reachable only from this class's own methods.
    ///
    /// A list on the CLASS rather than a flag on `Param`, because `Param` is also a function
    /// parameter and a private parameter is not a thing. Linear lookup, over a handful of
    /// names, at a site that already resolves a type.
    pub private_fields: Vec<String>,
    /// Where this item was written, for errors about the item itself.
    pub span: Span,
}

/// `const NAME: Type = <compile-time value>;` — a name for a literal.
///
/// ## What `const` adds that `let` does not, measured rather than assumed
///
/// `let` without `mutable` is already immutable, so "immutable" is not the answer. The
/// question was asked properly — by running the compiler — and there are three answers, of
/// which the third on its own settles it:
///
/// 1. **A top-level `let` is not in scope inside a function.** Top-level statements ARE
///    the body of `main`, so `let LIMIT: Int = 100;` followed by `function bump(x: Int)
///    -> Int { return x + LIMIT; }` is rejected with `unknown variable: LIMIT`. Every
///    magic number in `lib/` is inside a function, so a top-level `let` could not have
///    named one. A `const` is an ITEM, and an item is visible to every body in the
///    program.
/// 2. **A `const` is a literal, not a load.** It is folded here at check time and reaches
///    codegen already lowered to `TypedExprKind::IntLit`/`DecimalLit`/`BoolLit`/`StrLit`,
///    so it costs nothing at run time, is legal inside a `pure` function, and needs no
///    global storage and no initialization order. That is why `codegen.rs` learned
///    nothing about `const` at all.
/// 3. **A `let` cannot appear in a module AT ALL.** `main.rs` refuses a top-level statement
///    in any file reached by `use` — *"a module holds declarations, not statements: this
///    would run when `helper.bx` was used, and a `use` is not a call"* (spec/1.0/M6-MODULES.md
///    §1.3). So the answer to "why not just use a top-level `let`?" for `lib/math.bx` is
///    not that it would be awkward: it does not compile. A `const` is a declaration, so it
///    is allowed there, which is the entire reason A2 unblocks a standard-library module.
///
/// ## What may be on the right-hand side
///
/// A literal, another `const` declared ABOVE this one, or `+ - *` and unary `-` over
/// those — **for `Int` only**. Folding is done with checked arithmetic and a fold that
/// overflows is a compile error, never a wrap.
///
/// `/` is absent and that was a correction, not a design: the first evaluator folded it with
/// `checked_div` and shipped its own division-by-zero refusal, which made `const HALF: Int =
/// LIMIT / 2;` legal in a language where `let half: Int = n / 2;` is not — Burxt refuses `/`
/// on two Ints because one operator cannot say whether it rounds toward zero or down. Knowing
/// the operands at compile time does not answer that question. So a const `/` is now refused
/// by exactly the rule a `let` `/` is refused by, and gets the same sentence.
///
/// The arithmetic is not decoration and this was also measured: `INT_MIN` **cannot be
/// written as a literal**. `-9223372036854775808` is lexed as a negation of
/// `9223372036854775808`, which is `integer literal too large`, so the one constant A2
/// exists to name needs `-9223372036854775807 - 1` — a fold. A literal-only `const`
/// would have shipped without the flagship case.
///
/// **What it costs.** `Decimal`, `String` and `Bool` consts must be a single literal (or
/// a copy of another const): no arithmetic. Deliberate, and the reasons differ per type.
/// A `Decimal` `*` narrows and so needs a rounding contract, which would put a rounding
/// mode somewhere other than a signature for the first time; `+` would then be the only
/// Decimal operator allowed, and "one of them works" is a worse rule to remember than
/// "none do". A `String` `+` folds perfectly well, but `+` on Strings MEANS allocate, and
/// a reader meeting `const GREETING: String = A + B;` would rightly ask where it lives —
/// so a String const stays exactly as expressive as a String literal. Every item A2 was
/// filed to unblock (`INT_MAX`, `INT_MIN`, CRC and hash polynomials, buffer sizes) is an
/// `Int`, so this is the smaller thing that is still the useful thing.
///
/// **Also not here, and named rather than discovered:** a `const` cannot appear in a TYPE
/// (`[Int; SIZE]`, `Decimal<SCALE>`) or as a `match` pattern. Both are separate grammars
/// in four files, neither is needed by anything on the A2 list, and a use site that is a
/// value is the whole of what a named number is for.
#[derive(Debug, Clone)]
pub struct ConstDef {
    pub name: String,
    /// Required, never inferred — unlike `let`, where `let count = 0;` is legal.
    ///
    /// A `const` is a name the whole program can read, and the declaration is the only
    /// place a reader will look to find out what it is. `let` can leave the annotation
    /// off because the initializer is one line above the use; a `const` used 900 lines
    /// away has no such consolation. It also settles a Decimal's scale where the reader
    /// can see it: `const RATE = 8.25%;` would be asking them to know the lexer's rules.
    pub declared: Type,
    pub value: Expr,
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
    pub interfaces: Vec<InterfaceDef>,
    pub impls: Vec<ImplBlock>,
    pub externs: Vec<ExternFn>,
    pub fns: Vec<FnDef>,
    pub methods: Vec<MethodDef>,
    /// `const` declarations, in SOURCE ORDER — which matters, because a `const` may
    /// only be built from consts declared above it. Everything else in this struct is
    /// hoisted; this one deliberately is not. Folding needs an order, and "the order it
    /// is written in" is the only one a reader can see. A hoisted const would let
    /// `const A: Int = B; const B: Int = 1;` work, and then someone would write a cycle.
    pub consts: Vec<ConstDef>,
    pub stmts: Vec<Stmt>,
}
