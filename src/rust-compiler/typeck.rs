//! Typechecker: the stage where Burxt's *thesis* is enforced.
//!
//! Rules for v0.0.1 (deliberately strict — correctness by construction):
//!   * Every `let` must declare a type, and the expression's inferred type must
//!     match it exactly. No implicit widening, no surprises.
//!   * `Int + Int = Int`, `Int * Int = Int`.
//!   * `Decimal<S> + Decimal<S> = Decimal<S>` — scales MUST match. Adding
//!     `Decimal<2>` to `Decimal<3>` is a compile error, not a silent coercion.
//!     The same goes for rounding contracts: `Decimal<2, RoundHalfEven>` and
//!     `Decimal<2>` are different types, and Burxt never reconciles them
//!     silently.
//!   * `Decimal<S> * Int = Decimal<S>` (scaling a money value by a count).
//!     This is the `price * qty` case. It is always exact, so no rounding
//!     contract is required.
//!   * `Decimal<S,R> * Decimal<S,R>` and `Decimal<S,R> / Decimal<S,R>` (or
//!     `/ Int`) produce digits beyond scale S, so they are only allowed when
//!     the operands carry a rounding contract R — that contract says exactly
//!     how the result returns to scale S. Without one: compile error.
//!   * `Int / Int` is refused for now: truncation is silent rounding, which is
//!     exactly what Burxt exists to prevent. It will return with explicit
//!     semantics.
//!   * There is no float type at all, so float↔decimal mixing is impossible by
//!     construction — the strongest possible version of "no silent float".
//!
//! Output: a `TypedProgram` where every expression is annotated with its type,
//! so codegen never has to re-derive types.

use crate::diag::{Diagnostic, Span};
use crate::ast::*;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

/// A typed expression: the original node plus its resolved type.
#[derive(Debug, Clone)]
pub struct TypedExpr {
    pub ty: Type,
    pub kind: TypedExprKind,
}

#[derive(Debug, Clone)]
pub enum TypedExprKind {
    IntLit(i64),
    /// Decimal literal already normalized to the binding's scale.
    /// `unscaled` is value * 10^scale.
    DecimalLit { unscaled: i64 },
    BoolLit(bool),
    StrLit(String),
    Var(String),
    /// Negation of a non-literal (literals are folded at check time).
    Neg(Box<TypedExpr>),
    Not(Box<TypedExpr>),
    /// `truncate(xs, n)` — drop everything past `n`. The counterpart to `push`, and
    /// the primitive a scope needs: leaving a block drops every binding it made.
    Truncate { place: Box<TypedExpr>, length: Box<TypedExpr> },
    /// `argument_count()` and `argument(n)` — the command line. A compiler needs to know which
    /// file it was asked to compile.
    ArgCount,
    Arg(Box<TypedExpr>),
    /// `write_file(path, contents)` — how a backend emits anything.
    WriteFile { path: Box<TypedExpr>, contents: Box<TypedExpr> },
    /// `write_bytes(path, buffer)` — the bytes of a growable `[Int]`, written out.
    WriteBytes { path: Box<TypedExpr>, buffer: Box<TypedExpr> },
    /// `substring(s, at, len)` — a copy of part of a String, in the current region.
    Substring { source: Box<TypedExpr>, at: Box<TypedExpr>, len: Box<TypedExpr> },
    /// `divide_floor`, `divide_toward_zero` or `remainder` on two Ints. Three names rather than one
    /// operator, because they disagree on negatives.
    IntDiv { kind: crate::codegen::IntDiv, lhs: Box<TypedExpr>, rhs: Box<TypedExpr> },
    Bit { kind: crate::codegen::BitOp, lhs: Box<TypedExpr>, rhs: Option<Box<TypedExpr>> },
    /// `old(expr)` in an `ensures` clause: the value that expression had on
    /// ENTRY, by index into the function's hoisted list.
    Old(usize),
    /// `read_file(path)`: the file's bytes as a region-allocated String.
    ReadFile(Box<TypedExpr>),
    CIsNull(Box<TypedExpr>),
    CStringAt(Box<TypedExpr>),
    CBytesAt { pointer: Box<TypedExpr>, count: Box<TypedExpr> },
    /// `hold(value)` — file a value in the handle table and answer the packed handle. M17.
    ///
    /// `of` is the CLASS NAME rather than a number, because the tag that guards a handle mixes
    /// in a fingerprint of the whole program, and that is not known until every declaration has
    /// been seen. Deciding it here would mean deciding it too early.
    Hold { value: Box<TypedExpr>, of: String },
    /// `held(handle)` — the value back, or one of three named refusals. M17.
    Held { handle: Box<TypedExpr>, of: String },
    CBytesTo { pointer: Box<TypedExpr>, bytes: Box<TypedExpr> },
    /// `to_string(v)`: the value's exact display form, region-allocated.
    ToString(Box<TypedExpr>),
    /// `byte_at(s, i)`: the i-th byte as an Int, bounds-checked at runtime.
    ByteAt { s: Box<TypedExpr>, index: Box<TypedExpr> },
    /// `byte_as_string(n)`: the one-byte String whose only byte is `n`, region-allocated.
    /// The exact inverse of `ByteAt`, and range-checked the same way an index is.
    ByteAsString(Box<TypedExpr>),
    /// `hash(x)`: a deterministic, unseeded hash of an Equatable value.
    ///
    /// Unseeded on purpose. The same input hashes the same in every run on every machine, which
    /// is what lets a map iterate in a defined order and a program that contains one stay
    /// reproducible. The trade — no HashDoS protection — and the trigger that would change it are
    /// in spec/1.0/M11-MAPS.md Decision 4.
    Hash(Box<TypedExpr>),
    /// `len(s)` on a String: a runtime byte scan (an array's length folds to a
    /// constant instead, so it never reaches codegen).
    StrLen(Box<TypedExpr>),
    /// Short-circuiting `&&` / `||`; codegen must not evaluate `rhs` when
    /// `lhs` already decides the result.
    Logical { op: LogicalOp, lhs: Box<TypedExpr>, rhs: Box<TypedExpr> },
    Binary {
        op: BinOp,
        lhs: Box<TypedExpr>,
        rhs: Box<TypedExpr>,
    },
    Compare {
        op: CmpOp,
        lhs: Box<TypedExpr>,
        rhs: Box<TypedExpr>,
    },
    Call { name: String, arguments: Vec<TypedExpr> },
    /// Method call, resolved to its receiver type. `receiver_mut` decides how
    /// codegen passes `base`: a true reference (mutating) or a value copy.
    MethodCall {
        receiver: String,
        method: String,
        receiver_mut: bool,
        base: Box<TypedExpr>,
        arguments: Vec<TypedExpr>,
    },
    /// Build an interface object from a concrete binding: a fat pointer pairing the
    /// binding's storage with the static (Type, Trait) vtable.
    DynCoerce { interface_name: String, concrete: String, var: String },
    /// A dynamically dispatched call: load slot `slot` from the receiver's
    /// vtable and call it with the data pointer.
    DynCall {
        interface_name: String,
        method: String,
        slot: u32,
        base: Box<TypedExpr>,
        arguments: Vec<TypedExpr>,
    },
    /// Struct construction; fields re-emitted in DECLARATION order, so
    /// codegen is purely positional.
    StructLit { name: String, fields: Vec<TypedExpr> },
    /// Field access, resolved to a positional index.
    Field { base: Box<TypedExpr>, index: u32 },
    /// Array literal (only ever a `let` initializer).
    ArrayLit(Vec<TypedExpr>),
    /// A growable-array literal: allocate in the region, then fill.
    SliceLit(Vec<TypedExpr>),
    /// `push(xs, v)` — append, growing in the region if needed. Returns the
    /// new length.
    Push { place: Box<TypedExpr>, value: Box<TypedExpr> },
    /// `len(xs)` on a growable array: a runtime field read.
    SliceLen(Box<TypedExpr>),
    /// Growable-array element read, bounds-checked against the runtime length.
    SliceIndex { base: Box<TypedExpr>, index: Box<TypedExpr> },
    /// `e?`: the success payload, or return the failure from the enclosing function.
    /// Everything the lowering needs is settled here — which variant fails, which
    /// succeeds, and what the caller's failure variant is. See spec/1.0/M8-ERRORS.md §1a.
    Try {
        value: Box<TypedExpr>,
        fail_tag: u32,
        ok_tag: u32,
        ret_enum: String,
        ret_fail_tag: u32,
    },
    /// Enum construction: the variant's index plus its payload values.
    VariantLit { enum_name: String, tag: u32, arguments: Vec<TypedExpr> },
    /// Bounds-checked indexed read from a place; `len` is the static length.
    Index { base: Box<TypedExpr>, len: u32, index: Box<TypedExpr> },
}

#[derive(Debug, Clone)]
pub enum TypedStmtKind {
    Let { name: String, ty: Type, value: TypedExpr },
    Assign { name: String, value: TypedExpr },
    /// Field assignment, path resolved to positional indices.
    AssignField { name: String, indices: Vec<u32>, value: TypedExpr },
    /// A call kept for its side effect; the result is evaluated and discarded.
    ExprStmt(TypedExpr),
    /// `exit(code)` — end the process with a status a shell can read.
    ///
    /// A statement rather than a builtin call, because a builtin has to answer with a type and this
    /// one never answers. Typing it `Int` would be a small lie in a language whose argument is that
    /// it does not tell them.
    Exit(TypedExpr),
    /// Element assignment through a field path: walk `indices` to the array
    /// field, then a bounds-checked element store.
    AssignFieldIndex {
        name: String,
        indices: Vec<u32>,
        len: u32,
        index: TypedExpr,
        value: TypedExpr,
    },
    /// Bounds-checked element assignment.
    AssignIndex { name: String, len: u32, index: TypedExpr, value: TypedExpr },
    Print { value: TypedExpr, to_stderr: bool },
    /// `region name { .. }`: open a region, run the body, release as a unit.
    Region { name: String, body: Vec<TypedStmt> },
    /// An ORDINARY block that the escape analysis proved keeps nothing — so it
    /// releases at its closing brace exactly as a `region` does. M14 slice 3 / A12.
    ///
    /// Not `Region`, and the difference is the point: a `region` is something the
    /// programmer wrote and every rule about it is a rule about their word. This is a
    /// placement the compiler chose, it carries no name, and it exists only where
    /// `place_releases` proved that nothing allocated inside reaches a binding declared
    /// outside. A block that fails the proof is simply left alone, which is exactly the
    /// behaviour before this variant existed — that is what makes M14 additive.
    Release { body: Vec<TypedStmt> },
    /// `for name in iterable { body }`. The element type and whether the array is fixed
    /// or growable are settled by the checker, so codegen only has to walk it.
    For { name: String, elem: Type, iterable: TypedExpr, body: Vec<TypedStmt> },
    /// `for name in start..end { body }`. Both bounds are Ints by the time this exists,
    /// so codegen carries no type question at all. See `ast::StmtKind::ForRange`.
    ForRange { name: String, start: TypedExpr, end: TypedExpr, body: Vec<TypedStmt> },
    /// `match` on an enum: arms in TAG order, each with the names bound to its
    /// payload slots. Exhaustiveness was proven by the typechecker.
    Match { value: TypedExpr, arms: Vec<TypedArm> },
    /// `print` of an interpolated string: emit each piece in order.
    PrintInterp { parts: Vec<TypedInterpPart>, to_stderr: bool },
    /// Leave the enclosing loop, or jump to its next test.
    Break,
    Continue,
    Return(TypedExpr),
    /// A guaranteed tail call: the frame is replaced, not stacked. Typeck has
    /// already proven the two signatures match, so codegen can emit `musttail`
    /// knowing LLVM will accept it.
    TailReturn { name: String, arguments: Vec<TypedExpr> },
    While { cond: TypedExpr, body: Vec<TypedStmt> },
    If {
        cond: TypedExpr,
        then_block: Vec<TypedStmt>,
        else_block: Option<Vec<TypedStmt>>,
    },
}

/// A checked statement, and **where it was written**.
///
/// The span is here rather than on each variant because the typed tree is built and
/// then REBUILT: `place_releases` walks a finished body and wraps runs of statements in
/// `Release`, so the typed tree is not structurally 1:1 with the `ast::Stmt` tree it came
/// from. That rules out every cheaper way of recovering a position later — a side table
/// keyed by traversal order, or a parallel walk of the two trees from `main.rs`, both
/// drift the moment a `Release` is inserted, and they drift SILENTLY. The symptom of a
/// drifted line table is a debugger stopping on the wrong line, which is worse than
/// having no debug info at all, so the position travels inside the node.
///
/// Added for C1 (DWARF). Nothing but codegen reads `span`, and codegen reads it only
/// when `-g` was asked for — but it is not optional in the type, because a statement
/// that cannot say where it came from is what this whole change exists to prevent.
#[derive(Debug, Clone)]
pub struct TypedStmt {
    pub kind: TypedStmtKind,
    pub span: Span,
}

impl TypedStmt {
    pub fn new(kind: TypedStmtKind, span: Span) -> Self {
        TypedStmt { kind, span }
    }
}

#[derive(Debug, Clone)]
pub struct TypedArm {
    pub tag: u32,
    /// (name, type) for each payload slot this arm binds.
    pub bindings: Vec<(String, Type)>,
    pub body: Vec<TypedStmt>,
}

/// An enum, ready for codegen: variants in declaration order, which fixes the
/// tag values.
#[derive(Debug, Clone)]
pub struct TypedEnum {
    pub name: String,
    pub variants: Vec<Vec<Type>>,
}

#[derive(Debug, Clone)]
pub enum TypedInterpPart {
    Lit(String),
    Expr(TypedExpr),
}

#[derive(Debug, Clone)]
pub struct TypedFn {
    pub name: String,
    pub parameters: Vec<(String, Type)>,
    /// Which parameters were declared `mutable` — parallel to `parameters`.
    ///
    /// A separate vector rather than a third tuple element, because `parameters` is read in a dozen
    /// places that do not care, and widening it would touch all of them to say nothing.
    ///
    /// Codegen is the consumer: a `mutable` aggregate parameter must NOT get LLVM's `byval`, so the
    /// callee receives a pointer to the CALLER's storage rather than to a copy. That is the same ABI
    /// decision `mutable self` has always made, and the same soundness argument.
    pub writable: Vec<bool>,
    pub ret: Type,
    pub body: Vec<TypedStmt>,
    /// Preconditions, in the order written: checked on entry.
    pub requires: Vec<TypedContract>,
    /// Postconditions: checked before every return, with `result` bound.
    pub ensures: Vec<TypedContract>,
    /// The termination measure, if one was declared, with the clause text for the
    /// message. Checked at every recursive CALL SITE rather than in the callee: the
    /// caller knows both measures, and a guaranteed tail call has no way back in to
    /// restore per-invocation state.
    pub decreases: Option<TypedContract>,
    /// The expressions inside `old(...)`, hoisted out in the order they appear.
    /// Evaluated once on ENTRY and stored; a clause reads the stored value. That is
    /// the whole mechanism behind a conservation law: compare after against before.
    pub olds: Vec<TypedExpr>,
}

/// A checked contract clause: the condition, and the text to quote if it fails.
#[derive(Debug, Clone)]
pub struct TypedContract {
    pub cond: TypedExpr,
    pub text: String,
    /// Where the clause was written. C1: a contract's code runs in the function's
    /// PROLOGUE, before any statement has set a position, so without this the
    /// instructions that check it would carry no location at all — and a call to a
    /// `pure` function from a clause then fails LLVM's verifier outright
    /// ("inlinable function call in a function with debug info must have a !dbg
    /// location"). Found by probing a contract that calls a pure function, which
    /// nothing in the suite did.
    pub span: Span,
}

/// A method, ready for codegen: `self` is always the first bound name, typed
/// as the receiver struct.
#[derive(Debug, Clone)]
pub struct TypedMethod {
    pub receiver: String,
    pub receiver_mut: bool,
    pub name: String,
    pub parameters: Vec<(String, Type)>,
    pub ret: Type,
    pub body: Vec<TypedStmt>,
    pub requires: Vec<TypedContract>,
    pub ensures: Vec<TypedContract>,
    pub olds: Vec<TypedExpr>,
}

/// An extern declaration, ready for codegen: the unmangled symbol name and
/// its full signature (codegen maps each Burxt type to its C ABI type).
#[derive(Debug, Clone)]
pub struct TypedExtern {
    pub name: String,
    pub parameters: Vec<Type>,
    pub ret: Type,
}

/// A struct, ready for codegen: field types in declaration order.
#[derive(Debug, Clone)]
pub struct TypedStruct {
    pub name: String,
    pub fields: Vec<Type>,
}

/// A vtable codegen must emit: one per (concrete type, trait) pair actually
/// used as `dyn`. `slots` lists the implementing methods in TRAIT-DECLARATION
/// order, which is what fixes each method's slot index at compile time.
#[derive(Debug, Clone)]
pub struct TypedVTable {
    pub interface_name: String,
    pub concrete: String,
    pub slots: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct TypedProgram {
    pub structs: Vec<TypedStruct>,
    pub enums: Vec<TypedEnum>,
    pub externs: Vec<TypedExtern>,
    pub fns: Vec<TypedFn>,
    pub methods: Vec<TypedMethod>,
    pub vtables: Vec<TypedVTable>,
    pub stmts: Vec<TypedStmt>,
}

pub struct TypeChecker {
    /// variable name -> (type, is mutable)
    env: HashMap<String, (Type, bool)>,
    /// A11. For a `dynamic` binding, the variable whose storage it borrows.
    ///
    /// An interface object is `{data, vtable}` and `data` IS the source binding's slot — see
    /// `DynCoerce` in `codegen.rs`, which copies nothing. So `let it: dynamic Iterator<Int> = c`
    /// makes `it` a second NAME for `c`, and every rule that reasons about a name has to be able
    /// to get back to the one that owns the bytes: `it` may be declared inside a region while
    /// `c` is declared outside it, and it is `c` that the growth lands in.
    ///
    /// **Why a side table rather than a word in the type or in the object.** The coercion site
    /// knows the answer, so nothing has to be carried at runtime — widening the fat pointer
    /// would change layout, ABI and `layout.bx`'s reported sizes to record a fact the compiler
    /// already has. And it is not part of the TYPE: two `dynamic Iterator<Int>` values are the
    /// same type whether or not their sources were `mutable`, and must stay assignable.
    ///
    /// Kept in step with `env`: saved and restored by `check_block`, cleared for each body. A
    /// `let` always writes or removes its own name (below), so no entry can go stale — and the
    /// entry is only ever consulted for a `Dyn`-typed receiver. Absent means "not known", which
    /// is the honest answer for a `dynamic` PARAMETER: its source is in another frame, and the
    /// call-site rule for a `mutable` parameter is what stands in for it there.
    dyn_source: HashMap<String, String>,
    /// `const` name -> the literal it folded to, with its declared type.
    ///
    /// Separate from `env` and NEVER cleared, which is the whole point: `env` is saved and
    /// restored around every block and replaced outright for every function body, because a
    /// binding's scope is its block. A const has no block — it is in scope in every body in
    /// the program, including a `pure` one, because what it resolves to is a literal.
    ///
    /// The value is a `TypedExprKind` rather than an `i64` so that all four literal types
    /// share one path, and so a use site can hand the already-lowered literal straight to
    /// codegen. That is why `codegen.rs` has no idea `const` exists.
    consts: HashMap<String, (Type, TypedExprKind)>,
    /// function name -> (parameter types, return type); collected up front so
    /// functions may be defined in any order and call each other.
    fns: HashMap<String, (Vec<Type>, Type)>,
    /// Parameter NAMES, so a rejected argument can be named rather than counted.
    ///
    /// `fns` keeps parameter TYPES and drops the names, though `Param::name` has always had them —
    /// nothing had needed them indexed by callee before. Kept beside `fns` rather than inside it
    /// because three of that table's five readers are `contains_key` and never touch the tuple;
    /// widening it would have made them all carry a field they do not use.
    fn_param_names: HashMap<String, Vec<String>>,
    /// The type parameters of every generic function, by name. Empty for all the
    /// others, so the common path is one `is_empty` away. See spec/1.0/M7-GENERICS.md.
    generics: HashMap<String, Vec<TypeParam>>,
    /// Generic ENUM declarations: their parameters and their variants, with the parameters
    /// still standing for nothing. Kept out of `enums` because a generic enum has no
    /// layout until a use says what its arguments are.
    generic_enums: HashMap<String, (Vec<TypeParam>, Vec<(String, Vec<Type>)>)>,
    /// Generic RECORD declarations: their parameters and their fields, parameters still
    /// standing for nothing. Kept out of `structs` for the reason generic enums are kept
    /// out of `enums` — a generic has no layout until a use says what its arguments are.
    generic_records: HashMap<String, (Vec<TypeParam>, Vec<(String, Type)>)>,
    /// Methods whose receiver is a generic record. Held back until an instantiation exists,
    /// then one copy is made per instantiation with the parameters substituted.
    generic_methods: Vec<MethodDef>,
    /// Instantiations of generic classes, in the order they were first needed, so the
    /// methods for each can be made once their record is.
    wanted_records: RefCell<Vec<(String, Vec<Type>)>>,
    made_records: RefCell<HashMap<String, Vec<(String, Type)>>>,
    made_record_order: RefCell<Vec<TypedStruct>>,
    /// The bound on each type parameter of the generic being checked, by parameter name.
    /// Empty except while a generic's own body is being checked — an instantiation has no
    /// parameters left, so it needs none of this.
    param_bounds: HashMap<String, Option<String>>,
    /// Every struct and enum name the program declares, collected before anything is
    /// registered — so an application of a type that exists but is not generic can say
    /// so, instead of calling it unknown.
    declared_type_names: HashSet<String>,
    /// What each function and method declares it `touches`, and what the current body may.
    ///
    /// DECLARED, not inferred — the opposite of the call taken for `allocates`, and deliberately
    /// so. `allocates` carried no promise a reviewer needed, which is why inferring it removed
    /// pure ceremony (v0.0.142). `touches network` IS the promise they need: if the compiler
    /// worked it out, it would not be in the signature, and being in the signature is the entire
    /// point. So this is transitive by DECLARATION, exactly as `pure` already is.
    fn_effects: HashMap<String, Vec<Effect>>,
    method_effects: HashMap<(String, String), Vec<Effect>>,
    /// What the body being checked is allowed to reach. Empty for a `pure` function and for the
    /// top level, which is why both refuse everything.
    allowed_effects: Vec<Effect>,
    /// Whose signature `allowed_effects` came from, for the message.
    effects_owner: String,
    /// Interface names, collected before `expand_program` so a bare `Tax` in a signature can be
    /// rewritten to `dynamic Tax`. Separate from `interfaces`, which holds the signatures and is
    /// not populated until the declaration pass — by which time the rewrite is over.
    interface_names: HashSet<String>,
    /// `interface Mapper<T> { ... }` — its parameters and its signature set, collected in the
    /// same pass as `generic_records` and for the same reason: `expand` needs it, and `expand`
    /// runs before the declaration pass. Roadmap A9.
    ///
    /// A generic interface is deliberately NOT in `interface_names`, so a bare `Mapper` is not
    /// rewritten to `dynamic Mapper`. It has no signature set until a use says what `T` is, and
    /// the message a reader wants for `x: Mapper` is that it takes an argument — not the
    /// "unknown interface" the rewrite would produce two passes later.
    generic_interfaces: HashMap<String, (Vec<TypeParam>, Vec<InterfaceSig>)>,
    /// Instantiations of generic interfaces, made on demand by `expand`: mangled name ->
    /// its substituted signature set. A `RefCell` side table for the same reason
    /// `made_records` is one — `expand` takes `&self` — and it is emptied into `interfaces`
    /// in the declaration pass, after which every reader sees one table and no rule needs
    /// to know which half a signature set came from.
    interfaces_made: RefCell<HashMap<String, Vec<InterfaceSig>>>,
    /// Instantiations of generic enums, made on demand: mangled name -> variants, and
    /// mangled name -> what it was an instantiation OF, so a value's type can be read
    /// back into `(Option, [Int])` when a variant has no payload to infer from.
    made_enums: RefCell<HashMap<String, Vec<(String, Vec<Type>)>>>,
    made_order: RefCell<Vec<TypedEnum>>,
    instance_of: RefCell<HashMap<String, (String, Vec<Type>)>>,
    /// The anonymous class behind a tuple: its symbol -> its element types, in order.
    ///
    /// A SECOND map beside `instance_of` rather than an entry in it, and the reason is that
    /// `instance_of` answers "what generic was this made from", which for a tuple is nothing.
    /// Folding tuples in would have meant giving each one a fake owner name, and `show` reads
    /// that same map to spell a type back — so the price of one saved field would have been a
    /// wrong name in every message that printed a tuple. `unify` is the only reader: it needs
    /// a route from `Named("(String, Int)")` back to its elements, or `(T, Int)` cannot bind
    /// `T` and the two compilers disagree about a program.
    tuple_of: RefCell<HashMap<String, Vec<Type>>>,
    /// Every `(generic, type arguments)` pair reached, in discovery order, and the set
    /// already recorded so a pair is emitted once. Checking an instantiation can add
    /// more — a generic calling a generic — so this is drained to a fixpoint.
    wanted: RefCell<Vec<(String, Vec<Type>)>>,
    seen_instantiations: RefCell<HashSet<String>>,
    /// (receiver, method) pairs declared `allocates`, so a call site can be checked
    /// for an open region in the same one pass the free-function form uses.
    alloc_methods: HashSet<(String, String)>,
    /// which of those names are `extern fn` declarations. They share `fns` so
    /// call checking is uniform, but they are NOT Burxt functions — a tail-call
    /// guarantee, for one, stops at the C boundary.
    extern_names: HashSet<String>,
    /// extern name -> each parameter's DECLARED C-side shape (type, marshaller).
    /// `fns` holds what Burxt code must pass; this holds what C receives, which
    /// is what boundary-exactness errors have to talk about.
    extern_parameters: HashMap<String, Vec<(Type, Option<Marshal>)>>,
    /// Which of each function's parameters were declared `mutable`, by function name.
    ///
    /// Read at the CALL site, because that is where the caller's obligation is: a `mutable`
    /// parameter changes the caller's value, so the caller has to be holding one that may change.
    fn_writable: HashMap<String, Vec<bool>>,
    /// The same, per method. `methods` records only `receiver_mut`, because until A12
    /// nothing outside `check_method` needed to know which of a method's own parameters
    /// were `mutable`. Per-block release does: a `mutable` parameter is the ONLY way a
    /// callee can write into storage its caller owns, so it is exactly the question
    /// "can this call put something in a place that outlives this block?".
    method_writable: HashMap<(String, String), Vec<bool>>,
    /// The names bound as PARAMETERS of the function being checked.
    ///
    /// Only used to give followable advice. `cannot modify x` used to suggest `let mutable x`, which
    /// for a parameter is impossible — there is no `let` to change. Since v0.0.201 there is a real
    /// answer (`mutable x: T` in the signature), and a message that names the wrong one is worse than
    /// a short one, because a reader trusts it and loses time.
    current_params: std::collections::HashSet<String>,
    /// struct name -> fields (name, type) in declaration order; hoisted first.
    structs: HashMap<String, Vec<(String, Type)>>,
    /// enum name -> variants (name, payload types) in declaration order, which
    /// is what fixes each variant's tag.
    enums: HashMap<String, Vec<(String, Vec<Type>)>>,
    /// (receiver, method name) -> (is mutating, param types, return type)
    methods: HashMap<(String, String), (bool, Vec<Type>, Type)>,
    /// interface name -> its method signatures, in declaration order (slot order)
    interfaces: HashMap<String, Vec<InterfaceSig>>,
    /// which (trait, concrete type) pairs have an explicit impl
    impls: HashSet<(String, String)>,
    /// (trait, concrete) pairs that need a vtable because the interface is used
    /// as `dyn` somewhere — pay for what you use.
    dyn_interfaces: HashSet<String>,
    /// return type of the function currently being checked, if any.
    /// Where the checker currently is, for attaching a position to any error it
    /// returns. Updated on entering a statement or a top-level item, and refined
    /// to the exact sub-expression by `check_expr`.
    ///
    /// A `Cell` because expression checking is `&self` — the position is
    /// bookkeeping for diagnostics, not part of the checking itself, and threading
    /// `&mut` through every checker method to carry it would say otherwise.
    current_span: Cell<Span>,
    /// C2. A declaration a DEPENDENCY did not make `public`: name -> the package it is in.
    ///
    /// Consulted only when a name fails to resolve or is about to be used, so it costs nothing in
    /// the ordinary case and turns "unknown function: `helper`" — true and useless when `helper` is
    /// sitting in the dependency the reader is looking at — into a sentence that says what to do.
    package_private: std::collections::BTreeMap<String, String>,
    /// Which byte ranges belong to which dependency. Only FOREIGN ranges are listed: an offset
    /// matching none of them is the root package, which is the common case.
    ///
    /// Both tables are needed because privacy is a RELATION and not a property. A helper a package
    /// keeps to itself is perfectly visible to the rest of that package, so the question is never
    /// "is this private" but "is this private FROM HERE".
    package_ranges: Vec<(usize, usize, String)>,
    /// Set once an error has claimed a position, so the INNERMOST failing
    /// expression keeps it as the error propagates outward.
    error_located: Cell<bool>,
    /// Every expression's span and resolved type, in check order. This is what
    /// answers "what is the type here?" — hover, in the language server.
    expr_types: RefCell<Vec<(Span, Type)>>,
    /// Errors found so far. Checking a statement that fails does not stop the
    /// checker: it classes the problem and moves to the next statement, so one
    /// mistake does not hide the other five.
    errors: Vec<Diagnostic>,
    current_ret: Option<Type>,
    /// The enclosing function's name and parameter types. A guaranteed tail
    /// call needs them: LLVM only guarantees the call when caller and callee
    /// prototypes match, so that has to be checked before promising it.
    current_signature: Option<(String, Vec<Type>)>,
    /// Function names declared `allocates`: they build values in their CALLER's
    /// region, so they may allocate without opening one and may return what they
    /// built. Hoisted with the signatures, so call sites can be checked in one
    /// pass.
    alloc_fns: HashSet<String>,
    /// Function names declared `pure`: their result depends only on their
    /// arguments, checked. Hoisted with the signatures so a call to one can be
    /// judged in a single pass, in either direction.
    pure_fns: HashSet<String>,
    /// Methods declared `pure`, keyed the way every other method table here is keyed. Beside
    /// `pure_fns` rather than folded into it, because `(receiver, name)` is what a method call
    /// resolves to and a flat name would collide the moment two classes both have `sum`.
    pure_methods: HashSet<(String, String)>,
    /// How many loops enclose the statement being checked. `break` and `continue`
    /// outside a loop have nothing to act on, and saying so beats generating a jump
    /// to nowhere.
    loop_depth: u32,
    /// The names bound by an enclosing `for i in a..b`, innermost last. Only for the
    /// MESSAGE: a loop counter is immutable like a `for` element, but the generic advice
    /// ("declare it `let mutable i: Int`") names a `let` that does not exist and cannot
    /// be written — the same defect `how_to_make_writable` was factored out to fix for
    /// parameters. A Vec of names rather than a third state in `env`, because `env` is
    /// `(Type, bool)` and read in dozens of places: one more state there would be a
    /// refactor across the file to improve one sentence.
    loop_counters: Vec<String>,
    /// True while checking an `ensures` clause specifically: only there does
    /// `old(...)` mean anything, and only there is `result` in scope.
    in_ensures: bool,
    /// The `old(...)` expressions collected for the function being checked.
    olds: RefCell<Vec<TypedExpr>>,
    /// True while checking a contract clause. Clauses are checked under the `pure`
    /// rule, but they are not `pure fn` bodies — telling a reader to "drop `pure`
    /// from `f`" when `f` never declared it would be nonsense.
    in_contract: bool,
    /// The `pure` function being checked, if any. Held by NAME because every
    /// refusal below names both functions — the reader needs to know which promise
    /// is being broken as well as what broke it.
    in_pure: Option<String>,
    /// True while checking the body of an `allocates` function: the caller's
    /// region is the region in effect, even though none is open here.
    in_caller_region: bool,
    /// the region currently open, if any. One level only in this slice, so
    /// this doubles as the nesting guard.
    current_region: Option<String>,

    /// Bindings holding storage from a region THIS function opened.
    ///
    /// Found while testing M14, and it is a use-after-free that produced a silently wrong
    /// answer — the failure class this language exists to refuse:
    ///
    /// ```text
    /// function leaked(tag: Int) -> String {
    ///     region inner {
    ///         let s: String = "secret-" + to_string(tag);
    ///         return s;                    // accepted, and printed an EMPTY string
    ///     }
    /// }
    /// ```
    ///
    /// The return rule asks `expr_allocates`, which answers for a concatenation but not for
    /// a NAME bound to one — a variable read fell through to `false`. So returning the
    /// expression was refused and returning it via a binding was not, and codegen releases
    /// the region before the `ret`, so the pointer handed back was into freed bytes.
    ///
    /// The type cannot carry this: a literal String lives in `.rodata` and a concatenated
    /// one lives in a region, and both are `String`. So it is recorded per binding, at the
    /// `let` — which is the one place the checker already knows, because it computes
    /// exactly this to decide whether the `let` needed a region at all.
    ///
    /// A PARAMETER is deliberately never in here. A String parameter may well be region
    /// storage, but it is the CALLER's, and the caller's region outlives the call — so
    /// returning a parameter is safe and must keep working.
    region_locals: HashSet<String>,
    /// Names DECLARED inside the currently open `region`, whether or not they hold region
    /// storage. `region_locals` cannot answer this: it holds only the names that were
    /// *found to allocate*, so a binding declared inside a region holding a literal is
    /// absent from it — and telling those two apart is exactly what the assignment rule
    /// needs. Assigning region storage to a name declared INSIDE the region is fine, it
    /// dies with the region; assigning it to one declared OUTSIDE is a use-after-free.
    region_scope: HashSet<String>,

    /// Per class, the field names declared `private`, and per (class, method) the private
    /// methods. The class is the SCOPE: a private member is reachable only from that class's
    /// own methods.
    ///
    /// This is the first visibility Burxt has ever had. Before it, a file could reach into a
    /// transitively-imported file's helpers and read any type's fields directly, bypassing
    /// whatever method it provided. FILE-level privacy is deliberately not attempted: `use` is
    /// a text pre-pass that concatenates files, so by the time anything is checked there are no
    /// files, only one long program. A class needs no such knowledge — it is its own boundary.
    private_fields: HashMap<String, Vec<String>>,
    private_methods: HashSet<(String, String)>,
    /// The class whose method is being checked, if any. What makes `private` mean anything.
    current_receiver: Option<String>,

    // ---- M14 slice 1: working out `allocates` instead of asking for it ------
    //
    // The compiler has always computed this. `expr_allocates` walks a body and answers
    // whether it allocates, and the declared word was then checked against that answer —
    // so the programmer was being asked to write down a fact the checker derived. In
    // `examples/pos/receipt.bx` it was on 3 functions out of 3, which is an annotation
    // carrying no information at all.
    //
    // It was REQUIRED for an ordering reason rather than a semantic one: a call site has
    // to know whether its callee allocates, and a callee may be declared 200 lines later,
    // so the answer had to be available before any body was read. Inference needs a
    // fixpoint over the call graph instead of one pass — `a` allocates because it calls
    // `b`, which allocates because it calls `c`.
    /// True in a THROWAWAY checker whose only job is to answer "which functions
    /// allocate?". While set, `has_region` never refuses — it classes what wanted a
    /// region and answers yes, so the pass reaches the end of every body instead of
    /// stopping at the first allocation.
    probing: bool,
    /// True while Pass 1 is collecting signatures, false once bodies are being checked.
    /// Read off a finished PROBE to ask whether it died before it could learn anything.
    declaring: bool,
    /// A probe abandoned Pass 1, so `alloc_fns` is empty for reasons that have nothing to do
    /// with what allocates. Every rule that consults the inference must stand down.
    probe_truncated: bool,
    /// Who is being probed: `(receiver, name)`, receiver empty for a free function.
    probe_owner: RefCell<(String, String)>,
    /// What the probe found. `RefCell` because `has_region` is a query — it answers a
    /// question about the checker and must not need `&mut` to do it.
    probe_fns: RefCell<HashSet<String>>,
    probe_methods: RefCell<HashSet<(String, String)>>,

    // ---- B25: does a call GROW something the caller owns? ---------------------
    //
    // B20 refuses `region r { push(xs, 11); }` for an `xs` declared outside. One call away it
    // was still a silent use-after-free, because the growth happens in the callee's body where
    // no region is open:
    //
    // ```text
    // function grow(mutable dst: [Int], v: Int) -> Int { push(dst, v); return v; }
    // let mutable xs: [Int] = [];
    // region r { let a: Int = grow(xs, 11); ... }        // accepted; xs[0] printed 777
    // ```
    //
    // `alloc_fns` cannot answer it. "Does this function allocate?" is the wrong question — a
    // function that builds a String to print allocates and touches nobody's array, and refusing
    // on that answer falsely rejects correct code. Measured, rather than reasoned about: with
    // the rule keyed on `alloc_fns` this program is refused, and it is sound —
    //
    // ```text
    // function note(mutable seen: [Int], i: Int) -> Int { print("at " + to_string(i));
    //                                                     seen[0] = i; return 0; }
    // ```
    //
    // — so the question has to be asked PER PARAMETER: does region storage land in *this* one?
    /// Free functions that put region storage into a `mutable` parameter, by index. Per index
    /// rather than per function, so `f(mutable a, mutable b)` that grows only `a` still accepts
    /// an outer `b`.
    grow_params: HashSet<(String, usize)>,
    /// Methods that put region storage into their `mutable self`. Separate from `grow_params`
    /// because a method may not declare a `mutable` parameter at all — only `mutable self` —
    /// so the receiver is the whole of the question for a method, and it has no index.
    grow_self: HashSet<(String, String)>,
    probe_grow_params: RefCell<HashSet<(String, usize)>>,
    probe_grow_self: RefCell<HashSet<(String, String)>>,
    /// This body's `mutable` parameters, name → position. What lets a growth found deep in a
    /// body be attributed to the parameter it lands in.
    current_writable_params: HashMap<String, usize>,
    /// True while checking a method declared `mutable self`.
    current_self_writable: bool,

    // ---- B32: does a call HAND BACK storage it was given? ---------------------
    //
    // B20, B21, B25, B26, B27 and B35 were each one construct missing from an enumeration, and
    // each was closed by adding an arm to `expr_allocates`. This one says the enumeration is the
    // wrong shape:
    //
    // ```text
    // function pass(s: String) -> String { return s; }
    // let mutable kept: String = "";
    // region r { let built: String = "secret-" + "value"; kept = pass(built); }
    // print(kept);                                  // secret-value, then 0 once reused
    // ```
    //
    // `pass` does not ALLOCATE — it returns bytes somebody else built — so `expr_allocates`
    // answers false and every escape rule goes quiet. The question the rules need is not "was
    // this built here?" but "does this point into the open region?", which is ALIASING. A
    // seventh arm cannot express it, because the aliasing happens in a body somewhere else.
    //
    // So it is the same shape as the three properties above: a fact about a callee, worked out
    // over the call graph before any body is checked, and read at the call site.
    /// Free functions whose result may point at whatever argument `i` points at. Per index for
    /// the same reason `grow_params` is: `pick(a, b, first)` that can hand back either one must
    /// taint on either, and `wrap(fresh, s)` that only ever returns `fresh` must taint on
    /// neither.
    relay_params: HashSet<(String, usize)>,
    /// The same fact for methods, `(receiver, method, source)` — where source `0` is the
    /// RECEIVER and `i + 1` is argument `i`. One set rather than `grow_self`'s two, because a
    /// method parameter can be relayed even though it can never be `mutable`: `self.name` and
    /// a handed-in String reach the caller by the same `return`.
    relay_methods: HashSet<(String, String, usize)>,
    probe_relay_params: RefCell<HashSet<(String, usize)>>,
    probe_relay_methods: RefCell<HashSet<(String, String, usize)>>,
    /// This body's parameters, name → position — ALL of them, where `current_writable_params`
    /// holds only the `mutable` ones. Relaying needs every parameter: `pass(s)` hands back an
    /// immutable one.
    current_param_positions: HashMap<String, usize>,
    /// A `match` arm's payload name → what the SCRUTINEE could still be pointing at.
    ///
    /// **Without this, a relay through a pattern binding is invisible, and the consequence is a
    /// use-after-free that answers rather than crashes.** `collect_relayed_sources` walks `Field`,
    /// `Index`, `Try`, `StructLit` and `VariantLit` correctly, but its `Var` arm resolves only
    /// `current_param_positions` — so a name introduced by a pattern is not a parameter, the walk
    /// stops, and the function is never recorded as relaying anything:
    ///
    ///     pure function json_as_text(field: Json) -> Option<String> {
    ///         match field { Text(s) => { return Option.Some(s); } … }
    ///     }
    ///
    /// `json_as_text` relays its parameter and nothing knew. So `ReleasePass::allocates` answered
    /// NO for a call to it, the caller's binding was not marked, `store` did not taint the frame,
    /// the frame was judged to keep nothing, and the `Release` freed the tree the returned String
    /// still pointed into. Reading it twice gave two different answers.
    ///
    /// Scoped to the arm, and cleared the same way `region_locals` is, for the same reason its
    /// comment gives: a second arm may bind the same name to an `Int` payload, which relays
    /// nothing and must not inherit this arm's sources.
    relay_aliases: HashMap<String, Vec<RelaySource>>,
}

/// What one run of `infer_allocates` worked out about the call graph.
///
/// Named fields rather than a tuple only because there are six of them now. They are one
/// value because they are found in one set of rounds and are mutually dependent — see
/// `infer_allocates`.
struct CallGraphFacts {
    fns: HashSet<String>,
    methods: HashSet<(String, String)>,
    grow_params: HashSet<(String, usize)>,
    grow_self: HashSet<(String, String)>,
    relay_params: HashSet<(String, usize)>,
    relay_methods: HashSet<(String, String, usize)>,
    /// The inference is unusable: a Pass 1 refusal killed the probe before any body was read.
    truncated: bool,
}

/// Where a returned value's storage came from, when it came from the caller.
///
/// Two cases rather than one index, because a method's receiver is not in its parameter list —
/// the same split `grow_self` makes, and the reason `relay_methods` numbers `self` as `0`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RelaySource {
    Receiver,
    Parameter(usize),
}

/// The names a program may not declare.
///
/// The builtins, and three the RUNTIME owns. Stage-1 has refused all three since it was written;
/// stage-0 refused only the builtins, so `function main()` compiled with one compiler and not the
/// other until v0.0.124. No fixture declared any of them, so the differential test never saw it —
/// and the first thing a newcomer arriving from Rust or C types is `function main`.
///
/// Why each of the three, because the reasons are not the same:
///
/// - **`main`** — a Burxt program IS its top-level statements, and the compiler emits `@main` from
///   them. A function called `main` therefore looks like an entry point and is not one. Stage-0
///   emitted it safely as `bx.main`, so nothing collided; it was a trap rather than a crash, which
///   is exactly the kind of thing this language refuses.
/// - **`exit`** — the runtime calls libc's `exit` to end a program on a failed contract or a bounds
///   violation. A program that shadowed it would change what a panic does.
/// - **`result`** — reserved for `ensures` clauses, where it names the value being returned.
fn is_reserved_name(name: &str) -> bool {
    matches!(
        name,
        "len" | "byte_at" | "push" | "read_file" | "to_string" | "old" | "substring" | "truncate"
            | "write_file" | "argument" | "argument_count" | "divide_floor"
            | "divide_toward_zero" | "remainder" | "write_bytes" | "hash"
            | "main" | "exit" | "result"
            // Ten that were implemented and never reserved, so a program could declare a function
            // with the same name and collide unpredictably — found by the 1.0 scan (roadmap B6).
            // `docs/reference/builtins.md` claims to be generated from THIS list, so it was missing
            // them too: one omission showing up twice, which is what a single source of truth is for.
            | "bit_and" | "bit_or" | "bit_xor" | "bit_not"
            | "shift_left" | "shift_right_zeros" | "shift_right_sign"
            | "c_is_null" | "c_string_at" | "c_bytes_at" | "c_bytes_to"
            // The exact inverse of `byte_at`, and the only builtin that turns a number into
            // bytes (roadmap A13). Reserved from the first version it existed, unlike the ten
            // above — being in this list is what the editor grammar and the generated reference
            // are scraped from, so a builtin that is not here is a builtin no tool knows.
            | "byte_as_string"
            // M17. Reserved from the version they arrived in, so the editor grammar and the
            // generated reference know about them on day one — the ten above were not, and a
            // builtin absent from this list is a builtin no tool has heard of.
            | "handle_of" | "handle_value"
    )
}

/// The value of a WRITTEN-DOWN integer, or None for anything computed.
///
/// `Neg` is unwrapped because the lexer reads no sign: `-1` is a unary minus WRAPPING the literal
/// `1`, never a literal holding -1. A check that only looked at `IntLit` would therefore see
/// nothing at all for every negative argument — which is exactly the bug the Burxt-side
/// `c_bytes_at` rule had for as long as it existed, found by a fixture rather than by reading.
fn written_int(e: &TypedExpr) -> Option<i64> {
    match &e.kind {
        TypedExprKind::IntLit(n) => Some(*n),
        TypedExprKind::Neg(inner) => match &inner.kind {
            TypedExprKind::IntLit(n) => Some(-*n),
            _ => None,
        },
        _ => None,
    }
}

impl TypeChecker {
    pub fn new() -> Self {
        TypeChecker {
            env: HashMap::new(),
            dyn_source: HashMap::new(),
            consts: HashMap::new(),
            fns: HashMap::new(),
            fn_param_names: HashMap::new(),
            generics: HashMap::new(),
            generic_enums: HashMap::new(),
            generic_records: HashMap::new(),
            generic_methods: Vec::new(),
            wanted_records: RefCell::new(Vec::new()),
            made_records: RefCell::new(HashMap::new()),
            made_record_order: RefCell::new(Vec::new()),
            param_bounds: HashMap::new(),
            declared_type_names: HashSet::new(),
            interface_names: HashSet::new(),
            generic_interfaces: HashMap::new(),
            interfaces_made: RefCell::new(HashMap::new()),
            fn_effects: HashMap::new(),
            method_effects: HashMap::new(),
            allowed_effects: Vec::new(),
            effects_owner: String::new(),
            made_enums: RefCell::new(HashMap::new()),
            made_order: RefCell::new(Vec::new()),
            instance_of: RefCell::new(HashMap::new()),
            tuple_of: RefCell::new(HashMap::new()),
            wanted: RefCell::new(Vec::new()),
            seen_instantiations: RefCell::new(HashSet::new()),
            alloc_fns: HashSet::new(),
            alloc_methods: HashSet::new(),
            pure_fns: HashSet::new(),
            pure_methods: HashSet::new(),
            in_pure: None,
            in_contract: false,
            loop_depth: 0,
            loop_counters: Vec::new(),
            in_ensures: false,
            olds: RefCell::new(Vec::new()),
            in_caller_region: false,
            extern_names: HashSet::new(),
            extern_parameters: HashMap::new(),
            fn_writable: HashMap::new(),
            method_writable: HashMap::new(),
            current_params: std::collections::HashSet::new(),
            structs: HashMap::new(),
            enums: HashMap::new(),
            methods: HashMap::new(),
            interfaces: HashMap::new(),
            impls: HashSet::new(),
            dyn_interfaces: HashSet::new(),
            current_span: Cell::new(Span::default()),
            package_private: std::collections::BTreeMap::new(),
            package_ranges: Vec::new(),
            error_located: Cell::new(false),
            expr_types: RefCell::new(Vec::new()),
            errors: Vec::new(),
            current_ret: None,
            current_signature: None,
            current_region: None,
            region_locals: HashSet::new(),
            region_scope: HashSet::new(),
            private_fields: HashMap::new(),
            private_methods: HashSet::new(),
            current_receiver: None,
            probing: false,
            declaring: false,
            probe_truncated: false,
            probe_owner: RefCell::new((String::new(), String::new())),
            grow_params: HashSet::new(),
            grow_self: HashSet::new(),
            probe_grow_params: RefCell::new(HashSet::new()),
            probe_grow_self: RefCell::new(HashSet::new()),
            current_writable_params: HashMap::new(),
            relay_params: HashSet::new(),
            relay_methods: HashSet::new(),
            probe_relay_params: RefCell::new(HashSet::new()),
            probe_relay_methods: RefCell::new(HashSet::new()),
            current_param_positions: HashMap::new(),
            relay_aliases: HashMap::new(),
            current_self_writable: false,
            probe_fns: RefCell::new(HashSet::new()),
            probe_methods: RefCell::new(HashSet::new()),
        }
    }

    /// Check a program, reporting WHERE any problem is.
    ///
    /// The position is attached here, once, from wherever the checker had reached
    /// — so every one of the ~160 error sites inside stays a plain sentence, and
    /// a nested statement naturally yields the most precise position because it
    /// was the last thing entered.
    /// Tell the checker which declarations belong to which package. C2.
    pub fn with_packages(
        &mut self,
        private: std::collections::BTreeMap<String, String>,
        ranges: Vec<(usize, usize, String)>,
    ) {
        self.package_private = private;
        self.package_ranges = ranges;
    }

    /// Which package the code being checked right now is in — `None` for the root package.
    fn package_here(&self) -> Option<&str> {
        let at = self.current_span.get().start as usize;
        self.package_ranges
            .iter()
            .find(|(from, to, _)| at >= *from && at <= *to)
            .map(|(_, _, name)| name.as_str())
    }

    /// If `name` is a dependency's private declaration and we are not inside that dependency, the
    /// sentence to refuse with. C2.
    fn refuse_if_package_private(&self, name: &str) -> Option<String> {
        let owner = self.package_private.get(name)?;
        if self.package_here() == Some(owner.as_str()) {
            return None;
        }
        Some(format!(
            "`{}` is declared in the package `{}` but not `public`, so this package cannot reach \
             it. A package exposes what it means to support — if `{}` is meant to be part of that, \
             the fix belongs in `{}`, by writing `public` in front of its declaration.",
            name, owner, name, owner
        ))
    }

    pub fn check(&mut self, prog: &Program) -> Result<TypedProgram, Vec<Diagnostic>> {
        // M14: work out which functions allocate before checking anything, so `allocates`
        // need not be written. See `probing` on the struct for why this needs a fixpoint
        // and not a pass.
        let found = Self::infer_allocates(prog);
        self.alloc_fns.extend(found.fns);
        self.alloc_methods.extend(found.methods);
        self.grow_params.extend(found.grow_params);
        self.grow_self.extend(found.grow_self);
        self.relay_params.extend(found.relay_params);
        self.relay_methods.extend(found.relay_methods);
        self.probe_truncated = found.truncated;

        let result = self.check_program_inner(prog);
        if let Err(message) = result {
            // A declaration-level failure stops the pass it was in, so it arrives
            // here rather than through `record`.
            self.record(message);
            return Err(self.take_errors());
        }
        if self.errors.is_empty() {
            return result.map_err(|_| unreachable!());
        }
        Err(self.take_errors())
    }

    /// The errors found, in the order a reader meets them, each one only once.
    fn take_errors(&mut self) -> Vec<Diagnostic> {
        let mut out = std::mem::take(&mut self.errors);
        out.sort_by_key(|d| (d.span.start, d.span.end));
        out
    }

    /// Check a run of contract clauses. `result_ty` is `Some` for `ensures`, which
    /// binds `result` to the returned value; `None` for `requires`, where there is
    /// no result yet — and saying that plainly beats "unknown name `result`".
    fn check_contracts(
        &mut self,
        clauses: &[Contract],
        result_ty: Option<&Type>,
    ) -> Result<Vec<TypedContract>, String> {
        let mut out = Vec::new();
        self.in_ensures = result_ty.is_some();
        for clause in clauses {
            self.current_span.set(clause.span);
            if let Some(ty) = result_ty {
                if self.env.contains_key("result") {
                    return Err(
                        "`ensures` binds the name `result` to the returned value, and \
                         something here is already called `result`. Rename it — Burxt \
                         does not shadow."
                            .to_string(),
                    );
                }
                self.env.insert("result".to_string(), (ty.clone(), false));
            }
            let checked = self.check_expr(&clause.cond, Some(&Type::Bool));
            if result_ty.is_some() {
                self.env.remove("result");
            }
            let cond = match checked {
                Ok(c) => c,
                Err(message)
                    if result_ty.is_none() && message.contains("result") =>
                {
                    return Err(
                        "`result` has no meaning in a `requires` clause: it is checked \
                         on entry, before there is a result. Use `ensures` for a claim \
                         about the return value."
                            .to_string(),
                    )
                }
                Err(message) => return Err(message),
            };
            if cond.ty != Type::Bool {
                return Err(format!(
                    "a contract clause must be a Bool, but `{}` has type {}",
                    clause.text, cond.ty
                ));
            }
            out.push(TypedContract { cond, text: clause.text.clone(), span: clause.span });
        }
        self.in_ensures = false;
        Ok(out)
    }

    /// Refuse something a `pure` function may not do, naming both the promise and
    /// what would break it. `None` when we are not in a pure function.
    fn impure(&self, what: &str) -> Option<String> {
        self.in_pure.as_ref().map(|name| {
            if self.in_contract {
                format!(
                    "a contract clause on `{}` may not {}: a clause that can change \
                     the program is not a check, it is a second program that runs \
                     only when someone is looking.",
                    name, what
                )
            } else {
                format!(
                    "`pure function {}` may not {}: a pure function's result must depend \
                     only on its arguments, which is the whole of what `pure` \
                     promises. Pass the value in as a parameter instead.",
                    Self::shown_fn_name(name), what
                )
            }
        })
    }

    /// Is there a region to allocate in at this point?
    ///
    /// Either one is lexically open, or we are inside an `allocates` function and
    /// the caller's region is in effect. The two are the same question everywhere
    /// allocation is checked, so they are answered in one place.
    fn has_region(&self) -> bool {
        // Probing: nothing is refused, and anything that wanted a region while none was
        // lexically open is exactly the definition of "this function allocates in its
        // caller's region". One choke point answers the question for all eight sites that
        // ask it, which is why the inference is a dozen lines rather than a sweep.
        if self.probing {
            if self.current_region.is_none() {
                let (receiver, name) = self.probe_owner.borrow().clone();
                if name.is_empty() {
                    // The top level. It has no signature to carry the answer, and it is
                    // where the program's own region lives — nothing to record.
                } else if receiver.is_empty() {
                    self.probe_fns.borrow_mut().insert(name);
                } else {
                    self.probe_methods.borrow_mut().insert((receiver, name));
                }
            }
            return true;
        }
        // M14 slice 2: there is ALWAYS somewhere to build.
        //
        // This used to be `current_region.is_some() || in_caller_region`, and the whole
        // "there is no region open here" family of refusals hung off it. Codegen settles the
        // question: `burxt.alloc` is a **global bump pointer** and has no region state at all,
        // so nothing ever needed a region in order to allocate. A `region` block is purely a
        // RELEASE mechanism — a mark, and one store to put the cursor back.
        //
        // So the requirement was never protecting memory. It was asking the programmer to
        // name a scope so the compiler could decide where to release, and the answer to
        // "where does this live?" is now the enclosing block, ultimately the program.
        //
        // Sound by construction, and this is the part worth being careful about: **nothing
        // new is released.** Only a `region` block releases, and its rules are untouched —
        // `region_locals` still refuses letting a value built inside one escape it. The cost
        // is memory held for the program's lifetime unless you opt into a region, which is
        // the bias §2 Decision 2 chose deliberately: a wrong guess must cost memory, never
        // correctness.
        //
        // Releasing per block — the constant-memory win for `ring_up`'s loop — is a separate
        // slice, because it is the half that CAN dangle if the escape analysis is wrong.
        true
    }

    /// Which functions and methods allocate — worked out rather than declared.
    ///
    /// **Still load-bearing after slice 2, for a different reason than it was built for.**
    /// Worth reading before deleting anything that looks dead here.
    ///
    /// It was built to answer "does this need a region?", and slice 2 deleted that question —
    /// `has_region` now returns true unconditionally, so every `if !self.has_region()` guard
    /// below is unreachable and looks like debris. Removing them would break the escape rule
    /// SILENTLY, in two ways:
    ///
    ///   * `expr_allocates` asks `alloc_fns` whether a CALL produces region storage. Without
    ///     it, `function bad() -> String { region r { return build(); } }` would be accepted
    ///     and hand back freed bytes.
    ///   * the probe classes through `has_region`, so the guards are where the answer comes
    ///     from. Delete the callers and the set empties.
    ///
    /// So the question the machinery answers changed — from "where may this be built?" to
    /// "does this expression produce region storage?" — and only the second one was ever
    /// about safety.
    ///
    /// A THROWAWAY checker per round, never this one. Sharing would be a real bug and not
    /// merely untidy: checking a body creates generic instantiations and classes them in
    /// `seen_instantiations` so each is emitted once, so a probe pass on the live checker
    /// would mark them seen and the real pass would emit none of them.
    ///
    /// The fixpoint is least-to-greatest and therefore correct rather than merely
    /// terminating: each round starts from what the last one found, `expr_allocates` is
    /// monotone in that set, and a set that only grows over a finite number of names has
    /// to stop growing. `a` allocates because it calls `b`, which allocates because it
    /// calls `c` — so one round per link in the longest chain, which is why this iterates
    /// instead of asking once.
    ///
    /// Errors are DISCARDED here, deliberately. A body that does not typecheck contributes
    /// nothing, and the real pass reports the problem with its own message — so the
    /// diagnostics a user sees are exactly the ones they saw before M14, in the same order.
    /// A probe that reported anything would be a second source of truth for error text.
    ///
    /// It answers FOUR questions in one set of rounds — which functions allocate (M14), which
    /// put that storage into a `mutable` parameter (B25), and which hand a parameter's storage
    /// back as their result (B32, the two `relay` sets). They share the rounds because they
    /// share the walk and are mutually monotone: each one reads `expr_allocates`, which reads
    /// all of them, so running them separately would mean passes converging on each other.
    /// A relay is only found once its callee's relay is known — `pass2` returns `pass(s)` —
    /// which is the same one-round-per-link argument the paragraph above makes for `allocates`.
    fn infer_allocates(prog: &Program) -> CallGraphFacts {
        let mut fns: HashSet<String> = HashSet::new();
        let mut methods: HashSet<(String, String)> = HashSet::new();
        let mut grow_params: HashSet<(String, usize)> = HashSet::new();
        let mut grow_self: HashSet<(String, String)> = HashSet::new();
        let mut relay_params: HashSet<(String, usize)> = HashSet::new();
        let mut relay_methods: HashSet<(String, String, usize)> = HashSet::new();
        // One round per link in the longest call chain. The bound is the number of
        // functions, which no chain can exceed without repeating a name, and it is a
        // backstop rather than an expectation — real programs settle in two or three.
        let ceiling = prog.fns.len() + prog.methods.len() + 1;
        let mut truncated = false;
        for _ in 0..ceiling {
            let mut probe = TypeChecker::new();
            probe.probing = true;
            probe.alloc_fns = fns.clone();
            probe.alloc_methods = methods.clone();
            probe.grow_params = grow_params.clone();
            probe.grow_self = grow_self.clone();
            probe.relay_params = relay_params.clone();
            probe.relay_methods = relay_methods.clone();
            // **THE ONE PLACE THIS IS DETECTED, and it is deliberately not a list of checks.**
            //
            // `check_program_inner` has thirty-one refusal sites. Any of them firing abandons this
            // probe, so everything it had not reached yet is uninferred — and because this error is
            // discarded, silently. Guarding the refusals would encode the rule once per site and
            // leave the rest for the next person to trip; asking the finished probe whether it died
            // catches all of them, including the ones nobody has written yet.
            //
            // **`&& probe.declaring` used to be here and it made definition ORDER decide which of
            // two errors a reader sees.** A death during Pass 1 leaves nothing inferred, which the
            // gate caught. A death while checking a BODY leaves everything after that body
            // uninferred, which it did not — so:
            //
            //     pure function fill(items: [Node], from: Int, mutable out: [Node]) -> Int
            //     function without_first(items: [Node]) -> [Node]           // defined AFTER
            //
            // reported `without_first cannot return [Node], because its storage lives in a region`
            // — a function with no defect, never called by `fill`, and fine the moment `pure` is
            // dropped from `fill`. Swap the two definitions and the same file reports the real
            // error. The probe died at `fill`'s body with `declaring` already false, so
            // `without_first` was never credited as allocating and RULE 2 refused it.
            //
            // Two errors where the WRONG one wins is worse than one error, because it sends the
            // reader to a file with nothing wrong in it — 800 lines away, in the case that found
            // this. A probe that died knows less than a probe that finished, whenever it died.
            //
            // Standing down more often is the safe direction: every rule that consumes this
            // inference is a REFUSAL, so a stale `truncated` accepts rather than rejects — and the
            // program that killed the probe has a real error of its own to report.
            if probe.check_program_inner(prog).is_err() {
                truncated = true;
            }
            let found_fns = probe.probe_fns.borrow().clone();
            let found_methods = probe.probe_methods.borrow().clone();
            let found_params = probe.probe_grow_params.borrow().clone();
            let found_self = probe.probe_grow_self.borrow().clone();
            let found_relay_params = probe.probe_relay_params.borrow().clone();
            let found_relay_methods = probe.probe_relay_methods.borrow().clone();
            let grew = !found_fns.is_subset(&fns)
                || !found_methods.is_subset(&methods)
                || !found_params.is_subset(&grow_params)
                || !found_self.is_subset(&grow_self)
                || !found_relay_params.is_subset(&relay_params)
                || !found_relay_methods.is_subset(&relay_methods);
            fns.extend(found_fns);
            methods.extend(found_methods);
            grow_params.extend(found_params);
            grow_self.extend(found_self);
            relay_params.extend(found_relay_params);
            relay_methods.extend(found_relay_methods);
            if !grew {
                break;
            }
        }
        CallGraphFacts { fns, methods, grow_params, grow_self, relay_params, relay_methods, truncated }
    }

    /// Does this function build its answer in the caller's region?
    ///
    /// One question, one answer, whether the programmer wrote `allocates` or the probe
    /// worked it out. Everything below asks through here rather than reading the AST flag,
    /// so there is no way for the two to disagree.
    fn allocates_fn(&self, name: &str) -> bool {
        self.alloc_fns.contains(name)
    }

    /// Does this function hand back storage that arrived as a PARAMETER?
    ///
    /// **The identity case, and the reason the escape rule needed a second question.** RULE 2
    /// refuses returning region data unless the function allocates — because an allocation goes
    /// into the CALLER's region and so outlives the call. That argument is right and it is not
    /// the only way for storage to belong to the caller: a parameter's storage is the caller's
    /// already, by construction, without anything being allocated at all.
    ///
    /// Asking only about allocation made the rule a PROXY, and the proxy was measurably wrong in
    /// both directions. `f(xs) -> [Int] { return xs; }` was refused, while the same function with
    /// a junk allocation the answer never touches was accepted — so the check was satisfied by a
    /// line that had nothing to do with what was returned, and a reader who deleted the dead line
    /// broke the build.
    ///
    /// Nothing new is inferred here. `record_relay` has recorded exactly this fact since B32,
    /// per parameter index and gated on `may_be_region_storage`, for the call-site taint rules.
    /// This asks the question the escape rule was always trying to ask.
    fn relays_a_parameter(&self, name: &str) -> bool {
        self.relay_params.iter().any(|(f, _)| f == name)
    }

    fn allocates_method(&self, receiver: &str, name: &str) -> bool {
        self.alloc_methods.contains(&(receiver.to_string(), name.to_string()))
    }

    /// Refuse a call that reaches further than this signature admits.
    ///
    /// Written once, because the messages ARE the agent's instruction set and two wordings for one
    /// rule is how a language starts feeling arbitrary.
    fn effect_refusal(&self, callee: &str, e: Effect) -> String {
        if self.effects_owner.is_empty() {
            return format!(
                "`{}` touches {}, and top-level code declares nothing it touches. Call it from a \
                 function that says `touches {}`, where a reader can see it.",
                callee, e, e
            );
        }
        format!(
            "`{}` touches {}, but `{}` does not say it does. Add `touches {}` to `{}`'s \
             signature — so anyone reading it can see what this call can reach — or stop calling \
             `{}`.",
            callee, e, self.effects_owner, e, self.effects_owner, callee
        )
    }

    /// The sentence that tells a reader how to get a region, written once.
    fn needs_region(&self, what: &str) -> String {
        format!(
            "{}, so it needs a region: there is none open here. Wrap it in \
             `region name {{ ... }}`, or declare the enclosing function \
             `-> ... allocates` to build in the caller's region.",
            what
        )
    }

    /// Record a problem at wherever the checker currently is, and keep going.
    fn record(&mut self, message: impl Into<String>) {
        let d = Diagnostic::new(message, self.current_span.get());
        // The same message at the same place twice is one problem, not two.
        if !self.errors.iter().any(|e| e.span == d.span && e.message == d.message) {
            self.errors.push(d);
        }
    }

    /// Keep checking usefully after a statement failed.
    ///
    /// An **annotated** `let` states its type, so even when the initializer is wrong
    /// the binding's type is known. Binding it anyway means the rest of the function
    /// checks against the type the author asked for, instead of drowning the real
    /// error in a cascade of "unknown name" noise.
    ///
    /// An **inferred** `let` whose initializer failed has nothing to recover with, and
    /// nothing is guessed. That is the stated cost of spec/1.0/M10-ERGONOMICS.md §1 — half
    /// of an advantage Burxt used to have for free — and it is a real argument for
    /// annotating bindings in a long function.
    /// Record that this `(generic, type arguments)` pair is needed, and answer the symbol
    /// it will have. Recording is idempotent: a generic called in fifty places is emitted
    /// once, and a generic called nowhere is emitted never — which is what lets a library
    /// declare generics at no cost. See spec/1.0/M7-GENERICS.md Decision 4.
    /// Is this name an enum? Either declared concretely, or made on demand as an
    /// instantiation of a generic one — a caller has no reason to care which.
    /// Replace every concrete generic application written anywhere in the program with the
    /// `Named` type of its instantiation, making those instantiations as it goes.
    ///
    /// A pre-pass over the AST rather than a substitution threaded through the checker, for
    /// the reason `specialise` gives: after it, every rule in this file sees ordinary
    /// nominal types and none of them has to remember that generics exist.
    fn expand_program(&self, prog: &mut Program) -> Result<(), String> {
        // The span is set per item as the walk goes, so a refusal from this pass points at
        // the declaration that caused it rather than at the top of the file.
        for st in &mut prog.structs {
            self.current_span.set(st.span);
            if !st.type_parameters.is_empty() {
                continue;             // the generic itself: its parameters stay parameters
            }
            for f in &mut st.fields {
                f.ty = self.expand(&f.ty)?;
            }
        }
        for e in &mut prog.enums {
            self.current_span.set(e.span);
            if !e.type_parameters.is_empty() {
                continue;             // the generic itself: its parameters stay parameters
            }
            for v in &mut e.variants {
                for t in &mut v.payload {
                    *t = self.expand(t)?;
                }
            }
        }
        for ex in &mut prog.externs {
            self.current_span.set(ex.span);
            for p in &mut ex.parameters {
                p.ty = self.expand(&p.ty)?;
            }
            ex.ret = self.expand(&ex.ret)?;
        }
        for f in &mut prog.fns {
            self.current_span.set(f.span);
            self.expand_fn_types(&mut f.parameters, &mut f.ret, &mut f.body)?;
        }
        for m in &mut prog.methods {
            self.current_span.set(m.span);
            self.expand_fn_types(&mut m.parameters, &mut m.ret, &mut m.body)?;
        }
        for im in &mut prog.impls {
            self.current_span.set(im.span);
            // `implement Mapper<Int> for Doubler` becomes an impl of the ordinary interface
            // `Mapper$Int`, resolved HERE so that every reader below — `check_impl`, the
            // duplicate-impl set, the vtable emission loop, the method lookup — keeps taking
            // the plain interface name it has always taken. This is the same trick the rest of
            // the pass plays with types, applied to the one name that is not one.
            if !im.interface_arguments.is_empty() {
                let arguments = std::mem::take(&mut im.interface_arguments);
                if !self.generic_interfaces.contains_key(&im.interface_name) {
                    return Err(format!(
                        "unknown interface `{}` — declare it with `interface {}<...> {{ ... }}`",
                        im.interface_name, im.interface_name
                    ));
                }
                match self.expand_interface(&im.interface_name, &arguments)? {
                    Type::Dyn(symbol) => im.interface_name = symbol,
                    // Arguments that still mention a parameter — `class Box<T> implements
                    // Mapper<T>`. Nothing is monomorphised yet and there is no instantiation
                    // to implement, so this is refused rather than silently registering an
                    // impl under a name no lookup will ever form. A9 does not do generic
                    // IMPLS; see the deferred note on the roadmap row.
                    _ => {
                        return Err(format!(
                            "`implement {}<...> for {}` must name concrete type arguments — \
                             a class cannot implement an interface at a type it is still \
                             generic over.",
                            im.interface_name, im.type_name
                        ))
                    }
                }
            } else if self.generic_interfaces.contains_key(&im.interface_name) {
                // `implement Mapper for Doubler` — the arguments left off. Same sentence the
                // `Named` arm of `expand` gives for the same mistake in a signature.
                let (parameters, _) = &self.generic_interfaces[&im.interface_name];
                return Err(format!(
                    "`{}` takes {} type argument(s), so write `{}<{}>` rather than `{}` \
                     on its own.",
                    im.interface_name,
                    parameters.len(),
                    im.interface_name,
                    parameters.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", "),
                    im.interface_name
                ));
            }
            for m in &mut im.methods {
                self.current_span.set(m.span);
                self.expand_fn_types(&mut m.parameters, &mut m.ret, &mut m.body)?;
            }
        }
        self.expand_block(&mut prog.stmts)?;
        Ok(())
    }

    fn expand_fn_types(
        &self,
        parameters: &mut [Param],
        ret: &mut Type,
        body: &mut [Stmt],
    ) -> Result<(), String> {
        for p in parameters.iter_mut() {
            p.ty = self.expand(&p.ty)?;
        }
        *ret = self.expand(ret)?;
        self.expand_block(body)
    }

    fn expand_block(&self, stmts: &mut [Stmt]) -> Result<(), String> {
        for st in stmts {
            self.current_span.set(st.span);
            match &mut st.kind {
                StmtKind::Let { declared, .. } => {
                    if let Some(t) = declared {
                        *t = self.expand(t)?;
                    }
                }
                StmtKind::While { body, .. }
                | StmtKind::Region { body, .. }
                | StmtKind::For { body, .. }
                | StmtKind::ForRange { body, .. } => self.expand_block(body)?,
                StmtKind::If { then_block, else_block, .. } => {
                    self.expand_block(then_block)?;
                    if let Some(b) = else_block {
                        self.expand_block(b)?;
                    }
                }
                StmtKind::Match { arms, .. } => {
                    for a in arms {
                        self.expand_block(&mut a.body)?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Is this a class? Declared concretely, or made on demand as an instantiation of a
    /// generic one — a caller has no reason to care which.
    fn is_record(&self, name: &str) -> bool {
        self.structs.contains_key(name) || self.made_records.borrow().contains_key(name)
    }

    fn fields_of(&self, name: &str) -> Option<Vec<(String, Type)>> {
        if let Some(f) = self.structs.get(name) {
            return Some(f.clone());
        }
        if let Some(f) = self.made_records.borrow().get(name) {
            return Some(f.clone());
        }
        // A generic's OWN name, with its fields still in terms of its parameters. Only reachable
        // while checking the generic's own body, where `Map { ... }` means "this record, arguments
        // not yet known". A bare `Map` as a type annotation is refused before it gets here, by the
        // rule that a generic name always needs its arguments.
        self.generic_records.get(name).map(|(_, fields)| fields.clone())
    }

    fn is_enum(&self, name: &str) -> bool {
        self.enums.contains_key(name) || self.made_enums.borrow().contains_key(name)
    }

    /// Does `ty` embed `target`'s own bytes, directly or through anything it contains?
    ///
    /// **By value is the whole of the question.** A width is unbounded only when a type contains
    /// ITSELF — a variant carrying `[Json]` carries a pointer, a length and a capacity no matter how
    /// wide a `Json` is, so recursion through a slice always terminates. Same for `dynamic`, which is
    /// a pointer pair. An ARRAY does embed its element, so it recurses.
    ///
    /// This replaces the rule both compilers used to state — "a variant may not carry an enum" —
    /// which was a proxy for this one and wrong whenever the recursion went through a pointer. It was
    /// also a proxy stage-0 did not itself obey: `enum X { V(SomeEnum) }` written out was refused,
    /// while the identical shape reached through `Option<Json>` was allowed and worked, because the
    /// instantiation path never ran this check. One rule now, and the permissive path was the correct
    /// one.
    ///
    /// `seen` is the cycle guard, and it is what makes the walk terminate on the very shapes it
    /// exists to refuse: `enum A { Go(B) }` / `enum B { Back(A) }` would otherwise recur forever
    /// while deciding that it recurs forever.
    fn embeds_by_value(&self, ty: &Type, target: &str, seen: &mut Vec<String>) -> bool {
        match ty {
            // A pointer, whatever it points at. This is the case the old rule got wrong.
            Type::Slice(_) | Type::Dyn(_) => false,
            Type::Array { elem, .. } => self.embeds_by_value(elem, target, seen),
            Type::Named(name) => {
                if name == target {
                    return true;
                }
                if seen.iter().any(|s| s == name) {
                    return false;                    // already walked; not a fresh path to `target`
                }
                seen.push(name.clone());
                let mut found = false;
                if let Some(fields) = self.structs.get(name) {
                    found = fields.iter().any(|(_, t)| self.embeds_by_value(t, target, seen));
                } else if let Some(variants) = self.variants_of(name) {
                    found = variants
                        .iter()
                        .any(|(_, p)| p.iter().any(|t| self.embeds_by_value(t, target, seen)));
                }
                seen.pop();
                found
            }
            _ => false,
        }
    }

    fn variants_of(&self, name: &str) -> Option<Vec<(String, Vec<Type>)>> {
        if let Some(v) = self.enums.get(name) {
            return Some(v.clone());
        }
        self.made_enums.borrow().get(name).cloned()
    }

    /// Replace every concrete generic application in a type with the `Named` type of its
    /// instantiation, making that instantiation if this is the first time it is asked for.
    ///
    /// An application whose arguments still mention a type parameter is left alone: it is
    /// inside a generic being checked generically, and it becomes concrete when that
    /// generic is instantiated. See spec/1.0/M7-GENERICS.md Decision 4.
    /// Make one instantiation of a generic interface and hand back the `Dyn` that names it.
    ///
    /// Shared by the three spellings that reach it — `Mapper<Int>`, `dynamic Mapper<Int>` and
    /// the `implement Mapper<Int> for D` header — so the mangled name they agree on is
    /// computed in exactly one place. If the impl header and the parameter type ever disagreed
    /// about what `Mapper<Int>` is called, the impl would register a vtable nothing looks up
    /// and the call would fail to resolve at a site that never mentions generics.
    fn expand_interface(&self, name: &str, arguments: &[Type]) -> Result<Type, String> {
        let (parameters, methods) = self.generic_interfaces[name].clone();
        let arguments: Vec<Type> =
            arguments.iter().map(|a| self.expand(a)).collect::<Result<_, _>>()?;
        if arguments.len() != parameters.len() {
            return Err(format!(
                "`{}` takes {} type argument(s), but {} were given",
                name,
                parameters.len(),
                arguments.len()
            ));
        }
        // Still inside a generic's own body: `Mapper<T>` is not an interface any value has
        // yet, and it becomes one when the enclosing generic is instantiated. The same bail
        // the record, enum and tuple arms make, for the same reason.
        if arguments.iter().any(mentions_param) {
            return Ok(Type::DynGeneric { name: name.to_string(), arguments });
        }
        // An argument naming nothing — `dynamic Mapper<T, U>` in a method whose receiver
        // declares only `T`, so `U` parsed as an ordinary type name and no declaration
        // matches it. Caught HERE because the alternative is mangling it into the symbol and
        // reporting "unknown interface `Mapper$Int$U`", which names a thing the author never
        // wrote and buries the one word that is actually wrong. A method takes its type
        // parameters from its receiver, so this is the shape a reader hits first when
        // reaching for a second one.
        for a in &arguments {
            if let Type::Named(n) = a {
                if !self.declared_type_names.contains(n)
                    && !self.is_record(n)
                    && !self.is_enum(n)
                    && !self.interfaces.contains_key(n)
                {
                    return Err(format!(
                        "`{}` in `{}<...>` names no type. A method's type parameters come \
                         from its receiver — `function (self: List<T>) ...` declares `T` \
                         and nothing else.",
                        n, name
                    ));
                }
            }
        }
        let symbol = mangle(name, &arguments);
        if !self.interfaces_made.borrow().contains_key(&symbol) {
            // Reserved before it is filled in, so an interface whose own method mentions it —
            // `interface Chain<T> { function then(self, next: Chain<T>) -> Int }` — cannot make
            // this recurse forever. The generic-record arm reserves for the same reason; unlike
            // a class, an interface CAN legally hold itself, because a `Dyn` is a pointer and
            // has a size no matter what it points at, so this reservation is the whole fix
            // rather than half of one. `tests/pass/a_generic_interface_may_name_itself.bx`.
            self.interfaces_made.borrow_mut().insert(symbol.clone(), Vec::new());
            let map: HashMap<String, Type> = parameters
                .iter()
                .map(|p| p.name.clone())
                .zip(arguments.iter().cloned())
                .collect();
            let mut made: Vec<InterfaceSig> = Vec::new();
            for signature in &methods {
                let mut made_sig = signature.clone();
                for p in &mut made_sig.parameters {
                    p.ty = self.expand(&substitute(&p.ty, &map))?;
                }
                made_sig.ret = self.expand(&substitute(&made_sig.ret, &map))?;
                made.push(made_sig);
            }
            self.interfaces_made.borrow_mut().insert(symbol.clone(), made);
            // So `show` spells it `Mapper<Int>` and never `Mapper$Int`. A reader did not
            // write the mangled name and must not be shown it — the rule `instance_of`
            // exists for, and the one an interface would have slipped through, because the
            // `Dyn` arm of `show` did not consult this map until A9 added it.
            self.instance_of
                .borrow_mut()
                .insert(symbol.clone(), (name.to_string(), arguments.clone()));
        }
        Ok(Type::Dyn(symbol))
    }

    fn expand(&self, ty: &Type) -> Result<Type, String> {
        match ty {
            // A bare interface name MEANS a dynamic one. `rule: Tax` is what Java, C#,
            // TypeScript and PHP all write, and it is what v0.0.155 made Burxt write: `dynamic`
            // was Rust's `dyn`, and requiring it made every polymorphic parameter carry a word
            // the reader had to decode and the writer had to remember.
            //
            // Nothing downstream changes, because this pass exists for exactly this: after it,
            // no rule below knows the sugar happened — the same argument the generic
            // instantiation cases below make. `dynamic Tax` stays legal and identical.
            //
            // The static path is still expressible and still explicit: `<T: Tax>` monomorphises
            // and pays no vtable. What went is the ceremony on the DEFAULT.
            Type::Named(name) if self.interface_names.contains(name) => {
                Ok(Type::Dyn(name.clone()))
            }
            // `Mapper` written bare when it takes a parameter. The arm above would have made
            // it `dynamic Mapper`, and the failure would then surface as "unknown interface
            // `Mapper`" two passes later — true, and useless, because the interface is right
            // there. Say the actual thing instead.
            Type::Named(name) if self.generic_interfaces.contains_key(name) => {
                let (parameters, _) = &self.generic_interfaces[name];
                Err(format!(
                    "`{}` takes {} type argument(s), so write `{}<{}>` rather than `{}` \
                     on its own.",
                    name,
                    parameters.len(),
                    name,
                    parameters
                        .iter()
                        .map(|p| p.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    name
                ))
            }
            // A generic INTERFACE application, in either spelling: a bare `Mapper<Int>` (which
            // means a dynamic one, by the same v0.0.155 rule as a bare `Tax`) and an explicit
            // `dynamic Mapper<Int>` both land here and both leave as `Dyn("Mapper$Int")`.
            //
            // This is the generic-record arm below with "make a class" replaced by "make a
            // signature set", and that parallel is the whole design. The instantiation is an
            // ordinary interface under a mangled name, so **the vtable is keyed by exactly what
            // it was always keyed by** — `dyn_interfaces`, `check_impl`, the vtable emission
            // loop and the method lookup all take an interface NAME, and `Mapper$Int` and
            // `Mapper$String` are two names. Nothing downstream learned anything.
            Type::Generic { name, arguments } if self.generic_interfaces.contains_key(name) => {
                self.expand_interface(name, arguments)
            }
            Type::DynGeneric { name, arguments } => {
                if !self.generic_interfaces.contains_key(name) {
                    // `dynamic Holder<Int>` where `Holder` is a generic CLASS. Refused with
                    // the sentence the argument-less `dynamic Holder` already gives, which is
                    // the entire reason `DynGeneric` is a variant rather than sugar for
                    // `Generic` — desugaring in the parser made this program compile.
                    return Err(format!(
                        "unknown interface `{}` — declare it with `interface {} {{ ... }}`",
                        name, name
                    ));
                }
                self.expand_interface(name, arguments)
            }
            // A generic RECORD application. Same shape as the enum case below: the concrete
            // instantiation becomes an ordinary nominal record, made once, and after that no
            // rule in this file knows generics exist.
            Type::Generic { name, arguments } if self.generic_records.contains_key(name) => {
                let (parameters, fields) = self.generic_records[name].clone();
                let arguments: Vec<Type> =
                    arguments.iter().map(|a| self.expand(a)).collect::<Result<_, _>>()?;
                if arguments.len() != parameters.len() {
                    return Err(format!(
                        "`{}` takes {} type argument(s), but {} were given",
                        name,
                        parameters.len(),
                        arguments.len()
                    ));
                }
                if arguments.iter().any(mentions_param) {
                    return Ok(Type::Generic { name: name.clone(), arguments });
                }
                let symbol = mangle(name, &arguments);
                if !self.is_record(&symbol) {
                    // Reserved before it is filled in, so a class whose field mentions
                    // itself cannot make this recurse forever.
                    self.made_records.borrow_mut().insert(symbol.clone(), Vec::new());
                    let map: HashMap<String, Type> = parameters
                        .iter()
                        .map(|p| p.name.clone())
                        .zip(arguments.iter().cloned())
                        .collect();
                    let mut made: Vec<(String, Type)> = Vec::new();
                    for (fname, ty) in &fields {
                        made.push((fname.clone(), self.expand(&substitute(ty, &map))?));
                    }
                    for (fname, ty) in &made {
                        if ty == &Type::Named(symbol.clone()) {
                            self.made_records.borrow_mut().remove(&symbol);
                            return Err(format!(
                                "`{}` cannot contain itself: `{}.{}` would have to be the \
                                 same size as the whole class.",
                                symbol, name, fname
                            ));
                        }
                    }
                    self.made_records.borrow_mut().insert(symbol.clone(), made.clone());
                    self.instance_of
                        .borrow_mut()
                        .insert(symbol.clone(), (name.clone(), arguments.clone()));
                    self.made_record_order.borrow_mut().push(TypedStruct {
                        name: symbol.clone(),
                        fields: made.iter().map(|(_, t)| t.clone()).collect(),
                    });
                    self.wanted_records
                        .borrow_mut()
                        .push((name.clone(), arguments.clone()));
                }
                Ok(Type::Named(symbol))
            }
            // A tuple becomes an anonymous class, made once, named by its own spelling. After
            // this point nothing in the compiler knows tuples exist — see `ast::Type::Tuple`
            // for the measurement that chose this over a new aggregate kind.
            //
            // The shape is deliberately the generic-record arm above with the two generic
            // parts removed: there is no declaration to substitute into and no `instance_of`
            // entry, because a tuple is not an instantiation OF anything. What stays is the
            // part that matters — `made_records` so every lookup finds it, and
            // `made_record_order` so codegen emits the LLVM type.
            Type::Tuple(elements) => {
                let elements: Vec<Type> =
                    elements.iter().map(|e| self.expand(e)).collect::<Result<_, _>>()?;
                // Still inside a generic's own body: `(T, String)` is not a type any value
                // has yet, and it becomes one when the generic is instantiated. Exactly the
                // `mentions_param` bail the two arms above make, for the same reason.
                if elements.iter().any(mentions_param) {
                    return Ok(Type::Tuple(elements));
                }
                let symbol = self.tuple_symbol(&elements);
                if !self.is_record(&symbol) {
                    let made: Vec<(String, Type)> = elements
                        .iter()
                        .enumerate()
                        .map(|(i, t)| (i.to_string(), t.clone()))
                        .collect();
                    self.made_records.borrow_mut().insert(symbol.clone(), made.clone());
                    self.made_record_order.borrow_mut().push(TypedStruct {
                        name: symbol.clone(),
                        fields: elements.clone(),
                    });
                    self.tuple_of.borrow_mut().insert(symbol.clone(), elements.clone());
                }
                Ok(Type::Named(symbol))
            }
            Type::Generic { name, arguments } => {
                let (parameters, variants) = self.generic_enums.get(name).cloned().ok_or_else(|| {
                    if self.declared_type_names.contains(name) {
                        format!(
                            "`{}` is not generic, so it takes no type arguments — write \
                             `{}` on its own.",
                            name, name
                        )
                    } else {
                        format!("unknown generic type `{}`", name)
                    }
                })?;
                let arguments: Vec<Type> =
                    arguments.iter().map(|a| self.expand(a)).collect::<Result<_, _>>()?;
                if arguments.len() != parameters.len() {
                    return Err(format!(
                        "`{}` takes {} type argument(s), but {} were given",
                        name,
                        parameters.len(),
                        arguments.len()
                    ));
                }
                if arguments.iter().any(mentions_param) {
                    return Ok(Type::Generic { name: name.clone(), arguments });
                }
                let symbol = mangle(name, &arguments);
                if !self.is_enum(&symbol) {
                    // Reserve the name BEFORE filling it in, so an enum whose payload
                    // mentions itself cannot make this recurse forever.
                    self.made_enums.borrow_mut().insert(symbol.clone(), Vec::new());
                    let map: HashMap<String, Type> = parameters
                        .iter()
                        .map(|p| p.name.clone())
                        .zip(arguments.iter().cloned())
                        .collect();
                    let mut made: Vec<(String, Vec<Type>)> = Vec::new();
                    for (vname, payload) in &variants {
                        let mut ps = Vec::with_capacity(payload.len());
                        for t in payload {
                            ps.push(self.expand(&substitute(t, &map))?);
                        }
                        made.push((vname.clone(), ps));
                    }
                    // The same rule the concrete declarations get, said in terms of the
                    // type argument that caused it — because that is what the author wrote.
                    for (vname, ps) in &made {
                        for t in ps {
                            match t {
                                Type::Int
                                | Type::Bool
                                | Type::String
                                | Type::Decimal { .. } => {}
                                // A class or an array payload is allowed since v0.0.118, so
                                // `Option<Point>` can be made. An ENUM payload still cannot: an
                                // enum inside an enum has no finite size without indirection, and
                                // that is a memory-model question rather than a layout one.
                                Type::Named(n) if !self.is_enum(n) => {}
                                Type::Array { .. } | Type::Slice(_) => {}
                                other => {
                                    self.made_enums.borrow_mut().remove(&symbol);
                                    return Err(format!(
                                        "`{}` cannot be made: `{}.{}` would carry {} {}, \
                                         which has no layout here. A variant carries a \
                                         scalar, a String, a class or an array.",
                                        Type::Generic {
                                            name: name.clone(),
                                            arguments: arguments.clone()
                                        },
                                        name,
                                        vname,
                                        other.article(),
                                        other
                                    ));
                                }
                            }
                        }
                    }
                    for (vname, ps) in &made {
                        if ps.iter().any(|t| t == &Type::Named(symbol.clone())) {
                            self.made_enums.borrow_mut().remove(&symbol);
                            return Err(format!(
                                "`{}` cannot carry itself: `{}.{}` would have to hold a \
                                 value the same size as the whole enum, plus a tag.",
                                symbol, name, vname
                            ));
                        }
                    }
                    self.made_enums.borrow_mut().insert(symbol.clone(), made.clone());
                    self.instance_of
                        .borrow_mut()
                        .insert(symbol.clone(), (name.clone(), arguments.clone()));
                    self.made_order.borrow_mut().push(TypedEnum {
                        name: symbol.clone(),
                        variants: made.into_iter().map(|(_, p)| p).collect(),
                    });
                }
                Ok(Type::Named(symbol))
            }
            Type::Array { elem, len } => Ok(Type::Array {
                elem: Box::new(self.expand(elem)?),
                len: *len,
            }),
            Type::Slice(elem) => Ok(Type::Slice(Box::new(self.expand(elem)?))),
            other => Ok(other.clone()),
        }
    }

    /// `Option.Some(3)` — work out what the arguments are, then build the variant of the
    /// instantiation. Two sources, in this order: what the payload says, and what the
    /// context expects. `Option.None` has no payload, so it needs the context, and says
    /// so when there is none.
    fn build_generic_variant(
        &self,
        enum_name: &str,
        variant: &str,
        arguments: &[Expr],
        expected: Option<&Type>,
    ) -> Result<TypedExpr, String> {
        let (parameters, variants) = self.generic_enums[enum_name].clone();
        let tag = variants.iter().position(|(n, _)| n == variant).ok_or_else(|| {
            format!(
                "`{}` has no variant named `{}`. Its variants are: {}.",
                enum_name,
                variant,
                variants.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", ")
            )
        })?;
        let payload = &variants[tag].1;
        if arguments.len() != payload.len() {
            return Err(format!(
                "`{}.{}` carries {} value(s), but {} were given",
                enum_name,
                variant,
                payload.len(),
                arguments.len()
            ));
        }

        let mut map: HashMap<String, Type> = HashMap::new();
        // What the context asks for comes first: it is the only thing that can settle a
        // variant with no payload, and it is what the author wrote down.
        if let Some(Type::Named(want)) = expected {
            if let Some((of, type_args)) = self.instance_of.borrow().get(want).cloned() {
                if of == enum_name {
                    for (p, a) in parameters.iter().zip(type_args) {
                        map.insert(p.name.clone(), a);
                    }
                }
            }
        }
        // The same question, asked of an expectation that has NOT been monomorphised —
        // `Option<T>` rather than `Option$Int`. This is the A3 fix, and the arm above could not
        // answer it: `instance_of` is keyed by an instantiation's SYMBOL, so it only ever knows
        // about a type some caller already made concrete.
        //
        // Inside `function first_of<T>(xs: [T]) -> Option<T>`, the declared return type is a
        // `Type::Generic` whose argument is still `Param("T")`, so `return Option.None;` fell
        // through both this map and the payload loop below and reported that nothing says what
        // `T` is — while the signature said it, three lines up. The enclosing return type IS the
        // context; it just had not been looked at in the one state where it is not yet a name.
        if let Some(Type::Generic { name, arguments }) = expected {
            if name == enum_name {
                for (p, a) in parameters.iter().zip(arguments) {
                    map.insert(p.name.clone(), a.clone());
                }
            }
        }
        for (i, (declared, argument)) in payload.iter().zip(arguments).enumerate() {
            if !mentions_param(declared) {
                continue;
            }
            let actual = self.check_expr(argument, None)?.ty;
            let instances = self.instance_of.borrow().clone();
            let tuples = self.tuple_of.borrow().clone();
            unify(declared, &actual, &mut map, &instances, &tuples).map_err(|why| {
                format!("in `{}.{}`, payload {}: {}", enum_name, variant, i + 1, why)
            })?;
        }
        let mut type_args = Vec::with_capacity(parameters.len());
        for p in &parameters {
            match map.get(&p.name) {
                Some(t) => type_args.push(t.clone()),
                None => {
                    let call = if payload.is_empty() {
                        format!("{}.{}", enum_name, variant)
                    } else {
                        format!("{}.{}(...)", enum_name, variant)
                    };
                    // The advice names the RETURN TYPE first, because since A3 that is the
                    // commonest way to answer this and it used not to work: a `return` inside
                    // `-> Option<T>` now reads `T` from the signature. The old message offered
                    // only an annotated `let` and said "nothing here does" to programs whose
                    // signature said it three lines up — true about what the checker looked at,
                    // false about the program, which is the worst kind of accurate message.
                    return Err(format!(
                        "`{}.{}` does not say what `{}` is, and nothing here does. Give it a \
                         context that names `{}`: return it from a function declared \
                         `-> {}<{}>`, annotate where it lands — `let x: {}<...> = {};` — or \
                         pass it to something whose parameter says.",
                        enum_name, variant, p.name, p.name, enum_name, p.name, enum_name, call
                    ))
                }
            }
        }
        // If a type argument is still a parameter, this construction is inside a generic being
        // checked GENERICALLY, and there is nothing to instantiate yet: the copy appears when the
        // enclosing generic is instantiated, and its body then names a concrete type here. The
        // same rule a generic CALL already follows — see `mentions_param` at the call site — and
        // the same reason: `Option<T>` has no layout until a caller says what `T` is.
        //
        // Without this, `Option.Some(xs[0])` inside `first_of<T>` reached `expand` with
        // `[Param("T")]`, could not produce a name, and reported **"codegen bug: an instantiation
        // is not a named type"** — an internal message, to a user, about their perfectly good
        // program. So A3 was two bugs behind one symptom: `None` had no context to read, and
        // `Some` had context and could not use it.
        //
        // Answering an abstract type is safe because the abstract body is never emitted —
        // `check` deliberately holds it back ("the generic itself is CHECKED and never EMITTED:
        // there is no layout for a `T` until a caller says what it is"). Nothing downstream ever
        // sees this node; the specialised copy is what codegen gets.
        if type_args.iter().any(mentions_param) {
            let mut typed_args = Vec::new();
            for (i, (argument, declared)) in arguments.iter().zip(payload).enumerate() {
                // The payload type with what IS known substituted in, so a wrong argument is
                // still caught here rather than surviving to every instantiation.
                let want = substitute(declared, &map);
                let t = self.check_expr(argument, Some(&want))?;
                if !self.storable(&t.ty, &want) {
                    return Err(format!(
                        "in `{}.{}`, payload {} must be {}, but it has type {}",
                        enum_name,
                        variant,
                        i + 1,
                        want,
                        t.ty
                    ));
                }
                typed_args.push(t);
            }
            return Ok(TypedExpr {
                ty: Type::Generic { name: enum_name.to_string(), arguments: type_args },
                kind: TypedExprKind::VariantLit {
                    enum_name: enum_name.to_string(),
                    tag: tag as u32,
                    arguments: typed_args,
                },
            });
        }
        let concrete = self.expand(&Type::Generic {
            name: enum_name.to_string(),
            arguments: type_args,
        })?;
        let Type::Named(symbol) = &concrete else {
            return Err("codegen bug: an instantiation is not a named type".to_string());
        };
        let variants = self
            .variants_of(symbol)
            .ok_or_else(|| format!("codegen bug: `{}` was not made", symbol))?;
        self.build_variant(symbol, variants, variant, arguments)
    }

    /// Does this type argument satisfy the bound the signature declared?
    ///
    /// The two the language ships mirror exactly what it already allows: `Ordered` is `Int`
    /// and `Decimal<S>`, because those are the types `<` works on; `Equatable` adds `Bool`
    /// and `String`, because those are the types `==` works on. A bound cannot promise more
    /// than the language delivers, so when Strings gain an ordering they gain `Ordered`
    /// here and nowhere else.
    ///
    /// Any other bound names a declared trait, and satisfying it means having an `impl`.
    fn satisfies(
        &self,
        argument: &Type,
        bound: &str,
        callee: &str,
        param: &str,
    ) -> Result<(), String> {
        let instances = self.instance_of.borrow().clone();
        let shown = show(argument, &instances);
        // A type PARAMETER satisfies a bound when its own declaration says so. That is the whole
        // job of a bound: `Map<K: Equatable, V>` built inside `map_new<K: Equatable, V>` passes `K`
        // along, and the promise travels with it. Checked before the concrete cases below, because
        // a parameter is not any of them.
        if let Type::Param(n) = argument {
            if self.param_bounds.get(n).cloned().flatten().as_deref() == Some(bound) {
                return Ok(());
            }
            return Err(format!(
                "`{}` needs `{}: {}`, and the type parameter `{}` carries no such bound. Write \
                 `{}: {}` where it is declared, so the promise travels with it.",
                callee, param, bound, n, n, bound
            ));
        }
        match bound {
            "Ordered" => match argument {
                Type::Int | Type::Decimal { .. } | Type::String => Ok(()),
                _ => Err(format!(
                    "`{}` needs `{}: Ordered`, and {} has no order. Ordered is Int, \
                     Decimal and String — the types `<` works on.",
                    callee, param, shown
                )),
            },
            "Equatable" => match argument {
                Type::Int | Type::Bool | Type::String | Type::Decimal { .. } => Ok(()),
                _ => Err(format!(
                    "`{}` needs `{}: Equatable`, and two {} values cannot be compared. \
                     Equatable is Int, Bool, String and Decimal — the types `==` works on.",
                    callee, param, shown
                )),
            },
            interface_name => {
                if !self.interfaces.contains_key(interface_name) {
                    return Err(format!(
                        "`{}` bounds `{}` by `{}`, which is not an interface this program \
                         declares. A bound is `Ordered`, `Equatable`, or a declared interface.",
                        callee, param, interface_name
                    ));
                }
                let concrete = match argument {
                    Type::Named(n) => n.clone(),
                    _ => {
                        return Err(format!(
                            "`{}` needs `{}: {}`, and {} is not a type that can implement \
                             an interface — only a class or an enum can.",
                            callee, param, interface_name, shown
                        ))
                    }
                };
                if self.impls.contains(&(interface_name.to_string(), concrete.clone())) {
                    return Ok(());
                }
                Err(format!(
                    "`{}` needs `{}: {}`, and `{}` does not implement it. Write `implement {} \
                     for {} {{ ... }}` — conformance is declared, never inferred from \
                     having the right method names.",
                    callee, param, interface_name, shown, interface_name, concrete
                ))
            }
        }
    }

    /// Which instantiation a class literal means. For a non-generic record: itself. For a
    /// generic one: the arguments come from the context when it names them, and otherwise are
    /// inferred from the field values — the same two sources, in the same order, that a
    /// generic enum's variant uses. See spec/1.0/M7-GENERICS.md.
    fn instantiate_record(
        &self,
        name: &str,
        given: &[(String, Expr)],
        expected: Option<&Type>,
    ) -> Result<Type, String> {
        let Some((parameters, fields)) = self.generic_records.get(name).cloned() else {
            return Ok(Type::Named(name.to_string()));
        };
        let mut map: HashMap<String, Type> = HashMap::new();
        // An expectation that is itself an application — `-> Map<K, V>` inside a generic, or
        // `Map<String, Int>` before instantiation — names the arguments directly. Read first,
        // because what the context says beats what the field values imply.
        if let Some(Type::Generic { name: want, arguments }) = expected {
            if want == name && arguments.len() == parameters.len() {
                for (p, a) in parameters.iter().zip(arguments) {
                    map.insert(p.name.clone(), a.clone());
                }
            }
        }
        if let Some(Type::Named(want)) = expected {
            if let Some((of, arguments)) = self.instance_of.borrow().get(want).cloned() {
                if of == name {
                    for (p, a) in parameters.iter().zip(arguments) {
                        map.insert(p.name.clone(), a);
                    }
                }
            }
        }
        let instances = self.instance_of.borrow().clone();
        let tuples = self.tuple_of.borrow().clone();
        for (fname, declared) in &fields {
            if !mentions_param(declared) {
                continue;
            }
            // Only fields that could still settle something. `Stack<Int>` in the annotation
            // has already said what T is, and asking `items: []` to say it too would fail —
            // an empty array literal cannot name its own type, which is a rule of its own.
            if parameters.iter().all(|p| map.contains_key(&p.name)) {
                break;
            }
            let Some((_, value)) = given.iter().find(|(n, _)| n == fname) else {
                continue;             // a missing field is reported below, not here
            };
            // A field whose value cannot be typed on its own says nothing here. The real
            // error, if there is one, comes from checking the field against its type below.
            let Ok(typed) = self.check_expr(value, None) else { continue };
            unify(declared, &typed.ty, &mut map, &instances, &tuples)
                .map_err(|why| format!("in `{}.{}`: {}", name, fname, why))?;
        }
        let mut type_args = Vec::with_capacity(parameters.len());
        for p in &parameters {
            match map.get(&p.name) {
                Some(t) => type_args.push(t.clone()),
                None => {
                    return Err(format!(
                        "`{}` does not say what `{}` is, and nothing here does. Write the \
                         type where the value lands — `let x: {}<...> = {} {{ ... }};`",
                        name, p.name, name, name
                    ))
                }
            }
        }
        for (p, argument) in parameters.iter().zip(&type_args) {
            if let Some(bound) = &p.bound {
                self.satisfies(argument, bound, name, &p.name)?;
            }
        }
        match self.expand(&Type::Generic { name: name.to_string(), arguments: type_args })? {
            Type::Named(symbol) => Ok(Type::Named(symbol)),
            // Still abstract, because an argument mentions a type parameter — which is what
            // `Map { entries: [], slots: [], live: 0 }` looks like INSIDE `Map`'s own generic
            // function. `expand` leaves it alone on purpose, by the same rule the function path
            // uses, and this used to call that a codegen bug.
            //
            // There is no instantiation to name yet, so the answer is the generic's own name. Its
            // fields are typed in terms of its parameters, which is exactly what checking this body
            // needs — and nothing will ever lower it, because `specialise` clones the UNTYPED
            // declaration and the copy is checked fresh with the arguments substituted. The
            // abstract pass validates; the concrete pass compiles.
            // Answered as the APPLICATION and not as a bare name, so that it still equals the
            // `Box<T>` a signature wrote. A bare `Box` would compare unequal to `Box<T>` and the
            // return check would refuse the very literal it asked for.
            still @ Type::Generic { .. } => Ok(still),
            other => Err(format!("codegen bug: `{}` instantiated to {}", name, other)),
        }
    }

    fn want(&self, name: &str, type_args: &[Type]) -> String {
        let symbol = mangle(name, type_args);
        if self.seen_instantiations.borrow_mut().insert(symbol.clone()) {
            self.wanted.borrow_mut().push((name.to_string(), type_args.to_vec()));
        }
        symbol
    }

    fn recover_from(&mut self, s: &Stmt) {
        if let StmtKind::Let { name, mutable, declared: Some(declared), .. } = &s.kind {
            if self.validate_type(declared).is_ok() && !self.env.contains_key(name) {
                self.env.insert(name.clone(), (declared.clone(), *mutable));
            }
        }
    }

    /// Blame a particular sub-expression for the error about to be returned, and
    /// stop any enclosing expression from taking the blame instead.
    ///
    /// Needed because `check_expr`'s "innermost failing expression claims the
    /// position" rule is about expressions that FAILED — when a parent's own check
    /// fails over a child that was individually fine (a wrong argument, a value
    /// that does not match its declared type), the parent knows better than the
    /// rule does.
    fn blame(&self, span: Span) {
        self.current_span.set(span);
        self.error_located.set(true);
    }

    /// Every expression's span and type, innermost last — the table hover reads.
    /// Every expression's type, spelled the way the AUTHOR would write it.
    ///
    /// Monomorphisation left `Named("Holder$Int")` and, since A9, `Dyn("Mapper$Int")` — names
    /// nobody typed. The only caller is the language server, and hover puts this text under a
    /// reader's cursor, so it is exactly the audience `show` and `declared_name` exist to
    /// protect. **Measured before it was changed:** hovering a `Holder<Int>` on a pristine
    /// v0.0.276 answered ```` ```burxt\nHolder$Int\n``` ````, so this was a live leak for
    /// generic CLASSES before A9 added a second way to reach it.
    ///
    /// `written_form` hands back the pre-`expand` spelling — `Generic` and `DynGeneric` — which
    /// are the two variants whose `Display` already prints what was written. No new rendering
    /// path, and none that could disagree with the one diagnostics use.
    pub fn expr_types(&self) -> Vec<(Span, Type)> {
        let instances = self.instance_of.borrow();
        self.expr_types
            .borrow()
            .iter()
            .map(|(span, ty)| (*span, written_form(ty, &instances)))
            .collect()
    }

    /// One copy of every method on a generic record, per instantiation of that record.
    ///
    /// Called twice, and idempotently: once before the bodies are checked, because a body may
    /// call a method on a generic record; and again in the instantiation drain, because a body
    /// can be what discovers a NEW instantiation. Registering a method that already exists is
    /// a no-op, so calling it more often than needed costs nothing and missing a call costs
    /// `Stack$Int has no method named push_one` — which is exactly what it cost me.
    fn instantiate_record_methods(
        &mut self,
        methods: &mut Vec<TypedMethod>,
    ) -> Result<(), String> {
    // Classes first: a generic function's body may call a method on one, so the
    // method has to exist before the function that calls it is checked.
    let classes: Vec<(String, Vec<Type>)> =
        std::mem::take(&mut *self.wanted_records.borrow_mut());
    for (record, arguments) in classes {
        let Some((parameters, _)) = self.generic_records.get(&record).cloned() else {
            continue;
        };
        let symbol = mangle(&record, &arguments);
        let map: HashMap<String, Type> = parameters
            .iter()
            .map(|p| p.name.clone())
            .zip(arguments.iter().cloned())
            .collect();
        let mine: Vec<MethodDef> = self
            .generic_methods
            .iter()
            .filter(|m| m.receiver == record)
            .cloned()
            .collect();
        for m in mine {
            // The receiver's own parameter names are what the class's arguments
            // bind to, in order — `self: Stack<T>` against `Stack<Int>` binds T.
            let mut local: HashMap<String, Type> = HashMap::new();
            for (named, p) in m.receiver_arguments.iter().zip(&parameters) {
                if let Some(t) = map.get(&p.name) {
                    local.insert(named.clone(), t.clone());
                }
            }
            let mut concrete = specialise_method(&m, &local, &symbol);
            // BEFORE expanding, not after: expanding an instantiation can refuse, and until
            // A9 gave it a reason to, the stale span from whatever item was processed last
            // is what a refusal here would have pointed at.
            self.current_span.set(concrete.span);
            self.expand_fn_types(
                &mut concrete.parameters,
                &mut concrete.ret,
                &mut concrete.body,
            )?;
            let key = (symbol.clone(), concrete.name.clone());
            if self.methods.contains_key(&key) {
                continue;      // made already, for an earlier use of the same type
            }
            // The declaration pass validates a hand-written method's types; an instantiation
            // is made after that pass has run, so nothing validated these until now — and
            // `validate_type` is what records an interface as `dyn`-used. **This was a live
            // bug before A9 and independent of it**: a plain `function (self: List<T>)
            // counted(f: dynamic Step)` compiled to `codegen bug: no signature for Step.apply`
            // on v0.0.276, because no vtable was emitted for an interface nothing had
            // registered. Measured on the pristine baseline before this line was added.
            for p in &concrete.parameters {
                // B17: the caret goes on the type that is wrong.
                self.current_span.set(p.ty_span);
                self.validate_type(&p.ty)?;
            }
            self.validate_type(&concrete.ret)?;
            let param_tys: Vec<Type> =
                concrete.parameters.iter().map(|p| p.ty.clone()).collect();
            self.method_writable.insert(
                key.clone(),
                concrete.parameters.iter().map(|p| p.writable).collect(),
            );
            self.methods.insert(
                key,
                (concrete.receiver_mut, param_tys, concrete.ret.clone()),
            );
            if concrete.allocates {
                self.alloc_methods.insert((symbol.clone(), concrete.name.clone()));
            }
            let checked = self.check_method(&concrete)?;
            methods.push(checked);
        }
    }
        Ok(())
    }

    /// Fold every `const` initializer down to one literal, in the order they are written.
    ///
    /// This runs before anything else is checked, because a body 200 lines below may read a
    /// const and the answer has to exist by then — the same argument `infer_allocates` makes
    /// for `allocates`.
    ///
    /// A failure here is RECORDED and the const is registered anyway, at its declared type
    /// with a zero value. Without that, one bad `const` becomes an `unknown variable` at
    /// every one of its use sites and the real error is the smallest thing on screen. The
    /// same reason `recover_from` binds an annotated `let` whose initializer failed.
    fn fold_consts(&mut self, prog: &Program) {
        for c in &prog.consts {
            self.current_span.set(c.span);
            self.error_located.set(false);
            let folded = self.fold_one_const(c);
            let value = match folded {
                Ok(value) => value,
                Err(message) => {
                    self.record(message);
                    // Zero in the declared type's shape, so the use sites still type.
                    match &c.declared {
                        Type::Decimal { .. } => TypedExprKind::DecimalLit { unscaled: 0 },
                        Type::Bool => TypedExprKind::BoolLit(false),
                        Type::String => TypedExprKind::StrLit(String::new()),
                        _ => TypedExprKind::IntLit(0),
                    }
                }
            };
            self.consts.insert(c.name.clone(), (c.declared.clone(), value));
        }
    }

    /// One `const`: its name checked, its type checked, its initializer folded.
    fn fold_one_const(&mut self, c: &ConstDef) -> Result<TypedExprKind, String> {
        if is_reserved_name(&c.name) {
            return Err(format!(
                "`{}` is a built-in name and cannot be declared as a `const`",
                c.name
            ));
        }
        if self.consts.contains_key(&c.name) {
            return Err(format!(
                "`{}` is already declared as a `const` — Burxt does not shadow, and a second \
                 `const {}` would silently hide the first",
                c.name, c.name
            ));
        }
        // A const's type is one of the four types that HAVE a literal, and that is not an
        // arbitrary shortlist: a const IS a literal with a name, so a type with no literal
        // form has nothing a const could hold. That single rule covers `CInt`, `CDouble`,
        // `CPointer`, arrays, classes and enums at once, and says the same thing about all
        // of them instead of six separate refusals a reader has to collect.
        match &c.declared {
            Type::Int | Type::Bool | Type::String | Type::Decimal { .. } => {}
            other => {
                return Err(format!(
                    "a `const` may be an Int, a Decimal, a String or a Bool, not {} {} — a \
                     `const` is a literal with a name, and {} has no literal to name. Use a \
                     `let`, or a function that builds one.",
                    other.article(),
                    other,
                    other
                ))
            }
        }
        let value = self.fold_const_expr(&c.value, &c.declared)?;
        // The literal against the annotation, by the same rule `let` uses.
        let ty = self.type_of_const_value(&value, &c.declared);
        if !self.storable(&ty, &c.declared) {
            self.blame(c.value.span);
            return Err(format!(
                "type mismatch in `const {}`: declared {}, but the value is {}",
                c.name, c.declared, ty
            ));
        }
        Ok(value)
    }

    /// The type a folded const value has. Only the four literal kinds reach here, and a
    /// Decimal's scale is the DECLARED one because `fold_const_expr` normalized it there.
    fn type_of_const_value(&self, value: &TypedExprKind, declared: &Type) -> Type {
        match value {
            TypedExprKind::IntLit(_) => Type::Int,
            TypedExprKind::BoolLit(_) => Type::Bool,
            TypedExprKind::StrLit(_) => Type::String,
            TypedExprKind::DecimalLit { .. } => declared.clone(),
            // Unreachable: `fold_const_expr` returns nothing else. Answering with the
            // declaration rather than panicking, because a checker that aborts on its own
            // invariant is worse than one that accepts a program it already validated.
            _ => declared.clone(),
        }
    }

    /// Evaluate a constant expression, or say why it is not one.
    ///
    /// The grammar is deliberately small — literals, consts declared above, and `+ - *`
    /// with unary `-` over Ints. See `ast::ConstDef` for what that costs and why the cost
    /// was chosen. Every arithmetic step is CHECKED: an overflow is a compile error, which
    /// is the one thing this evaluator must not get wrong. A folded constant that wrapped
    /// would put a wrong number in the binary with no run-time check left to catch it,
    /// because by codegen it is a literal — so this is the only place the guarantee can be
    /// made, and `checked_*` is how it is made.
    fn fold_const_expr(&mut self, e: &Expr, declared: &Type) -> Result<TypedExprKind, String> {
        match &e.kind {
            // The four leaves go through `check_expr`, so a const literal is typed by
            // exactly the code a `let` literal is typed by — including a Decimal taking
            // its scale from the annotation and `8.25%` meaning what it means. Duplicating
            // that here is how the two would drift.
            ExprKind::IntLit(_)
            | ExprKind::DecimalLit { .. }
            | ExprKind::BoolLit(_)
            | ExprKind::StrLit(_) => Ok(self.check_expr(e, Some(declared))?.kind),

            ExprKind::Var(name) => match self.consts.get(name) {
                Some((_, value)) => Ok(value.clone()),
                None => {
                    self.blame(e.span);
                    Err(format!(
                        "`{}` cannot be used in a `const`: an initializer may hold literals \
                         and consts declared ABOVE it, and nothing else. A `const` is folded \
                         at compile time, so there is no run-time value for it to read.",
                        name
                    ))
                }
            },

            ExprKind::Neg(inner) => {
                let folded = self.fold_const_expr(inner, declared)?;
                match folded {
                    TypedExprKind::IntLit(n) => Ok(TypedExprKind::IntLit(
                        n.checked_neg().ok_or_else(|| self.const_overflow("negating"))?,
                    )),
                    // A negated Decimal literal, which stage-0 already folds for `let`.
                    TypedExprKind::DecimalLit { unscaled } => Ok(TypedExprKind::DecimalLit {
                        unscaled: unscaled
                            .checked_neg()
                            .ok_or_else(|| self.const_overflow("negating"))?,
                    }),
                    other => {
                        self.blame(e.span);
                        Err(format!(
                            "`-` needs a number, and this is {}",
                            self.const_kind_name(&other)
                        ))
                    }
                }
            }

            ExprKind::Binary { op, lhs, rhs } => {
                // `/` is not part of the const grammar, and NOT because folding it is hard —
                // it is because `/` on two Ints is refused everywhere in Burxt: one operator
                // cannot say whether it rounds toward zero or down. That rule does not stop
                // applying because the operands are known at compile time, so the refusal is
                // delegated to `check_expr` rather than reworded here. A const `/` gets the
                // same sentence a `let` gets, which names `divide_floor` and
                // `divide_toward_zero` — the two functions the author actually needs.
                //
                // This was found by MEASURING: the first version of this evaluator folded `/`
                // with `checked_div` and even had its own division-by-zero refusal, which made
                // `const HALF: Int = LIMIT / 2;` legal in a language where `let half: Int = n / 2;`
                // is not. Two rules for one operator, decided by where it was written.
                if matches!(op, BinOp::Div) {
                    self.check_expr(e, Some(declared))?;
                }
                let left = self.fold_const_expr(lhs, declared)?;
                let right = self.fold_const_expr(rhs, declared)?;
                let (TypedExprKind::IntLit(a), TypedExprKind::IntLit(b)) = (&left, &right) else {
                    self.blame(e.span);
                    return Err(format!(
                        "arithmetic in a `const` is `+ - *` over Ints, and this is {} {} {}. A \
                         Decimal `*` narrows and so would need a rounding contract, and `+` on \
                         Strings means allocate — neither belongs in a value folded at compile \
                         time. Write the literal, or compute it in a function.",
                        self.const_kind_name(&left),
                        op,
                        self.const_kind_name(&right)
                    ));
                };
                let (a, b) = (*a, *b);
                let answer = match op {
                    BinOp::Add => a.checked_add(b),
                    BinOp::Sub => a.checked_sub(b),
                    BinOp::Mul => a.checked_mul(b),
                    // Unreachable: `check_expr` above already returned the "which rounding did
                    // you mean" refusal. Answering `None` rather than panicking, so a future
                    // relaxation of Int `/` shows up as an overflow message rather than a crash.
                    BinOp::Div => None,
                };
                match answer {
                    Some(n) => Ok(TypedExprKind::IntLit(n)),
                    None => {
                        self.blame(e.span);
                        Err(self.const_overflow(match op {
                            BinOp::Add => "adding",
                            BinOp::Sub => "subtracting",
                            BinOp::Mul => "multiplying",
                            BinOp::Div => "dividing",
                        }))
                    }
                }
            }

            // Everything else: a call, a comparison, `&&`, an index, a class literal, an
            // interpolated String. Refused by one message rather than a dozen, and the
            // message names the rule instead of the shape — a reader who wrote
            // `const N: Int = len(xs);` needs to know that consts are folded, not that
            // `Call` is an unsupported node kind.
            _ => {
                self.blame(e.span);
                Err("a `const` initializer is folded at compile time, so it may only hold \
                     literals, consts declared above it, and `+ - *` over Ints. This is \
                     none of those — compute it in a function, or bind it with `let`."
                    .to_string())
            }
        }
    }

    /// The overflow message, written once. Named after the operation because "arithmetic
    /// overflow" in a constant is otherwise a hunt through the expression.
    fn const_overflow(&self, doing: &str) -> String {
        format!(
            "this `const` overflows an Int while {} — a constant is folded at compile time, \
             and folding cannot wrap: by the time the program runs there is no arithmetic \
             left to trap on. An Int holds -9223372036854775808 to 9223372036854775807.",
            doing
        )
    }

    /// What a folded value IS, for a message. Not `Type`, because the point of the sentence
    /// is which literal the author wrote.
    fn const_kind_name(&self, value: &TypedExprKind) -> &'static str {
        match value {
            TypedExprKind::IntLit(_) => "an Int",
            TypedExprKind::DecimalLit { .. } => "a Decimal",
            TypedExprKind::BoolLit(_) => "a Bool",
            TypedExprKind::StrLit(_) => "a String",
            _ => "not a literal",
        }
    }

    /// The message for a name that a `const` already owns, or None when it is free.
    ///
    /// A parameter or a `let` may not reuse a const's name. Burxt refuses shadowing
    /// everywhere else and this is the same rule: without it, a parameter would win the
    /// name inside one body and the const would be invisible there — the reader's eye
    /// would go to the declaration at the top of the file and read the wrong value.
    fn shadows_a_const(&self, name: &str) -> Option<String> {
        self.consts.get(name).map(|(ty, _)| {
            format!(
                "`{}` is already declared as a `const {}: {}` — Burxt does not shadow, and a \
                 const is in scope in every function. Use a different name here.",
                name, name, ty
            )
        })
    }

    fn check_program_inner(&mut self, prog: &Program) -> Result<TypedProgram, String> {
        // Pass -2: the consts, folded to literals before anything can read one.
        self.fold_consts(prog);
        // Pass -1: collect the generic ENUM declarations, then rewrite every concrete
        // application of one — `Option<Int>` — into the nominal type of its instantiation.
        // After this pass no rule below has to know that generics exist.
        for st in &prog.structs {
            if st.type_parameters.is_empty() {
                continue;
            }
            self.current_span.set(st.span);
            let mut seen: Vec<&str> = Vec::new();
            for p in &st.type_parameters {
                if seen.contains(&p.name.as_str()) {
                    return Err(format!(
                        "`{}` declares the type parameter `{}` twice",
                        st.name, p.name
                    ));
                }
                seen.push(&p.name);
            }
            self.generic_records.insert(
                st.name.clone(),
                (
                    st.type_parameters.clone(),
                    st.fields.iter().map(|f| (f.name.clone(), f.ty.clone())).collect(),
                ),
            );
        }
        for e in &prog.enums {
            if e.type_parameters.is_empty() {
                continue;
            }
            self.current_span.set(e.span);
            // The parser already refuses a duplicate; this is the same rule stated where
            // the declaration is registered, which is where a reader looks for it.
            let mut seen: Vec<&str> = Vec::new();
            for p in &e.type_parameters {
                if seen.contains(&p.name.as_str()) {
                    return Err(format!(
                        "`{}` declares the type parameter `{}` twice",
                        e.name, p.name
                    ));
                }
                seen.push(&p.name);
            }
            self.generic_enums.insert(
                e.name.clone(),
                (
                    e.type_parameters.clone(),
                    e.variants.iter().map(|v| (v.name.clone(), v.payload.clone())).collect(),
                ),
            );
        }
        for st in &prog.structs {
            self.declared_type_names.insert(st.name.clone());
        }
        for e in &prog.enums {
            self.declared_type_names.insert(e.name.clone());
        }
        for t in &prog.interfaces {
            // A GENERIC interface has no signature set until a use says what its arguments
            // are, exactly as a generic class has no layout — so it is collected here and
            // its instantiations are made on demand by `expand`. Roadmap A9.
            if !t.type_parameters.is_empty() {
                // The declaration pass below cannot make this check for a generic interface,
                // because it never registers one in `interfaces` — so it is made here, where
                // the collection happens, rather than left to a map insert that would silently
                // keep the last one.
                if self.generic_interfaces.contains_key(&t.name)
                    || self.interface_names.contains(&t.name)
                {
                    self.current_span.set(t.span);
                    return Err(format!("interface `{}` is defined twice", t.name));
                }
                self.generic_interfaces
                    .insert(t.name.clone(), (t.type_parameters.clone(), t.methods.clone()));
                self.declared_type_names.insert(t.name.clone());
                continue;
            }
            if self.generic_interfaces.contains_key(&t.name) {
                self.current_span.set(t.span);
                return Err(format!("interface `{}` is defined twice", t.name));
            }
            self.interface_names.insert(t.name.clone());
        }
        // The builtins that reach the world, registered as if they were externs that declared it.
        // One transitive rule then covers builtins, externs, functions and methods alike, rather
        // than a second rule for builtins that would have to be kept in step with the first.
        //
        // `print` is deliberately absent: it would be on almost every function, and an annotation
        // that appears on everything tells a reviewer nothing — the lesson `allocates` taught.
        // `argument`/`argument_count` read the command line, which is `input`.
        for (builtin, effect) in [
            ("read_file", Effect::Files),
            ("write_file", Effect::Files),
            ("write_bytes", Effect::Files),
            ("argument", Effect::Input),
            ("argument_count", Effect::Input),
        ] {
            self.fn_effects.insert(builtin.to_string(), vec![effect]);
        }
        let mut owned = prog.clone();
        self.expand_program(&mut owned)?;
        let prog = &owned;

        // Pass 0: hoist struct declarations, then validate them (field types
        // must exist; no struct may contain itself, directly or transitively).
        for s in &prog.structs {
            self.current_span.set(s.span);
            // A GENERIC record has no layout until a use says what its arguments are, so it
            // was collected in pass -1 rather than registered here. Its instantiations become
            // ordinary classes, made on demand.
            if !s.type_parameters.is_empty() {
                continue;
            }
            if self.structs.contains_key(&s.name) {
                return Err(format!("class `{}` is defined twice", s.name));
            }
            let mut fields = Vec::new();
            for f in &s.fields {
                if fields.iter().any(|(n, _)| n == &f.name) {
                    return Err(format!(
                        "class `{}` declares the field `{}` twice",
                        s.name, f.name
                    ));
                }
                if let Some(m) = f.marshal {
                    return Err(format!(
                        "class `{}`: field `{}` is marked `as {}`, but marshalling \
                         describes how a value crosses a FOREIGN boundary, not how \
                         it is stored. Drop the `as {}`.",
                        s.name, f.name, m, m
                    ));
                }
                fields.push((f.name.clone(), f.ty.clone()));
            }
            if !s.private_fields.is_empty() {
                self.private_fields.insert(s.name.clone(), s.private_fields.clone());
            }
            self.structs.insert(s.name.clone(), fields);
        }
        // Enum names must be known before any type is validated, exactly like
        // struct names — a payload or field may name an enum declared later.
        for e in &prog.enums {
            self.current_span.set(e.span);
            if self.enums.contains_key(&e.name) || self.structs.contains_key(&e.name) {
                return Err(format!("`{}` is declared twice", e.name));
            }
            if e.variants.is_empty() {
                return Err(format!(
                    "enum `{}` has no variants, so no value of it could ever exist",
                    e.name
                ));
            }
            let mut seen: Vec<&str> = Vec::new();
            for v in &e.variants {
                if seen.contains(&v.name.as_str()) {
                    return Err(format!(
                        "enum `{}` declares the variant `{}` twice",
                        e.name, v.name
                    ));
                }
                seen.push(&v.name);
            }
            // A GENERIC enum has no layout until a use says what its arguments are, so it
            // was collected in pass -1 rather than registered here.
            if !e.type_parameters.is_empty() {
                continue;
            }
            self.enums.insert(
                e.name.clone(),
                e.variants
                    .iter()
                    .map(|v| (v.name.clone(), v.payload.clone()))
                    .collect(),
            );
        }
        // Interfaces: signature sets only, hoisted so impls may precede them.
        //
        // The instantiations `expand` made come first, so that from here down there is ONE
        // table and no rule has to know whether a signature set was written or made. They
        // cannot collide with a declared name: `$` is not a character an identifier can hold.
        for (symbol, sigs) in self.interfaces_made.borrow().iter() {
            self.interfaces.insert(symbol.clone(), sigs.clone());
        }
        for t in &prog.interfaces {
            self.current_span.set(t.span);
            // A generic interface itself has no signature set — only its instantiations do,
            // and those were just merged in above. Mirrors the `type_parameters.is_empty()`
            // skip that generic classes and generic enums both make in the passes above.
            if !t.type_parameters.is_empty() {
                let mut seen: Vec<&str> = Vec::new();
                for signature in &t.methods {
                    if seen.contains(&signature.name.as_str()) {
                        return Err(format!(
                            "interface `{}` declares the method `{}` twice",
                            t.name, signature.name
                        ));
                    }
                    seen.push(&signature.name);
                }
                continue;
            }
            if self.interfaces.contains_key(&t.name) {
                return Err(format!("interface `{}` is defined twice", t.name));
            }
            if self.structs.contains_key(&t.name) {
                return Err(format!(
                    "`{}` is already a class — an interface cannot reuse the name",
                    t.name
                ));
            }
            let mut seen: Vec<&str> = Vec::new();
            for signature in &t.methods {
                if seen.contains(&signature.name.as_str()) {
                    return Err(format!(
                        "interface `{}` declares the method `{}` twice",
                        t.name, signature.name
                    ));
                }
                seen.push(&signature.name);
            }
            self.interfaces.insert(t.name.clone(), t.methods.clone());
        }
        // Validate the types inside the signatures only once every interface name
        // is known, so interfaces may reference each other in any order.
        for t in &prog.interfaces {
            self.current_span.set(t.span);
            for signature in &t.methods {
                for p in &signature.parameters {
                    // B17: the caret belongs on the type that is wrong, not on the declaration
                    // that contains it.
                    self.current_span.set(p.ty_span);
                    self.current_span.set(p.ty_span);
                    self.validate_type(&p.ty)?;
                }
                self.validate_type(&signature.ret)?;
            }
        }

        // Payloads are scalars only in this cut: an aggregate payload reopens
        // the recursive-size question (an enum containing itself is infinite),
        // which needs indirection and therefore M1.
        for e in &prog.enums {
            self.current_span.set(e.span);
            // A generic declaration's payload is a parameter, which is neither a scalar
            // nor an aggregate until a use says what it is. Its INSTANTIATIONS are checked
            // where they are made, in `expand`.
            if !e.type_parameters.is_empty() {
                continue;
            }
            for v in &e.variants {
                for (i, t) in v.payload.iter().enumerate() {
                    // **The same door every other type goes through, and this loop was not using
                    // it.** `Type::Named(_) => {}` below accepts any name at all, so
                    // `enum E { A(Nope) }` typechecked — and then `burxt run` on a program that
                    // MATCHES on it panicked in codegen with `no entry found for key`, because the
                    // layout pass looked up a type nobody declared. A checker that accepts a
                    // program the backend cannot compile is worse than one that refuses too much:
                    // the error arrives as a crash, with a Rust backtrace, naming a file the author
                    // has never opened.
                    //
                    // Struct fields have called `validate_type` since B17. Enum payloads never did,
                    // and the arms below are about LAYOUT — is this width finite — which is a
                    // different question that assumed the type existed.
                    self.validate_type(t)?;
                    match t {
                        Type::Int | Type::Bool | Type::String | Type::Decimal { .. } => {}
                        // An enum payload is fine when its width is FINITE, which is the rule this
                        // used to approximate by refusing every enum payload. What actually makes a
                        // width unbounded is a type containing ITSELF by value; recursion through a
                        // slice is a pointer and always terminates. See `embeds_by_value`.
                          // NOT gated on `is_enum(n)`, and that gate was a hole. The question is
                          // whether the width is FINITE, which does not depend on what KIND of type
                          // the payload is — a payload that is a CLASS embedding this enum makes it
                          // wider than itself exactly as an enum payload would:
                          //
                          //     enum E { V(F) }   class F { e: E }
                          //     class Node { next: Option<Node> }
                          //
                          // Both passed `burxt check` and then killed the compiler in
                          // `payload_cells`, which walks `Named` through fields and variants with no
                          // cycle guard. A stack overflow with no message is the one failure this
                          // language forbids, and it was the compiler doing it. Stage-1 refused both
                          // by name already, so this closes a divergence as well as a crash.
                          Type::Named(n)
                              if self.embeds_by_value(t, &e.name, &mut Vec::new()) =>
                        {
                            return Err(format!(
                                "`{}.{}` payload {} is `{}`, which contains `{}` by value — so \
                                 `{}` would have to be wider than itself. Carry it behind a slice \
                                 (`[{}]`) instead: a slice is a pointer, so the size is finite and \
                                 the recursion still works.",
                                e.name,
                                v.name,
                                i + 1,
                                n,
                                e.name,
                                e.name,
                                e.name
                            ))
                        }
                        // A RECORD or an ARRAY payload is allowed since v0.0.118. The question
                        // this rule deferred was "how wide is a variant when the widest one holds a
                        // record", and that is the same question `cells_of` already answers for a
                        // record: a layout is a count of cells, and an enum is one tag cell plus the
                        // widest payload. Nothing recursive is involved, which is why the enum case
                        // above is still refused and this one no longer is.
                        Type::Named(_) | Type::Array { .. } | Type::Slice(_) => {}
                        other => {
                            return Err(format!(
                                "`{}.{}` payload {} is {} {}, which has no layout here. A \
                                 variant carries a scalar, a String, a class or an array.",
                                e.name,
                                v.name,
                                i + 1,
                                other.article(),
                                other
                            ))
                        }
                    }
                }
            }
        }
        let enums: Vec<TypedEnum> = prog
            .enums
            .iter()
            .map(|e| TypedEnum {
                name: e.name.clone(),
                variants: e.variants.iter().map(|v| v.payload.clone()).collect(),
            })
            .collect();

        for s in &prog.structs {
            self.current_span.set(s.span);
            for f in &s.fields {
                // B17: same rule for a field's type.
                self.current_span.set(f.ty_span);
                self.validate_type(&f.ty)?;
            }
            self.check_struct_finite(&s.name, &mut Vec::new())?;
        }
        // The generic declarations are skipped: a class whose field is a type parameter has
        // no layout, and codegen only ever sees the instantiations, appended further down.
        let structs: Vec<TypedStruct> = prog
            .structs
            .iter()
            .filter(|s| s.type_parameters.is_empty())
            .map(|s| TypedStruct {
                name: s.name.clone(),
                fields: s.fields.iter().map(|f| f.ty.clone()).collect(),
            })
            .collect();

        // Pass 1: collect every signature, so order of definition never matters.
        self.declaring = true;
        let mut externs = Vec::new();
        for e in &prog.externs {
            self.current_span.set(e.span);
            self.check_extern(e)?;
            // Burxt code always sees CInt as Int; the width conversion is
            // codegen's job at the call site.
            // What Burxt code must pass, as opposed to what C receives:
            // - CInt and CDouble are C's widths; Burxt passes an Int.
            // - `Decimal<S> as scaled` keeps its exact Burxt type, scale and
            //   all, because the scale IS the contract.
            // A width joins CInt here, and that is the half of A7 a caller actually sees: Burxt
            // code passes and receives an ordinary Int, and the narrowing to `u8` or the widening
            // from `i32` happens at the call in codegen. So a width never becomes the type of a
            // Burxt expression — which is the same fact `validate_type` enforces from the other
            // side, and the reason nothing downstream needed an arm.
            let seen = |t: &Type| match t {
                Type::CInt | Type::CDouble | Type::Width { .. } => Type::Int,
                other => other.clone(),
            };
            let param_tys: Vec<Type> = e.parameters.iter().map(|p| seen(&p.ty)).collect();
            self.fns.insert(e.name.clone(), (param_tys, seen(&e.ret)));
            self.fn_param_names
                .insert(e.name.clone(), e.parameters.iter().map(|p| p.name.clone()).collect());
            self.extern_names.insert(e.name.clone());
            // The boundary is where effects have to be DECLARED: there is no body to reason
            // about, so whatever a C function reaches, only its declaration can say. An extern
            // declaring nothing is taken at its word, which is right for `strlen` and would be a
            // lie for `system` — so lib/ declares its own.
            if !e.touches.is_empty() {
                self.fn_effects.insert(e.name.clone(), e.touches.clone());
            }
            self.extern_parameters.insert(
                e.name.clone(),
                e.parameters.iter().map(|p| (p.ty.clone(), p.marshal)).collect(),
            );
            externs.push(TypedExtern {
                name: e.name.clone(),
                parameters: e.parameters.iter().map(|p| p.ty.clone()).collect(),
                ret: e.ret.clone(),
            });
        }
        for f in &prog.fns {
            self.current_span.set(f.span);
            // **A refusal here abandons the allocation probe**, which runs this whole pass
            // before a single body is read. That is not a reason to soften the refusal: it is why
            // `probe_truncated` exists, and why the rule that CONSUMES the inference stands down
            // instead. See the detection at `infer_allocates` and the stand-down below.
            //
            // The symptom, before that existed, was an error against a file the reader did not
            // write. `main` is a reserved name, since a Burxt program is its top-level statements:
            //
            //     use "string.bx";
            //     function main() -> Int { return 0; }
            //
            //     error: function `string_split` cannot return [String], because its storage
            //     lives in a region and would not outlive it.
            //      --> string.bx:246:48
            //
            // The reserved-name refusal was never shown — the false one REPLACED it. `string.bx`
            // is valid, the reader never called `string_split`, and renaming `main` to anything
            // else makes the whole thing vanish. Aimed, by construction, at the first name anyone
            // arriving from another language types.
            //
            // Six triggers were found by testing — reserved name, defined twice, unknown type,
            // `pure` + `touches`, and two more in the methods pass below — and the first fix
            // guarded five of them here. That was the wrong shape twice over: it missed the
            // methods pass, and it read as complete while twenty-six other refusal sites in this
            // function could do the same thing. A trigger nobody has written is not a trigger that
            // does not exist.
            if self.fns.contains_key(&f.name) {
                return Err(format!("function `{}` is defined twice", f.name));
            }
            if is_reserved_name(&f.name) {
                return Err(format!(
                    "`{}` is a name the language owns, so a program may not declare it",
                    f.name
                ));
            }
            for p in &f.parameters {
                // B17: the caret goes on the type that is wrong.
                self.current_span.set(p.ty_span);
                self.validate_type(&p.ty)?;
            }
            self.validate_type(&f.ret)?;
            // Returning an array would need array-valued expressions to be
            // bindable (`let a: [Int; 3] = f();`), which is the whole-array
            // copy question deferred with collections. Parameters are fine.
            if matches!(f.ret, Type::Array { .. }) {
                return Err(format!(
                    // B41. `a class HOLDING IT`, which is stage-1's wording and the clearer
                    // one: "return a class" alone reads as "return some other thing", and
                    // the advice is to wrap the array, not to abandon it.
                    "function `{}` cannot return an array yet — returning one needs \
                     whole-array binding, which arrives with collections. Return \
                     a class holding it, or fill an array the caller owns.",
                    f.name
                ));
            }
            // RULE 2 of escape checking: returning region data would let it outlive the
            // region the caller opened — UNLESS the function declared `allocates`, which
            // says the storage belongs to the CALLER's region and therefore outlives the
            // call by construction. That is the same argument M1a §2 made for a String,
            // and it applies to a growable array for exactly the same reason. The rule
            // predated `allocates` and never learned about it, which came out the first
            // time a standard-library function wanted to answer `[String]`.
            // `self.alloc_fns` rather than `f.allocates`: it is seeded with what the probe
            // worked out (M14), so a function that plainly builds its answer no longer has
            // to say so. Writing the word still works and is still verified.
            //
            // Not while PROBING: this rule is a consequence of not allocating, and the
            // probe is what decides that. Applied early it aborted the declaration pass
            // before a single body was read, so the probe found nothing and every function
            // that builds its own answer stayed refused — the inference silently did
            // nothing at all. The real pass applies it with the answer in hand.
            // `f.allocates` as well as `allocates_fn`, and the order is why: the declared word is
            // recorded into `alloc_fns` further down this same loop body, so reading only the set
            // asks about a function whose own declaration has not been filed yet. The word was
            // verified and then ignored — `f(xs: [Int]) -> [Int] allocates { return xs; }` was
            // refused for not allocating while saying that it did.
            if !self.probing
                && !self.probe_truncated
                && self.region_allocated(&f.ret)
                && !f.allocates
                && !self.allocates_fn(&f.name)
                && !self.relays_a_parameter(&f.name)
            {
                return Err(format!(
                    "function `{}` cannot return {}, because its storage lives in a region \
                     and would not outlive it. Fill an array the caller owns, or \
                     return a scalar summary.",
                    f.name, f.ret
                ));
            }
            // Returning an interface object stays refused. A `dyn` borrows its source
            // BINDING, which is a local — so the borrow dangles on return
            // whether or not a region is involved. Regions bound
            // region-allocated data's lifetime; they do not change what an interface
            // object points at.
            if matches!(f.ret, Type::Dyn(_)) {
                return Err(format!(
                    "function `{}` cannot return an interface object — it borrows the value it \
                     refers to, which would not outlive the call. Take one as a \
                     parameter instead.",
                    f.name
                ));
            }
            let param_tys = f.parameters.iter().map(|p| p.ty.clone()).collect();
            self.fns.insert(f.name.clone(), (param_tys, f.ret.clone()));
            self.fn_param_names
                .insert(f.name.clone(), f.parameters.iter().map(|p| p.name.clone()).collect());
            self.fn_writable
                .insert(f.name.clone(), f.parameters.iter().map(|p| p.writable).collect());
            if !f.type_parameters.is_empty() {
                self.generics.insert(f.name.clone(), f.type_parameters.clone());
            }
            if f.allocates {
                self.alloc_fns.insert(f.name.clone());
            }
            if !f.touches.is_empty() {
                self.fn_effects.insert(f.name.clone(), f.touches.clone());
            }
            if f.is_pure && !f.touches.is_empty() {
                return Err(format!(
                    "`pure function {}` cannot also `touches {}`: `pure` means the answer depends \
                     on the arguments and nothing else, which is the same thing as touching \
                     nothing. Drop one of the two.",
                    f.name,
                    f.touches.iter().map(|e| e.to_string()).collect::<Vec<_>>().join(", ")
                ));
            }
            if f.is_pure {
                self.pure_fns.insert(f.name.clone());
            }
        }

        // Collect the methods declared inside impl blocks alongside the
        // free-standing ones: an interface method is just a method that also counts
        // toward a contract, so it uses the SAME machinery.
        let mut all_methods: Vec<&MethodDef> = prog.methods.iter().collect();
        for im in &prog.impls {
            self.current_span.set(im.span);
            for m in &im.methods {
                all_methods.push(m);
            }
        }

        // Methods are namespaced by (receiver, name), so they never collide
        // with free functions and may be declared in any order.
        for m in all_methods.iter().copied() {
            // **First line, as in the functions pass above, and for a reason that cost a wrong
            // file in an error message.** `current_span` is sticky: it holds whatever was set
            // last. This pass only set it inside the generic-receiver branch and for parameter
            // types, so every other refusal here pointed wherever the FUNCTIONS pass had left it
            // — the last function declared, in whichever file that was.
            //
            // `pure function C.m` cannot also `touches files` therefore arrived carrying
            // `--> helper.bx:1:20`, a caret under an unrelated function in a file the reader did
            // not write. The sentence was right and the place was somebody else's.
            self.current_span.set(m.span);
            // A method on a GENERIC record is held back: its receiver has no layout until a
            // use says what the arguments are. One copy is registered per instantiation, in
            // the drain loop below, so `Stack<Int>` and `Stack<String>` get their own.
            if !m.receiver_arguments.is_empty() {
                self.current_span.set(m.span);
                // A4, refused BY NAME rather than by accident, and checked HERE because a
                // held-back method never reaches the registration below — the first version of
                // this rule sat there and could never fire, so it accepted every generic case in
                // silence.
                //
                // `pure` itself is no problem over a type parameter: purity is about what the
                // answer depends on, and a parameter changes nothing about that. What is unsettled
                // is which COPY owns the promise. `pure_methods` is keyed by `(receiver, name)`
                // and the receiver here is the mangled instantiation, so `Stack<Int>` and
                // `Stack<String>` would each register the same promise under a different key —
                // workable, but it means a call resolves purity against whichever copy the caller
                // named, and nothing today checks that all copies agree. That is a question about
                // monomorphised keys, not about `pure`.
                //
                // Named rather than blanket-refused, because a refusal that happens to cover a
                // case is how the `?` gap survived its whole life.
                if m.is_pure {
                    return Err(format!(
                        "`pure function (self: {}<{}>) {}` is not available yet: a method on a \
                         generic class is checked once per instantiation, so one `pure` promise \
                         would have to stand for every copy at once. `pure` on a method of a \
                         NON-generic class works today. This is a question about which copy owns \
                         the promise rather than about purity — see spec/1.0/ROADMAP-1.0.md A4.",
                        m.receiver,
                        m.receiver_arguments.join(", "),
                        m.name
                    ));
                }
                let Some((parameters, _)) = self.generic_records.get(&m.receiver) else {
                    return Err(format!(
                        "`self: {}<...>` names type parameters, and `{}` is not generic.",
                        m.receiver, m.receiver
                    ));
                };
                if parameters.len() != m.receiver_arguments.len() {
                    return Err(format!(
                        "`{}` is generic over {} parameter(s), and this receiver names {}.",
                        m.receiver,
                        parameters.len(),
                        m.receiver_arguments.len()
                    ));
                }
                self.generic_methods.push((*m).clone());
                continue;
            }
            if !self.structs.contains_key(&m.receiver) {
                return Err(format!(
                    "method `{}` is declared for unknown type `{}` — declare it \
                     with `class {} {{ ... }}`",
                    m.name, m.receiver, m.receiver
                ));
            }
            let key = (m.receiver.clone(), m.name.clone());
            if self.methods.contains_key(&key) {
                return Err(format!(
                    "`{}` already has a method named `{}`",
                    m.receiver, m.name
                ));
            }
            for p in &m.parameters {
                // B17: the caret goes on the type that is wrong.
                self.current_span.set(p.ty_span);
                self.validate_type(&p.ty)?;
            }
            self.validate_type(&m.ret)?;
            if matches!(m.ret, Type::Array { .. }) {
                return Err(format!(
                    "method `{}.{}` cannot return an array yet — the same limit \
                     as free functions.",
                    m.receiver, m.name
                ));
            }
            // B53. A free function returning an interface object is refused because the object
            // BORROWS the value behind it, and a method was not — the same spelling, the same
            // stated reason, two answers. Found while closing B47, by checking before copying the
            // free-function arm into stage-1: copying it would have closed one divergence by
            // opening another.
            //
            // **Ruled the conservative way, and the measurement is why it needed a ruling rather
            // than a fix.** The method version RUNS correctly, including across a call that reuses
            // the stack, because an aggregate parameter is `byval` — the copy lives in the CALLER's
            // frame and outlives the call. So stage-0 may well be over-strict on free functions
            // rather than unsound on methods, and one experiment is not a memory model.
            //
            // Between "relax a safety refusal on the strength of one run" and "refuse both", the
            // region model states its own direction: the failure is memory, never a dangling
            // pointer. Nothing in this repository returns a `dynamic` from anywhere, so making them
            // agree costs nothing today and costs a compatibility promise tomorrow.
            if matches!(m.ret, Type::Dyn(_)) {
                return Err(format!(
                    "method `{}.{}` cannot return an interface object — it borrows the value it \
                     refers to, which would not outlive the call. Return the concrete type, or a \
                     class holding it.",
                    m.receiver, m.name
                ));
            }
            let param_tys = m.parameters.iter().map(|p| p.ty.clone()).collect();
            if m.allocates {
                self.alloc_methods.insert(key.clone());
            }
            if m.private {
                self.private_methods.insert(key.clone());
            }
            if !m.touches.is_empty() {
                self.method_effects.insert(key.clone(), m.touches.clone());
            }
            // ---- A4: `pure` on a method -------------------------------------------------
            //
            // Two refusals belong on the DECLARATION, where the contradiction is visible without
            // reading the body. Both mirror a rule a pure free function already follows, and the
            // wording is deliberately the same shape: one marker, one reason, wherever it appears.
            if m.is_pure && m.receiver_mut {
                return Err(format!(
                    "`pure function (mutable self: {}) {}` cannot be both: `pure` means the \
                     answer depends on the arguments and nothing else, and `mutable self` says \
                     this call changes the receiver. Drop one of the two. (It matters more than it \
                     looks: a contract clause may call a `pure` method, so this would let a \
                     precondition rewrite the value it is checking.)",
                    m.receiver, m.name
                ));
            }
            if m.is_pure && !m.touches.is_empty() {
                return Err(format!(
                    "`pure function {}.{}` cannot also `touches {}`: `pure` means the answer \
                     depends on the arguments and nothing else, which is the same thing as \
                     touching nothing. Drop one of the two.",
                    m.receiver,
                    m.name,
                    m.touches.iter().map(|e| e.to_string()).collect::<Vec<_>>().join(", ")
                ));
            }
            if m.is_pure {
                self.pure_methods.insert(key.clone());
            }
            self.method_writable
                .insert(key.clone(), m.parameters.iter().map(|p| p.writable).collect());
            self.methods.insert(key, (m.receiver_mut, param_tys, m.ret.clone()));
        }

        // Impls: satisfaction must be EXACT — every interface method present, with
        // Pass 1 is over: every signature is registered and bodies come next.
        self.declaring = false;

        // matching receiver form and types. A partial or mismatched impl names
        // the offending method.
        for im in &prog.impls {
            self.current_span.set(im.span);
            self.check_impl(im)?;
            self.impls.insert((im.interface_name.clone(), im.type_name.clone()));
        }

        // Every method on a generic record whose instantiation the DECLARED types already
        // named — before any body is checked, because a body may call one.
        let mut methods: Vec<TypedMethod> = Vec::new();
        self.instantiate_record_methods(&mut methods)?;

        // Pass 2: check each function body.
        //
        // A GENERIC's body is checked here too, with its type parameters standing for
        // nothing — which is what catches misuse at the declaration rather than at every
        // call. An unbounded `T` can be stored, copied, passed and returned, and nothing
        // else; the error says so and names the bound that would allow more.
        let mut fns = Vec::new();
        for f in &prog.fns {
            self.current_span.set(f.span);
            // While a GENERIC's own body is checked, its parameters' bounds are what says
            // which operations are allowed. An instantiation has no parameters left, so
            // this is empty for every other function.
            self.param_bounds = f
                .type_parameters
                .iter()
                .map(|p| (p.name.clone(), p.bound.clone()))
                .collect();
            let checked = self.check_fn(f)?;
            self.param_bounds.clear();
            // The generic itself is CHECKED and never EMITTED: there is no layout for a
            // `T` until a caller says what it is. Its instantiations are added below.
            if f.type_parameters.is_empty() {
                fns.push(checked);
            }
        }
        for m in all_methods.iter().copied() {
            if !m.receiver_arguments.is_empty() {
                continue;             // held back; checked per instantiation below
            }
            methods.push(self.check_method(m)?);
        }

        // Pass 3: top-level statements (the implicit main).
        //
        // No owner: an allocation out here belongs to the program, which has no signature
        // to carry the answer. Without clearing it, a probing pass would credit whichever
        // function happened to be checked last.
        *self.probe_owner.borrow_mut() = (String::new(), String::new());
        self.current_receiver = None;
        // Same reason, for B25's half of the probe: out here there are no parameters, so a map
        // left over from the last function would attribute a top-level growth to it.
        self.current_writable_params.clear();
        self.current_self_writable = false;
        // Top-level code may reach anything. There is no signature here for a reviewer to read,
        // because the file itself is what they are reading — so nothing is hidden by allowing it,
        // and forbidding it would mean no program could do I/O at its entry point.
        self.allowed_effects =
            vec![Effect::Files, Effect::Commands, Effect::Clock, Effect::Input, Effect::Network, Effect::Model];
        self.effects_owner = String::new();
        let mut stmts = Vec::new();
        // Top-level statements recover exactly as a function body's do.
        for s in &prog.stmts {
            if stmts.last().is_some_and(stmt_returns) {
                self.current_span.set(s.span);
                self.record("unreachable statement: this code comes after a `return`");
                break;
            }
            match self.check_stmt(s) {
                Ok(kind) => stmts.push(TypedStmt::new(kind, s.span)),
                Err(message) => {
                    self.record(message);
                    self.recover_from(s);
                }
            }
        }

        // Pass 2b: one copy of each generic per `(generic, type arguments)` pair the
        // program actually reached. Checking an instantiation can discover more — a
        // generic calling a generic — so this drains to a fixpoint rather than iterating
        // a fixed list. See spec/1.0/M7-GENERICS.md Decision 4.
        let by_name: HashMap<&str, &FnDef> =
            prog.fns.iter().map(|f| (f.name.as_str(), f)).collect();
        let mut guard = 0usize;
        loop {
            self.instantiate_record_methods(&mut methods)?;
            let batch: Vec<(String, Vec<Type>)> = std::mem::take(&mut *self.wanted.borrow_mut());
            if batch.is_empty() && self.wanted_records.borrow().is_empty() {
                break;
            }
            guard += 1;
            if guard > 64 {
                // A generic that instantiates itself at a new type on every pass —
                // `fn f<T>(x: T) { f(wrap(x)); }` shaped — would never converge. Refused
                // with the reason rather than compiled until the machine gives up.
                return Err(
                    "this program instantiates generics without end: a generic reaches \
                     itself at a new type argument every time, so there is no finite set \
                     of copies to emit."
                        .to_string(),
                );
            }
            for (name, type_args) in batch {
                let generic = by_name
                    .get(name.as_str())
                    .ok_or_else(|| format!("codegen bug: no generic named `{}`", name))?;
                let parameters = self
                    .generics
                    .get(&name)
                    .ok_or_else(|| format!("codegen bug: `{}` is not generic", name))?;
                let map: HashMap<String, Type> =
                    parameters.iter().map(|p| p.name.clone()).zip(type_args.iter().cloned()).collect();
                let mut concrete = specialise(generic, &map, &mangle(&name, &type_args));
                // Set before the expansion for the reason `instantiate_record_methods` gives:
                // the expansion can refuse, and a refusal must not point at the last item
                // some earlier loop happened to touch.
                self.current_span.set(concrete.span);
                // Substituting can make a generic application concrete — `Option<T>`
                // becomes `Option<Int>` — so the instantiation is expanded again here.
                self.expand_fn_types(
                    &mut concrete.parameters,
                    &mut concrete.ret,
                    &mut concrete.body,
                )?;
                // The same validation an instantiated METHOD needs, and for the same reason:
                // this copy is made after the declaration pass, so `dynamic Step` in a generic
                // function's signature registers no vtable without it. See the note in
                // `instantiate_record_methods`.
                for p in &concrete.parameters {
                    // B17: the caret goes on the type that is wrong.
                    self.current_span.set(p.ty_span);
                    self.validate_type(&p.ty)?;
                }
                self.validate_type(&concrete.ret)?;
                // Registered under its mangled name so a recursive generic call inside
                // the body resolves, and so `allocates`/`pure` carry over.
                self.fn_writable.insert(
                    concrete.name.clone(),
                    concrete.parameters.iter().map(|p| p.writable).collect(),
                );
                let param_tys = concrete.parameters.iter().map(|p| p.ty.clone()).collect();
                self.fns
                    .insert(concrete.name.clone(), (param_tys, concrete.ret.clone()));
                if concrete.allocates {
                    self.alloc_fns.insert(concrete.name.clone());
                }
                if concrete.is_pure {
                    self.pure_fns.insert(concrete.name.clone());
                }
                fns.push(self.check_fn(&concrete)?);
            }
        }

        // A vtable is emitted only for impls of interfaces actually used as `dyn`
        // — if a type never becomes an interface object, it costs nothing.
        let mut vtables = Vec::new();
        for im in &prog.impls {
            self.current_span.set(im.span);
            if !self.dyn_interfaces.contains(&im.interface_name) {
                continue;
            }
            let sigs = &self.interfaces[&im.interface_name];
            vtables.push(TypedVTable {
                interface_name: im.interface_name.clone(),
                concrete: im.type_name.clone(),
                // trait-declaration order fixes each slot index
                slots: sigs.iter().map(|s| s.name.clone()).collect(),
            });
        }

        // Every instantiation of a generic enum, in the order it was first needed, so
        // codegen has a layout for each one.
        let mut enums = enums;
        enums.extend(self.made_order.borrow().iter().cloned());
        let mut structs = structs;
        structs.extend(self.made_record_order.borrow().iter().cloned());

        // A12 / M14 slice 3. LAST, once every body is checked and every table the escape
        // analysis reads is final — `alloc_fns`, `alloc_methods` and the relay sets are
        // filled by a fixpoint that runs before the real pass, and generic instantiations
        // are checked in the loop above, so asking any earlier would ask half the program.
        //
        // Nothing here can refuse: it only decides WHERE a release goes.
        let mut fns = fns;
        for f in &mut fns {
            let params: Vec<String> = f.parameters.iter().map(|(n, _)| n.clone()).collect();
            let body = std::mem::take(&mut f.body);
            f.body = self.place_releases(&params, body, true);
        }
        let mut methods = methods;
        for m in &mut methods {
            let mut params: Vec<String> = vec!["self".to_string()];
            params.extend(m.parameters.iter().map(|(n, _)| n.clone()));
            let body = std::mem::take(&mut m.body);
            m.body = self.place_releases(&params, body, true);
        }
        // The top level is not wrapped — there is nothing after it to release into, and
        // the process exit reclaims the arena whole. Its inner blocks are, which is where
        // the loop in §3 lives.
        let stmts = self.place_releases(&[], stmts, false);
        Ok(TypedProgram { structs, enums, externs, fns, methods, vtables, stmts })
    }

    /// An impl must satisfy its trait EXACTLY: every declared method present,
    /// same receiver form, same parameter types, same return type.
    fn check_impl(&self, im: &ImplBlock) -> Result<(), String> {
        // Every message below names the interface as the AUTHOR wrote it. After A9 the
        // stored name may be a mangled instantiation — `Mapper$Int` — and a reader who
        // wrote `Mapper<Int>` must never be shown the symbol the compiler made up. The
        // lookups a few lines down still use the real key; only the prose is translated.
        let shown_interface = self.shown_type_name(&im.interface_name);
        let sigs = self.interfaces.get(&im.interface_name).ok_or_else(|| {
            format!(
                "unknown interface `{}` — declare it with `interface {} {{ ... }}`",
                shown_interface, shown_interface
            )
        })?;
        if !self.structs.contains_key(&im.type_name) {
            return Err(format!(
                "`implement {} for {}`: unknown type `{}` — declare it with \
                 `class {} {{ ... }}`",
                shown_interface, im.type_name, im.type_name, im.type_name
            ));
        }
        if self.impls.contains(&(im.interface_name.clone(), im.type_name.clone())) {
            return Err(format!(
                "`{}` already implements `{}`",
                im.type_name, shown_interface
            ));
        }

        // `class X implements Y` — the class's OWN methods satisfy it, so conformance is
        // checked against the method table rather than against a list this block carries.
        // A class may also have methods the interface never mentions, which is normal and is
        // why the "not a method of the interface" complaint below does not apply here.
        if im.declared_on_class {
            for signature in sigs {
                let key = (im.type_name.clone(), signature.name.clone());
                let (receiver_mut, param_tys, ret) = self.methods.get(&key).cloned().ok_or_else(|| {
                    format!(
                        "`class {} implements {}` is missing the method `{}`. Every interface \
                         method must be implemented — Burxt has no default bodies.",
                        im.type_name, shown_interface, signature.name
                    )
                })?;
                if receiver_mut != signature.receiver_mut {
                    return Err(format!(
                        "in `class {} implements {}`, method `{}` declares `{}self` but the \
                         interface declares `{}self`.",
                        im.type_name,
                        shown_interface,
                        signature.name,
                        if receiver_mut { "mutable " } else { "" },
                        if signature.receiver_mut { "mutable " } else { "" }
                    ));
                }
                if param_tys.len() != signature.parameters.len() {
                    return Err(format!(
                        "in `class {} implements {}`, method `{}` takes {} parameter(s) but the \
                         interface declares {}.",
                        im.type_name,
                        shown_interface,
                        signature.name,
                        param_tys.len(),
                        signature.parameters.len()
                    ));
                }
                for (i, (have, want)) in param_tys.iter().zip(&signature.parameters).enumerate() {
                    if have != &want.ty {
                        return Err(format!(
                            "in `class {} implements {}`, method `{}` parameter {} is {} but the \
                             interface declares {}.",
                            im.type_name, shown_interface, signature.name, i + 1, have, want.ty
                        ));
                    }
                }
                if ret != signature.ret {
                    return Err(format!(
                        "in `class {} implements {}`, method `{}` returns {} but the interface \
                         declares {}.",
                        im.type_name, shown_interface, signature.name, ret, signature.ret
                    ));
                }
            }
            return Ok(());
        }

        // Every method in the block must belong to the interface...
        for m in &im.methods {
            if !sigs.iter().any(|s| s.name == m.name) {
                return Err(format!(
                    "`implement {} for {}` defines `{}`, which is not a method of \
                     `{}`. Its methods are: {}.",
                    shown_interface,
                    im.type_name,
                    m.name,
                    shown_interface,
                    sigs.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", ")
                ));
            }
            if m.receiver != im.type_name {
                return Err(format!(
                    "in `implement {} for {}`, method `{}` has receiver `self: {}` — \
                     it must be `self: {}`.",
                    shown_interface, im.type_name, m.name, m.receiver, im.type_name
                ));
            }
        }

        // ...and every interface method must be present, matching exactly.
        for signature in sigs {
            let found = im.methods.iter().find(|m| m.name == signature.name).ok_or_else(|| {
                format!(
                    "`implement {} for {}` is missing the method `{}`. Every interface \
                     method must be implemented — Burxt has no default bodies.",
                    shown_interface, im.type_name, signature.name
                )
            })?;
            if found.receiver_mut != signature.receiver_mut {
                return Err(format!(
                    "in `implement {} for {}`, method `{}` declares `{}self` but the \
                     interface declares `{}self`.",
                    shown_interface,
                    im.type_name,
                    signature.name,
                    if found.receiver_mut { "mutable " } else { "" },
                    if signature.receiver_mut { "mutable " } else { "" }
                ));
            }
            if found.parameters.len() != signature.parameters.len() {
                return Err(format!(
                    "in `implement {} for {}`, method `{}` takes {} parameter(s) but \
                     the interface declares {}.",
                    shown_interface,
                    im.type_name,
                    signature.name,
                    found.parameters.len(),
                    signature.parameters.len()
                ));
            }
            for (i, (fp, sp)) in found.parameters.iter().zip(&signature.parameters).enumerate() {
                if fp.ty != sp.ty {
                    return Err(format!(
                        "in `implement {} for {}`, method `{}` parameter {} is {} but \
                         the interface declares {}.",
                        shown_interface,
                        im.type_name,
                        signature.name,
                        i + 1,
                        fp.ty,
                        sp.ty
                    ));
                }
            }
            if found.ret != signature.ret {
                return Err(format!(
                    "in `implement {} for {}`, method `{}` returns {} but the interface \
                     declares {}.",
                    shown_interface, im.type_name, signature.name, found.ret, signature.ret
                ));
            }
        }
        Ok(())
    }

    /// Coerce a concrete struct value to an interface object where one is expected.
    /// Lives here rather than in `let` so that struct fields, call arguments
    /// and returns all coerce too — every site that knows its expected type.
    /// The source must be a plain variable: the fat pointer borrows its storage,
    /// and an expression has none.
    fn coerce_dyn(
        &self,
        interface_name: &str,
        e: &Expr,
    ) -> Result<TypedExpr, String> {
        let ExprKind::Var(var) = &e.kind else {
            return Err(format!(
                "a `dynamic {}` must come from a variable — an interface object borrows the \
                 storage of the value it refers to, and an expression has none.",
                self.shown_type_name(interface_name)
            ));
        };
        let (src_ty, _) = self
            .env
            .get(var)
            .ok_or_else(|| self.unknown_name(var))?
            .clone();
        let concrete = match &src_ty {
            Type::Named(c) if self.is_record(c) => c.clone(),
            Type::Dyn(_) => {
                return Err(format!(
                    "`{}` is already an interface object; re-borrowing one is deferred \
                     until Burxt tracks borrows.",
                    var
                ))
            }
            other => {
                return Err(format!(
                    "`{}` has type {}, which cannot be a `dynamic {}` — only a class \
                     that implements the interface can.",
                    var, other, interface_name
                ))
            }
        };
        if !self.impls.contains(&(interface_name.to_string(), concrete.clone())) {
            return Err(format!(
                "`{}` does not implement `{}` — add `implement {} for {} {{ ... }}`.",
                concrete,
                self.shown_type_name(interface_name),
                self.shown_type_name(interface_name),
                concrete
            ));
        }
        Ok(TypedExpr {
            ty: Type::Dyn(interface_name.to_string()),
            kind: TypedExprKind::DynCoerce {
                interface_name: interface_name.to_string(),
                concrete,
                var: var.clone(),
            },
        })
    }

    /// The place a mutation targets must bottom out in a `let mut` binding.
    /// How to make `name` writable — the advice half of every "declared immutable" message.
    ///
    /// One function because there are FIVE of those messages (a reassignment, a field, an element on
    /// two paths, and `push`/`truncate`), and four of them told a reader to write `let mutable` on a
    /// PARAMETER, which is impossible: there is no `let` to change. A message a reader trusts and
    /// cannot follow costs more than a short one, so the advice is computed in one place from the one
    /// fact that decides it — whether this name is a parameter.
    fn how_to_make_writable(&self, name: &str, ty: &Type) -> String {
        let shown = self.shown(ty);
        // The RECEIVER, which is neither a `let` nor a parameter and had the same defect as both:
        // it was told to write `let mutable self`, which is not a thing anyone can write. Found by
        // A11's sweep — stage-1 records parameters by DEPTH and `self` sits at that depth, so the
        // two compilers gave two wrong answers instead of one, which is how a divergence starts.
        if name == "self" {
            return format!(
                "It is the RECEIVER: declare the method `function (mutable self: {}) ...` to \
                 allow it.",
                shown
            );
        }
        if !self.current_params.contains(name) {
            return format!("Declare it `let mutable {}: {}` to allow it.", name, shown);
        }
        if crate::codegen::is_aggregate(ty) {
            return format!(
                "It is a PARAMETER, and a parameter is a copy unless the signature says otherwise: \
                 declare it `mutable {}: {}`, which also tells every caller that this call changes \
                 what they passed.",
                name, shown
            );
        }
        format!(
            "It is a PARAMETER, and {} {} is copied when it crosses — so changing it here could not \
             reach the caller anyway. Take a copy you own: `let mutable own: {} = {};`",
            ty.article(),
            shown,
            shown,
            name
        )
    }

    /// The argument for a `mutable` parameter has to be a place the caller may change.
    ///
    /// Separate from `require_mutable_place` because the message has to name the CALL: the reader is
    /// looking at `sort(xs)` and the reason is in `sort`'s signature, which is somewhere else.
    fn require_mutable_argument(&self, callee: &str, i: usize, e: &Expr) -> Result<(), String> {
        match &e.kind {
            ExprKind::Var(_) | ExprKind::Field { .. } | ExprKind::Index { .. } => {
                self.require_mutable_place(e).map_err(|why| {
                    format!(
                        "argument {} of `{}` is declared `mutable`, so this call can change it — {}",
                        i + 1,
                        callee,
                        why
                    )
                })
            }
            _ => Err(format!(
                "argument {} of `{}` is declared `mutable`, so the call changes what it is given \
                 — and this is not something that can be changed. Pass a `let mutable` binding, \
                 so the change has somewhere to land and a reader can see where.",
                i + 1,
                callee
            )),
        }
    }

    fn require_mutable_place(&self, e: &Expr) -> Result<(), String> {
        let mut cur = e;
        loop {
            match &cur.kind {
                ExprKind::Var(name) => {
                    let (ty, mutable) = self
                        .env
                        .get(name)
                        .ok_or_else(|| self.unknown_name(name))?;
                    if !*mutable {
                        return Err(format!(
                            "cannot modify `{}`: it was declared immutable. {}",
                            name,
                            self.how_to_make_writable(name, ty)
                        ));
                    }
                    return Ok(());
                }
                ExprKind::Field { base, .. } => cur = base,
                ExprKind::Index { base, .. } => cur = base,
                _ => {
                    return Err(
                        "this can only modify a variable, or a field or element of \
                         one — not a temporary value."
                            .to_string(),
                    )
                }
            }
        }
    }

    /// The binding an lvalue ultimately writes into: `g.items[0]` is a write to `g`.
    ///
    /// Every escape rule below is about a NAME — was it declared inside the open region or
    /// outside it — and three of the four ways to reach a place are spelled through a field
    /// or an index. Walking to the root is what lets one question answer all four.
    fn place_root<'a>(e: &'a Expr) -> Option<&'a str> {
        let mut cur = e;
        loop {
            match &cur.kind {
                ExprKind::Var(name) => return Some(name),
                ExprKind::Field { base, .. } | ExprKind::Index { base, .. } => cur = base,
                _ => return None,
            }
        }
    }

    /// The open region's name, when `name` was declared OUTSIDE it — otherwise `None`.
    ///
    /// The one question behind B20, B21 and the whole-name rule that came before them: region
    /// storage may land in a binding declared INSIDE the region, because it dies with the
    /// region, and never in one declared outside it, because that binding outlives the bytes.
    ///
    /// `None` when no region is open, which is the common case and the reason this is a
    /// separate function: the rule must fire because a name is *outside* the region, never
    /// merely because a region happens to be open — ~100 `push` sites across `tests/pass`
    /// depend on that distinction.
    fn declared_outside_open_region(&self, name: &str) -> Option<String> {
        match &self.current_region {
            Some(open) if !self.region_scope.contains(name) => Some(open.clone()),
            _ => None,
        }
    }

    /// Record that region storage lands in one of THIS body's `mutable` parameters.
    ///
    /// The B25 half of the probe, and it mirrors `has_region` exactly: a query that records
    /// while probing and does nothing in the real pass, so the fixpoint and the rule read the
    /// same fact from the same place. Called with the ROOT of a place — `dst`, `dst.items`,
    /// `self.lines` all record against `dst` or `self`.
    ///
    /// Transitivity comes through the call site rather than a second walk: `outer` passing its
    /// own `dst` to `inner`, whose parameter is already known to grow, records `outer` on the
    /// next round. One round per link, which is the round structure `infer_allocates` already
    /// has.
    fn record_param_growth(&self, root: &str) {
        if !self.probing {
            return;
        }
        let (receiver, name) = self.probe_owner.borrow().clone();
        if name.is_empty() {
            return;                   // the top level owns its storage; nothing to attribute
        }
        if root == "self" {
            if self.current_self_writable && !receiver.is_empty() {
                self.probe_grow_self.borrow_mut().insert((receiver, name));
            }
            return;
        }
        if let Some(i) = self.current_writable_params.get(root) {
            if receiver.is_empty() {
                self.probe_grow_params.borrow_mut().insert((name, *i));
            }
        }
    }

    /// Record that THIS body's `return` may hand back storage it was given. The B32 half of
    /// the probe, and it mirrors `record_param_growth`: it records while probing and does
    /// nothing in the real pass, so the fixpoint and the rule read one fact from one place.
    ///
    /// Gated on `may_be_region_storage`, and that gate is what keeps the property from
    /// over-refusing. A getter returning `self.count` hands back a COPY of an Int — nothing
    /// points anywhere afterwards — so recording it would taint every `let n: Int = c.count()`
    /// inside a region and refuse a shape that is everywhere. Only a value that can carry a
    /// pointer into a region can relay one.
    fn record_relay(&self, returned: &TypedExpr) {
        if !self.probing || !self.may_be_region_storage(&returned.ty) {
            return;
        }
        let (receiver, name) = self.probe_owner.borrow().clone();
        if name.is_empty() {
            return;                   // the top level has no signature to carry the answer
        }
        for source in self.relayed_sources(returned) {
            match (source, receiver.is_empty()) {
                // A free function has no receiver, so `self` cannot be its answer.
                (RelaySource::Receiver, false) => {
                    self.probe_relay_methods.borrow_mut().insert((
                        receiver.clone(),
                        name.clone(),
                        0,
                    ));
                }
                (RelaySource::Receiver, true) => {}
                (RelaySource::Parameter(i), true) => {
                    self.probe_relay_params.borrow_mut().insert((name.clone(), i));
                }
                (RelaySource::Parameter(i), false) => {
                    self.probe_relay_methods.borrow_mut().insert((
                        receiver.clone(),
                        name.clone(),
                        i + 1,
                    ));
                }
            }
        }
    }

    /// Which of this body's parameters the value could still be pointing at.
    ///
    /// The mirror of `expr_allocates`, and the two must stay opposites: that one asks which
    /// expressions BUILD storage, this one asks which hand back storage they were given, and
    /// a form is in exactly one of the two. So joining two Strings, `to_string`, `substring`
    /// and every other builder is deliberately absent — `return "hi " + who` makes a fresh
    /// String and relays nothing, and treating it as a relay would refuse most of `lib/`.
    ///
    /// Reaching THROUGH a value is a relay, because a field, an element and a payload are all
    /// pointers into the same storage: `return b.name` hands back `b`'s bytes.
    fn relayed_sources(&self, e: &TypedExpr) -> Vec<RelaySource> {
        let mut found = Vec::new();
        self.collect_relayed_sources(e, &mut found);
        found
    }

    fn collect_relayed_sources(&self, e: &TypedExpr, found: &mut Vec<RelaySource>) {
        match &e.kind {
            TypedExprKind::Var(name) => {
                if name == "self" {
                    found.push(RelaySource::Receiver);
                } else if let Some(i) = self.current_param_positions.get(name) {
                    found.push(RelaySource::Parameter(*i));
                } else if let Some(sources) = self.relay_aliases.get(name) {
                    // A `match` payload name. It is not a parameter, but it points into whatever
                    // the scrutinee pointed at — so it carries the scrutinee's sources. Without
                    // this the walk stopped here and the enclosing function was recorded as
                    // relaying nothing, which is how region storage came to be released while a
                    // returned value still pointed at it.
                    found.extend(sources.iter().copied());
                }
            }
            // Reaching into a value does not copy what it points at.
            TypedExprKind::Field { base, .. } => self.collect_relayed_sources(base, found),
            TypedExprKind::Index { base, index, .. }
            | TypedExprKind::SliceIndex { base, index } => {
                self.collect_relayed_sources(base, found);
                self.collect_relayed_sources(index, found);
            }
            // `?` hands back the payload of what it unwrapped, so it hands back its sources.
            TypedExprKind::Try { value, .. } => self.collect_relayed_sources(value, found),
            // An aggregate built here is fresh, but what it HOLDS is not: `return Box { name: s }`
            // gives the caller a Box pointing at `s`'s bytes.
            TypedExprKind::StructLit { fields, .. } => {
                for f in fields {
                    self.collect_relayed_sources(f, found);
                }
            }
            TypedExprKind::VariantLit { arguments, .. } => {
                for a in arguments {
                    self.collect_relayed_sources(a, found);
                }
            }
            TypedExprKind::ArrayLit(items) | TypedExprKind::SliceLit(items) => {
                for i in items {
                    self.collect_relayed_sources(i, found);
                }
            }
            // A relay through a relay. This is the link the fixpoint exists for: `pass2` is
            // only known to relay once `pass` is, which is one more round.
            TypedExprKind::Call { name, arguments } => {
                for (i, a) in arguments.iter().enumerate() {
                    if self.relay_params.contains(&(name.clone(), i)) {
                        self.collect_relayed_sources(a, found);
                    }
                }
            }
            TypedExprKind::MethodCall { receiver, method, base, arguments, .. } => {
                if self.relay_methods.contains(&(receiver.clone(), method.clone(), 0)) {
                    self.collect_relayed_sources(base, found);
                }
                for (i, a) in arguments.iter().enumerate() {
                    if self.relay_methods.contains(&(receiver.clone(), method.clone(), i + 1)) {
                        self.collect_relayed_sources(a, found);
                    }
                }
            }
            TypedExprKind::DynCall { interface_name, method, base, arguments, .. } => {
                if self.dyn_call_relays(interface_name, method, 0) {
                    self.collect_relayed_sources(base, found);
                }
                for (i, a) in arguments.iter().enumerate() {
                    if self.dyn_call_relays(interface_name, method, i + 1) {
                        self.collect_relayed_sources(a, found);
                    }
                }
            }
            _ => {}
        }
    }

    /// A callee's name as the programmer WROTE it.
    ///
    /// `mangle` gives a generic's instantiation a symbol like `add_one$Int`, and a reader who
    /// typed `add_one(xs, 11)` should not be told about a name they have never seen. A no-op for
    /// every ordinary function, since only `mangle` puts a `$` in a name.
    fn declared_name(name: &str) -> &str {
        name.split_once('$').map(|(declared, _)| declared).unwrap_or(name)
    }

    /// B25's refusal. B20's sentence with the CALL named as the cause instead of `push`,
    /// because that is what the reader is looking at — the growth is in a body somewhere else.
    fn growing_an_outer_binding(name: &str, open: &str, callee: &str) -> String {
        format!(
            "`{}` was declared outside `region {}`, so it cannot grow inside it — `{}` grows it, \
             and the bytes are released at the closing brace, leaving `{}` reading whatever the \
             region hands out next. Declare `{}` inside the region, or grow it outside it.",
            name, open, callee, name, name
        )
    }

    /// B21's refusal, one sentence for all three ways a value reaches an outer place.
    ///
    /// `part` is the word for what is being written — "field" or "element". The rest is the
    /// whole-name refusal's voice, because it is the same rule: the reader who has met one of
    /// these should not have to read the other to recognise it.
    fn assigning_into_outer_region(name: &str, open: &str, part: &str) -> String {
        format!(
            "`{}` was declared outside `region {}`, so its {} cannot be assigned a value built \
             inside it — the bytes are released at the closing brace and `{}` would read whatever \
             the region hands out next. Declare `{}` inside the region, or build the value \
             outside it.",
            name, open, part, name, name
        )
    }

    /// Does evaluating this expression produce region-allocated storage? Needed
    /// because a concatenated String lives in a region while a literal lives in
    /// .rodata, and the two share one type — so the type alone cannot say.
    /// `burxt explain memory` — what each function builds, from the same inference every rule uses.
    ///
    /// M14 §7's argument for this: the honest cost of inferring `allocates` is that the memory story
    /// leaves the source, and the answer is not to put the annotation back but to make the fact
    /// **queryable** — wanted occasionally rather than stated always.
    ///
    /// **What it reports today is WHETHER and WHAT, not WHERE.** §7's sketch has a third column —
    /// the destination block, "released per iteration" — and that column is per-block release, which
    /// is the other half of slice 3 and is not built. Printing a guess there would be worse than
    /// leaving it out, so the footer says what is missing rather than the table implying it is
    /// complete.
    pub fn memory_report(&self, prog: &Program, typed: &TypedProgram, source: &str) -> String {
        let lines = crate::diag::LineIndex::new(source);
        let mut out = String::new();
        let mut rows: Vec<(usize, String, Vec<String>)> = Vec::new();

        for (declared, checked) in prog.fns.iter().zip(typed.fns.iter()) {
            let line = lines.locate(declared.span.start).line;
            let causes = if self.allocates_fn(&checked.name) {
                let found = self.all_allocations(&checked.body);
                if found.is_empty() {
                    // Flagged by the fixpoint but nothing nameable in the body — a `dynamic` call is
                    // the usual reason. Say so rather than printing an empty cell.
                    vec!["allocates, through a call this report cannot name".to_string()]
                } else {
                    found
                }
            } else {
                Vec::new()
            };
            rows.push((line, format!("{}()", checked.name), causes));
        }
        for (declared, checked) in prog.methods.iter().zip(typed.methods.iter()) {
            let line = lines.locate(declared.span.start).line;
            let causes = if self.allocates_method(&checked.receiver, &checked.name) {
                let found = self.all_allocations(&checked.body);
                if found.is_empty() {
                    vec!["allocates, through a call this report cannot name".to_string()]
                } else {
                    found
                }
            } else {
                Vec::new()
            };
            rows.push((line, format!("{}.{}()", checked.receiver, checked.name), causes));
        }
        rows.sort_by_key(|(line, _, _)| *line);

        let widest = rows.iter().map(|(_, name, _)| name.len()).max().unwrap_or(0);
        for (line, name, causes) in &rows {
            if causes.is_empty() {
                out.push_str(&format!("{:>5}  {:<width$}  nothing\n", line, name, width = widest));
                continue;
            }
            out.push_str(&format!(
                "{:>5}  {:<width$}  {}\n",
                line,
                name,
                causes[0],
                width = widest
            ));
            for extra in &causes[1..] {
                out.push_str(&format!("{:>5}  {:<width$}  {}\n", "", "", extra, width = widest));
            }
        }
        if rows.is_empty() {
            out.push_str("this program declares no functions\n");
        }
        out.push_str(
            "\nwhether and what, from the same inference `allocates` is derived from. WHERE it \
             lands\nis per-block release, which is not built — see spec/1.0/M14-IMPLICIT-REGIONS.md \
             slice 3.\n",
        );
        out
    }

    /// The first thing in a body that allocates, described in a few words.
    ///
    /// Exists so `allocates nothing` can say WHY rather than only that it is wrong. A refusal that
    /// names the offending call is a fix; one that says "it allocates somewhere" is a search.
    ///
    /// Best effort by design: it walks statements in order and answers the first cause it can name.
    /// It never decides WHETHER the claim is broken — `allocates_fn` does that, from the fixpoint —
    /// so a body this cannot describe still gets refused, just with a shorter message.
    fn first_allocation(&self, body: &[TypedStmt]) -> Option<String> {
        for stmt in body {
            if let Some(found) = self.first_allocation_in_stmt(stmt) {
                return Some(found);
            }
        }
        None
    }

    /// Every nameable allocation in a body, in source order, with duplicates collapsed.
    ///
    /// `burxt explain memory` wants all of them where `allocates nothing` wants the first. Same walk,
    /// asked a different question — so a body cannot be described one way by the refusal and another
    /// way by the report.
    pub fn all_allocations(&self, body: &[TypedStmt]) -> Vec<String> {
        let mut found = Vec::new();
        self.collect_allocations(body, &mut found);
        let mut seen: Vec<String> = Vec::new();
        for one in found {
            if !seen.contains(&one) {
                seen.push(one);
            }
        }
        seen
    }

    fn collect_allocations(&self, body: &[TypedStmt], into: &mut Vec<String>) {
        for stmt in body {
            // The single-statement walk already knows every shape; asking it per statement and then
            // recursing into blocks keeps one description of a body's structure rather than two.
            if let Some(found) = self.first_allocation_in_stmt(stmt) {
                into.push(found);
            }
            match &stmt.kind {
                TypedStmtKind::If { then_block, else_block, .. } => {
                    self.collect_allocations(then_block, into);
                    if let Some(other) = else_block {
                        self.collect_allocations(other, into);
                    }
                }
                TypedStmtKind::While { body, .. }
                | TypedStmtKind::Region { body, .. }
                | TypedStmtKind::Release { body }
                | TypedStmtKind::For { body, .. }
                | TypedStmtKind::ForRange { body, .. } => self.collect_allocations(body, into),
                TypedStmtKind::Match { arms, .. } => {
                    for arm in arms {
                        self.collect_allocations(&arm.body, into);
                    }
                }
                _ => {}
            }
        }
    }

    fn first_allocation_in_stmt(&self, stmt: &TypedStmt) -> Option<String> {
        let describe = |e: &TypedExpr| -> Option<String> { self.describe_allocation(e) };
        match &stmt.kind {
            TypedStmtKind::Let { value, .. }
            | TypedStmtKind::Assign { value, .. }
            | TypedStmtKind::AssignField { value, .. }
            | TypedStmtKind::ExprStmt(value)
            | TypedStmtKind::Return(value)
            | TypedStmtKind::Print { value, .. } => describe(value),
            TypedStmtKind::If { cond, then_block, else_block } => describe(cond)
                .or_else(|| self.first_allocation(then_block))
                .or_else(|| else_block.as_ref().and_then(|b| self.first_allocation(b))),
            TypedStmtKind::While { body, .. } => self.first_allocation(body),
            TypedStmtKind::Region { body, .. } | TypedStmtKind::Release { body } => {
                self.first_allocation(body)
            }
            TypedStmtKind::For { body, .. } => self.first_allocation(body),
            // The BOUNDS cannot allocate — both are Ints — so only the body is walked.
            TypedStmtKind::ForRange { body, .. } => self.first_allocation(body),
            TypedStmtKind::Match { arms, .. } => {
                arms.iter().find_map(|arm| self.first_allocation(&arm.body))
            }
            _ => None,
        }
    }

    /// Does a call through an interface object build its result in the caller's region?
    ///
    /// Yes if ANY implementation of the interface does. The call site cannot know which one
    /// runs — that is what `dynamic` means — so the conservative answer is the only sound one
    /// (M14 §10: a wrong guess costs memory, never correctness).
    ///
    /// This is ONE function because the fact was answered in two places and only one of them
    /// was right. The `no region open here` check at the `DynCall` site computed it inline;
    /// `expr_allocates` had no `DynCall` arm at all, and every escape rule asks
    /// `expr_allocates` — so a value built behind a vtable escaped its region through whole-name
    /// assignment, `return`, a field, an element, and one call away through a `mutable`
    /// parameter. B26, a live use-after-free: `h.tag = d.name()` printed `item AB` and then the
    /// next region's bytes. The `allocates nothing` fixture for the dynamic path is exactly why
    /// it looked covered — two rules asking the same question, and only one of them had been
    /// asked through a `dynamic`.
    fn dyn_call_allocates(&self, interface_name: &str, method: &str) -> bool {
        self.impls.iter().any(|(implemented, concrete)| {
            implemented == interface_name
                && self.alloc_methods.contains(&(concrete.clone(), method.to_string()))
        })
    }

    /// Does a call through an interface object hand back storage it was given?
    ///
    /// Yes if ANY implementation does, for the reason `dyn_call_allocates` gives: the call site
    /// cannot know which one runs. `source` is numbered as in `relay_methods` — `0` the
    /// receiver, `i + 1` argument `i`.
    fn dyn_call_relays(&self, interface_name: &str, method: &str, source: usize) -> bool {
        self.impls.iter().any(|(implemented, concrete)| {
            implemented == interface_name
                && self.relay_methods.contains(&(concrete.clone(), method.to_string(), source))
        })
    }

    /// Does a call through an interface object GROW its receiver — B25's question, asked of a
    /// vtable slot instead of a named class.
    ///
    /// The third member of the `dyn_call_allocates` / `dyn_call_relays` family and the same
    /// answer for the same reason: yes if ANY implementation does, because the call site cannot
    /// know which one runs. A11 is what makes it reachable — before it, no mutating method could
    /// be called through a `dynamic` at all, so `grow_self` had never been asked this way.
    fn dyn_call_grows_self(&self, interface_name: &str, method: &str) -> bool {
        self.impls.iter().any(|(implemented, concrete)| {
            implemented == interface_name
                && self.grow_self.contains(&(concrete.clone(), method.to_string()))
        })
    }

    /// A11. The rule for calling a `mutable self` method through an interface object, and the
    /// whole of what A11 adds.
    ///
    /// This used to be a flat refusal whose sentence said the compiler *"still cannot tell
    /// whether the value behind the object was declared mutable"*. It can: the fat pointer's
    /// data half IS the source binding's storage (`DynCoerce` in `codegen.rs` copies nothing),
    /// the coercion site knows whether that binding was `mutable`, and `dyn_source` remembers
    /// it. Nothing had to be added to the object, so the layout and the ABI are unchanged —
    /// which was worth measuring before widening a two-word value that `layout.bx` reports on.
    ///
    /// The rule is the CONCRETE one, unchanged and reused down to its wording: the receiver must
    /// be a variable, and that variable must be writable. The only thing the interface adds is a
    /// second name for the value, and the answer to that is to hand the caller back the name
    /// that owns the bytes — a growth inside a region has to be tested against `c`, not against
    /// the `it` that borrows it.
    ///
    /// **Aliasing, decided and recorded rather than deferred.** Two interface objects over one
    /// mutable value is legal here, and so is using the value's own name alongside them. It is
    /// sound for the reason a second name for one object is always sound in Burxt: there is no
    /// concurrency, a class instance never moves, and every path writes the same field slots the
    /// concrete call would write. What aliasing WOULD break is a rule that reasons about one
    /// name while the bytes belong to another — which is exactly the case `dyn_source` closes,
    /// and the reason it is not optional.
    fn dyn_mutating_receiver(
        &self,
        shown_receiver: &str,
        method: &str,
        base: &Expr,
    ) -> Result<String, String> {
        let ExprKind::Var(name) = &base.kind else {
            return Err(format!(
                "`{}` is a mutating method (`function (mutable self: {}) ...`); \
                 it can only be called on a variable, not an expression.",
                method, shown_receiver
            ));
        };
        let (ty, mutable) = self
            .env
            .get(name)
            .ok_or_else(|| format!("unknown variable: {}", name))?;
        if !*mutable {
            return Err(format!(
                "cannot call the mutating method `{}` on `{}`: it was declared immutable. {}",
                method,
                name,
                self.how_to_make_writable(name, ty)
            ));
        }
        Ok(self.dyn_source.get(name).cloned().unwrap_or_else(|| name.clone()))
    }

    /// A few words for the thing that allocates, or None when it cannot be named simply.
    fn describe_allocation(&self, e: &TypedExpr) -> Option<String> {
        // The `dynamic` arm comes BEFORE the guard. It no longer has to — `expr_allocates` has a
        // `DynCall` arm since B26 — but it still SHOULD: this walk names the first nameable cause
        // for a message about a missing region, and a dyn call is the one path hardest to find by
        // reading, so it is named whenever it is reached rather than only when the impls that are
        // in scope happen to allocate.
        //
        // Naming it is sound rather than a guess: this walk returns the FIRST nameable cause, so a
        // dyn call is only reached when nothing before it allocated — and the claim has to hold for
        // every implementation anyway, so the method is the actionable name even without knowing
        // which one broke it.
        if let TypedExprKind::DynCall { interface_name, method, .. } = &e.kind {
            return Some(format!(
                "`.{}(...)` through a `dynamic {}` allocates — and the claim has to hold for EVERY \
                 implementation, so one that allocates is enough to break it",
                method, interface_name
            ));
        }
        if !self.expr_allocates(e) {
            return None;
        }
        match &e.kind {
            TypedExprKind::Call { name, .. } => Some(format!(
                "`{}(...)` builds its answer in the caller's region",
                Self::shown_fn_name(name)
            )),
            TypedExprKind::MethodCall { receiver, method, .. } => Some(format!(
                "`{}.{}(...)` builds its answer in the caller's region",
                self.shown_type_name(receiver),
                method
            )),
            TypedExprKind::Binary { op: BinOp::Add, lhs, .. } if lhs.ty == Type::String => {
                Some("joining two Strings builds a new one".to_string())
            }
            TypedExprKind::Substring { .. } => Some("`substring(...)` builds a new String".to_string()),
            TypedExprKind::ByteAsString(_) => {
                Some("`byte_as_string(...)` builds a one-byte String".to_string())
            }
            TypedExprKind::ToString(_) => Some("`to_string(...)` builds a String".to_string()),
            TypedExprKind::ReadFile(_) => Some("`read_file(...)` builds a String".to_string()),
            TypedExprKind::CStringAt(_) => {
                Some("`c_string_at(...)` copies C's bytes into a String".to_string())
            }
            TypedExprKind::CBytesAt { .. } => {
                Some("`c_bytes_at(...)` copies C's bytes into an array".to_string())
            }
            TypedExprKind::SliceLit(_) => Some("a growable array is built here".to_string()),
            TypedExprKind::Push { .. } => Some("`push(...)` may grow the array".to_string()),
            _ => None,
        }
    }

    fn expr_allocates(&self, e: &TypedExpr) -> bool {
        match &e.kind {
            TypedExprKind::SliceLit(_)
            | TypedExprKind::Push { .. }
            | TypedExprKind::ReadFile(_)
            | TypedExprKind::CStringAt(_)
            | TypedExprKind::CBytesAt { .. }
            | TypedExprKind::ByteAsString(_)
            | TypedExprKind::Substring { .. } => true,
            // A call to an `allocates` function or method built its result in OUR
            // region, so it is region storage here and the same escape rules apply.
            //
            // B32: or it built nothing and handed back what it was GIVEN, which is region
            // storage here too if the argument was. `pass(built)` allocates nothing and points
            // straight into the open region, and that was the last way out of every rule below.
            // The property comes from `infer_allocates`; see `relay_params`.
            TypedExprKind::Call { name, arguments } => {
                self.alloc_fns.contains(name)
                    || arguments.iter().enumerate().any(|(i, a)| {
                        self.relay_params.contains(&(name.clone(), i)) && self.expr_allocates(a)
                    })
            }
            TypedExprKind::MethodCall { receiver, method, base, arguments, .. } => {
                self.alloc_methods.contains(&(receiver.clone(), method.clone()))
                    || (self.relay_methods.contains(&(receiver.clone(), method.clone(), 0))
                        && self.expr_allocates(base))
                    || arguments.iter().enumerate().any(|(i, a)| {
                        self.relay_methods.contains(&(receiver.clone(), method.clone(), i + 1))
                            && self.expr_allocates(a)
                    })
            }
            // B26. Same reason as the two arms above, for the call whose callee is not known
            // until run time: if any implementation allocates, the result HERE is region
            // storage and every escape rule below has to see it. See `dyn_call_allocates`.
            TypedExprKind::DynCall { interface_name, method, base, arguments, .. } => {
                self.dyn_call_allocates(interface_name, method)
                    || (self.dyn_call_relays(interface_name, method, 0)
                        && self.expr_allocates(base))
                    || arguments.iter().enumerate().any(|(i, a)| {
                        self.dyn_call_relays(interface_name, method, i + 1)
                            && self.expr_allocates(a)
                    })
            }
            // Becoming an interface object BORROWS the storage of the value it refers to —
            // the compiler says exactly that when it insists the source be a variable — so a
            // `dynamic Holder` is region storage precisely when the binding behind it is.
            //
            // **Without this arm the coercion LAUNDERED the taint, and the result was a live
            // use-after-free that both compilers' suites, the fixpoint and the 133-program
            // corpus all passed with open.** Measured on a pristine v0.0.276, with no generics
            // anywhere in the program:
            //
            //     region r {
            //         let b: Box = Box { s: "secret-" + "value" };
            //         let h: dynamic Holder = b;
            //         kept = h.get();          // accepted; prints garbage after the region
            //     }
            //
            // `kept = b.get()` on the same method is REFUSED, and so is `kept = b.s`. The only
            // difference is the hop through the interface object, and the taint died there:
            // `let h = b` asked this function about a `DynCoerce`, got the `_` arm's `false`,
            // and never put `h` in `region_locals`. Every rule downstream then asked a correctly
            // implemented question about a value it had been told was safe.
            //
            // So the DynCall arm above was never the gap — `dyn_call_allocates` fires correctly,
            // proven by the same program with an allocating method, which IS refused. This is
            // B33's and B34's shape a third time: a node that passes a value along and answers
            // `false` because nobody wrote its arm. The `_` at the bottom of this match is where
            // all three lived.
            TypedExprKind::DynCoerce { var, .. } => self.region_locals.contains(var),
            // B34. `?` yields the Ok payload of the value it unwraps, so the answer for the
            // unwrap is the answer for what was unwrapped: the taint has to pass THROUGH it.
            // Without this arm `let got: String = make(n)?;` inside a region laundered it, and
            // stage-0 accepted what stage-1 refuses — a verdict divergence with stage-0 as the
            // permissive one, which is the worse direction for the compiler that is the spec.
            TypedExprKind::Try { value, .. } => self.expr_allocates(value),
            // B33. `argument(n)` is COPIED into the region, with a length header, and
            // `codegen.rs:3831` explains why in capitals: `argv` holds C's strings, which have
            // no header, so handing one back directly would make `len` read whatever the loader
            // left in front of it. A copy in the region is region storage like any other, and
            // the comment at the `Arg` site claimed the opposite for as long as the copy has
            // existed — see there.
            TypedExprKind::Arg(_) => true,
            // Bool renders to a literal; the others allocate.
            TypedExprKind::ToString(v) => v.ty != Type::Bool,
            TypedExprKind::Binary { op: BinOp::Add, lhs, rhs }
                if lhs.ty == Type::String && rhs.ty == Type::String =>
            {
                true
            }
            TypedExprKind::Binary { lhs, rhs, .. } => {
                self.expr_allocates(lhs) || self.expr_allocates(rhs)
            }
            TypedExprKind::Neg(i) | TypedExprKind::Not(i) => self.expr_allocates(i),
            // An aggregate is only as safe as what it holds. Without these three, a
            // struct literal, an enum variant or an array could carry region storage
            // out of its region unnoticed — which is a use-after-free, and exactly
            // the silent-wrongness this language exists to refuse. Found by writing a
            // self-hosted checker and asking what its error type could be.
            TypedExprKind::StructLit { fields, .. } => {
                fields.iter().any(|f| self.expr_allocates(f))
            }
            TypedExprKind::VariantLit { arguments, .. } => {
                arguments.iter().any(|a| self.expr_allocates(a))
            }
            TypedExprKind::ArrayLit(items) => items.iter().any(|i| self.expr_allocates(i)),
            // A name bound to region storage IS region storage. Without this, `return s`
            // slipped past the rule that refuses `return "a" + "b"` — see `region_locals`.
            TypedExprKind::Var(name) => self.region_locals.contains(name),
            // B36. Reaching INTO region storage does not always come back with region
            // storage, and until v0.0.272 these three arms could not tell the difference:
            // they asked only whether the thing reached THROUGH held any, so
            // `total = b.n` and `total = made[0]` were refused for an `Int`.
            //
            // The narrowing asks the reached-for thing's own type. `b.n` yields an Int,
            // which is a COPY of a scalar and has nowhere to dangle from; `b.label`
            // yields a String, which is the same bytes and still cannot leave.
            //
            // Sound for an aggregate field too, and that took measuring rather than
            // reasoning: a class value is copied INLINE, nested fields included, so
            //
            //     class Inner { n: Int }   class Outer { inner: Inner, s: String }
            //     let mutable b: Outer = a;  b.inner.n = 99;  print(a.inner.n);   // 1
            //
            // — reading an all-scalar aggregate out of a region-built parent copies its
            // bytes out. Which is why `may_be_region_storage` is the right predicate here
            // and a flat "is it a scalar?" would refuse correct programs.
            //
            // **The whole-name spellings are untouched, and that is the line between a
            // narrowing and a hole.** `kept = made` and `kept = b` never reach these arms
            // — they are `Var`, caught one arm up by `region_locals` — so both are still
            // refused. Measured, not assumed: the fixtures are the four rows of B36.
            TypedExprKind::Field { base, .. } => {
                self.may_be_region_storage(&e.ty) && self.expr_allocates(base)
            }
            // B45. The INDEX is not asked at all, and gating it was not enough.
            //
            // `kept = made[idx()]` where `made` was built OUTSIDE the region was refused
            // because `idx()` allocates on its way to an Int. But what comes back from an
            // index is an element of the BASE; nothing the subscript computed is in it.
            // Keeping the term inside B36's gate only hid the false refusal behind a
            // scalar element type — with `[String]` it fired again. Dropped, which is what
            // the report asked for and what stage-1 already does.
            TypedExprKind::Index { base, .. } | TypedExprKind::SliceIndex { base, .. } => {
                self.may_be_region_storage(&e.ty) && self.expr_allocates(base)
            }
            _ => false,
        }
    }

    /// Does a value of this type have its storage in a region? Region-allocated
    /// values may not outlive their region, which is what the two rules below
    /// enforce. A struct is tainted by any region-allocated field; enum
    /// payloads are scalars, so an enum never is.
    fn region_allocated(&self, ty: &Type) -> bool {
        match ty {
            Type::Slice(_) => true,
            Type::Named(n) => self
                .structs
                .get(n)
                .map(|fs| fs.iter().any(|(_, t)| self.region_allocated(t)))
                .unwrap_or(false),
            Type::Array { elem, .. } => self.region_allocated(elem),
            _ => false,
        }
    }

    /// COULD a value of this type be region storage? A different question from
    /// `region_allocated`, which asks whether the type is ALWAYS in a region — a String
    /// may be a `.rodata` literal or a concatenation living in a region, and both are
    /// `String`, so `region_allocated` answers no for it and must keep doing so.
    ///
    /// This exists for B27, and only to keep the taint OFF names that cannot dangle. A
    /// `match` arm's binding takes its value from the enum that was matched on, so when
    /// that enum was built in the region the binding is region storage too — but a
    /// payload of type Int is a COPY of a scalar, and tainting that would refuse correct
    /// programs. False positives are as much a failure as false negatives here.
    ///
    /// A type parameter and a `dynamic` answer YES: neither says what the storage is, and
    /// M14 §10 is explicit that a wrong guess must cost memory rather than correctness.
    ///
    /// ### Two holes, in two arms, fixed together in v0.0.272 — B39 and B42
    ///
    /// A generic reaches this in one of two shapes depending on whether it has been
    /// monomorphised yet, and **each shape had its own way of answering "no" to a type
    /// that holds a String.** Fixing either alone leaves the other open and looks fixed:
    ///
    /// * `Named("Wrapper$Int")` — an INSTANTIATION, which lives in `made_records` /
    ///   `made_enums` and not in `structs` / `enums`. The old arm consulted only the
    ///   latter two and fell through to `false`. Found from A12: `lib/json.bx` printed a
    ///   truncated document because a block holding a `Result$Json$String` was released.
    ///
    /// * `Generic { name: "Wrapper", arguments: [Int] }` — the old arm asked only the
    ///   ARGUMENTS, and the arguments say nothing about a field that is a `String`
    ///   whatever `T` is:
    ///
    ///   ```text
    ///   class Wrapper<T> { t: T, note: String }
    ///   enum Holder<T> { Empty, Full(T) }
    ///   region r { let w: Wrapper<Int> = Wrapper { t: 1, note: "secret-" + "value" };
    ///              match Holder.Full(w) { Full(x) => { kept = x; } Empty => { } } }
    ///   print(kept.note);        // printed the NEXT region's bytes
    ///   ```
    ///
    ///   Every argument is an `Int`, so the payload was not tainted, so `kept = x` was
    ///   accepted. **This one was live** — stage-0 compiled and ran it.
    ///
    /// So the declaration is consulted too, with the arguments substituted in. Substituted
    /// rather than merely walked, because `Option<Int>` genuinely holds nothing: answering
    /// on the parameters alone would make every `Option` region storage and refuse correct
    /// programs, and a false positive is as much a failure here as a false negative.
    ///
    /// `seen` breaks the cycle a self-referential application would otherwise spin on —
    /// `List<Int>` whose payload is `List<Int>`. Answering `false` for a type already
    /// being asked about is right rather than merely terminating: the recursion returns to
    /// it only through a field, and that field's OTHER siblings are what decide.
    fn may_be_region_storage(&self, ty: &Type) -> bool {
        self.may_be_region_storage_within(ty, &mut Vec::new())
    }

    fn may_be_region_storage_within(&self, ty: &Type, seen: &mut Vec<String>) -> bool {
        match ty {
            // A `Handle<T>` is an i64 and it still counts, which is the arm somebody will be
            // tempted to make `false`. The INTEGER holds no pointer; the table ENTRY it names
            // holds one, into the region. Close the region the value was built in and the entry
            // dangles, so a handle escaping a region is the same use-after-free as a String
            // escaping one — reached one level of indirection later, where it is harder to see.
            Type::Handle(_) => true,
            Type::String | Type::Slice(_) => true,
            // A tuple, still written as one — inside a generic, before `expand` has turned
            // it into the anonymous class the `Named` arm below already answers for.
            //
            // **This arm is not dead and it is not defence in depth.** `(T, Int)` inside a
            // generic body reaches here with `T` a `Param`, which the `Param` arm answers
            // YES to; without this arm the `_` this replaces would have said NO, and B39's
            // whole lesson is that a wrong NO here is silent. The expanded case is the one
            // a fixture can reach today and it goes through `Named` -> `made_records`; this
            // is the one that goes quiet first when a rule starts asking earlier.
            Type::Tuple(elements) => {
                elements.iter().any(|t| self.may_be_region_storage_within(t, seen))
            }
            Type::Named(n) => {
                if seen.iter().any(|s| s == n) {
                    return false;
                }
                seen.push(n.clone());
                // `fields_of` and `variants_of` resolve an instantiation as readily as a
                // declaration, which is the whole of the first fix.
                let answer = match (self.fields_of(n), self.variants_of(n)) {
                    (Some(fields), _) => {
                        fields.iter().any(|(_, t)| self.may_be_region_storage_within(t, seen))
                    }
                    (None, Some(variants)) => variants.iter().any(|(_, payload)| {
                        payload.iter().any(|t| self.may_be_region_storage_within(t, seen))
                    }),
                    // Not a class, not an enum, not an instantiation of either.
                    // `validate_type` refuses such a name, so this is unreachable in a
                    // program that checks — and the answer for an unreachable case must
                    // still be the safe one, because "unresolvable" answering NO is
                    // exactly how the two holes above stayed invisible.
                    (None, None) => true,
                };
                seen.pop();
                answer
            }
            Type::Array { elem, .. } => self.may_be_region_storage_within(elem, seen),
            // The ARGUMENTS are deliberately not asked on their own. Substitution binds
            // them and the fields are read THROUGH them, so an argument that reaches
            // storage already shows up as a field — and an argument that reaches no field
            // is a PHANTOM parameter, where asking would condemn a type that stores
            // nothing. `class Tagged<T> { n: Int }` is one Int in the layout whatever `T`
            // is, and `Tagged<String>` must stay copyable out of a region. Stage-1 shipped
            // that false refusal in v0.0.272 and dropped the same loop in v0.0.273; this
            // arm is unreachable in stage-0 today, which is exactly why it would have sat
            // here wrong and unnoticed.
            Type::Generic { name, arguments } => {
                let key = format!("{}", ty);
                if seen.iter().any(|s| *s == key) {
                    return false;
                }
                seen.push(key);
                let answer = self.generic_body_holds_storage(name, arguments, seen);
                seen.pop();
                answer
            }
            // A `Dyn` is YES unconditionally and always has been: an interface object points
            // at some implementor's instance, the set of implementors is open, and any one of
            // them may hold a String. `DynGeneric` is the SAME answer for the same reason, and
            // it is here rather than folded into a `_` because B39/B42 were two arms of this
            // predicate both answering wrong for generic instantiations and it took three
            // agents to see.
            //
            // **`DynGeneric` is NOT reachable today, and that was measured rather than assumed.**
            // Flipping this arm to `false` and re-running changes no program's answer: by the
            // time the escape analysis asks, every `dynamic Mapper<Int>` has been through
            // `expand` and is a `Dyn`, which the same arm answers. The probe was a generic
            // function taking `dynamic Sink<T>` whose result escapes a region — refused
            // identically with the arm true and with it false.
            //
            // Kept anyway, and the tuple arm above states the reason in its own words: a wrong
            // NO here is silent, and "only reachable from one place today" is exactly what was
            // true of that arm before it wasn't. What a fixture CAN reach is the expanded path —
            // `tests/fail/a_generic_interface_escapes_its_region.bx`, which pins that a generic
            // interface is region storage and refuses byte-identically to the non-generic
            // interface it copies.
            Type::Param(_) | Type::Dyn(_) | Type::DynGeneric { .. } => true,
            // The scalars, listed rather than left to a `_` arm. A type this predicate
            // has never heard of must not inherit "holds nothing" in silence; that is
            // the same failure as the two arms above, one milestone later.
            Type::Int
            | Type::Bool
            | Type::CInt
            | Type::Width { .. }
            | Type::CPointer
            | Type::CDouble
            | Type::Decimal { .. } => false,
        }
    }

    /// The fields (or variant payloads) a generic declares, with this application's
    /// arguments put in for its parameters. `class Wrapper<T> { t: T, note: String }`
    /// applied to `Int` holds an `Int` and a `String`, and it is the second that matters.
    fn generic_body_holds_storage(
        &self,
        name: &str,
        arguments: &[Type],
        seen: &mut Vec<String>,
    ) -> bool {
        if let Some((parameters, fields)) = self.generic_records.get(name) {
            let map = Self::argument_map(parameters, arguments);
            return fields
                .iter()
                .any(|(_, t)| self.may_be_region_storage_within(&substitute(t, &map), seen));
        }
        if let Some((parameters, variants)) = self.generic_enums.get(name) {
            let map = Self::argument_map(parameters, arguments);
            return variants.iter().any(|(_, payload)| {
                payload
                    .iter()
                    .any(|t| self.may_be_region_storage_within(&substitute(t, &map), seen))
            });
        }
        // An application of something this checker has no declaration for. It is refused
        // elsewhere; here the honest answer is the conservative one.
        true
    }

    fn argument_map(parameters: &[TypeParam], arguments: &[Type]) -> HashMap<String, Type> {
        parameters
            .iter()
            .map(|p| p.name.clone())
            .zip(arguments.iter().cloned())
            .collect()
    }

    /// A Named type must refer to a declared struct; CInt never leaves the
    /// C boundary. `dyn Trait` must name a declared trait — and using one
    /// classes that the interface needs vtables.
    fn validate_type(&mut self, ty: &Type) -> Result<(), String> {
        // C2. Before anything else, and before the lookups below, because a dependency's private
        // class RESOLVES perfectly well — it is in the table, it is real, and it is simply not
        // ours to name. Every type a program writes down comes through here: parameters, `let`
        // bindings, fields and returns. One place rather than four, which is what B47 and B7's
        // method hole both cost a version for getting wrong.
        let named = match ty {
            Type::Named(name) => Some(name),
            Type::Dyn(name) => Some(name),
            Type::Generic { name, .. } => Some(name),
            _ => None,
        };
        if let Some(name) = named {
            if let Some(why) = self.refuse_if_package_private(name) {
                return Err(why);
            }
        }
        if let Type::Dyn(name) = ty {
            if !self.interfaces.contains_key(name) {
                return Err(format!(
                    "unknown interface `{}` — declare it with `interface {} {{ ... }}`",
                    name, name
                ));
            }
            self.dyn_interfaces.insert(name.clone());
            return Ok(());
        }
        match ty {
            Type::Named(name)
                if !self.is_record(name) && !self.is_enum(name) =>
            {
                Err(self.refuse_if_package_private(name).unwrap_or_else(|| format!(
                    "unknown type `{}` — declare it with `class {} {{ ... }}` or \
                     `enum {} {{ ... }}`",
                    name, name, name
                )))
            }
            Type::CInt => Err(
                "CInt only exists at the C boundary (external function signatures) — \
                 use Int in Burxt code; values convert at the call."
                    .to_string(),
            ),
            // A width is boundary-only, and THIS ARM IS WHAT MAKES THAT TRUE. `validate_type` runs
            // on every `let`, parameter, return and field; an `external function` signature is
            // checked by `check_extern`'s own allowlist instead, which is the only path that does
            // not come through here. So one refusal in one place buys the whole rule — and it is
            // also why `layout.bx`, `layout_of`, `review` and the language server need no arm: a
            // width can never be the type of anything they walk.
            //
            // `CInt` has two SPECIALISED copies of this message further down, for a slice element
            // and an array element. A width deliberately has neither: the element arms fall through
            // to `other => self.validate_type(...)`, which recurses back here and gives one wording
            // for all five positions. That is not tidiness — stage-1 refuses a width in the PARSER,
            // where there is no way to know whether the type being read is an element, so a
            // specialised message here would be a message the two compilers could not both produce.
            Type::Width { .. } => Err(format!(
                "`{}` only exists at the C boundary (external function signatures) — \
                 use Int in Burxt code; values convert at the call.",
                ty
            )),
            // Elements may be scalars OR aggregates: a `[Node; 256]` is
            // stack-allocatable, which is what makes an arena-style AST
            // (children referenced by index, never by pointer) possible without
            // any heap. Refused: nested arrays, because `a[i][j]` cannot be
            // written — indexing takes a binding name, not an expression — and
            // interface objects, because they borrow and storing a borrow needs
            // tracking Burxt does not have.
            Type::Slice(elem) => match elem.as_ref() {
                Type::Slice(_) | Type::Array { .. } => Err(
                    "a growable array cannot hold another array yet — its element \
                     would need its own region reasoning. Use a class element."
                        .to_string(),
                ),
                Type::Dyn(t) => Err(format!(
                    "a growable array cannot hold `dynamic {}` yet — region-allocated \
                     interface objects arrive in a later slice.",
                    t
                )),
                Type::CInt => Err(
                    "CInt only exists at the C boundary — use Int for elements"
                        .to_string(),
                ),
                other => self.validate_type(&other.clone()),
            },
            Type::Array { elem, .. } => match elem.as_ref() {
                Type::Array { .. } => Err(
                    "arrays of arrays are not available yet — `a[i][j]` cannot be \
                     written, since indexing applies to a binding rather than an \
                     expression. Use one array of a class instead."
                        .to_string(),
                ),
                Type::Dyn(t) => Err(format!(
                    "an array cannot hold `dynamic {}` — an interface object borrows the value \
                     it refers to, and storing borrows needs tracking Burxt does not \
                     have yet.",
                    t
                )),
                Type::CInt => Err(
                    "CInt only exists at the C boundary — use Int for array elements"
                        .to_string(),
                ),
                other => self.validate_type(&other.clone()),
            },
            _ => Ok(()),
        }
    }

    /// No struct may contain itself, directly or through other structs —
    /// it would have no finite size.
    fn check_struct_finite(&self, name: &str, trail: &mut Vec<String>) -> Result<(), String> {
        if trail.iter().any(|t| t == name) {
            return Err(format!(
                "a `{}` cannot contain a `{}` — it would have no finite size \
                 (containment cycle: {} -> {}). Hold it behind a slice (`[{}]`) instead: \
                 a slice is a pointer, so the size is finite and the recursion still works.",
                as_written(&trail[0]),
                as_written(&trail[0]),
                trail.iter().map(|t| as_written(t)).collect::<Vec<_>>().join(" -> "),
                as_written(name),
                as_written(&trail[0])
            ));
        }
        trail.push(name.to_string());
        if let Some(fields) = self.fields_of(name) {
            for (_, ty) in fields {
                self.follow_finite(&ty, trail)?;
            }
        } else if let Some(variants) = self.variants_of(name) {
            // An ENUM reached from a struct field is the same containment question, and
            // leaving it out was a hole:
            //
            //     class Node { label: String, next: Option<Node> }
            //
            // passed `check` and then overflowed the compiler's own stack in `payload_cells`.
            // `Option<Node>` is monomorphised to a `Named` before it gets here, so the walk
            // only had to be willing to step through variants as well as fields.
            for (_, payload) in variants {
                for ty in &payload {
                    self.follow_finite(ty, trail)?;
                }
            }
        }
        trail.pop();
        Ok(())
    }

    /// Follow one field or payload for the finiteness walk.
    ///
    /// **A slice ends it, and that is the whole reason the rule is usable.** `[Node]` is a
    /// pointer, a length and a capacity whatever it points at, so `class Node { kids: [Node] }`
    /// and `class Node { kids: Map<String, Node> }` are both finite — both compile today, and
    /// refusing either would be worse than the crash this fixes. An ARRAY is N copies by value,
    /// so it does not end the walk. Same distinction `embeds_by_value` draws for enum payloads.
    fn follow_finite(&self, ty: &Type, trail: &mut Vec<String>) -> Result<(), String> {
        match ty {
            Type::Named(inner) => self.check_struct_finite(inner, trail),
            Type::Array { elem, .. } => self.follow_finite(elem, trail),
            _ => Ok(()),
        }
    }

    /// Render a struct's fields as `name: Type, ...` for error messages.
    /// B28. The source spelling of a type, for a message a person reads.
    ///
    /// An instantiation is keyed internally by a MANGLED symbol — `Holder$Int` — and that
    /// symbol leaked into eight diagnostics. A reader never wrote `$`, cannot search their
    /// file for it, and has no way to learn that it means `<`: it is a compiler-internal
    /// key appearing where a source spelling belongs. `show` already knows the way back,
    /// through `instance_of`; these two wrappers are just the places that have to ask.
    fn shown(&self, ty: &Type) -> String {
        show(ty, &self.instance_of.borrow())
    }

    /// The name the anonymous class behind a tuple is filed under: the tuple as it was
    /// written, `(Int, String)`.
    ///
    /// Built from `shown` rather than `Display`, so an element that is itself a generic
    /// instantiation comes back as `Wrapper<Int>` and not `Wrapper$Int` — the mangled
    /// spelling would otherwise leak into the tuple's name and out of it into every message
    /// that prints one.
    fn tuple_symbol(&self, elements: &[Type]) -> String {
        let inner: Vec<String> = elements.iter().map(|e| self.shown(e)).collect();
        format!("({})", inner.join(", "))
    }

    /// Is this class one the reader wrote, or the anonymous one behind a tuple? Asked by
    /// name, because "field" and "position" are different words and a message that uses the
    /// wrong one sends the reader looking for a declaration that does not exist.
    fn is_tuple_symbol(name: &str) -> bool {
        name.starts_with('(')
    }

    fn shown_type_name(&self, symbol: &str) -> String {
        show(&Type::Named(symbol.to_string()), &self.instance_of.borrow())
    }

    /// The same, for a FUNCTION: `grow$Int` is one instantiation of `grow`, and `grow` is
    /// the name in the file. No arguments are added back — the reader wrote none, and the
    /// message is about the function they wrote.
    fn shown_fn_name(name: &str) -> &str {
        match name.split_once('$') {
            Some((declared, _)) => declared,
            None => name,
        }
    }

    fn field_list(&self, name: &str) -> String {
        // `fields_of`, not `structs`, or an INSTANTIATION lists no fields at all —
        // `Holder$Int has no field named nope. Its fields are: .` was the measured output,
        // and the empty list is the half of that message that was supposed to help.
        self.fields_of(name)
            .map(|fs| {
                fs.iter()
                    .map(|(n, t)| format!("{}: {}", n, self.shown(t)))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default()
    }

    /// The FFI contract: Int and String cross the C boundary as parameters
    /// (String passes a borrowed, read-only `const char*`). C has no Decimal —
    /// passing the raw scaled integer would silently shed its scale and
    /// rounding contract, the exact meaning-loss Burxt exists to refuse.
    /// Returns stay Int-only: Burxt cannot yet track who owns memory a C
    /// function returns.
    fn check_extern(&self, e: &ExternFn) -> Result<(), String> {
        const RESERVED: [&str; 6] = ["printf", "fprintf", "fputs", "exit", "stderr", "main"];
        if e.name == "len" || e.name == "byte_at" || e.name == "push" || e.name == "read_file" || e.name == "to_string" || e.name == "old" || e.name == "substring" || e.name == "truncate" || e.name == "write_file" || e.name == "argument" || e.name == "argument_count" || e.name == "divide_floor" || e.name == "divide_toward_zero" || e.name == "remainder" || e.name == "write_bytes" || e.name == "hash" || e.name == "byte_as_string" {
            return Err(format!("the name `{}` is reserved for a built-in", e.name));
        }
        if RESERVED.contains(&e.name.as_str()) {
            return Err(format!(
                "external function `{}`: this symbol is used by the Burxt runtime itself. \
                 Call it through a differently-named C wrapper.",
                e.name
            ));
        }
        // An `extern fn` declares an external FACT, not a definition, so two modules may
        // both declare it — `use "lib/fs.bx"` and `use "lib/os.bx"` both need `system`,
        // and neither can know the other did. Identical signatures are harmless; a
        // MISMATCH is not, because then the program holds two beliefs about one symbol.
        if let Some((parameters, ret)) = self.fns.get(&e.name) {
            // A width joins CInt here, and that is the half of A7 a caller actually sees: Burxt
            // code passes and receives an ordinary Int, and the narrowing to `u8` or the widening
            // from `i32` happens at the call in codegen. So a width never becomes the type of a
            // Burxt expression — which is the same fact `validate_type` enforces from the other
            // side, and the reason nothing downstream needed an arm.
            let seen = |t: &Type| match t {
                Type::CInt | Type::CDouble | Type::Width { .. } => Type::Int,
                other => other.clone(),
            };
            let mine: Vec<Type> = e.parameters.iter().map(|p| seen(&p.ty)).collect();
            if !self.extern_names.contains(&e.name) {
                return Err(format!("function `{}` is defined twice", e.name));
            }
            if parameters != &mine || ret != &seen(&e.ret) {
                return Err(format!(
                    "external function `{}` is declared twice with different signatures — one \
                     symbol cannot be two functions, and a program holding both beliefs \
                     would call whichever the linker picked",
                    e.name
                ));
            }
            return Ok(());
        }
        for p in &e.parameters {
            match (&p.ty, p.marshal) {
                // A Decimal crosses ONLY through a declared marshaller. This is
                // the boundary-exactness rule: not "Decimals cannot cross" (a
                // missing feature) but "Decimals cross only through an encoding
                // that cannot lose them" (a guarantee).
                (Type::Decimal { scale, .. }, Some(Marshal::Scaled)) => {
                    let _ = scale;
                }
                (Type::Decimal { scale, .. }, None) => {
                    return Err(format!(
                        "in external function `{}`, parameter `{}` is {} and C has no \
                         decimal type, so the crossing has to say how the value is \
                         encoded. Declare `{}: {} as scaled` to pass the exact \
                         unscaled integer (C receives it scaled by 10^{}), or take \
                         a String and pass `to_string({})` as text.",
                        e.name, p.name, p.ty, p.name, p.ty, scale, p.name
                    ))
                }
                // A marshaller on anything else is meaningless: there is no
                // encoding question to answer.
                (other, Some(m)) => {
                    return Err(format!(
                        "in external function `{}`, parameter `{}` is {}, which C holds \
                         directly — `as {}` only means something for a Decimal, \
                         whose scale C has no way to carry.",
                        e.name, p.name, other, m
                    ))
                }
                // The widths join this list and nowhere else, which IS the boundary rule: this
                // is the one path that does not go through `validate_type`, and `validate_type`
                // refuses every width. Roadmap A7.
                (
                    Type::Int
                    | Type::String
                    | Type::CInt
                    | Type::CDouble
                    | Type::CPointer
                    | Type::Width { .. },
                    None,
                ) => {}
                (other, None) => {
                    return Err(format!(
                        "in external function `{}`, parameter `{}` has type {}, but only \
                         Int, CInt, a sized width (i32, u8, u32, u64), CDouble, String and a \
                         marshalled Decimal may cross the C boundary for now — C has no {}, \
                         and the raw value would silently lose its meaning.",
                        e.name, p.name, other, other
                    ))
                }
            }
        }
        // A CDouble return has nowhere exact to land: Burxt has no float type,
        // and inventing an inexact receiver to complete the matrix would
        // contradict the thesis. Say how to get the value exactly instead.
        if e.ret == Type::CDouble {
            return Err(format!(
                "external function `{}` returns CDouble, but Burxt has no float type to \
                 receive it exactly — a double cannot hold most decimal amounts. \
                 Have the C function return the scaled integer (declare `-> Int`), \
                 or return it as text.",
                e.name
            ));
        }
        // A CPointer return is how the pointer wall opens, and the reason it can open safely is
        // that Burxt never has to answer the ownership question. It does not hold the pointer as
        // anything it can act on: `c_is_null` asks whether the call failed and `c_string_at`
        // COPIES the bytes out. Freeing, if C wants freeing, is an `external function free` the
        // program calls in the open — visible in a signature rather than inferred by the compiler.
        //
        // A String return stays refused, and that is the same rule rather than an omission: a
        // String is a Burxt value with an owner, so accepting one here would be a claim about
        // whose memory it is. `-> CPointer` plus `c_string_at` says the same thing and says who
        // copied.
        if !matches!(e.ret, Type::Int | Type::CInt | Type::CPointer | Type::Width { .. }) {
            return Err(format!(
                "external function `{}` returns {}, but only Int, CInt, a sized width \
                 (i32, u8, u32, u64) or CPointer may cross the C boundary as a return — a {} \
                 is a Burxt value with an owner, and C cannot say whose it is. If the C \
                 function returns a pointer, declare `-> CPointer` and read it with \
                 `c_string_at`, which copies. (If it returns a 32-bit `int`, declare `-> CInt` \
                 or `-> i32` so the sign survives.)",
                e.name, e.ret, e.ret
            ));
        }
        Ok(())
    }

    fn check_fn(&mut self, f: &FnDef) -> Result<TypedFn, String> {
        self.current_span.set(f.span);
        // Who a probing pass should credit for an allocation found in this body.
        *self.probe_owner.borrow_mut() = (String::new(), f.name.clone());
        // A free function is inside no class, so it may reach nothing private — UNLESS it is an
        // associated function, whose qualified name `Account.open` says which class it belongs
        // to. That is what lets a constructor build a value with private fields.
        self.current_receiver = f.name.split_once('.').map(|(holder, _)| holder.to_string());
        self.env.clear();
        self.dyn_source.clear();
        self.current_params.clear();
        self.region_locals.clear();
        // B25: which parameters a growth found in this body can be attributed to. Rebuilt per
        // body rather than cleared once, because a stale map would credit this function's
        // growth to whichever function was checked last.
        self.current_writable_params.clear();
        self.current_self_writable = false;
        // B32: every parameter, `mutable` or not — a relay hands back what it was given and
        // never writes to it. Rebuilt per body for the same reason the map above is.
        self.current_param_positions.clear();
        for (i, p) in f.parameters.iter().enumerate() {
            if p.writable {
                self.current_writable_params.insert(p.name.clone(), i);
            }
            self.current_param_positions.insert(p.name.clone(), i);
        }
        let mut parameters = Vec::new();
        for p in &f.parameters {
            if let Some(m) = p.marshal {
                return Err(format!(
                    "in `function {}`, parameter `{}` is marked `as {}`, but marshalling \
                     only exists where there is a foreign encoding to marshal \
                     into. A Burxt-to-Burxt call passes the value itself, exactly \
                     — drop the `as {}`.",
                    f.name, p.name, m, m
                ));
            }
            // `mutable` only on aggregates, and that is a rule about MEANING rather than a
            // limitation. On a scalar the word would have to mean "you get your own copy to change",
            // which is a fact about the body and not about the call — so one word would mean two
            // different things depending on the type, decided silently. That is the shape of thing
            // this language exists to refuse.
            // A `pure` function may not take a `mutable` parameter, and this is not a technicality:
            // `pure` promises the answer depends on the arguments and NOTHING ELSE, which is the same
            // promise as changing nothing. A pure function that rewrites its caller's array is a
            // contradiction the signature would be asserting in both directions at once — and worse,
            // `pure` is what a CONTRACT is allowed to call, so a precondition could have quietly
            // rearranged the data it was checking.
            if p.writable && f.is_pure {
                return Err(format!(
                    "`pure function {}` cannot take `mutable {}: {}`: `pure` means the answer \
                     depends on the arguments and nothing else, which is the same thing as changing \
                     nothing — and `mutable` says this call changes what the caller passed. Drop one \
                     of the two. (It matters more than it looks: a contract clause may call a `pure` \
                     function, so this would let a precondition rewrite the data it is checking.)",
                    f.name, p.name, p.ty
                ));
            }
            if p.writable && !crate::codegen::is_aggregate(&p.ty) {
                return Err(format!(
                    "`mutable {}: {}` says the caller's value may change, and a {} is copied when \
                     it crosses — so there would be nothing for the caller to see. If you want a \
                     copy to modify inside the body, say so where the copy is: \
                     `let mutable {}: {} = ...;`",
                    p.name, p.ty, p.ty, p.name, p.ty
                ));
            }
            if let Some(message) = self.shadows_a_const(&p.name) {
                return Err(message);
            }
            self.current_params.insert(p.name.clone());
            if self.env.insert(p.name.clone(), (p.ty.clone(), p.writable)).is_some() {
                return Err(format!(
                    "function `{}` has two parameters named `{}`",
                    f.name, p.name
                ));
            }
            parameters.push((p.name.clone(), p.ty.clone()));
        }
        self.current_ret = Some(f.ret.clone());
        // From the set, not the word: an inferred `allocates` has to put the body in the
        // same state a written one does, or the inference would only remove the error and
        // not the reason for it.
        self.in_caller_region = self.allocates_fn(&f.name);
        let outer_allowed = std::mem::replace(
            &mut self.allowed_effects,
            self.fn_effects.get(&f.name).cloned().unwrap_or_default(),
        );
        let outer_effects_owner = std::mem::replace(&mut self.effects_owner, f.name.clone());
        self.in_pure = if f.is_pure { Some(f.name.clone()) } else { None };
        self.current_signature =
            Some((f.name.clone(), f.parameters.iter().map(|p| p.ty.clone()).collect()));
        // Contracts are checked BEFORE the body, in the parameter scope, because
        // that is the scope they run in. `requires` sees the arguments; `ensures`
        // additionally sees `result`.
        //
        // Both are checked under the `pure` rule, whatever the function itself is
        // declared: a clause that can print, read a file or call into C is not a
        // check, it is a second program that only runs when someone is looking.
        let saved_pure = self.in_pure.clone();
        self.in_pure = Some(f.name.clone());
        self.in_contract = true;
        self.olds.borrow_mut().clear();
        let requires = self.check_contracts(&f.requires, None)?;
        let ensures = if f.ensures.is_empty() {
            Vec::new()
        } else if crate::codegen::is_aggregate(&f.ret) {
            self.current_span.set(f.ensures[0].span);
            return Err(format!(
                "`ensures` on `{}` is not supported yet: it returns {} {}, which \
                 travels through a hidden pointer into the caller's storage, so \
                 binding `result` to it needs care a scalar does not. Return a \
                 scalar, or drop the clause.",
                f.name,
                f.ret.article(),
                f.ret
            ));
        } else {
            self.check_contracts(&f.ensures, Some(&f.ret))?
        };
        // The measure lives in the same scope and under the same rule as a clause:
        // parameters only, and pure. A measure that could read state outside the
        // call would make the call-site substitution a lie.
        let decreases = match &f.decreases {
            None => None,
            Some(clause) => {
                self.current_span.set(clause.span);
                if !calls_itself(&f.body, &f.name) {
                    return Err(format!(
                        "`{}` never calls itself, so `decreases {}` has nothing to \
                         check. A reader would take it to mean something; drop it, or \
                         make the recursion real.",
                        Self::shown_fn_name(&f.name), clause.text
                    ));
                }
                let measure = self.check_expr(&clause.cond, Some(&Type::Int))?;
                if measure.ty != Type::Int {
                    return Err(format!(
                        "a termination measure must be an Int, but `{}` has type {}. A \
                         Decimal measure can shrink forever without arriving — 1.00, \
                         0.50, 0.25 — which is the failure `decreases` exists to rule \
                         out.",
                        clause.text, measure.ty
                    ));
                }
                Some(TypedContract { cond: measure, text: clause.text.clone(), span: clause.span })
            }
        };
        self.in_pure = saved_pure;
        self.in_contract = false;

        let errors_before = self.errors.len();
        let body = self.check_block(&f.body)?;
        self.current_ret = None;
        self.in_caller_region = false;
        self.in_pure = None;
        self.current_signature = None;
        self.env.clear();
        self.dyn_source.clear();
        self.region_locals.clear();

        // Only prove the return paths if the body actually checked. A statement
        // that failed produced no TypedStmt, so "must end by returning" would be
        // a second, misleading complaint about the same mistake.
        if self.errors.len() == errors_before && !block_returns(&body) {
            self.current_span.set(f.span);
            return Err(format!(
                "function `{}` must end by returning {} {} on every path \
                 (its last statement must be a `return`, or an if/else where \
                 both branches return)",
                f.name,
                f.ret.article(),
                f.ret
            ));
        }
        let olds = std::mem::take(&mut *self.olds.borrow_mut());
        self.allowed_effects = outer_allowed;
        self.effects_owner = outer_effects_owner;
        let writable = f.parameters.iter().map(|p| p.writable).collect();
        // `allocates nothing` — a CLAIM, checked here against the same inference every other
        // question about allocation asks. Not a second source of truth: `allocates_fn` is the one
        // answer, whether the programmer wrote the marker or the probe worked it out.
        //
        // Checked at the END of the body rather than the start, because the body is what decides,
        // and checked against the TRANSITIVE answer, because a claim that stopped at the first call
        // would be worth nothing — a function that calls something that allocates does allocate.
        if f.allocates_nothing && self.allocates_fn(&f.name) {
            self.blame(f.span);
            return Err(format!(
                "`function {}` claims `allocates nothing`, and it does allocate{}. \
                 Either drop the claim, or move the building into a function that does not \
                 make it.",
                f.name,
                match self.first_allocation(&body) {
                    Some(why) => format!(" — {}", why),
                    None => String::new(),
                }
            ));
        }
        Ok(TypedFn { name: f.name.clone(), parameters, writable, ret: f.ret.clone(), body, requires, ensures, decreases, olds })
    }

    /// Check a method body. `self` is bound like any parameter, with its
    /// mutability set from `receiver_mut` — so `self.field = ...` obeys the
    /// exact same AssignField rule an ordinary `let mut` binding would.
    fn check_method(&mut self, m: &MethodDef) -> Result<TypedMethod, String> {
        self.current_span.set(m.span);
        *self.probe_owner.borrow_mut() = (m.receiver.clone(), m.name.clone());
        // Inside this body, this class's private members are reachable. Nowhere else — which
        // means this has to be RESTORED, not merely set. Set-and-forget left it pointing at
        // whichever class was checked last, so the top level inherited that class's privileges
        // and a private method called from outside compiled cleanly.
        let outer_receiver = self.current_receiver.replace(m.receiver.clone());
        self.env.clear();
        self.dyn_source.clear();
        self.region_locals.clear();
        // B25. A method may not declare a `mutable` parameter — only `mutable self` — so the
        // map is empty here by construction and the receiver carries the whole question.
        self.current_writable_params.clear();
        self.current_self_writable = m.receiver_mut;
        // B32. A method parameter may not be `mutable`, but it can still be RELAYED — handed
        // straight back by `return` — so unlike the map above this one is not empty here.
        self.current_param_positions.clear();
        for (i, p) in m.parameters.iter().enumerate() {
            self.current_param_positions.insert(p.name.clone(), i);
        }
        self.env.insert(
            "self".to_string(),
            (Type::Named(m.receiver.clone()), m.receiver_mut),
        );
        let mut parameters = Vec::new();
        for p in &m.parameters {
            // `mutable` on a METHOD parameter is refused rather than half-supported, and the reason
            // is which failure each choice produces. Supporting it means threading writability
            // through two more `byval` sites — the method declaration and the method call — and
            // missing either one gives a callee that writes to a copy while the caller sees nothing.
            // That is a silent wrong answer, and a refusal is never one.
            //
            // `mutable self` already covers the case that actually comes up: a method changing its
            // own receiver. A method that must change something ELSE it was handed can be a free
            // function today, and this becomes additive when it is worth doing.
            if p.writable {
                return Err(format!(
                    "in `{}.{}`, parameter `{}` is declared `mutable`, which is not available on a \
                     METHOD yet — only on a free function. If the method should change its own \
                     receiver, write `function (mutable self) {}(...)`; otherwise make it a \
                     function, where `mutable {}: {}` works today.",
                    m.receiver, m.name, p.name, m.name, p.name, p.ty
                ));
            }
            if let Some(mar) = p.marshal {
                return Err(format!(
                    "in `{}.{}`, parameter `{}` is marked `as {}`, but marshalling \
                     only exists at a foreign boundary. A Burxt-to-Burxt call \
                     passes the value itself, exactly — drop the `as {}`.",
                    m.receiver, m.name, p.name, mar, mar
                ));
            }
            if p.name == "self" {
                return Err(format!(
                    "in `{}.{}`: `self` is already the receiver; parameters \
                     cannot reuse the name",
                    m.receiver, m.name
                ));
            }
            if let Some(message) = self.shadows_a_const(&p.name) {
                return Err(message);
            }
            if self.env.insert(p.name.clone(), (p.ty.clone(), false)).is_some() {
                return Err(format!(
                    "method `{}.{}` has two parameters named `{}`",
                    m.receiver, m.name, p.name
                ));
            }
            parameters.push((p.name.clone(), p.ty.clone()));
        }
        self.current_ret = Some(m.ret.clone());
        self.in_caller_region = self.allocates_method(&m.receiver, &m.name);
        let outer_allowed = std::mem::replace(
            &mut self.allowed_effects,
            self.method_effects
                .get(&(m.receiver.clone(), m.name.clone()))
                .cloned()
                .unwrap_or_default(),
        );
        let outer_effects_owner =
            std::mem::replace(&mut self.effects_owner, format!("{}.{}", m.receiver, m.name));

        // Contracts, in the receiver-and-parameter scope, under the `pure` rule —
        // exactly as on a free function. A MUTATING method is where they earn the
        // most: `old(...)` compares the state after against the state before.
        let saved_pure = self.in_pure.clone();
        self.in_pure = Some(format!("{}.{}", m.receiver, m.name));
        self.in_contract = true;
        self.olds.borrow_mut().clear();
        let requires = self.check_contracts(&m.requires, None)?;
        let ensures = if m.ensures.is_empty() {
            Vec::new()
        } else if crate::codegen::is_aggregate(&m.ret) {
            self.current_span.set(m.ensures[0].span);
            return Err(format!(
                "`ensures` on `{}.{}` is not supported yet: it returns {} {}, which \
                 travels through a hidden pointer into the caller's storage, so \
                 binding `result` to it needs care a scalar does not. Return a \
                 scalar, or drop the clause.",
                m.receiver,
                m.name,
                m.ret.article(),
                m.ret
            ));
        } else {
            self.check_contracts(&m.ensures, Some(&m.ret))?
        };
        self.in_pure = saved_pure;
        self.in_contract = false;
        let olds = std::mem::take(&mut *self.olds.borrow_mut());

        // A4: the BODY of a `pure` method is checked under the pure rule too. Before A4 this was
        // set only around the contracts above — a method could not carry the marker, so its body
        // had no promise to be held to, and the contract case worked because a clause is held to
        // purity whether or not the method is.
        let saved_body_pure = self.in_pure.clone();
        if m.is_pure {
            self.in_pure = Some(format!("{}.{}", m.receiver, m.name));
        }
        let body = self.check_block(&m.body)?;
        self.in_pure = saved_body_pure;
        self.current_ret = None;
        self.in_caller_region = false;
        self.env.clear();
        self.dyn_source.clear();
        self.region_locals.clear();

        if !block_returns(&body) {
            return Err(format!(
                "method `{}.{}` must end by returning {} {} on every path \
                 (its last statement must be a `return`, or an if/else where \
                 both branches return)",
                m.receiver,
                m.name,
                m.ret.article(),
                m.ret
            ));
        }
        self.current_receiver = outer_receiver;
        self.allowed_effects = outer_allowed;
        self.effects_owner = outer_effects_owner;
        Ok(TypedMethod {
            receiver: m.receiver.clone(),
            receiver_mut: m.receiver_mut,
            name: m.name.clone(),
            parameters,
            ret: m.ret.clone(),
            body,
            requires,
            ensures,
            olds,
        })
    }

    /// `return tail f(arguments)` — the guarantee, checked.
    ///
    /// LLVM's `musttail` either compiles to a real tail call or fails the build,
    /// which is exactly the contract Burxt wants: declare the intent, and the
    /// compiler guarantees it or refuses with a reason. But `musttail` is only
    /// legal when the caller's and callee's prototypes MATCH, so that condition
    /// is checked here, in words, rather than surfacing as an LLVM verifier
    /// message no one can act on.
    fn check_tail_return(&mut self, e: &Expr) -> Result<TypedStmtKind, String> {
        let ret = self.current_ret.clone().ok_or_else(|| {
            "`return` only makes sense inside a function".to_string()
        })?;
        let (caller, caller_params) = self.current_signature.clone().ok_or_else(|| {
            "a guaranteed tail call only makes sense inside a function".to_string()
        })?;
        let (name, arguments) = match &e.kind {
            ExprKind::Call { name, arguments } => (name.clone(), arguments),
            // The parser already refused anything else.
            _ => return Err("`return tail` must be followed by a call".to_string()),
        };

        // A region is released when it is left, and a tail call never comes
        // back — so the release would have to happen BEFORE the call, while the
        // arguments may still point into the region. Refused rather than
        // silently handing over freed storage.
        if let Some(region) = self.current_region.clone() {
            return Err(format!(
                "`return tail` cannot leave the region `{}`: the region is \
                 released on the way out, but a tail call never returns to do \
                 it, and the arguments may point into it. Move the call outside \
                 the region, or use an ordinary `return`.",
                region
            ));
        }

        if self.extern_names.contains(&name) {
            return Err(format!(
                "`{}` is an `external function`, so Burxt cannot guarantee a tail call \
                 into it: the C side owns that ABI, and the width conversion \
                 Burxt does on the result has to happen after the call returns.",
                name
            ));
        }
        let (parameters, callee_ret) = self.fns.get(&name).cloned().ok_or_else(|| {
            format!(
                "unknown function `{}` — a guaranteed tail call needs a `function` \
                 declared in this program.",
                name
            )
        })?;

        // The prototypes must match for the guarantee to exist at all. Say so
        // in terms of the two signatures, not in terms of LLVM.
        let scalar = |t: &Type| {
            matches!(
                t,
                Type::Int | Type::Bool | Type::String | Type::CInt | Type::Decimal { .. }
            )
        };
        if callee_ret != ret || parameters != caller_params {
            return Err(format!(
                "a guaranteed tail call reuses this frame, so `{}` and `{}` must \
                 have the SAME signature — `{}` takes ({}) -> {}, but `{}` takes \
                 ({}) -> {}. Use an ordinary `return` for a call that differs.",
                caller,
                name,
                caller,
                Self::type_list(&caller_params),
                ret,
                name,
                Self::type_list(&parameters),
                callee_ret
            ));
        }
        if !parameters.iter().all(scalar) || !scalar(&ret) {
            return Err(format!(
                "a guaranteed tail call is limited to scalar parameters and \
                 returns for now — `{}` passes or returns an aggregate, which \
                 travels by hidden pointer into storage this frame owns. Use an \
                 ordinary `return`.",
                name
            ));
        }

        // Ordinary argument checking: a tail call is still a call.
        if arguments.len() != parameters.len() {
            return Err(format!(
                "`{}` takes {} argument{}, but {} {} given",
                name,
                parameters.len(),
                if parameters.len() == 1 { "" } else { "s" },
                arguments.len(),
                if arguments.len() == 1 { "was" } else { "were" }
            ));
        }
        let mut typed_args = Vec::new();
        for (argument, want) in arguments.iter().zip(parameters.iter()) {
            let t = self.check_expr(argument, Some(want))?;
            if !self.storable(&t.ty, want) {
                return Err(format!(
                    "`{}` expects {} {} here, but this argument has type {}",
                    name,
                    want.article(),
                    want,
                    t.ty
                ));
            }
            typed_args.push(t);
        }
        Ok(TypedStmtKind::TailReturn { name, arguments: typed_args })
    }


    /// The arms of a `match`, given the enum being matched and its variants.
    ///
    /// Shared by the two ways a scrutinee can arrive: an ordinary enum, and a generic
    /// one still holding parameters because we are inside the generic that declared it.
    /// One implementation, so exhaustiveness and payload binding cannot differ between
    /// the declaration and its instantiations.
    /// A `match` on a scalar, desugared to an `if / else if` chain.
    ///
    /// The rule here is the OPPOSITE of the enum rule, and that is worth saying plainly because
    /// two `match` rules that differ will otherwise read as arbitrary. An enum match REFUSES a
    /// wildcard, because listing every variant is the whole point — add a variant later and the
    /// match becomes an error naming it. A scalar match REQUIRES one, because `Int` cannot be
    /// enumerated and a match with no catch-all would be a hole with no error to mark it.
    fn desugar_scalar_match(
        &mut self,
        subject: &Expr,
        scrutinee: TypedExpr,
        arms: &[MatchArm],
        at: Span,
    ) -> Result<TypedStmtKind, String> {
        self.current_span.set(at);
        let ty = scrutinee.ty.clone();
        let shown = format!("{}", ty);

        // Split off the wildcard, which must be last: an arm after it could never run, and
        // silently unreachable code is the kind of thing a reviewer should never have to spot.
        let mut cases: Vec<(&MatchArm, &MatchLiteral)> = Vec::new();
        let mut fallback: Option<&MatchArm> = None;
        for (i, arm) in arms.iter().enumerate() {
            if !arm.bindings.is_empty() {
                return Err(format!(
                    "`{}` is a literal, so it carries nothing to name. Payload names belong to \
                     an enum variant.",
                    arm.variant
                ));
            }
            match &arm.literal {
                Some(lit) => {
                    if fallback.is_some() {
                        return Err(format!(
                            "`{}` comes after the `_` arm, so it could never run. Put `_` last.",
                            arm.variant
                        ));
                    }
                    if cases.iter().any(|(_, seen)| *seen == lit) {
                        return Err(format!("`{}` is matched twice in this `match`", arm.variant));
                    }
                    let literal_ty = match lit {
                        MatchLiteral::Int(_) => Type::Int,
                        MatchLiteral::Text(_) => Type::String,
                        MatchLiteral::Truth(_) => Type::Bool,
                    };
                    // One equality, no coercion — the same rule `==` follows everywhere.
                    if !matches!(
                        (&ty, lit),
                        (Type::Decimal { .. }, MatchLiteral::Int(_))
                    ) && literal_ty != ty
                    {
                        return Err(format!(
                            "this `match` is on {}, but `{}` is {}. There is one equality in \
                             Burxt and it never converts.",
                            shown, arm.variant, literal_ty
                        ));
                    }
                    cases.push((arm, lit));
                }
                None if arm.variant == "_" => {
                    if fallback.is_some() {
                        return Err("this `match` has two `_` arms".to_string());
                    }
                    fallback = Some(arm);
                }
                None => {
                    return Err(format!(
                        "this `match` is on {}, so `{}` is not a pattern it can have — a scalar \
                         match takes literals and `_`, not variant names.",
                        shown, arm.variant
                    ));
                }
            }
            let _ = i;
        }

        let Some(fallback) = fallback else {
            return Err(format!(
                "this `match` on {} has no `_` arm. A scalar cannot be enumerated, so a match \
                 without a catch-all would leave values with nowhere to go — which is the \
                 opposite of an enum match, where `_` is refused because listing every variant \
                 is the point.",
                shown
            ));
        };

        // Built back to front, so each arm's else-branch is the chain already assembled.
        let mut chain = self.check_block(&fallback.body)?;
        for (arm, lit) in cases.iter().rev() {
            let literal_expr = Expr {
                kind: match lit {
                    MatchLiteral::Int(n) => ExprKind::IntLit(*n),
                    MatchLiteral::Text(s) => ExprKind::StrLit(s.clone()),
                    MatchLiteral::Truth(b) => ExprKind::BoolLit(*b),
                },
                span: at,
            };
            let cond = self.check_expr(
                &Expr {
                    kind: ExprKind::Compare {
                        op: CmpOp::Eq,
                        lhs: Box::new(subject.clone()),
                        rhs: Box::new(literal_expr),
                    },
                    span: at,
                },
                Some(&Type::Bool),
            )?;
            let then_block = self.check_block(&arm.body)?;
            // The whole chain is one `match`, so every `if` in it is blamed on the `match`
            // — `at`. The arms' own statements keep their own spans, so a debugger steps
            // from the `match` line into the arm the value took, which is what the reader
            // wrote even though it is not what the tree now says.
            chain = vec![TypedStmt::new(
                TypedStmtKind::If { cond, then_block, else_block: Some(chain) },
                at,
            )];
        }
        // A chain of one is the wildcard alone, which is a block and not an `if`.
        Ok(match chain.len() {
            1 => chain.into_iter().next().unwrap().kind,
            _ => TypedStmtKind::If {
                cond: self.check_expr(
                    &Expr { kind: ExprKind::BoolLit(true), span: at },
                    Some(&Type::Bool),
                )?,
                then_block: chain,
                else_block: None,
            },
        })
    }

    fn check_match_arms(
        &mut self,
        variants: Vec<(String, Vec<Type>)>,
        scrutinee: TypedExpr,
        arms: &[MatchArm],
        at: Span,
        shown: String,
    ) -> Result<TypedStmtKind, String> {
            // B27. A pattern binding is the SIXTH place a name enters scope, and it was the
            // first one that never asked this: if the value being matched on is region storage,
            // so is everything the pattern pulls out of it. `region r { let w = W.Some("a" +
            // "b"); match w { Some(s) => { kept = s; } } }` with `kept` declared outside was
            // accepted, printed `secret-1`, and printed the next region's bytes afterwards —
            // the escape rules all consult `region_locals`, and no pattern ever wrote to it.
            //
            // Asked ONCE, here, rather than per arm: the scrutinee does not change between
            // arms, and `expr_allocates` reads `region_locals`, which the arms may add to.
            let scrutinee_allocates =
                self.current_region.is_some() && self.expr_allocates(&scrutinee);
            let mut typed_arms: Vec<TypedArm> = Vec::new();
            for arm in arms {
                // Checking the previous arm's body moved the position; an error
                // about THIS arm's pattern belongs to the match, not to the arm
                // above it. (Found by shadowing a name in examples/lexer.bx and
                // being pointed at the wrong line.)
                self.current_span.set(at);
                if arm.variant == "_" {
                    return Err(
                        "Burxt has no `_` wildcard arm: it would silently absorb \
                         variants added later, which is the one thing exhaustive \
                         matching exists to prevent. List the remaining variants."
                            .to_string(),
                    );
                }
                let tag = variants
                    .iter()
                    .position(|(n, _)| *n == arm.variant)
                    .ok_or_else(|| {
                        format!(
                            "`{}` has no variant named `{}`. Its variants are: {}.",
                            shown,
                            arm.variant,
                            variants
                                .iter()
                                .map(|(n, _)| n.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    })? as u32;
                if typed_arms.iter().any(|a| a.tag == tag) {
                    return Err(format!(
                        "`{}` is matched twice in this `match`",
                        arm.variant
                    ));
                }
                let payload = &variants[tag as usize].1;
                if arm.bindings.len() != payload.len() {
                    return Err(format!(
                        "`{}.{}` carries {} value(s), but this pattern names {}. \
                         Name every payload value, so nothing is silently \
                         dropped.",
                        shown,
                        arm.variant,
                        payload.len(),
                        arm.bindings.len()
                    ));
                }

                // Payload names are ordinary immutable locals, scoped to the arm.
                let saved = self.env.clone();
                let mut bindings = Vec::new();
                let mut tainted: Vec<String> = Vec::new();
                // What the scrutinee could still be pointing at. Asked once per arm rather than
                // per binding, and only while probing, because that is when `record_relay` reads
                // the answer. See `relay_aliases`: without it a `return Option.Some(s)` out of a
                // `match` on a parameter records no relay at all.
                let scrutinee_sources = self.relayed_sources(&scrutinee);
                let mut aliased: Vec<String> = Vec::new();
                for (name, ty) in arm.bindings.iter().zip(payload) {
                    if let Some(message) = self.shadows_a_const(name) {
                        self.env = saved;
                        return Err(message);
                    }
                    if self.env.contains_key(name) {
                        self.env = saved;
                        return Err(format!(
                            "`{}` is already declared — a pattern binding may not \
                             shadow it, the same rule `let` follows.",
                            name
                        ));
                    }
                    self.env.insert(name.clone(), (ty.clone(), false));
                    if !scrutinee_sources.is_empty() && self.may_be_region_storage(ty) {
                        // Gated on the TYPE for the same reason `record_relay` is: a payload that
                        // cannot hold region storage cannot carry a pointer out, so recording it
                        // would taint every `match` on an enum with an Int payload.
                        self.relay_aliases.insert(name.clone(), scrutinee_sources.clone());
                        aliased.push(name.clone());
                    }
                    if scrutinee_allocates && self.may_be_region_storage(ty) {
                        // Tracked so it can be taken back out below. The taint follows the
                        // NAME, and this name is gone at the arm's closing brace — while a
                        // second arm may legitimately bind the same name to an Int payload,
                        // which must not inherit this arm's taint.
                        if self.region_locals.insert(name.clone()) {
                            tainted.push(name.clone());
                        }
                    }
                    bindings.push((name.clone(), ty.clone()));
                }
                let body = self.check_block(&arm.body);
                self.env = saved;
                for name in tainted.drain(..) {
                    self.region_locals.remove(&name);
                }
                for name in aliased.drain(..) {
                    self.relay_aliases.remove(&name);
                }
                typed_arms.push(TypedArm { tag, bindings, body: body? });
            }

            // THE feature: every variant must be handled. Add a variant to
            // the enum later and every incomplete match becomes an error
            // naming exactly what to handle.
            let missing: Vec<&str> = variants
                .iter()
                .enumerate()
                .filter(|(i, _)| !typed_arms.iter().any(|a| a.tag == *i as u32))
                .map(|(_, (n, _))| n.as_str())
                .collect();
            if !missing.is_empty() {
                return Err(format!(
                    "this `match` on `{}` does not handle {}. Every variant must \
                     be handled — that is what makes adding a variant later a \
                     compile error instead of a silent fall-through.",
                    shown,
                    missing
                        .iter()
                        .map(|m| format!("`{}`", m))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }

            typed_arms.sort_by_key(|a| a.tag);
            Ok(TypedStmtKind::Match { value: scrutinee, arms: typed_arms })
    }
    /// Render a parameter list for an error message.
    fn type_list(types: &[Type]) -> String {
        types.iter().map(|t| t.to_string()).collect::<Vec<_>>().join(", ")
    }

    /// Check a block's statements in a child scope: names declared inside are
    /// gone after the closing brace. Also refuses unreachable code — anything
    /// following a statement that always returns.
    fn check_block(&mut self, stmts: &[Stmt]) -> Result<Vec<TypedStmt>, String> {
        let saved = self.env.clone();
        // A11: `dyn_source` is scoped exactly as `env` is, and here only. The other three
        // places that insert a name — a match arm's payload, a `for` element, a `for` range's
        // counter — cannot be a `dynamic` bound from a coercion, and each of their bodies is a
        // block, so every `let` still passes through here.
        let saved_dyn = self.dyn_source.clone();
        let mut out: Vec<TypedStmt> = Vec::new();
        let errors_before = self.errors.len();
        for s in stmts {
            if out.last().is_some_and(stmt_diverges) {
                self.current_span.set(s.span);
                let after = match out.last().map(|p| &*p) {
                    Some(TypedStmt { kind: TypedStmtKind::Break, .. }) => "`break`",
                    Some(TypedStmt { kind: TypedStmtKind::Continue, .. }) => "`continue`",
                    _ => "`return`",
                };
                self.record(format!(
                    "unreachable statement: this code comes after {}",
                    after
                ));
                // One report is enough: everything after a `return` is
                // unreachable, and saying so five times is noise.
                break;
            }
            match self.check_stmt(s) {
                // The ONE place a checked statement is paired with where it was written.
                // `check_stmt` answers what the statement IS; the span it came from is
                // right here in `s`, so nothing below has to thread a position through.
                Ok(kind) => out.push(TypedStmt::new(kind, s.span)),
                Err(message) => {
                    // Record and CARRY ON. A compiler that stops at the first
                    // problem makes the reader fix one thing, recompile, and
                    // discover the next — five times in a row.
                    self.record(message);
                    self.recover_from(s);
                }
            }
        }
        self.env = saved;
        self.dyn_source = saved_dyn;
        let _ = errors_before;
        Ok(out)
    }

    /// B17. Where the caret goes on a statement that writes to a name: **the name**,
    /// not the whole line.
    ///
    /// The two compilers already agreed on the message and on the line and column; they
    /// disagreed on the LENGTH, stage-0 underlining `kept = "a" + "b";` where stage-1
    /// underlines `kept`. Stage-1's is the better one — the caret should sit on the thing
    /// the reader has to change — and it is the half an editor squiggles and the language
    /// server returns, so it is not cosmetic.
    ///
    /// The name is the statement's first token in all four spellings, `kept`, `acc +=`,
    /// `b.name` and `g.items[0]`, so the span is derivable and nothing in the AST has to
    /// carry a second one. For a path the ROOT is blamed, which is stage-1's answer too:
    /// `b` is the binding that outlives the region, and `.name` is not the problem.
    ///
    /// Set through `current_span` rather than `blame`, deliberately: `blame` also claims
    /// the position, and an error found INSIDE the value must still be able to take it.
    /// `k = 1 + missing` points at `missing` in both compilers and has to keep doing so.
    fn blame_target(&self, s: &Stmt, name: &str) {
        let start = s.span.start as usize;
        self.current_span.set(Span::new(start, start + name.len()));
    }

    /// Answers what the statement IS. The position it was written at is attached by the
    /// caller — see `check_block`, which is the only one — so no arm here has to carry it.
    fn check_stmt(&mut self, s: &Stmt) -> Result<TypedStmtKind, String> {
        // Remember where we are. Errors below are returned as plain messages and
        // the position is attached once, at the boundary — so a nested statement
        // naturally reports the innermost (most precise) position, and no error
        // site has to thread a span through.
        self.current_span.set(s.span);
        // A fresh statement, so the next error is free to claim its own position.
        self.error_located.set(false);
        match &s.kind {
            StmtKind::Let { name, mutable, declared, value } => {
                if let Some(message) = self.shadows_a_const(name) {
                    return Err(message);
                }
                if self.env.contains_key(name) {
                    return Err(format!(
                        "`{}` is already declared — Burxt does not allow shadowing; \
                         a second `let {}` would silently hide the first. Use a new \
                         name, or `{} = ...` if it was declared `let mutable`.",
                        name, name, name
                    ));
                }
                // Two paths, and only the first line of each differs: with an annotation
                // the value is checked AGAINST a type, without one the value IS the type.
                // Every rule after that is the same, which is the point — inference removes
                // typing, not checking. See spec/1.0/M10-ERGONOMICS.md §1 Decision 3.
                let (bound, typed) = match declared {
                    Some(declared) => {
                        self.validate_type(declared)?;
                        // RULE 1 of escape checking: a region-allocated value may only
                        // be bound inside a region. Because block bindings do not
                        // escape their block, this single rule stops region data from
                        // outliving its region by assignment — there is nowhere outside
                        // to put it.
                        if self.region_allocated(declared) && !self.has_region() {
                            return Err(self.needs_region(&format!(
                                "`let {}: {}` holds a growable array, which lives in a region",
                                name, declared
                            )));
                        }
                        // An array exists only behind a binding: it must be created
                        // right here, from a literal (whole-array copies are deferred).
                        if matches!(declared, Type::Array { .. })
                            && !matches!(value.kind, ExprKind::ArrayLit(_))
                        {
                            return Err(format!(
                                "`let {}: {}` must be initialized with an array literal, \
                                 e.g. [1, 2, 3] — copying a whole array is deferred.",
                                name, declared
                            ));
                        }
                        let typed = self.check_expr(value, Some(declared))?;
                        if !self.storable(&typed.ty, declared) {
                            // The declaration is fine; it is the value that disagrees.
                            self.blame(value.span);
                            return Err(format!(
                                "type mismatch in `let {}`: declared {}, but expression \
                                 has type {}",
                                name,
                                self.shown(declared),
                                self.shown(&typed.ty)
                            ));
                        }
                        (declared.clone(), typed)
                    }
                    None => {
                        let typed = self.check_expr(value, None)?;
                        // The same region rule, asked of the type that was found rather
                        // than the one that was written. It can still fire: a call can
                        // answer with a struct that holds a growable array.
                        if self.region_allocated(&typed.ty) && !self.has_region() {
                            return Err(self.needs_region(&format!(
                                "`let {} = ...` holds a growable array ({}), which lives \
                                 in a region",
                                name, typed.ty
                            )));
                        }
                        (typed.ty.clone(), typed)
                    }
                };
                // The same question the rules above just asked, remembered instead of
                // discarded: did this initializer build something in a region THIS
                // function opened? If so the binding cannot leave, and `return name` has
                // to be refused exactly as `return <that expression>` already was.
                if self.current_region.is_some() && self.expr_allocates(&typed) {
                    self.region_locals.insert(name.clone());
                }
                if self.current_region.is_some() {
                    self.region_scope.insert(name.clone());
                }
                // A11. An interface object borrows the storage of the value behind it, so this
                // `let` gives that value a second name. Two things follow, and both are settled
                // here because this is the one place that knows the source AND the new binding's
                // `mutable`.
                //
                // **The permission may not be upgraded.** `let mutable it: dynamic I = d` with
                // `d` immutable would hand out through `it` exactly the write `d.method()` was
                // refused — the same bytes, one word of laundering. So a `mutable` interface
                // object may only be built from a `mutable` value. The other direction is fine
                // and stays legal: an immutable `let` over a mutable value simply cannot write.
                //
                // **The source is remembered**, because every escape rule is about a NAME and
                // from here on the reader will write `it`. See `dyn_source`.
                match &typed.kind {
                    TypedExprKind::DynCoerce { var, .. } => {
                        // `coerce_dyn` already refused an unknown name, so the entry is here.
                        let (source_ty, source_mutable) = self
                            .env
                            .get(var)
                            .cloned()
                            .ok_or_else(|| self.unknown_name(var))?;
                        if *mutable && !source_mutable {
                            return Err(format!(
                                "`let mutable {}: {}` would borrow `{}`, which was declared \
                                 immutable — an interface object points at the storage of the \
                                 value behind it, so a `mutable` one could change `{}` through \
                                 `{}`. {} Or drop `mutable` from `{}`.",
                                name,
                                self.shown(&bound),
                                var,
                                var,
                                name,
                                self.how_to_make_writable(var, &source_ty),
                                name
                            ));
                        }
                        self.dyn_source.insert(name.clone(), var.clone());
                    }
                    // `let b: dynamic I = a` where `a` is ALREADY an interface object: no
                    // coercion happens, the fat pointer is copied, and both names reach the one
                    // value — so the source travels with the copy, and so does the ceiling on
                    // what may be written through it. Without this arm the rule above is one
                    // `let` away from being stepped around, which is B27's shape exactly.
                    TypedExprKind::Var(from) if matches!(bound, Type::Dyn(_)) => {
                        let from_mutable = self.env.get(from).map(|(_, m)| *m).unwrap_or(false);
                        if *mutable && !from_mutable {
                            return Err(format!(
                                "`let mutable {}: {}` would copy `{}`, which was declared \
                                 immutable — both names reach the one value behind the interface \
                                 object, so a `mutable` copy could change what `{}` may not. {} \
                                 Or drop `mutable` from `{}`.",
                                name,
                                self.shown(&bound),
                                from,
                                from,
                                self.how_to_make_writable(from, &bound),
                                name
                            ));
                        }
                        match self.dyn_source.get(from).cloned() {
                            Some(root) => {
                                self.dyn_source.insert(name.clone(), root);
                            }
                            // Anything whose source this frame cannot name — a `dynamic`
                            // PARAMETER, above all. `remove` rather than leave, so a name can
                            // never inherit a previous block's answer.
                            None => {
                                self.dyn_source.remove(name);
                            }
                        }
                    }
                    _ => {
                        self.dyn_source.remove(name);
                    }
                }
                // A local that BINDS relayed storage carries the same sources, exactly as a
                // `match` payload does — see `relay_aliases`. `json_at` reaches its parameter
                // through both hops at once:
                //
                //     Object(fields) => { let f: Field = fields[i]; return Option.Some(f.value); }
                //
                // `fields` is a pattern binding and `f` is a `let`. Teaching only the pattern left
                // the trail dying one statement later, which is the same defect one hop along —
                // the rule is not "a pattern binding relays", it is "a name bound to relayed
                // storage relays".
                //
                // `remove` on the empty case rather than leaving it, for the reason `dyn_source`
                // gives directly above: a name must never inherit a previous block's answer.
                if self.may_be_region_storage(&bound) {
                    let sources = self.relayed_sources(&typed);
                    if sources.is_empty() {
                        self.relay_aliases.remove(name);
                    } else {
                        self.relay_aliases.insert(name.clone(), sources);
                    }
                } else {
                    self.relay_aliases.remove(name);
                }
                self.env.insert(name.clone(), (bound.clone(), *mutable));
                Ok(TypedStmtKind::Let { name: name.clone(), ty: bound, value: typed })
            }
            StmtKind::Assign { name, value } => {
                self.blame_target(s, name);
                // Before the env lookup, because a const is not in `env` and "unknown
                // variable: LIMIT" is the wrong sentence for a name that is right there at
                // the top of the file.
                if let Some((ty, _)) = self.consts.get(name) {
                    return Err(format!(
                        "cannot assign to `{}`: it is a `const {}: {}`, which is a name for a \
                         literal rather than a place to store one. There is nothing to assign \
                         to — by the time the program runs, every use of `{}` IS the literal.",
                        name, name, ty, name
                    ));
                }
                let (declared, mutable) = self
                    .env
                    .get(name)
                    .ok_or_else(|| format!("unknown variable: {}", name))?
                    .clone();
                if matches!(declared, Type::Array { .. }) {
                    return Err(format!(
                        "whole-array assignment is deferred — assign elements \
                         individually: {}[0] = ...",
                        name
                    ));
                }
                if !mutable {
                    if self.loop_counters.iter().any(|c| c == name) {
                        return Err(format!(
                            "cannot assign to `{}`: it is the counter of a `for` range, and \
                             the loop recomputes it on every pass — an assignment here would \
                             be silently thrown away. Assigning to a loop variable is a bug, \
                             not a feature. Change the range, or copy it: \
                             `let mutable {}_at: Int = {};`",
                            name, name, name
                        ));
                    }
                    return Err(format!(
                        "cannot assign to `{}`: it was declared immutable. {}",
                        name,
                        self.how_to_make_writable(name, &declared)
                    ));
                }
                let typed = self.check_expr(value, Some(&declared))?;
                if !self.storable(&typed.ty, &declared) {
                    return Err(format!(
                        "cannot assign {} {} to `{}`, which was declared {}",
                        typed.ty.article(), typed.ty, name, declared
                    ));
                }
                // Assignment can put region storage into a binding that did not hold any:
                // `let mutable s: String = "x"; region r { s = "a" + "b"; }`. The `let`
                // saw a literal, so only this can know.
                //
                // **And until v0.0.222 it only MARKED it, which was a silent use-after-free.**
                // The mark taints the name so `return s` is refused, but `region_locals` is
                // cleared when the region closes, so the taint evaporated and the dangling
                // binding was read afterwards with no complaint at all. Measured:
                //
                //     let mutable kept: String = "";
                //     region one_turn { kept = "turn " + to_string(turn); }
                //     print("after turn " + to_string(turn) + ": " + kept);
                //
                // printed `after turn 0: after turn 0` — `kept` reading back as the print's own
                // concatenation buffer, because the region's bytes had been handed out again.
                // Compiled clean, wrong answer, no diagnostic. That is the precise failure this
                // language exists to refuse, in the feature built to prevent it.
                //
                // The rule: region storage may be assigned to a name declared INSIDE the region
                // (it dies with the region, which is correct) but never to one declared outside
                // it. Reported by a subagent writing the Burxt language server, which hit it
                // building a per-turn arena — the third defect found by writing the second
                // implementation.
                if self.current_region.is_some() && self.expr_allocates(&typed) {
                    if !self.region_scope.contains(name) {
                        let open = self.current_region.clone().unwrap_or_default();
                        return Err(format!(
                            "`{}` was declared outside `region {}`, so it cannot be assigned a \
                             value built inside it — the bytes are released at the closing brace \
                             and `{}` would read whatever the region hands out next. Declare `{}` \
                             inside the region, or build the value outside it.",
                            name, open, name, name
                        ));
                    }
                    self.region_locals.insert(name.clone());
                }
                // A11, the `let`'s rules asked again where the same thing can happen a second
                // time: `d = other` re-points an interface object. `d` is `mutable` or this
                // statement was already refused above, so the source must be `mutable` too, and
                // whatever `d` borrowed before is no longer what it borrows.
                if matches!(declared, Type::Dyn(_)) {
                    match &typed.kind {
                        TypedExprKind::DynCoerce { var, .. } => {
                            let (source_ty, source_mutable) = self
                                .env
                                .get(var)
                                .cloned()
                                .ok_or_else(|| self.unknown_name(var))?;
                            if !source_mutable {
                                return Err(format!(
                                    "`{}` may be written through, so it may not be pointed at \
                                     `{}`, which was declared immutable — the assignment would \
                                     let `{}` change `{}`. {}",
                                    name,
                                    var,
                                    name,
                                    var,
                                    self.how_to_make_writable(var, &source_ty)
                                ));
                            }
                            self.dyn_source.insert(name.clone(), var.clone());
                        }
                        TypedExprKind::Var(from) => {
                            if !self.env.get(from).map(|(_, m)| *m).unwrap_or(false) {
                                return Err(format!(
                                    "`{}` may be written through, so it may not be pointed at \
                                     `{}`, which was declared immutable — the assignment would \
                                     let `{}` change what `{}` may not. {}",
                                    name,
                                    from,
                                    name,
                                    from,
                                    self.how_to_make_writable(from, &declared)
                                ));
                            }
                            match self.dyn_source.get(from).cloned() {
                                Some(root) => {
                                    self.dyn_source.insert(name.clone(), root);
                                }
                                None => {
                                    self.dyn_source.remove(name);
                                }
                            }
                        }
                        _ => {
                            self.dyn_source.remove(name);
                        }
                    }
                }
                Ok(TypedStmtKind::Assign { name: name.clone(), value: typed })
            }
            StmtKind::AssignField { name, path, value } => {
                self.blame_target(s, name);
                let lvalue = format!("{}.{}", name, path.join("."));
                let (mut cur_ty, mutable) = self
                    .env
                    .get(name)
                    .ok_or_else(|| format!("unknown variable: {}", name))?
                    .clone();
                if !mutable {
                    return Err(format!(
                        "cannot assign to `{}`: `{}` was declared immutable. {}",
                        lvalue,
                        name,
                        self.how_to_make_writable(name, &cur_ty)
                    ));
                }
                let mut indices = Vec::new();
                for field in path {
                    let (index, field_ty) = self.resolve_field(&cur_ty, field)?;
                    indices.push(index);
                    cur_ty = field_ty;
                }
                let typed = self.check_expr(value, Some(&cur_ty))?;
                if !self.storable(&typed.ty, &cur_ty) {
                    return Err(format!(
                        "cannot assign {} {} to `{}`, which was declared {}",
                        typed.ty.article(), typed.ty, lvalue, cur_ty
                    ));
                }
                // B21. The rule the whole-name `Assign` arm has carried since v0.0.222, which
                // was never extended to the three spellings that reach a place through a field
                // or an index — so `region r { b.name = "hello-" + "world"; }` compiled clean and
                // `b.name` afterwards read the next allocation's bytes. Not a weak check: there
                // was none.
                if self.expr_allocates(&typed) {
                    // B25, and it is why this is no longer gated on a region being open: in
                    // `function rename(mutable b: Box, tag: String) { b.name = "n-" + tag; }`
                    // there IS no region, and the storage still lands in the caller's `b`.
                    // Measured — the field spelling of B25 corrupted just as the `push` one did.
                    self.record_param_growth(name);
                    if self.current_region.is_some() {
                        if let Some(open) = self.declared_outside_open_region(name) {
                            return Err(Self::assigning_into_outer_region(name, &open, "field"));
                        }
                        // Declared INSIDE the region, so the store is fine — but the binding now
                        // holds region storage, and `return b` has to be refused exactly as
                        // `return b.name` already is. The whole-name arm records this; without
                        // the same line here the taint is lost the moment it arrives via a field.
                        self.region_locals.insert(name.clone());
                    }
                }
                Ok(TypedStmtKind::AssignField { name: name.clone(), indices, value: typed })
            }
            StmtKind::AssignFieldIndex { name, path, index, value } => {
                self.blame_target(s, name);
                let lvalue = format!("{}.{}", name, path.join("."));
                let (mut cur_ty, mutable) = self
                    .env
                    .get(name)
                    .ok_or_else(|| self.unknown_name(name))?
                    .clone();
                if !mutable {
                    return Err(format!(
                        "cannot assign to `{}[...]`: `{}` was declared immutable. {}",
                        lvalue,
                        name,
                        self.how_to_make_writable(name, &cur_ty)
                    ));
                }
                let mut indices = Vec::new();
                for field in path {
                    let (i, t) = self.resolve_field(&cur_ty, field)?;
                    indices.push(i);
                    cur_ty = t;
                }
                // A growable array field is assignable too, with `len` 0 marking a bound
                // that is only known at run time. `self.cache[i] = v` is what an indexed
                // table wants to write, and refusing it forced a linear search instead.
                let (elem, len) = match &cur_ty {
                    Type::Array { elem, len } => (elem.as_ref().clone(), *len),
                    Type::Slice(elem) => (elem.as_ref().clone(), 0),
                    other => {
                        return Err(format!(
                            "`{}[...]` indexing needs an array, but `{}` has type {}",
                            lvalue, lvalue, other
                        ))
                    }
                };
                let index = self.check_index(&format!("{}", cur_ty), len, index)?;
                let typed = self.check_expr(value, Some(&elem))?;
                if !self.storable(&typed.ty, &elem) {
                    return Err(format!(
                        "cannot assign {} {} to `{}[...]`, which holds {}",
                        typed.ty.article(),
                        typed.ty,
                        lvalue,
                        elem
                    ));
                }
                // B21, reached through a field AND an index: `g.items[0] = "a" + "b"`.
                if self.expr_allocates(&typed) {
                    self.record_param_growth(name);          // B25, same reason as the field arm
                    if self.current_region.is_some() {
                        if let Some(open) = self.declared_outside_open_region(name) {
                            return Err(Self::assigning_into_outer_region(name, &open, "element"));
                        }
                        self.region_locals.insert(name.clone());
                    }
                }
                Ok(TypedStmtKind::AssignFieldIndex {
                    name: name.clone(),
                    indices,
                    len,
                    index,
                    value: typed,
                })
            }
            StmtKind::AssignIndex { name, index, value } => {
                self.blame_target(s, name);
                let (declared, mutable) = self
                    .env
                    .get(name)
                    .ok_or_else(|| format!("unknown variable: {}", name))?
                    .clone();
                // A growable array is assignable too, and its bound is its LENGTH, which is
                // only known at run time — so `len` is 0 here and codegen checks the
                // header. Stage-1 has allowed this since it had arrays at all; stage-0
                // refusing it was a divergence found by writing a program that needed it.
                let (elem, len) = match &declared {
                    Type::Array { elem, len } => (elem.as_ref().clone(), *len),
                    Type::Slice(elem) => (elem.as_ref().clone(), 0),
                    other => {
                        return Err(format!(
                            "`{}[...]` indexing needs an array, but `{}` has type {}",
                            name, name, other
                        ))
                    }
                };
                if !mutable {
                    return Err(format!(
                        "cannot assign to `{}[...]`: `{}` was declared immutable. {}",
                        name,
                        name,
                        self.how_to_make_writable(name, &declared)
                    ));
                }
                let index = self.check_index(&format!("{}", declared), len, index)?;
                let typed = self.check_expr(value, Some(&elem))?;
                if !self.storable(&typed.ty, &elem) {
                    return Err(format!(
                        "cannot assign {} {} to `{}[...]`, which holds {}",
                        typed.ty.article(), typed.ty, name, elem
                    ));
                }
                // B21, reached through an index: `names[0] = "a" + "b"`.
                if self.expr_allocates(&typed) {
                    self.record_param_growth(name);          // B25, same reason as the field arm
                    if self.current_region.is_some() {
                        if let Some(open) = self.declared_outside_open_region(name) {
                            return Err(Self::assigning_into_outer_region(name, &open, "element"));
                        }
                        self.region_locals.insert(name.clone());
                    }
                }
                Ok(TypedStmtKind::AssignIndex { name: name.clone(), len, index, value: typed })
            }
            StmtKind::ExprStmt(e) => {
                // `exit(code)` — a STATEMENT, handled here rather than as a builtin call, because a
                // builtin has to answer with a type and this one never answers at all. Typing it
                // `Int` would be a small lie in a language whose whole argument is that it does not
                // tell them; refusing it in a value position costs one arm and says the truth.
                if let ExprKind::Call { name, arguments, .. } = &e.kind {
                    if name == "exit" {
                        if let Some(why) = self.impure("exit") {
                            return Err(why);
                        }
                        if arguments.len() != 1 {
                            return Err(
                                "exit(code) takes one Int — the status a shell reads".to_string()
                            );
                        }
                        let code = self.check_expr(&arguments[0], Some(&Type::Int))?;
                        if code.ty != Type::Int {
                            return Err(format!(
                                "exit(code) takes an Int, but this has type {}",
                                code.ty
                            ));
                        }
                        // 0..=255, because that is what a status IS. POSIX hands the shell the low
                        // eight bits, so `exit(256)` arrives as 0 — a program reporting SUCCESS
                        // while trying to report failure, which is the worst direction for this
                        // particular mistake to go. A literal is refused now; anything else is
                        // checked at runtime.
                        if let TypedExprKind::IntLit(n) = &code.kind {
                            if *n < 0 || *n > 255 {
                                return Err(format!(
                                    "exit({}) cannot be reported: a process status is 0 to 255, \
                                     and a shell reads only the low eight bits — so {} would \
                                     arrive as {}. Pick a status in range.",
                                    n, n, n & 0xff
                                ));
                            }
                        }
                        return Ok(TypedStmtKind::Exit(code));
                    }
                }
                let typed = self.check_expr(e, None)?;
                Ok(TypedStmtKind::ExprStmt(typed))
            }
            StmtKind::Region { name, body } => {
                // One level only in this slice — nesting is deferred with a
                // reason rather than half-supported.
                if let Some(open) = &self.current_region {
                    return Err(format!(
                        "`region {}` cannot open inside `region {}` — nested regions \
                         are not available yet. Close the outer one first, or use a \
                         single region for both.",
                        name, open
                    ));
                }
                if self.env.contains_key(name) {
                    return Err(format!(
                        "`{}` is already a variable, so it cannot name a region too \
                         — pick a different name.",
                        name
                    ));
                }
                self.current_region = Some(name.clone());
                let outer_scope = std::mem::take(&mut self.region_scope);
                let checked = self.check_block(body);
                self.region_scope = outer_scope;
                self.current_region = None;
                Ok(TypedStmtKind::Region { name: name.clone(), body: checked? })
            }
            StmtKind::Match { value, arms } => {
                let scrutinee = self.check_expr(value, None)?;
                // Inside a generic being checked generically the scrutinee's type is still
                // `Option<T>`: its VARIANTS are known even though `T` is not, so the arms,
                // the exhaustiveness and the payload bindings can all be checked here —
                // once, at the declaration — rather than at every instantiation.
                if let Type::Generic { name, arguments } = &scrutinee.ty {
                    if let Some((parameters, variants)) = self.generic_enums.get(name).cloned() {
                        let map: HashMap<String, Type> =
                            parameters.iter().map(|p| p.name.clone()).zip(arguments.iter().cloned()).collect();
                        let open: Vec<(String, Vec<Type>)> = variants
                            .into_iter()
                            .map(|(v, payload)| {
                                (v, payload.iter().map(|t| substitute(t, &map)).collect())
                            })
                            .collect();
                        let shown = show(&scrutinee.ty, &self.instance_of.borrow().clone());
                        return self.check_match_arms(
                            open,
                            scrutinee,
                            arms,
                            s.span,
                            shown,
                        );
                    }
                }
                // A SCALAR match — `match status { 200 => ..., _ => ... }`. Desugared to an
                // `if / else if` chain right here, so nothing below this and nothing in either
                // backend learns a new statement kind, and the comparison is the ordinary `==`
                // that is already correct for an Int and already calls `burxt.streq` for a
                // String. No new branching to get wrong, which matters more in a money language
                // than a switch table does.
                if matches!(
                    scrutinee.ty,
                    Type::Int | Type::Bool | Type::String | Type::Decimal { .. }
                ) {
                    return self.desugar_scalar_match(value, scrutinee, arms, s.span);
                }
                let enum_name = match &scrutinee.ty {
                    Type::Named(n) if self.is_enum(n) => n.clone(),
                    other => {
                        return Err(format!(
                            "`match` needs an enum value or a scalar, but this has type {}. \
                             Use `if` to branch on anything else.",
                            other
                        ))
                    }
                };
                let variants = self
                    .variants_of(&enum_name)
                    .ok_or_else(|| format!("codegen bug: no enum named `{}`", enum_name))?;

                let shown = show(&scrutinee.ty, &self.instance_of.borrow().clone());
                return self.check_match_arms(variants, scrutinee, arms, s.span, shown);
            }
            StmtKind::Break | StmtKind::Continue => {
                let word = if matches!(s.kind, StmtKind::Break) { "break" } else { "continue" };
                if self.loop_depth == 0 {
                    return Err(format!(
                        "`{}` only means something inside a loop: there is none here.",
                        word
                    ));
                }
                Ok(if word == "break" { TypedStmtKind::Break } else { TypedStmtKind::Continue })
            }
            StmtKind::For { name, iterable, body } => {
                // A `for` binding is a binding, so it may not take a const's name either. The
                // third of four places a name enters scope; `let`, a parameter and a `match`
                // arm are the others, and all four ask this because a name that resolves to
                // one thing in one body and another thing elsewhere is the silent wrongness
                // no-shadowing exists to prevent.
                if let Some(message) = self.shadows_a_const(name) {
                    return Err(message);
                }
                if self.env.contains_key(name) {
                    return Err(format!(
                        "`{}` is already declared — Burxt does not allow shadowing, and \
                         a `for` binding is a binding. Use a different name.",
                        name
                    ));
                }
                let iterable = self.check_expr(iterable, None)?;
                let elem = match &iterable.ty {
                    Type::Array { elem, .. } => elem.as_ref().clone(),
                    Type::Slice(elem) => elem.as_ref().clone(),
                    Type::String => {
                        return Err(
                            "`for` iterates an array, and a String is bytes: use \
                             `byte_at(s, i)`, which says BYTE so the byte-versus-character \
                             question cannot hide."
                                .to_string(),
                        )
                    }
                    other => {
                        return Err(format!(
                            "`for` iterates an array, and this is {} {}",
                            other.article(),
                            other
                        ))
                    }
                };
                // The element is a copy, and immutable: value semantics is not negotiable
                // for a convenience, and nothing may be written back through it.
                let saved = self.env.clone();
                self.env.insert(name.clone(), (elem.clone(), false));
                // B27, the loop half. A copy of an element is not a copy of the STORAGE it
                // points at: `for n in made { h.tag = n; }` over an array built inside the
                // region handed `h` the region's bytes, because the loop binding carried no
                // taint. Same rule as a pattern binding, same reason.
                let taint = self.current_region.is_some()
                    && self.expr_allocates(&iterable)
                    && self.may_be_region_storage(&elem);
                // `insert` answers false when the name was already tainted, which is exactly
                // the question "is this loop the one that has to take the taint back out?" —
                // the binding is gone at the closing brace, and a later name reusing the
                // spelling must not inherit it.
                let tainted = taint && self.region_locals.insert(name.clone());
                self.loop_depth += 1;
                let body = self.check_block(body);
                self.loop_depth -= 1;
                self.env = saved;
                if tainted {
                    self.region_locals.remove(name);
                }
                Ok(TypedStmtKind::For {
                    name: name.clone(),
                    elem,
                    iterable,
                    body: body?,
                })
            }
            StmtKind::ForRange { name, start, end, body } => {
                // A range's counter is a binding, so it may not take a const's name either —
                // the same check the array `for` above makes, and now the FIFTH place a name
                // enters scope rather than the fourth.
                //
                // This line is here because it was MISSING and measured missing. A6 was written
                // against a tree with no `const`; A2 added `shadows_a_const` to `let`, to a
                // parameter, to a `match` arm and to the array `for`, and a new statement kind
                // that binds a name is invisible to all four. The suite stayed green — no pass
                // fixture writes `const N` and `for N in 0..3` — and stage-1 was STRICTER than
                // stage-0, because its range arm was copied from an array arm that already had
                // the check. Found by running the combination rather than by reading either
                // side; the asymmetry is the tell, and `for N in 0..3` compiling while
                // `for N in xs` was refused is not a difference anyone would defend.
                if let Some(message) = self.shadows_a_const(name) {
                    return Err(message);
                }
                if self.env.contains_key(name) {
                    return Err(format!(
                        "`{}` is already declared — Burxt does not allow shadowing, and \
                         a `for` binding is a binding. Use a different name.",
                        name
                    ));
                }
                // Both bounds are counts, so both are Ints. A Decimal bound is refused
                // rather than truncated: `1.0..2.0` lexes perfectly (see `lexer.rs`) and
                // the only honest place to stop it is here, where the types are known.
                // Naming the bound that is wrong matters — `0..total` where `total` is a
                // `Decimal<2>` is the mistake this catches, and "one of the bounds" would
                // send the reader to the wrong end of the line.
                let start = self.check_expr(start, Some(&Type::Int))?;
                let end = self.check_expr(end, Some(&Type::Int))?;
                for (which, bound) in [("start", &start), ("end", &end)] {
                    if bound.ty != Type::Int {
                        return Err(format!(
                            "a `for` range counts, so its {} must be an Int, and this is {} \
                             {}. Ranges are Int-only on purpose: a Decimal range would have \
                             to invent a step, and a String range an order.",
                            which,
                            bound.ty.article(),
                            bound.ty
                        ));
                    }
                }
                // Decision 5: a range spelled with two literals that counts DOWN can only
                // be a mistake, both values are in hand, and refusing costs nothing.
                // `-3..3` is covered for free, because `check_expr` already folds a negated
                // integer literal into an `IntLit` (see the "Fold negated literals" comment
                // in `check_expr`) — so this needed no arithmetic of its own.
                // `0..n - 1` where `n` is 0 is NOT caught and runs zero times, which is the
                // named limit: a compiler refuses what it can SEE. A `const` bound is not
                // folded here either, deliberately: guessing about it would be the false
                // `Some` that `literal_int` exists to refuse to give.
                if let (Some(a), Some(b)) = (literal_int(&start), literal_int(&end)) {
                    if a > b {
                        return Err(format!(
                            "`{}..{}` counts down, and a `for` range only counts up — it \
                             would run zero times. Both bounds are literals here, so this \
                             cannot be anything but a mistake. To walk backwards, count up \
                             and subtract: `for k in 0..{} {{ let i = {} - 1 - k; ... }}`.",
                            a,
                            b,
                            a - b,
                            a
                        ));
                    }
                }
                // The counter is immutable — decision 4. It is a fresh Int each pass, so an
                // assignment to it could only be thrown away, and a loop variable that can
                // be written is how an off-by-one hides.
                let saved = self.env.clone();
                self.env.insert(name.clone(), (Type::Int, false));
                self.loop_counters.push(name.clone());
                self.loop_depth += 1;
                let body = self.check_block(body);
                self.loop_depth -= 1;
                self.loop_counters.pop();
                self.env = saved;
                Ok(TypedStmtKind::ForRange {
                    name: name.clone(),
                    start,
                    end,
                    body: body?,
                })
            }
            StmtKind::While { cond, body } => {
                let cond = self.check_expr(cond, None)?;
                if cond.ty != Type::Bool {
                    return Err(format!(
                        "a `while` condition must be a Bool (e.g. a comparison), \
                         but this one has type {}",
                        cond.ty
                    ));
                }
                self.loop_depth += 1;
                let body = self.check_block(body);
                self.loop_depth -= 1;
                Ok(TypedStmtKind::While { cond, body: body? })
            }
            StmtKind::Print { value: e, to_stderr } => {
                if let Some(why) = self.impure("print") {
                    // Output is an effect: a pure function computes its result and
                    // does nothing else.
                    return Err(why);
                }
                // An interpolated string prints its pieces in order, which
                // needs no allocation — so it is handled here rather than as a
                // String-valued expression.
                if let ExprKind::InterpStr(parts) = &e.kind {
                    let mut typed_parts = Vec::new();
                    for p in parts {
                        match p {
                            InterpPart::Lit(text) => {
                                typed_parts.push(TypedInterpPart::Lit(text.clone()))
                            }
                            InterpPart::Expr(inner) => {
                                let t = self.check_expr(inner, None)?;
                                match &t.ty {
                                    Type::Int
                                    | Type::Bool
                                    | Type::String
                                    | Type::Decimal { .. } => {}
                                    Type::Param(p) => {
                                        return Err(unbounded(p, "interpolated"))
                                    }
                                    // Not "not yet" — never. An address differs between runs.
                                    Type::CPointer => {
                                        return Err(
                                            "a CPointer has no display form and will not get \
                                             one: an address differs between runs, so printing \
                                             it would make this program's output different \
                                             every time. Interpolate `c_string_at(p)` instead."
                                                .to_string(),
                                        )
                                    }
                                    other => {
                                        return Err(format!(
                                            "cannot interpolate {} {} — only Int, \
                                             Bool, String and Decimal have a display \
                                             form so far.",
                                            other.article(),
                                            other
                                        ))
                                    }
                                }
                                typed_parts.push(TypedInterpPart::Expr(t));
                            }
                        }
                    }
                    return Ok(TypedStmtKind::PrintInterp { parts: typed_parts, to_stderr: *to_stderr });
                }
                let typed = self.check_expr(e, None)?;
                match &typed.ty {
                    Type::Param(p) => return Err(unbounded(p, "printed")),
                    // Refused for a reason that IS the thesis rather than caution: an address
                    // differs between runs, so a program that printed one would not be
                    // reproducible — and a Burxt program's output being the same every time is
                    // the property everything else here is built to protect.
                    Type::CPointer => {
                        return Err(
                            "print does not show a CPointer, and never will: an address differs \
                             between runs, so printing one would make this program's output \
                             different every time. Ask `c_is_null(p)` whether the call failed, \
                             or `c_string_at(p)` for what it points at."
                                .to_string(),
                        )
                    }
                    Type::Named(n) if self.is_enum(n) => {
                        return Err(format!(
                            "print does not know how to show a {} — `match` on it and \
                             print what each variant carries.",
                            n
                        ))
                    }
                    Type::Named(n) => {
                        return Err(format!(
                            "print does not know how to show a {} — print its fields.",
                            n
                        ))
                    }
                    Type::Array { .. } => {
                        return Err(format!(
                            "print does not know how to show a {} — print its \
                             elements.",
                            typed.ty
                        ))
                    }
                    Type::Dyn(t) => {
                        return Err(format!(
                            "print does not know how to show a `dynamic {}` — an interface \
                             object exposes only its trait methods, so call one \
                             and print that.",
                            self.shown_type_name(t)
                        ))
                    }
                    _ => {}
                }
                Ok(TypedStmtKind::Print { value: typed, to_stderr: *to_stderr })
            }
            StmtKind::Return(e) => {
                // The keyword, for the same reason and to the same length stage-1 uses.
                self.blame_target(s, "return");
                let ret = self.current_ret.clone().ok_or_else(|| {
                    "`return` only makes sense inside a function".to_string()
                })?;
                let typed = self.check_expr(e, Some(&ret))?;
                if !self.storable(&typed.ty, &ret) {
                    self.blame(e.span);
                    return Err(format!(
                        "this function returns {}, but the `return` expression has type {}",
                        ret, typed.ty
                    ));
                }
                // B32, before the escape rule rather than inside it: this is where the probe
                // learns whether the function hands its caller's storage back. It has to be
                // asked on EVERY return, not only the ones a region makes interesting — a
                // relay is usually written with no region in sight, and its call sites are
                // what the escape rules below then get right.
                self.record_relay(&typed);
                // Escape checking, the expression-level half. Which region the value
                // was built in decides everything:
                //
                //   - inside a `region` block THIS function opened: that region ends
                //     at the closing brace, so returning it dangles. Refused.
                //   - inside an `allocates` function with no local region: the bytes
                //     are the CALLER's, and the caller's region is still open when it
                //     receives them. Fine — this is the whole point of `allocates`.
                // The escape rule, and after slice 2 it is the ONLY one left: a value built
                // inside a `region` block cannot leave it, because that block releases at its
                // closing brace. Outside one there is nothing to escape from — the bytes live
                // in the program's arena and outlive every call.
                //
                // This used to also demand `in_caller_region`, i.e. the `allocates` word. That
                // half is gone: it asked whether the programmer had declared where the bytes
                // belong, and nothing releases them now except a region the programmer opened.
                // `may_be_region_storage` on the RETURNED TYPE, because `expr_allocates` answers
                // a question about the expression's history rather than about the value. A call
                // that allocated is a call that allocated; if it answered an `Int`, that `Int`
                // points at nothing and outlives anything.
                //
                // Without the gate this refused `let n: Int = measure(text); return n;` inside a
                // region — where `measure` builds a String and answers its length — and told the
                // author to "return a scalar summary", which is what they had written. The taint
                // travelled from the callee to a value that cannot carry it.
                //
                // This is the same substitution the escape rule made, one pass over: asking what
                // a function DID where the question is what the value IS. `record_relay` already
                // carries this gate for the same reason, and its comment says so — a getter
                // handing back an `Int` field taints nothing.
                if self.expr_allocates(&typed)
                    && self.may_be_region_storage(&typed.ty)
                    && self.current_region.is_some()
                {
                    // Probing: this is the SECOND way a function turns out to allocate, and
                    // missing it made the inference silently incomplete rather than wrong.
                    //
                    // `has_region` catches the first way — something inside the body wanted a
                    // region. But a function can allocate purely by RETURNING built data it
                    // never bound. `lib/map.bx`'s `map_new` is exactly that:
                    //
                    //     function map_new<K: Equatable, V>() -> Map<K, V> {
                    //         return Map { entries: [], slots: [], live: 0 };
                    //     }
                    //
                    // Two slice literals inside a struct literal, straight into the `return`.
                    // No `let`, so nothing ever asked `has_region`, so the probe credited
                    // nothing and the function stayed refused forever. `catalogue()` in the POS
                    // only worked because it happens to bind `let mutable shelf: [Item] = []`
                    // first — which is luck, not a rule.
                    //
                    // Asking `has_region` here classes it, and classes it in the one place that
                    // knows how: it credits the owner only when no local region is open, which
                    // is the same condition this rule is testing.
                    if self.probing {
                        let _ = self.has_region();
                        return Ok(TypedStmtKind::Return(typed));
                    }
                    return Err(format!(
                        "cannot return this {}: it was built inside a `region` block, which \
                         releases at its closing brace, so its storage would not outlive the \
                         call. Move the allocation out of the `region` block, or return a \
                         scalar summary.",
                        typed.ty
                    ));
                }
                Ok(TypedStmtKind::Return(typed))
            }
            StmtKind::TailReturn(e) => self.check_tail_return(e),
            StmtKind::If { cond, then_block, else_block } => {
                let cond = self.check_expr(cond, None)?;
                if cond.ty != Type::Bool {
                    return Err(format!(
                        "an `if` condition must be a Bool (e.g. a comparison), \
                         but this one has type {}",
                        cond.ty
                    ));
                }
                let then_block = self.check_block(then_block)?;
                let else_block = match else_block {
                    Some(b) => Some(self.check_block(b)?),
                    None => None,
                };
                Ok(TypedStmtKind::If { cond, then_block, else_block })
            }
        }
    }

    /// `expected` is the type context (the declared type of the enclosing
    /// `let`), used to normalize decimal literals to the right scale.
    /// Check an expression, and on the way record two things the checking itself
    /// does not need: this expression's resolved type (for hover), and — if it
    /// failed — its position, unless something further in has already claimed it.
    ///
    /// "Innermost claims it" is what makes the caret land on the sub-expression
    /// that is actually wrong instead of the whole statement: a child's wrapper
    /// runs before its parent's as the error propagates out.
    fn check_expr(&self, e: &Expr, expected: Option<&Type>) -> Result<TypedExpr, String> {
        let result = self.check_expr_kind(e, expected);
        match &result {
            Ok(typed) => self.expr_types.borrow_mut().push((e.span, typed.ty.clone())),
            Err(_) => {
                if !self.error_located.get() {
                    self.error_located.set(true);
                    self.current_span.set(e.span);
                }
            }
        }
        result
    }

    /// B17, for a CALL: the caret goes on the callee, not on the call and its arguments.
    ///
    /// A call expression begins at its callee name, so this needs nothing the AST does not
    /// already carry. `blame` rather than a bare `current_span.set`, because this fires
    /// from inside the expression checker and the wrapper there would otherwise claim the
    /// whole `push(xs, 11)`.
    ///
    /// The METHOD spelling is deliberately not done here: stage-1 underlines `add` in
    /// `bag.add(11)`, which starts after the receiver's text, and deriving that offset
    /// would mean assuming there is exactly one character between the receiver and the
    /// name. `bag . add(11)` would put the caret in the wrong place, silently. That one
    /// needs a real span on the method name, which lives in the parser.
    fn blame_callee(&self, call: Span, name: &str) {
        let start = call.start as usize;
        self.blame(Span::new(start, start + name.len()));
    }

    fn check_expr_kind(&self, e: &Expr, expected: Option<&Type>) -> Result<TypedExpr, String> {
        // A concrete value becomes an interface object wherever one is expected.
        if let Some(Type::Dyn(t)) = expected {
            let already = match &e.kind {
                ExprKind::Var(n) => {
                    matches!(self.env.get(n), Some((Type::Dyn(have), _)) if have == t)
                }
                _ => false,
            };
            if !already && !matches!(e.kind, ExprKind::MethodCall { .. } | ExprKind::Call { .. }) {
                return self.coerce_dyn(t, e);
            }
        }
        match &e.kind {
            // `e?` — the value, or an immediate return of the failure. Two decisions live
            // here, both from spec/1.0/M8-ERRORS.md §1a: the failure variant is recognised by
            // NAME (`Error` or `None`), never by the enum's type name, so a library type gets
            // it and a hardcoded one is not needed; and there is NO conversion, so the
            // enclosing function must fail the same way with the same payload.
            ExprKind::Try(inner) => {
                let value = self.check_expr(inner, None)?;
                let instances = self.instance_of.borrow().clone();
                let Type::Named(enum_name) = &value.ty else {
                    return Err(format!(
                        "`?` needs a value that is either a success or a failure, and this \
                         is {} {}. It works on an enum with two variants, one of them \
                         `Error` or `None`.",
                        value.ty.article(),
                        show(&value.ty, &instances)
                    ));
                };
                let Some(variants) = self.variants_of(enum_name) else {
                    return Err(format!(
                        "`?` needs an enum with two variants, and {} is not an enum.",
                        show(&value.ty, &instances)
                    ));
                };
                let shown = show(&value.ty, &instances);
                if variants.len() != 2 {
                    return Err(format!(
                        "`?` needs an enum with exactly two variants — a success and a \
                         failure — and {} has {}. Use `match`.",
                        shown,
                        variants.len()
                    ));
                }
                let Some(fail_at) = variants
                    .iter()
                    .position(|(n, _)| n == "Error" || n == "None")
                else {
                    return Err(format!(
                        "`?` recognises a failure by the variant's NAME — `Error` or `None` — \
                         and {} has neither. Rename the failing variant, or use `match`.",
                        shown
                    ));
                };
                let fail_name = variants[fail_at].0.clone();
                let fail_payload = variants[fail_at].1.clone();
                let ok_at = 1 - fail_at;
                let ok_payload = &variants[ok_at].1;
                if ok_payload.len() != 1 {
                    return Err(format!(
                        "`?` yields the success variant's value, and `{}.{}` carries {}. \
                         It must carry exactly one.",
                        shown,
                        variants[ok_at].0,
                        ok_payload.len()
                    ));
                }
                let yielded = ok_payload[0].clone();

                // The enclosing function has to fail the same way — Decision A.
                let Some(ret) = self.current_ret.clone() else {
                    return Err(
                        "`?` returns the failure from the enclosing function, and a \
                         top-level statement has none to return from. Put it in a `function` \
                         that answers with a failure of its own, or use `match`."
                            .to_string(),
                    );
                };
                let ret_variants = match &ret {
                    Type::Named(n) => self.variants_of(n),
                    _ => None,
                };
                let Some(ret_variants) = ret_variants else {
                    return Err(format!(
                        "`?` returns the failure from the enclosing function, which \
                         answers with {} — not something that can carry a failure. Give it \
                         a return type with an `{}` variant, or use `match`.",
                        show(&ret, &instances),
                        fail_name
                    ));
                };
                let Some(ret_fail_at) = ret_variants.iter().position(|(n, _)| *n == fail_name)
                else {
                    return Err(format!(
                        "`?` returns `{}` from the enclosing function, and {} has no `{}` \
                         variant. Make the two agree, or use `match`.",
                        fail_name,
                        show(&ret, &instances),
                        fail_name
                    ));
                };
                if ret_variants[ret_fail_at].1 != fail_payload {
                    return Err(format!(
                        "`?` does not convert between failures: this one carries {}, and \
                         the enclosing function's `{}` carries {}. Write the `match`, or \
                         make the two agree.",
                        Self::type_list(&fail_payload),
                        fail_name,
                        Self::type_list(&ret_variants[ret_fail_at].1)
                    ));
                }
                Ok(TypedExpr {
                    ty: yielded,
                    kind: TypedExprKind::Try {
                        value: Box::new(value),
                        fail_tag: fail_at as u32,
                        ok_tag: ok_at as u32,
                        ret_enum: match &ret {
                            Type::Named(n) => n.clone(),
                            _ => unreachable!("checked above"),
                        },
                        ret_fail_tag: ret_fail_at as u32,
                    },
                })
            }
            ExprKind::IntLit(n) => Ok(TypedExpr { ty: Type::Int, kind: TypedExprKind::IntLit(*n) }),

            ExprKind::BoolLit(b) => Ok(TypedExpr { ty: Type::Bool, kind: TypedExprKind::BoolLit(*b) }),

            ExprKind::StrLit(s) => {
                Ok(TypedExpr { ty: Type::String, kind: TypedExprKind::StrLit(s.clone()) })
            }

            // Producing a String VALUE from interpolation is exactly joining the
            // pieces, so it desugars to `to_string` + `+` rather than getting a
            // second lowering. One formatter, one concatenation, no drift: an
            // interpolated value and the hand-written join it stands for are the
            // same program by construction.
            ExprKind::InterpStr(parts) => {
                if !self.has_region() {
                    return Err(self.needs_region(
                        "building a String from interpolation allocates (printing one \
                         directly does not)",
                    ));
                }
                let mut joined: Option<TypedExpr> = None;
                for part in parts {
                    let piece = match part {
                        InterpPart::Lit(text) => TypedExpr {
                            ty: Type::String,
                            kind: TypedExprKind::StrLit(text.clone()),
                        },
                        InterpPart::Expr(inner) => {
                            let t = self.check_expr(inner, None)?;
                            match &t.ty {
                                // Already bytes — join it as it stands.
                                Type::String => t,
                                Type::Int | Type::Bool | Type::Decimal { .. } => TypedExpr {
                                    ty: Type::String,
                                    kind: TypedExprKind::ToString(Box::new(t)),
                                },
                                other => {
                                    return Err(format!(
                                        "cannot interpolate {} {} — only Int, Bool, \
                                         String and Decimal have a display form so far.",
                                        other.article(),
                                        other
                                    ))
                                }
                            }
                        }
                    };
                    joined = Some(match joined {
                        None => piece,
                        Some(acc) => TypedExpr {
                            ty: Type::String,
                            kind: TypedExprKind::Binary {
                                op: BinOp::Add,
                                lhs: Box::new(acc),
                                rhs: Box::new(piece),
                            },
                        },
                    });
                }
                // `""` interpolates to the empty string, which is a literal.
                Ok(joined.unwrap_or(TypedExpr {
                    ty: Type::String,
                    kind: TypedExprKind::StrLit(String::new()),
                }))
            }

            ExprKind::DecimalLit { unscaled, scale } => {
                // Determine the target scale (and rounding contract) from
                // context if available. The contract never rounds the literal
                // itself — literals must be exactly representable.
                let (target_scale, rounding) = match expected {
                    Some(Type::Decimal { scale: s, rounding }) => (*s, *rounding),
                    // No decimal context: the literal's own scale is its type.
                    _ => (*scale, None),
                };
                if target_scale > 18 {
                    return Err(format!(
                        "this decimal literal has {} fractional digits, but Decimal \
                         supports at most 18 — a scaled i64 holds no more",
                        target_scale
                    ));
                }
                let normalized = normalize_decimal(*unscaled, *scale, target_scale)?;
                Ok(TypedExpr {
                    ty: Type::Decimal { scale: target_scale, rounding },
                    kind: TypedExprKind::DecimalLit { unscaled: normalized },
                })
            }

            ExprKind::Var(name) => {
                // A const is looked for only after the bindings, so the common path costs one
                // lookup in an almost-always-empty map. There is no ambiguity to resolve:
                // `shadows_a_const` refuses a binding or a parameter that reuses the name, so
                // at most one of the two tables can hold it.
                if let Some((ty, value)) = self.consts.get(name) {
                    // The whole of what a const costs at run time: nothing. It reaches codegen
                    // as the literal it folded to, which is why `codegen.rs` needed no change
                    // and why a const is legal inside a `pure` function.
                    return Ok(TypedExpr { ty: ty.clone(), kind: value.clone() });
                }
                let (ty, _) = self
                    .env
                    .get(name)
                    .ok_or_else(|| self.unknown_name(name))?
                    .clone();
                Ok(TypedExpr { ty, kind: TypedExprKind::Var(name.clone()) })
            }

            ExprKind::Neg(inner) => {
                let t = self.check_expr(inner, expected)?;
                match &t.ty {
                    Type::Int | Type::Decimal { .. } => {}
                    other => {
                        return Err(format!(
                            "`-` needs a number, but this has type {}",
                            other
                        ))
                    }
                }
                // Fold negated literals so `-19.99` IS a literal (it can then
                // sit anywhere a literal can, and needs no runtime work).
                let kind = match t.kind {
                    TypedExprKind::IntLit(n) => TypedExprKind::IntLit(
                        n.checked_neg().ok_or("integer literal too small")?,
                    ),
                    TypedExprKind::DecimalLit { unscaled } => TypedExprKind::DecimalLit {
                        unscaled: unscaled.checked_neg().ok_or("decimal literal too small")?,
                    },
                    other => TypedExprKind::Neg(Box::new(TypedExpr { ty: t.ty.clone(), kind: other })),
                };
                Ok(TypedExpr { ty: t.ty, kind })
            }

            ExprKind::Not(inner) => {
                let t = self.check_expr(inner, None)?;
                if t.ty != Type::Bool {
                    return Err(format!(
                        "`!` needs a Bool, but this has type {} — Burxt has no \
                         truthiness, so there is nothing to negate.",
                        t.ty
                    ));
                }
                Ok(TypedExpr { ty: Type::Bool, kind: TypedExprKind::Not(Box::new(t)) })
            }

            ExprKind::Logical { op, lhs, rhs } => {
                // Both sides must be Bool: `&&`/`||` are not a coercion site.
                let l = self.check_expr(lhs, None)?;
                if l.ty != Type::Bool {
                    return Err(format!(
                        "the left side of `{}` must be a Bool, but it has type {} — \
                         Burxt has no truthiness.",
                        op, l.ty
                    ));
                }
                let r = self.check_expr(rhs, None)?;
                if r.ty != Type::Bool {
                    return Err(format!(
                        "the right side of `{}` must be a Bool, but it has type {} — \
                         Burxt has no truthiness.",
                        op, r.ty
                    ));
                }
                Ok(TypedExpr {
                    ty: Type::Bool,
                    kind: TypedExprKind::Logical {
                        op: *op,
                        lhs: Box::new(l),
                        rhs: Box::new(r),
                    },
                })
            }

            ExprKind::Binary { op, lhs, rhs } => {
                // Multiplication may mix scales (money × rate), so a literal
                // operand must not be forced to the result's scale — that is
                // what used to make `price * 8.25%` fail, since 0.0825 cannot
                // narrow to 2 places. If pushing the expected type into the
                // operands fails, re-check them at their own natural types and
                // let the result's rounding contract land the product.
                let (l, r) = match (
                    self.check_expr(lhs, expected),
                    self.check_expr(rhs, expected),
                ) {
                    (Ok(l), Ok(r)) => (l, r),
                    (first, second) if *op == BinOp::Mul => {
                        let original = first.err().or(second.err());
                        match (self.check_expr(lhs, None), self.check_expr(rhs, None)) {
                            (Ok(l), Ok(r)) => (l, r),
                            _ => return Err(original.expect("one attempt failed")),
                        }
                    }
                    (first, second) => return Err(first.err().or(second.err()).unwrap()),
                };
                let result_ty = self.check_binop(*op, &l.ty, &r.ty, expected)?;
                Ok(TypedExpr {
                    ty: result_ty,
                    kind: TypedExprKind::Binary {
                        op: *op,
                        lhs: Box::new(l),
                        rhs: Box::new(r),
                    },
                })
            }

            ExprKind::Compare { op, lhs, rhs } => {
                // The left side sets the type; the right side is checked
                // against it, so a literal like `0.00` adopts the money type
                // it is compared with (`balance > 0.00` just works).
                let l = self.check_expr(lhs, None)?;
                let r = self.check_expr(rhs, Some(&l.ty.clone()))?;
                self.check_compare(*op, &l.ty, &r.ty)?;
                Ok(TypedExpr {
                    ty: Type::Bool,
                    kind: TypedExprKind::Compare {
                        op: *op,
                        lhs: Box::new(l),
                        rhs: Box::new(r),
                    },
                })
            }

            ExprKind::Call { name, arguments } => {
                // What the callee reaches, the caller must admit to reaching. Placed at the TOP of
                // this arm on purpose: builtins, externs and ordinary functions all arrive here,
                // and one rule over one table means the three cannot disagree.
                //
                // My first attempt put this further down, past the builtin dispatch — so
                // `read_file` was invisible to the rule that exists to catch exactly that, which
                // is the failure mode this whole feature is about. Placement was the bug, not the
                // rule.
                //
                // Silent inside a `pure` function, so the purity rule below speaks instead. Both
                // are true there — a pure function declares no effects and may reach none — but
                // `pure function f may not read a file` names the PROMISE being broken, and the
                // effect message would only name the bookkeeping. One rule speaks per situation,
                // and the more specific one wins.
                if self.in_pure.is_none() {
                    if let Some(reaches) = self.fn_effects.get(name).cloned() {
                        for effect in &reaches {
                            if !self.allowed_effects.contains(effect) {
                                return Err(self.effect_refusal(name, *effect));
                            }
                        }
                    }
                }
                // `len` is a builtin over both arrays and strings, but the two
                // are different KINDS of length, and the difference is worth
                // keeping visible:
                //   * an array's length lives in its TYPE, so it folds to a
                //     constant and codegen never sees the call;
                //   * a string's length is a property of its DATA, so it is a
                //     byte scan at runtime.
                // `byte_at(s, i)` — the i-th BYTE of a string, bounds-checked.
                // Named for bytes on purpose: A4.4 refused a bare `s[i]`
                // because it would hide whether you get a byte or a character.
                // `push(xs, v)` appends to a growable array, growing it in the
                // region when it is full. It needs a mutable PLACE, the same
                // rule element assignment follows.
                // `truncate(xs, n)` — the only way to make a growable array shorter.
                // Earned by a self-hosted checker: leaving a block has to drop the
                // bindings it made, and without this a scope could only ever grow.
                if name == "truncate" {
                    if arguments.len() != 2 {
                        return Err(
                            "truncate(...) takes a growable array and a length: \
                             truncate(xs, n)"
                                .to_string(),
                        );
                    }
                    let place = self.check_expr(&arguments[0], None)?;
                    if !matches!(place.ty, Type::Slice(_)) {
                        return Err(format!(
                            "truncate(...) needs a growable array `[T]`, but this has \
                             type {}",
                            place.ty
                        ));
                    }
                    self.require_mutable_place(&arguments[0])?;
                    let length = self.check_expr(&arguments[1], Some(&Type::Int))?;
                    if length.ty != Type::Int {
                        return Err(format!(
                            "truncate(...) takes an Int length, but this has type {}",
                            length.ty
                        ));
                    }
                    return Ok(TypedExpr {
                        ty: Type::Int,
                        kind: TypedExprKind::Truncate {
                            place: Box::new(place),
                            length: Box::new(length),
                        },
                    });
                }
                if name == "push" {
                    if arguments.len() != 2 {
                        return Err(
                            "push(...) takes a growable array and a value: \
                             push(xs, v)"
                                .to_string(),
                        );
                    }
                    let place = self.check_expr(&arguments[0], None)?;
                    let elem = match &place.ty {
                        Type::Slice(e) => e.as_ref().clone(),
                        other => {
                            return Err(format!(
                                "push(...) needs a growable array `[T]`, but this has \
                                 type {}",
                                other
                            ))
                        }
                    };
                    self.require_mutable_place(&arguments[0])?;
                    // B22: `push` ALLOCATES, and until v0.0.264 it never said so.
                    //
                    // `has_region` is the sole recorder — the probe credits the owning function
                    // only when something in its body asks. `push` grows through
                    // `build_alloc_array` + memcpy, which is `burxt.alloc`, and it never asked. So
                    //
                    //     function fill(mutable dst: [Int], n: Int) -> Int allocates nothing {
                    //         while i < n { push(dst, i); i = i + 1; }
                    //
                    // was ACCEPTED with the claim intact: a signature saying "nothing" about a
                    // function that allocates, which for a language whose case is that a reviewer
                    // can trust what a signature says is worse than a crash. The direct form
                    // `let mutable xs: [Int] = []; push(xs, n);` was caught, but only because the
                    // `let` asks — so the hole was exactly "the storage came from the caller".
                    //
                    // Asked BEFORE the refusal below, because under probing this RECORDS, and a
                    // body that gets refused must still be classed: the probe discards errors and
                    // carries on, and a function credited only on the paths that typecheck would
                    // make the inference depend on which round found what.
                    let _ = self.has_region();
                    // B20: growing a container declared outside the open region is a
                    // use-after-free, and it was silent. `push` builds a FRESH buffer — the
                    // arena's next bytes — and stores it into the binding; the region's closing
                    // brace puts the bump pointer back, and the binding is left reading whatever
                    // the region hands out next. Measured on v0.0.263: five pushes into an outer
                    // `xs` inside a region, four into a later `ys`, and `xs[0]` printed 777.
                    //
                    // Same rule as whole-name assignment, asked about the RECEIVER rather than
                    // the value: the value is irrelevant here, because it is the buffer that
                    // moves into the region, not what is being appended.
                    if let Some(root) = Self::place_root(&arguments[0]) {
                        // B25's fact, recorded where the growth is: if this `push` grows one of
                        // the enclosing function's `mutable` parameters, then calling that
                        // function from inside a region grows the CALLER's binding, and the
                        // call site is the only place that can see whose binding it is.
                        self.record_param_growth(root);
                        if let Some(open) = self.declared_outside_open_region(root) {
                            self.blame_callee(e.span, "push");
                            return Err(format!(
                                "`{}` was declared outside `region {}`, so it cannot grow inside \
                                 it — `push` builds a new buffer in the region, and the bytes are \
                                 released at the closing brace, leaving `{}` reading whatever the \
                                 region hands out next. Declare `{}` inside the region, or grow \
                                 it outside it.",
                                root, open, root, root
                            ));
                        }
                    }
                    let value = self.check_expr(&arguments[1], Some(&elem))?;
                    if !self.storable(&value.ty, &elem) {
                        return Err(format!(
                            "push(...) appends {} to a {}, but the value has type {}",
                            elem,
                            place.ty,
                            value.ty
                        ));
                    }
                    return Ok(TypedExpr {
                        ty: Type::Int,
                        kind: TypedExprKind::Push {
                            place: Box::new(place),
                            value: Box::new(value),
                        },
                    });
                }
                // `read_file(path)` — the whole file as a String in the current
                // region. A builtin rather than user FFI because the result must
                // be region-allocated to be escape-checked; a raw `extern` that
                // returned a pointer could not be.
                // The command line, and writing a file: between them, a program can be
                // a compiler rather than a demonstration.
                if name == "argument_count" {
                    if !arguments.is_empty() {
                        return Err("argument_count() takes no arguments".to_string());
                    }
                    return Ok(TypedExpr { ty: Type::Int, kind: TypedExprKind::ArgCount });
                }
                if name == "argument" {
                    if arguments.len() != 1 {
                        return Err("argument(n) takes one Int".to_string());
                    }
                    let index = self.check_expr(&arguments[0], Some(&Type::Int))?;
                    if index.ty != Type::Int {
                        return Err(format!(
                            "argument(n) takes an Int, but this has type {}",
                            index.ty
                        ));
                    }
                    // This COPIES into the region — `codegen.rs:3831`, which says so in capitals
                    // and gives the reason: `argv` holds C's strings, and a C string has no
                    // length header, so handing the pointer back would make `len` read whatever
                    // the loader happened to place in front of it. One `strlen` at the boundary
                    // buys a real Burxt String.
                    //
                    // **This comment used to say the opposite** — "no region: the C runtime's
                    // argument strings outlive the program, so this borrows rather than copies" —
                    // and it was reasoned from for as long as the copy has existed. B33: it made
                    // `expr_allocates` answer false for `argument(n)`, so `kept = argument(0)`
                    // inside a region was accepted and `len(kept)` printed 22 and then 1 after
                    // the next region reused the bytes. The `Arg` arm of `expr_allocates` is the
                    // fix; this is the sentence that misled everyone who read it first.
                    return Ok(TypedExpr {
                        ty: Type::String,
                        kind: TypedExprKind::Arg(Box::new(index)),
                    });
                }
                if name == "write_file" {
                    if arguments.len() != 2 {
                        return Err("write_file(path, contents) takes two Strings".to_string());
                    }
                    let path = self.check_expr(&arguments[0], Some(&Type::String))?;
                    let contents = self.check_expr(&arguments[1], Some(&Type::String))?;
                    for (which, side) in [("path", &path), ("contents", &contents)] {
                        if side.ty != Type::String {
                            return Err(format!(
                                "write_file(...) takes a String {}, but this has type {}",
                                which, side.ty
                            ));
                        }
                    }
                    if let Some(why) = self.impure("write a file") {
                        return Err(why);
                    }
                    return Ok(TypedExpr {
                        ty: Type::Int,
                        kind: TypedExprKind::WriteFile {
                            path: Box::new(path),
                            contents: Box::new(contents),
                        },
                    });
                }
                // `write_bytes(path, buffer)` — the way out of quadratic string building.
                //
                // `a = a + b` in a loop copies the whole left side every time, so building
                // a megabyte of output an append at a time copies gigabytes. This project
                // paid for that four times (v0.0.68, v0.0.77, v0.0.82, v0.0.86) before
                // admitting the answer: a growable array already grows in amortised O(1),
                // so the missing piece was never a better String — it was a way to WRITE a
                // buffer of bytes. `push` fills it; this empties it.
                //
                // Anyone producing large output — a report, a serialiser, an HTML renderer,
                // a compiler — needs exactly this, which is the test a builtin has to pass.
                if name == "write_bytes" {
                    if arguments.len() != 2 {
                        return Err(
                            "write_bytes(path, buffer) takes a String and a [Int]".to_string()
                        );
                    }
                    let path = self.check_expr(&arguments[0], Some(&Type::String))?;
                    if path.ty != Type::String {
                        return Err(format!(
                            "write_bytes(...) takes a String path, but this has type {}",
                            path.ty
                        ));
                    }
                    let buffer = self.check_expr(&arguments[1], None)?;
                    match &buffer.ty {
                        Type::Slice(elem) if **elem == Type::Int => {}
                        other => {
                            return Err(format!(
                                "write_bytes(...) writes the bytes of a growable `[Int]`, \
                                 but this is {}. Each element is one byte, and a value \
                                 outside 0..255 is truncated to its low eight bits.",
                                other
                            ));
                        }
                    }
                    if let Some(why) = self.impure("write a file") {
                        return Err(why);
                    }
                    return Ok(TypedExpr {
                        ty: Type::Int,
                        kind: TypedExprKind::WriteBytes {
                            path: Box::new(path),
                            buffer: Box::new(buffer),
                        },
                    });
                }
                // `substring(s, at, len)` — the primitive a symbol table needs. A
                // lexer can already compare a span against a literal byte by byte;
                // what it could not do was KEEP the text, which is what a table of
                // names is made of.
                if name == "substring" {
                    if arguments.len() != 3 {
                        return Err(
                            "substring(...) takes a String, a start offset and a length"
                                .to_string(),
                        );
                    }
                    let source = self.check_expr(&arguments[0], Some(&Type::String))?;
                    if source.ty != Type::String {
                        return Err(format!(
                            "substring(...) reads a String, but the first argument has \
                             type {}",
                            source.ty
                        ));
                    }
                    let at = self.check_expr(&arguments[1], Some(&Type::Int))?;
                    let len = self.check_expr(&arguments[2], Some(&Type::Int))?;
                    for (which, side) in [("offset", &at), ("length", &len)] {
                        if side.ty != Type::Int {
                            return Err(format!(
                                "substring(...) takes an Int {}, but this one has type {}",
                                which, side.ty
                            ));
                        }
                    }
                    if !self.has_region() {
                        return Err(self.needs_region("substring(...) copies bytes"));
                    }
                    return Ok(TypedExpr {
                        ty: Type::String,
                        kind: TypedExprKind::Substring {
                            source: Box::new(source),
                            at: Box::new(at),
                            len: Box::new(len),
                        },
                    });
                }
                // `byte_as_string(n)` — the one-byte String whose only byte is `n`.
                //
                // **The exact inverse of `byte_at`**, and that is the property to hold onto:
                // `byte_at(byte_as_string(n), 0) == n` for every one of the 256 values, which
                // `tests/pass/byte_as_string.bx` checks as a loop rather than on a chosen few.
                //
                // It exists because there was NO Int-to-String path in the language. `substring`
                // of a literal is the only other one, and a source file must be valid UTF-8, so a
                // literal can only hold a byte >= 0x80 inside a complete multi-byte character —
                // which makes the 51 UTF-8 LEAD bytes reachable only through 51 characters from 51
                // blocks, six of them unassigned codepoints. That table was refused as a library
                // liability; this builtin is what it was refused in favour of. lib/string.bx's
                // "THE GAP" header has the four measured steps.
                //
                // No NUL carve-out, and that was measured rather than assumed: a Burxt String is
                // LENGTH-PREFIXED (an i64 at `s - 8`), so `len("a\0b")` is 3 and a zero byte is
                // ordinary. The full 0..255 range needs no special case.
                if name == "byte_as_string" {
                    if arguments.len() != 1 {
                        return Err(
                            "byte_as_string(...) takes one byte value: byte_as_string(n)"
                                .to_string(),
                        );
                    }
                    let n = self.check_expr(&arguments[0], Some(&Type::Int))?;
                    if n.ty != Type::Int {
                        return Err(format!(
                            "byte_as_string(...) takes an Int byte value, but this has type {}",
                            n.ty
                        ));
                    }
                    // A WRITTEN-DOWN value out of range is refused now; anything computed is
                    // checked at runtime. Both, rather than only the runtime trap `CInt` has:
                    // `tests/panic/cint_range.stderr` pins CInt's contract so it cannot gain a
                    // literal check without changing, but a NEW builtin pays nothing for one, and
                    // a program that cannot be right should not compile. The cost is that the two
                    // out-of-range messages are worded differently, which is why both name the
                    // same range in the same words.
                    // Worded to match stage-1's `check_builtin_args` BYTE FOR BYTE, backticks and
                    // missing full stop included. `tests/fail/` is an equality across the two
                    // compilers, and a wording that merely overlaps on the pinned substring is a
                    // wording that has not been compared.
                    if let Some(v) = written_int(&n) {
                        if v < 0 || v > 255 {
                            return Err(format!(
                                "`byte_as_string({})` has no answer: a byte is 0 to 255. \
                                 A codepoint above 255 is more than one byte — \
                                 `from_codepoint` in lib/string.bx encodes it",
                                v
                            ));
                        }
                    }
                    if !self.has_region() {
                        return Err(
                            self.needs_region("byte_as_string(...) builds a one-byte String")
                        );
                    }
                    return Ok(TypedExpr {
                        ty: Type::String,
                        kind: TypedExprKind::ByteAsString(Box::new(n)),
                    });
                }
                // ---- bit operations, by name ----
                //
                // Reversing a stated decision, and the reason is on record in
                // spec/FAR-HORIZON-ROADMAP.md §5: bitwise was refused when the language had no
                // ambition to store data. It does now, and without these a program cannot parse a
                // binary format, compute a checksum, or implement a hash — so it cannot write its
                // own file format, which is what the local-database vision is made of.
                //
                // NAMES rather than operators, for the reason `BitOp`'s doc gives: `a & b == c`
                // means `a & (b == c)` in C, and the right shift is genuinely two operations that
                // one symbol cannot distinguish.
                if let Some(kind) = match name.as_str() {
                    "bit_and" => Some(crate::codegen::BitOp::And),
                    "bit_or" => Some(crate::codegen::BitOp::Or),
                    "bit_xor" => Some(crate::codegen::BitOp::Xor),
                    "bit_not" => Some(crate::codegen::BitOp::Not),
                    "shift_left" => Some(crate::codegen::BitOp::Left),
                    "shift_right_zeros" => Some(crate::codegen::BitOp::RightZeros),
                    "shift_right_sign" => Some(crate::codegen::BitOp::RightSign),
                    _ => None,
                } {
                    let unary = kind == crate::codegen::BitOp::Not;
                    let wanted = if unary { 1 } else { 2 };
                    if arguments.len() != wanted {
                        return Err(format!(
                            "{}(...) takes {} Int{}",
                            name,
                            wanted,
                            if unary { "" } else { "s" }
                        ));
                    }
                    let lhs = self.check_expr(&arguments[0], Some(&Type::Int))?;
                    if lhs.ty != Type::Int {
                        return Err(format!(
                            "{}(...) works on the bits of an Int, but this has type {}. A Decimal \
                             has no bit pattern worth exposing — its meaning is its scale, and \
                             shifting it would change the number by a factor of two while the \
                             scale kept claiming otherwise.",
                            name, lhs.ty
                        ));
                    }
                    let mut rhs = None;
                    if !unary {
                        let side = self.check_expr(&arguments[1], Some(&Type::Int))?;
                        if side.ty != Type::Int {
                            return Err(format!(
                                "{}(...) works on Ints, but the second argument has type {}.",
                                name, side.ty
                            ));
                        }
                        // A shift DISTANCE outside 0..=63 has no answer: LLVM leaves it undefined,
                        // C leaves it undefined, and a language whose whole claim is that it does
                        // not answer questions wrongly cannot pick a number here. A literal is
                        // refused now — the same treatment a literal array index out of bounds
                        // gets — and anything else is checked at runtime.
                        let shifting = matches!(
                            kind,
                            crate::codegen::BitOp::Left
                                | crate::codegen::BitOp::RightZeros
                                | crate::codegen::BitOp::RightSign
                        );
                        if shifting {
                            if let TypedExprKind::IntLit(n) = &side.kind {
                                if *n < 0 || *n > 63 {
                                    return Err(format!(
                                        "{}(x, {}) has no answer: an Int is 64 bits, so a shift \
                                         distance is 0 to 63. Shifting by {} is not \
                                         \"everything falls off the end\" — it is undefined, and \
                                         this language does not answer undefined questions.",
                                        name, n, n
                                    ));
                                }
                            }
                        }
                        rhs = Some(Box::new(side));
                    }
                    return Ok(TypedExpr {
                        ty: Type::Int,
                        kind: TypedExprKind::Bit { kind, lhs: Box::new(lhs), rhs },
                    });
                }
                // Integer division, by name. `/` on two Ints stays refused: one
                // operator cannot say which way it rounds, and for negatives the
                // answers differ.
                if let Some(kind) = match name.as_str() {
                    "divide_floor" => Some(crate::codegen::IntDiv::Floor),
                    "divide_toward_zero" => Some(crate::codegen::IntDiv::Trunc),
                    "remainder" => Some(crate::codegen::IntDiv::Rem),
                    _ => None,
                } {
                    if arguments.len() != 2 {
                        return Err(format!("{}(...) takes two Ints", name));
                    }
                    let lhs = self.check_expr(&arguments[0], Some(&Type::Int))?;
                    let rhs = self.check_expr(&arguments[1], Some(&Type::Int))?;
                    for (which, side) in [("first", &lhs), ("second", &rhs)] {
                        if side.ty != Type::Int {
                            return Err(format!(
                                "{}(...) works on Ints, but the {} argument has type \
                                 {}. A Decimal divides with `/` and its own rounding \
                                 contract.",
                                name, which, side.ty
                            ));
                        }
                    }
                    return Ok(TypedExpr {
                        ty: Type::Int,
                        kind: TypedExprKind::IntDiv {
                            kind,
                            lhs: Box::new(lhs),
                            rhs: Box::new(rhs),
                        },
                    });
                }
                // `old(expr)` — the value `expr` had on entry. Hoisted out of the
                // clause here, so codegen can evaluate it once at the top of the
                // function and the clause can compare against what it stored.
                if name == "old" {
                    if !self.in_ensures {
                        return Err(
                            "`old(...)` only means something in an `ensures` clause: it \
                             is the value an expression had on ENTRY, and there is no \
                             entry to refer back to anywhere else."
                                .to_string(),
                        );
                    }
                    if arguments.len() != 1 {
                        return Err("old(...) takes one expression".to_string());
                    }
                    // `result` has no meaning inside `old`: the point of `old` is the
                    // state BEFORE the call, and there was no result then. Checked on
                    // the expression as written, which gives a better message than
                    // letting name resolution fail.
                    if mentions(&arguments[0], "result") {
                        return Err(
                            "`old(result)` is a contradiction: `old` is the state \
                             before the call, and there was no result then."
                                .to_string(),
                        );
                    }
                    let inner = self.check_expr(&arguments[0], None)?;
                    if crate::codegen::is_aggregate(&inner.ty) {
                        return Err(format!(
                            "`old(...)` holds {} {} at the moment of entry, and copying \
                             an aggregate to do that is not built yet. Take `old` of a \
                             field, or of a sum of fields — a Decimal or an Int.",
                            inner.ty.article(),
                            inner.ty
                        ));
                    }
                    let ty = inner.ty.clone();
                    let mut olds = self.olds.borrow_mut();
                    olds.push(inner);
                    return Ok(TypedExpr { ty, kind: TypedExprKind::Old(olds.len() - 1) });
                }
                // ---- the two things that may be done with a CPointer ----
                //
                // Together these are the whole pointer wall. `c_is_null` asks whether the C call
                // failed; `c_string_at` copies NUL-terminated bytes into a Burxt String. There is
                // no third operation, which is what makes a CPointer safe to hold: the pointer
                // never becomes a value the language must reason about the lifetime of, because
                // nothing in the language can follow it except a copy.
                if name == "c_is_null" {
                    if arguments.len() != 1 {
                        return Err("c_is_null(p) takes one CPointer".to_string());
                    }
                    let p = self.check_expr(&arguments[0], Some(&Type::CPointer))?;
                    if p.ty != Type::CPointer {
                        return Err(format!(
                            "c_is_null(...) takes a CPointer — the thing an `external function` \
                             handed back — but this has type {}",
                            p.ty
                        ));
                    }
                    return Ok(TypedExpr {
                        ty: Type::Bool,
                        kind: TypedExprKind::CIsNull(Box::new(p)),
                    });
                }
                // The copy is the wall. C's bytes become Burxt's bytes here and the pointer is
                // not kept, so who owns the C memory stops being a question Burxt has to answer —
                // and the answer to "is it still valid later" is that Burxt never looks again.
                //
                // A null pointer traps rather than answering "": an unset value and an empty one
                // are different facts, and returning the same String for both is the silent wrong
                // answer this language exists to refuse. Ask `c_is_null` first.
                if name == "c_string_at" {
                    if arguments.len() != 1 {
                        return Err("c_string_at(p) takes one CPointer".to_string());
                    }
                    let p = self.check_expr(&arguments[0], Some(&Type::CPointer))?;
                    if p.ty != Type::CPointer {
                        return Err(format!(
                            "c_string_at(...) takes a CPointer, but this has type {}",
                            p.ty
                        ));
                    }
                    if !self.has_region() {
                        return Err(self.needs_region(
                            "c_string_at(...) copies C's bytes into a Burxt String",
                        ));
                    }
                    return Ok(TypedExpr {
                        ty: Type::String,
                        kind: TypedExprKind::CStringAt(Box::new(p)),
                    });
                }
                // `c_bytes_at(p, n)` — N bytes from C, copied into a Burxt `[Int]`.
                //
                // The counterpart to `c_string_at`, and the same wall: the bytes are COPIED, so the
                // pointer never becomes something the language must reason about the lifetime of.
                // What differs is where the length comes from. `c_string_at` reads to a NUL, which is
                // a fact in the data; here the length is the CALLER's claim, and nothing in the type
                // can check it.
                //
                // That makes this the pointer wall's one soft edge, and it is named rather than
                // hidden: a caller who passes a length longer than the buffer reads past the end.
                // Declared, not inferred — the same bargain `as scaled` and `external function`
                // already make. What IS checked is the half that can be: a negative count is refused
                // at compile time when it is a literal, and trapped at runtime otherwise, because
                // "minus one bytes" is not a smaller read, it is an enormous one.
                //
                // `[Int]` rather than a Bytes type, because Burxt has no Bytes type yet (A4.4
                // deferred it with the trigger "binary I/O", which this fires). Each element is one
                // byte, 0..=255, which pairs with the `write_bytes` builtin that has had no inverse
                // since it was added.
                // `hold(value)` — file a value so a HOST can name it while Burxt is not
                // running, and answer the packed handle. M17.
                if name == "handle_of" {
                    if arguments.len() != 1 {
                        return Err(
                            "handle_of(value) takes the one value the host will hold on to".to_string()
                        );
                    }
                    let value = self.check_expr(&arguments[0], None)?;
                    // A CLASS, and the limit is the table rather than taste: a slot remembers
                    // WHERE a value is, so the value has to be somewhere. An `Int` is not — it
                    // travels in a register, there is no address to file, and `handle_of(7)` would
                    // have to invent storage nobody asked for. A class is also the shape this
                    // exists for: the application state a host carries between calls.
                    let Type::Named(class) = &value.ty else {
                        return Err(format!(
                            "handle_of(...) takes a class — the state a host holds between calls — \
                             and this is {}. A scalar has no place in the table to point at; \
                             put it in a class with the rest of the state.",
                            value.ty
                        ));
                    };
                    if !self.structs.contains_key(class) && !self.made_records.borrow().contains_key(class) {
                        return Err(format!(
                            "handle_of(...) takes a class and `{}` is not one — an enum has no \
                             single address to file.",
                            class
                        ));
                    }
                    let of = class.clone();
                    return Ok(TypedExpr {
                        ty: Type::Handle(Box::new(Type::Named(of.clone()))),
                        kind: TypedExprKind::Hold { value: Box::new(value), of },
                    });
                }
                // `held(handle)` — the value back, or one of three named refusals. M17.
                if name == "handle_value" {
                    if arguments.len() != 1 {
                        return Err("handle_value(handle) takes one handle".to_string());
                    }
                    let handle = self.check_expr(&arguments[0], None)?;
                    let Type::Handle(inner) = &handle.ty else {
                        return Err(format!(
                            "handle_value(...) takes a `Handle<...>`, the thing `handle_of` answered with, \
                             but this has type {}. A bare Int is not a handle: it carries no \
                             type for the table to check against.",
                            handle.ty
                        ));
                    };
                    let Type::Named(class) = inner.as_ref() else {
                        return Err(format!(
                            "handle_value(...) answers a class, and this handle names {}",
                            inner
                        ));
                    };
                    let of = class.clone();
                    return Ok(TypedExpr {
                        ty: Type::Named(of.clone()),
                        kind: TypedExprKind::Held { handle: Box::new(handle), of },
                    });
                }
                if name == "c_bytes_at" {
                    if arguments.len() != 2 {
                        return Err(
                            "c_bytes_at(p, n) takes a CPointer and a count of bytes".to_string()
                        );
                    }
                    let pointer = self.check_expr(&arguments[0], Some(&Type::CPointer))?;
                    if pointer.ty != Type::CPointer {
                        return Err(format!(
                            "c_bytes_at(...) takes a CPointer, the thing an `external function` \
                             handed back, but this has type {}",
                            pointer.ty
                        ));
                    }
                    let count = self.check_expr(&arguments[1], Some(&Type::Int))?;
                    if count.ty != Type::Int {
                        return Err(format!(
                            "c_bytes_at(p, n) takes a count of bytes as an Int, but this has \
                             type {}",
                            count.ty
                        ));
                    }
                    if let TypedExprKind::IntLit(n) = &count.kind {
                        if *n < 0 {
                            return Err(format!(
                                "c_bytes_at(p, {}) asks for a negative number of bytes, which is \
                                 not a smaller read — it is an enormous one. A count is 0 or more.",
                                n
                            ));
                        }
                    }
                    if !self.has_region() {
                        return Err(self.needs_region(
                            "c_bytes_at(...) copies C's bytes into a Burxt array",
                        ));
                    }
                    return Ok(TypedExpr {
                        ty: Type::Slice(Box::new(Type::Int)),
                        kind: TypedExprKind::CBytesAt {
                            pointer: Box::new(pointer),
                            count: Box::new(count),
                        },
                    });
                }
                // `c_bytes_to(p, bytes)` — Burxt's bytes, written into C's memory. Answers how many.
                //
                // The mirror of `c_bytes_at`, and the reason it exists is narrower than it looks.
                // `lib/os.bx` records the wall in prose: "Burxt can hold a pointer but cannot build a
                // struct behind one: `c_bytes_at` reads C's memory and nothing writes it." That one
                // sentence is why `nanosleep` was passed over for the obsolescent `usleep`, and why a
                // socket could `listen` but never `bind` — every one of those calls wants a small
                // struct filled in and handed over by pointer.
                //
                // Measured before it was built: a Burxt TCP server accepts a connection and answers
                // an HTTP request TODAY, with no compiler change, because a String reaches C as a
                // `char *` and `listen()` auto-binds. `bind()` to a CHOSEN port was the only thing
                // missing, and it is 16 bytes of `sockaddr_in`. One builtin, not a milestone.
                //
                // **The length is not a claim here — it is `len(bytes)`.** That is the half of
                // `c_bytes_at`'s soft edge this one closes: nothing can lie about how much is being
                // read out of Burxt. What stays the caller's claim is the DESTINATION's capacity,
                // and nothing in the type can check that, which is the same bargain `as scaled` and
                // `external function` already make. Named, not hidden.
                //
                // **An element outside 0..=255 traps, and does not mask.** `bit_and(x, 0xFF)` would
                // write a byte that is not the number the caller wrote down, which is exactly the
                // quiet wrong answer this language exists to refuse. The trap names the index.
                if name == "c_bytes_to" {
                    if arguments.len() != 2 {
                        return Err("c_bytes_to(p, bytes) takes a CPointer and an array of bytes"
                            .to_string());
                    }
                    let pointer = self.check_expr(&arguments[0], Some(&Type::CPointer))?;
                    if pointer.ty != Type::CPointer {
                        return Err(format!(
                            "c_bytes_to(...) writes into memory an `external function` handed \
                             back, so its first argument is a CPointer, but this has type {}",
                            pointer.ty
                        ));
                    }
                    let wanted = Type::Slice(Box::new(Type::Int));
                    let bytes = self.check_expr(&arguments[1], Some(&wanted))?;
                    if bytes.ty != wanted {
                        return Err(format!(
                            "c_bytes_to(p, bytes) writes an array of bytes — the [Int] that \
                             `c_bytes_at` answers and `to_bytes` builds — but this has type {}",
                            bytes.ty
                        ));
                    }
                    return Ok(TypedExpr {
                        ty: Type::Int,
                        kind: TypedExprKind::CBytesTo {
                            pointer: Box::new(pointer),
                            bytes: Box::new(bytes),
                        },
                    });
                }
                if name == "read_file" {
                    if let Some(why) = self.impure("read a file") {
                        return Err(why);
                    }
                    if arguments.len() != 1 {
                        return Err("read_file(...) takes one path".to_string());
                    }
                    let path = self.check_expr(&arguments[0], Some(&Type::String))?;
                    if path.ty != Type::String {
                        return Err(format!(
                            "read_file(...) takes a String path, but this has type {}",
                            path.ty
                        ));
                    }
                    if !self.has_region() {
                        return Err(self.needs_region(
                            "read_file(...) allocates the file's bytes",
                        ));
                    }
                    return Ok(TypedExpr {
                        ty: Type::String,
                        kind: TypedExprKind::ReadFile(Box::new(path)),
                    });
                }
                // `to_string(v)` — a value's exact display form, as a String.
                // Same formatting the printer uses, so the two can never drift.
                if name == "to_string" {
                    if arguments.len() != 1 {
                        return Err("to_string(...) takes one value".to_string());
                    }
                    let v = self.check_expr(&arguments[0], None)?;
                    match &v.ty {
                        Type::Int | Type::Bool | Type::Decimal { .. } => {}
                        Type::String => {
                            return Err(
                                "to_string(...) on a String would just copy it — use \
                                 the value directly."
                                    .to_string(),
                            )
                        }
                        other => {
                            return Err(format!(
                                "to_string(...) has no display form for {} {} yet — \
                                 only Int, Bool and Decimal.",
                                other.article(),
                                other
                            ))
                        }
                    }
                    // Bool needs no allocation: both spellings are literals.
                    if v.ty != Type::Bool && !self.has_region() {
                        return Err(self.needs_region(&format!(
                            "to_string(...) on {} {} allocates",
                            v.ty.article(),
                            v.ty
                        )));
                    }
                    return Ok(TypedExpr {
                        ty: Type::String,
                        kind: TypedExprKind::ToString(Box::new(v)),
                    });
                }
                if name == "hash" {
                    if arguments.len() != 1 {
                        return Err("hash(...) takes one value: hash(key)".to_string());
                    }
                    let v = self.check_expr(&arguments[0], None)?;
                    // Exactly the Equatable set — the types `==` works on. A key needs equality
                    // and a hash, and the set of types that have equality is the set that can
                    // have one, which is why there is no separate `Hashable`.
                    match &v.ty {
                        Type::Int | Type::Bool | Type::String | Type::Decimal { .. } => {}
                        other => {
                            return Err(format!(
                                "hash(...) needs an Equatable value, and {} {} is not one. \
                                 Equatable is Int, Bool, String and Decimal — the types `==` \
                                 works on. For a compound key, build a String from the parts.",
                                other.article(),
                                other
                            ))
                        }
                    }
                    return Ok(TypedExpr {
                        ty: Type::Int,
                        kind: TypedExprKind::Hash(Box::new(v)),
                    });
                }
                if name == "byte_at" {
                    if arguments.len() != 2 {
                        return Err(
                            "byte_at(...) takes a string and an index: byte_at(s, i)"
                                .to_string(),
                        );
                    }
                    let s = self.check_expr(&arguments[0], None)?;
                    if s.ty != Type::String {
                        return Err(format!(
                            "byte_at(...) reads a String, but the first argument has \
                             type {}",
                            s.ty
                        ));
                    }
                    let idx = self.check_expr(&arguments[1], None)?;
                    if idx.ty != Type::Int {
                        return Err(format!(
                            "a byte index must be an Int, but this one has type {}",
                            idx.ty
                        ));
                    }
                    return Ok(TypedExpr {
                        ty: Type::Int,
                        kind: TypedExprKind::ByteAt {
                            s: Box::new(s),
                            index: Box::new(idx),
                        },
                    });
                }
                if name == "len" {
                    if arguments.len() != 1 {
                        return Err(
                            "len(...) takes exactly one array or string".to_string()
                        );
                    }
                    let argument = self.check_expr(&arguments[0], None)?;
                    return match argument.ty {
                        Type::Array { len, .. } => Ok(TypedExpr {
                            ty: Type::Int,
                            kind: TypedExprKind::IntLit(len as i64),
                        }),
                        Type::String => Ok(TypedExpr {
                            ty: Type::Int,
                            kind: TypedExprKind::StrLen(Box::new(argument)),
                        }),
                        // a growable array knows its length at runtime
                        Type::Slice(_) => Ok(TypedExpr {
                            ty: Type::Int,
                            kind: TypedExprKind::SliceLen(Box::new(argument)),
                        }),
                        other => Err(format!(
                            "len(...) needs an array or a string, but this has type {}",
                            other
                        )),
                    };
                }
                // C2. Asked BEFORE the lookup, because the declaration is still in the table —
                // a package's private helper is real and perfectly usable inside that package, so
                // it cannot be removed from the program. An earlier attempt did exactly that and
                // broke the dependency's own code, which is the useful way to learn that privacy
                // is a relation between the use and the declaration rather than a property of one.
                if let Some(why) = self.refuse_if_package_private(name) {
                    return Err(why);
                }
                let (mut param_tys, mut ret) = self
                    .fns
                    .get(name)
                    .ok_or_else(|| {
                        // `exit` is real but is a STATEMENT, so reaching here means it was used
                        // where a value is wanted — and "unknown function" would send the reader
                        // looking for a spelling mistake.
                        if name == "exit" {
                            return "`exit(code)` ends the process, so it has no value to give — \
                                    write it as its own statement, `exit(1);`, rather than inside \
                                    an expression."
                                .to_string();
                        }
                        format!("unknown function: {}", name)
                    })?
                    .clone();
                // A generic call: infer what each type parameter stands for from the
                // arguments, then proceed as if the signature had been written that way.
                // `name` becomes the instantiation's symbol, so everything downstream —
                // purity, `allocates`, codegen — sees an ordinary function.
                let mut instantiated: Option<String> = None;
                let written_name = name.clone();
                if let Some(type_parameters) = self.generics.get(name).cloned() {
                    if arguments.len() != param_tys.len() {
                        return Err(format!(
                            "function `{}` takes {} argument(s), but {} were given",
                            name,
                            param_tys.len(),
                            arguments.len()
                        ));
                    }
                    let mut map: HashMap<String, Type> = HashMap::new();
                    for (i, (declared, argument)) in param_tys.iter().zip(arguments).enumerate() {
                        if !mentions_param(declared) {
                            continue;
                        }
                        let actual = self.check_expr(argument, None)?.ty;
                        let instances = self.instance_of.borrow().clone();
                        let tuples = self.tuple_of.borrow().clone();
                        unify(declared, &actual, &mut map, &instances, &tuples).map_err(|why| {
                            self.blame(argument.span);
                            format!("in the call to `{}`, argument {}: {}", name, i + 1, why)
                        })?;
                    }
                    // Anything the arguments could not settle, read from the EXPECTATION. A
                    // parameter that appears only in the return type has nothing to infer from at
                    // the call, but `let m: Map<String, Int> = map_new();` already says what it is
                    // — in the place this language says a type belongs. Unifying the declared
                    // return against what the context wants is therefore strictly better than a
                    // turbofish, and keeps "there is no turbofish" true.
                    //
                    // Second, not first: an argument is more specific than a context, and a
                    // context that disagrees should be reported as a mismatch by the ordinary
                    // return check rather than silently win here.
                    if type_parameters.iter().any(|p| !map.contains_key(&p.name)) {
                        if let Some(want) = expected {
                            let instances = self.instance_of.borrow().clone();
                            let tuples = self.tuple_of.borrow().clone();
                            // A failure is not an error here. The expectation may legitimately be
                            // unrelated — a call whose result is discarded, or one inside a bigger
                            // expression — and the real complaint is the one below, which names the
                            // parameter that is still unknown.
                            let _ = unify(&ret, want, &mut map, &instances, &tuples);
                        }
                    }
                    let mut type_args = Vec::with_capacity(type_parameters.len());
                    for p in &type_parameters {
                        match map.get(&p.name) {
                            Some(t) => type_args.push(t.clone()),
                            None => {
                                return Err(format!(
                                    "`{}` cannot tell what `{}` is from this call: no \
                                     argument mentions it, and the surrounding code does not \
                                     say either. Write the type where the value lands, as in \
                                     `let x: {} = {}(...);`",
                                    name,
                                    p.name,
                                    show(&ret, &self.instance_of.borrow().clone()),
                                    name
                                ))
                            }
                        }
                    }
                    // Substituting can leave a generic application — `Option<T>` becomes
                    // `Option<String>` — so each one is expanded into its instantiation
                    // before anything compares it with an argument's actual type.
                    let mut substituted = Vec::with_capacity(param_tys.len());
                    for t in &param_tys {
                        substituted.push(self.expand(&substitute(t, &map))?);
                    }
                    param_tys = substituted;
                    ret = self.expand(&substitute(&ret, &map))?;
                    for (p, argument) in type_parameters.iter().zip(&type_args) {
                        if let Some(bound) = &p.bound {
                            self.satisfies(argument, bound, &written_name, &p.name)?;
                        }
                    }
                    // If a type argument is still a parameter, this call is inside a
                    // generic being checked generically. There is nothing to emit yet:
                    // the copy appears when the ENCLOSING generic is instantiated, and
                    // its body then names a concrete type here.
                    if !type_args.iter().any(mentions_param) {
                        instantiated = Some(self.want(name, &type_args));
                    }
                }
                // `name` becomes the instantiation's SYMBOL from here on, because that is
                // what codegen must call. `written` stays what the author typed, because
                // that is what a message must say.
                let written = name.clone();
                let name = &instantiated.clone().unwrap_or_else(|| name.clone());
                if arguments.len() != param_tys.len() {
                    return Err(format!(
                        "function `{}` takes {} argument(s), but {} were given",
                        name,
                        param_tys.len(),
                        arguments.len()
                    ));
                }
                // Purity is transitive without being inferred: a pure function may
                // only call functions that make the same promise.
                if self.in_pure.is_some() {
                    if self.extern_names.contains(name) {
                        return Err(self
                            .impure(&format!("call `{}`, which crosses into C", name))
                            .unwrap_or_default());
                    }
                    if !self.pure_fns.contains(name) && self.fns.contains_key(name) {
                        let holder = self.in_pure.clone().unwrap_or_default();
                        return Err(if self.in_contract {
                            format!(
                                "a contract clause on `{}` may not call `{}`, which is \
                                 not declared `pure`: a clause must not be able to \
                                 change the program it checks. Declare `pure function {}`.",
                                holder, name, name
                            )
                        } else {
                            format!(
                                "`pure function {}` may not call `{}`, which is not declared \
                                 `pure`: the guarantee cannot rest on a function that \
                                 does not make it. Declare `pure function {}` too, or drop \
                                 `pure` from `{}`.",
                                holder, name, name, holder
                            )
                        });
                    }
                }
                // What the callee reaches, the caller reaches. Transitive by DECLARATION, the
                // same way `pure` above is — so a signature that says nothing about the network
                // is a signature you can trust about the network, however deep the call goes.
                //
                // That is the property `burxt review` needs: an agent cannot add an effect
                // without changing a signature, and a signature change is what a reviewer looks
                // at. Inferring this would have removed it from the signature, which is why
                // effects are declared and `allocates` is not.
                if let Some(reaches) = self.fn_effects.get(name).cloned() {
                    for e in &reaches {
                        if !self.allowed_effects.contains(e) {
                            return Err(self.effect_refusal(name, *e));
                        }
                    }
                }
                // An `allocates` callee builds in OUR region, so there has to be
                // one. Checked here rather than inside the callee, because this is
                // the site that can be wrapped.
                if self.alloc_fns.contains(name) && !self.has_region() {
                    return Err(format!(
                        "`{}` is declared `allocates`, so it builds its result in the \
                         caller's region — and there is none open here. Wrap the call \
                         in `region name {{ ... }}`, or declare this function \
                         `allocates` too.",
                        name
                    ));
                }
                let declared = self.extern_parameters.get(name).cloned();
                // Which parameters were declared `mutable`. Empty for an extern, which has none.
                let writable = self
                    .fn_writable
                    .get(name)
                    // A generic INSTANTIATION is registered in pass 2b, which runs AFTER the
                    // call sites that ask about it — so `add_one$Int` is absent from the map
                    // while `add_one` is present, and the whole `mutable` branch below was
                    // skipped for every generic call. Two things were silently missing because
                    // of it: B25 one instantiation away (measured — `region r { add_one(xs, 11)
                    // ... }` for an outer `xs` printed 777), and `require_mutable_argument`,
                    // which let an immutable `let` binding be handed to a generic `mutable`
                    // parameter and changed behind a reader who was told it could not be.
                    //
                    // Falling back to the generic's own vector is exact rather than an
                    // approximation: `specialise` clones the declaration and substitutes only
                    // TYPES, so an instantiation's parameters are writable exactly where the
                    // generic's are. `$` cannot occur in a declared name — `mangle` is what
                    // puts it there — so the split cannot collide with a real function.
                    .or_else(|| name.split_once('$').and_then(|(g, _)| self.fn_writable.get(g)))
                    .cloned()
                    .unwrap_or_default();
                let mut typed_args = Vec::new();
                for (i, (argument, param_ty)) in arguments.iter().zip(&param_tys).enumerate() {
                    // A `mutable` parameter changes the CALLER's value, so the caller must have one
                    // that may change. Two things are refused here and both are silent otherwise:
                    // handing over a `let` binding, which would change behind a reader who was told
                    // it could not; and handing over a literal or a computed value, which has no
                    // home for the change to land in — the callee would modify a temporary and the
                    // program would look like it worked.
                    if writable.get(i).copied().unwrap_or(false) {
                        self.require_mutable_argument(Self::declared_name(name), i, argument)?;
                        // B25: the callee grows THIS parameter, so the growth is really a growth
                        // of whatever the caller passed — and only here is it known whose
                        // binding that is. Per index, so a `mutable` parameter the callee never
                        // grows is unaffected.
                        if self.grow_params.contains(&(name.clone(), i)) {
                            if let Some(root) = Self::place_root(argument) {
                                // Recorded BEFORE the refusal, so a body that gets refused is
                                // still classed: the probe discards errors and carries on, and
                                // this is the link that makes the fixpoint transitive.
                                self.record_param_growth(root);
                                if let Some(open) = self.declared_outside_open_region(root) {
                                    self.blame_callee(e.span, Self::declared_name(name));
                                    return Err(Self::growing_an_outer_binding(
                                        root,
                                        &open,
                                        &format!("{}(...)", Self::declared_name(name)),
                                    ));
                                }
                            }
                        }
                    }
                    let typed = self.check_expr(argument, Some(param_ty))?;
                    if !self.storable(&typed.ty, param_ty) {
                        // Point at the argument, not at the whole call.
                        self.blame(argument.span);
                        // The boundary-exactness case gets its own message,
                        // because "argument 1 must be Int" would hide WHY: a
                        // double cannot hold the amount, and there are two
                        // encodings that can.
                        if let (Some(d), Type::Decimal { scale, .. }) = (&declared, &typed.ty) {
                            if d.get(i).map(|(t, _)| t) == Some(&Type::CDouble) {
                                return Err(format!(
                                    "a C `double` cannot hold {} exactly — a value \
                                     like 0.10 is not representable in binary \
                                     floating point, so this crossing would \
                                     silently change the amount. Declare the \
                                     parameter of `{}` as `{} as scaled` to pass \
                                     the exact unscaled integer (C receives it \
                                     scaled by 10^{}), or take a String and pass \
                                     `to_string(...)` as text.",
                                    typed.ty, name, typed.ty, scale
                                ));
                            }
                        }
                        // **Name the parameter, do not make the caller count.** "argument 2" is
                        // an instruction to go and count, and the answer is already in the
                        // compiler — `Param::name` has always carried it. A rejection in this
                        // language is supposed to read as advice, and "argument 2" is the weakest
                        // form of advice available: correct, and useless without the signature
                        // open beside it.
                        let which = match self.fn_param_names.get(name.as_str()).and_then(|p| p.get(i)) {
                            Some(p) => format!("`{}` (argument {})", p, i + 1),
                            None => format!("argument {}", i + 1),
                        };
                        return Err(format!(
                            "in the call to `{}`, {} must be {}, but it has type {}",
                            written, which, param_ty, typed.ty
                        ));
                    }
                    typed_args.push(typed);
                }
                Ok(TypedExpr { ty: ret, kind: TypedExprKind::Call { name: name.clone(), arguments: typed_args } })
            }

            ExprKind::StructLit { name, fields } => {
                // A generic record's literal names the class, not the instantiation:
                // `Pair { left: 7, right: "seven" }`. Which instantiation it is comes from
                // the context if there is one, and otherwise from the field values — the
                // same two sources, in the same order, that a generic enum's variant uses.
                // The literal's TYPE and the name its fields are read under are not always the
                // same thing: inside the generic itself the type is `Box<T>` while the fields live
                // under `Box`. Concrete instantiations agree, so this only differs abstractly.
                let literal_ty = self.instantiate_record(name, fields, expected)?;
                let name = &match &literal_ty {
                    Type::Named(symbol) => symbol.clone(),
                    Type::Generic { name: still, .. } => still.clone(),
                    other => return Err(format!("codegen bug: `{}` instantiated to {}", name, other)),
                };
                let declared = self
                    .fields_of(name)
                    .ok_or_else(|| {
                        format!(
                            "unknown type `{}` — declare it with `class {} {{ ... }}`",
                            name, name
                        )
                    })?;
                // A literal may name a PRIVATE field only inside the class itself. Until
                // v0.0.151 this was exempt, because with no constructors the rule would have
                // made such a class impossible to build from outside — so `private` protected
                // reads and not construction, and a class could not defend an invariant.
                //
                // Associated functions are the mechanism that closes it: `Account.open(...)` is
                // inside `Account`, so it may build one, and nothing else may.
                if self.current_receiver.as_deref() != Some(name.as_str()) {
                    if let Some(hidden) = self.private_fields.get(name) {
                        if let Some((given, _)) =
                            fields.iter().find(|(g, _)| hidden.iter().any(|h| h == g))
                        {
                            return Err(format!(
                                "`{}.{}` is private, so `{}` cannot be built here: a literal may \
                                 set a private field only inside `{}`. Give the class a \
                                 constructor — `function open(...) -> {}` in its body, called as \
                                 `{}.open(...)` — which is the point of making the field private.",
                                name, given, name, name, name, name
                            ));
                        }
                    }
                }
                // Every field exactly once; unknown names get the full list
                // (which doubles as typo help).
                for (given, _) in fields {
                    if !declared.iter().any(|(n, _)| n == given) {
                        return Err(format!(
                            "`{}` has no field named `{}`. Its fields are: {}.",
                            self.shown_type_name(name),
                            given,
                            self.field_list(name)
                        ));
                    }
                    if fields.iter().filter(|(g, _)| g == given).count() > 1 {
                        return Err(format!(
                            "in `{} {{ ... }}`, the field `{}` is given twice",
                            self.shown_type_name(name),
                            given
                        ));
                    }
                }
                // Re-emit in declaration order so codegen is positional.
                let mut typed_fields = Vec::new();
                for (fname, fty) in &declared {
                    let value = fields
                        .iter()
                        .find(|(g, _)| g == fname)
                        .map(|(_, v)| v)
                        .ok_or_else(|| {
                            format!(
                                "`{} {{ ... }}` is missing the field `{}: {}`. Every \
                                 field must be given a value — Burxt does not invent \
                                 defaults.",
                                name, fname, fty
                            )
                        })?;
                    let typed = self.check_expr(value, Some(fty))?;
                    if !self.storable(&typed.ty, fty) {
                        return Err(format!(
                            "in `{} {{ ... }}`, the field `{}` must be {}, but its \
                             value has type {}",
                            self.shown_type_name(name),
                            fname,
                            self.shown(fty),
                            self.shown(&typed.ty)
                        ));
                    }
                    typed_fields.push(typed);
                }
                Ok(TypedExpr {
                    ty: literal_ty,
                    kind: TypedExprKind::StructLit { name: name.clone(), fields: typed_fields },
                })
            }

            // `(1, "a")`. The elements are typed left to right, the tuple type is built from
            // what they came out as, and `expand` turns that into the anonymous class — so
            // the value this produces is an ordinary `StructLit` and codegen learns nothing.
            //
            // `expected` is pushed DOWN element-wise when it is a tuple of the same arity,
            // which is what makes `let pair: (Decimal<2>, String) = (1.50, "a");` work: a
            // decimal literal takes its scale from the type it is being stored into, and
            // with no expectation `1.50` would be `Decimal<2>` by luck rather than by
            // contract. A mismatched arity is left to the ordinary `storable` refusal below,
            // where the message can name both types.
            ExprKind::TupleLit(elements) => {
                // A declared type has already been through `expand`, so the expectation
                // arrives as the anonymous class rather than as `Type::Tuple` — the
                // `Named` arm is the one that fires in practice, and both are here because
                // a tuple reached from inside a generic has not been expanded yet.
                let pushed: Vec<Type> = match expected {
                    Some(Type::Tuple(want)) if want.len() == elements.len() => want.clone(),
                    Some(Type::Named(symbol)) if Self::is_tuple_symbol(symbol) => self
                        .fields_of(symbol)
                        .filter(|fields| fields.len() == elements.len())
                        .map(|fields| fields.into_iter().map(|(_, t)| t).collect())
                        .unwrap_or_default(),
                    _ => Vec::new(),
                };
                let mut typed = Vec::new();
                let mut types = Vec::new();
                for (i, element) in elements.iter().enumerate() {
                    let want = pushed.get(i).cloned();
                    let t = self.check_expr(element, want.as_ref())?;
                    types.push(t.ty.clone());
                    typed.push(t);
                }
                let ty = self.expand(&Type::Tuple(types))?;
                let symbol = match &ty {
                    Type::Named(symbol) => symbol.clone(),
                    // Still generic: `(T, Int)` inside `zip<T, U>`'s own body, where there is
                    // no class to make yet because there is no type to make it of.
                    //
                    // The name is the tuple as WRITTEN, and this node is never emitted — a
                    // generic's own body is checked (M7: the signature is the contract) and
                    // then thrown away; codegen only ever sees the `specialise`d copies, where
                    // every parameter has been substituted and `expand` has made the class.
                    // The generic record literal one arm up does exactly this and for exactly
                    // this reason: inside `Box<T>` its type is `Box<T>` and its `StructLit`
                    // names `Box`, which is no instantiation either.
                    other => other.to_string(),
                };
                Ok(TypedExpr { ty, kind: TypedExprKind::StructLit { name: symbol, fields: typed } })
            }

            ExprKind::Field { base, field } => {
                if let Some(r) = self.check_variant_lit(base, field, &[], expected) {
                    return r;
                }
                let typed_base = self.check_expr(base, None)?;
                let (index, ty) = self.resolve_field(&typed_base.ty, field)?;
                Ok(TypedExpr {
                    ty,
                    kind: TypedExprKind::Field { base: Box::new(typed_base), index },
                })
            }

            ExprKind::MethodCall { base, method, arguments } => {
                // Purity is transitive through a method call exactly as it is through a
                // function call — since A4, a method CAN carry the marker, so the question is
                // whether this particular one does rather than whether any could.
                //
                // The receiver's type is needed to ask, and it is only known after the base is
                // typed, so the check sits below rather than here. What used to sit here was a
                // flat refusal of every method call inside a pure function, which was correct
                // while `pure` was unspellable on a method and is now exactly the blanket
                // refusal A4 exists to replace.
                // `Account.open(...)` — an ASSOCIATED function, which reads exactly like an
                // enum variant and is told apart the same way `check_variant_lit` tells a
                // variant from a local binding: by what the name in front of the dot IS.
                //
                // The variant attempt comes first, so an enum keeps its meaning; a class name
                // could never be an enum name, because a program may not declare both.
                if let ExprKind::Var(holder) = &base.kind {
                    if !self.env.contains_key(holder) && self.structs.contains_key(holder) {
                        let qualified = format!("{}.{}", holder, method);
                        if self.fns.contains_key(&qualified) {
                            // Rewrite into an ordinary call and let the one call path handle
                            // it — arity, argument types, generics, purity, the escape rules.
                            // A second implementation of "checking a call" is how two of them
                            // drift apart.
                            let rewritten = Expr {
                                kind: ExprKind::Call {
                                    name: qualified,
                                    arguments: arguments.to_vec(),
                                },
                                span: e.span,
                            };
                            return self.check_expr(&rewritten, expected);
                        }
                    }
                }
                if let Some(r) = self.check_variant_lit(base, method, arguments, expected) {
                    return r;
                }
                let typed_base = self.check_expr(base, None)?;

                // A call on a `dyn Trait` is the ONE place dispatch happens at
                // runtime: find the method's slot from trait-declaration order.
                if let Type::Dyn(interface_name) = typed_base.ty.clone() {
                    let sigs = &self.interfaces[&interface_name];
                    let slot = sigs
                        .iter()
                        .position(|s| s.name == *method)
                        .ok_or_else(|| {
                            format!(
                                "`dynamic {}` has no method named `{}`. Its methods \
                                 are: {}.",
                                self.shown_type_name(&interface_name),
                                method,
                                sigs.iter()
                                    .map(|s| s.name.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            )
                        })?;
                    let signature = sigs[slot].clone();
                    // A11: a mutating method through an interface object. See
                    // `dyn_mutating_receiver` for the rule and why it needs no runtime word.
                    if signature.receiver_mut {
                        let shown_receiver = self.shown(&typed_base.ty);
                        let root =
                            self.dyn_mutating_receiver(&shown_receiver, method, base)?;
                        // B25 through a vtable, and the reason `root` is a name rather than the
                        // receiver's spelling: the growth lands in the value the object borrows,
                        // which may be declared in a different scope than the object is.
                        if self.dyn_call_grows_self(&interface_name, method) {
                            self.record_param_growth(&root);
                            if let Some(open) = self.declared_outside_open_region(&root) {
                                return Err(Self::growing_an_outer_binding(
                                    &root,
                                    &open,
                                    &format!("{}.{}(...)", shown_receiver, method),
                                ));
                            }
                        }
                    }
                    if arguments.len() != signature.parameters.len() {
                        return Err(format!(
                            "`dynamic {}.{}` takes {} argument(s), but {} were given",
                            interface_name,
                            method,
                            signature.parameters.len(),
                            arguments.len()
                        ));
                    }
                    let mut typed_args = Vec::new();
                    for (i, (argument, p)) in arguments.iter().zip(&signature.parameters).enumerate() {
                        let typed = self.check_expr(argument, Some(&p.ty))?;
                        if !self.storable(&typed.ty, &p.ty) {
                            return Err(format!(
                                "in the call to `dynamic {}.{}`, argument {} must be \
                                 {}, but it has type {}",
                                interface_name,
                                method,
                                i + 1,
                                p.ty,
                                typed.ty
                            ));
                        }
                        typed_args.push(typed);
                    }
                    // A method reached through an interface object allocates if ANY
                    // implementation of it does. The call site cannot know which one runs —
                    // that is what `dynamic` means — so the conservative answer is the only
                    // sound one, and it costs a region the program was going to need anyway.
                    //
                    // This check was MISSING, not merely weak: the branch returns before the
                    // `alloc_methods` test further down, so `allocates` was enforced for a
                    // direct call and silently skipped through an interface object. It corrupted
                    // nothing in the reproduction only because a region happened to be open
                    // further up the stack. spec/1.0/M14-IMPLICIT-REGIONS.md §5.
                    //
                    // The design §5 first proposed — `allocates` on trait signatures — is not
                    // needed and would have been worse: one fact in two places, with the
                    // trait declaring it and every impl having to agree. That is the failure
                    // spec/A7.0-NAMING.md exists to prevent. Read off the impls instead, now
                    // that they are inferred, and no syntax is involved at all.
                    //
                    // `leaks` is computed BEFORE asking `has_region`, and the order is
                    // load-bearing: under probing `has_region` RECORDS that something here
                    // wanted a region, so asking it first would record on every such call and
                    // mark half the program as allocating.
                    //
                    // The test itself lives in `dyn_call_allocates` because `expr_allocates`
                    // needs the same answer and had a different one — B26.
                    let leaks = self.dyn_call_allocates(&interface_name, method);
                    if leaks && !self.has_region() {
                        return Err(format!(
                            "`dynamic {}.{}` builds its result in the caller's region — some \
                             implementation of it allocates, and a call through an interface \
                             object cannot know which one runs. There is no region open \
                             here: wrap the call in `region name {{ ... }}`, or declare the \
                             enclosing function `allocates` too.",
                            interface_name, method
                        ));
                    }
                    return Ok(TypedExpr {
                        ty: signature.ret.clone(),
                        kind: TypedExprKind::DynCall {
                            interface_name,
                            method: method.clone(),
                            slot: slot as u32,
                            base: Box::new(typed_base),
                            arguments: typed_args,
                        },
                    });
                }

                // `f.apply(x)` where `f: dynamic Mapper<T, U>` inside a generic's OWN body,
                // before any instantiation has made `Mapper<T, U>` concrete.
                //
                // This is the `Type::Param` case immediately below, one step along: the bound
                // says which methods exist, so the body is checked against the interface rather
                // than against each instantiation — and here the interface is written out
                // rather than named by a bound. Substituting the interface's declared
                // parameters with what the signature wrote gives `apply(self, x: T) -> U` with
                // the ENCLOSING generic's parameters in it, which is exactly the contract the
                // body should be held to.
                //
                // **This arm is what makes a free generic `map` compile**, and without it A9
                // stops at `map(xs: [Int], ...)`. The abstract body is checked and never
                // emitted, so the node built here is discarded; every instantiation re-checks
                // the call with a real `Dyn` through the arm above, which is where the
                // allocation rule and the vtable slot are settled.
                if let Type::DynGeneric { name, arguments: type_arguments } =
                    typed_base.ty.clone()
                {
                    let shown_base = typed_base.ty.to_string();
                    let (parameters, methods) = self
                        .generic_interfaces
                        .get(&name)
                        .cloned()
                        .ok_or_else(|| format!("unknown interface `{}`", name))?;
                    let map: HashMap<String, Type> = parameters
                        .iter()
                        .map(|p| p.name.clone())
                        .zip(type_arguments.iter().cloned())
                        .collect();
                    let signature = methods
                        .iter()
                        .find(|s| s.name == *method)
                        .ok_or_else(|| {
                            format!(
                                "`{}` has no method named `{}`. Its methods are: {}.",
                                shown_base,
                                method,
                                methods
                                    .iter()
                                    .map(|s| s.name.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            )
                        })?
                        .clone();
                    // A11, the abstract half. This arm checks a free generic's body against the
                    // interface it wrote out, so the receiver is that body's own parameter and
                    // the question is the same one: was it declared `mutable`. Every
                    // instantiation re-checks the call through the `Dyn` arm above, which is
                    // where the growth rule is applied against a real implementation list.
                    if signature.receiver_mut {
                        // The name is discarded here on purpose: this body is checked and never
                        // emitted, and the growth rule needs a real implementation list, which
                        // an abstract `Mapper<T, U>` does not have.
                        self.dyn_mutating_receiver(&shown_base, method, base)?;
                    }
                    if arguments.len() != signature.parameters.len() {
                        return Err(format!(
                            "`{}.{}` takes {} argument(s), but {} were given",
                            shown_base,
                            method,
                            signature.parameters.len(),
                            arguments.len()
                        ));
                    }
                    let mut typed_args = Vec::new();
                    for (i, (argument, p)) in
                        arguments.iter().zip(&signature.parameters).enumerate()
                    {
                        let want = substitute(&p.ty, &map);
                        let typed = self.check_expr(argument, Some(&want))?;
                        if !self.storable(&typed.ty, &want) {
                            return Err(format!(
                                "in the call to `{}.{}`, argument {} must be {}, but it \
                                 has type {}",
                                shown_base,
                                method,
                                i + 1,
                                want,
                                typed.ty
                            ));
                        }
                        typed_args.push(typed);
                    }
                    return Ok(TypedExpr {
                        ty: substitute(&signature.ret, &map),
                        kind: TypedExprKind::DynCall {
                            // The un-mangled name: there is no instantiation yet, and this
                            // node is discarded with the rest of the abstract body. The copy
                            // that is emitted is built by the `Dyn` arm above, after
                            // substitution has produced a real `Mapper$Int$String`.
                            interface_name: name,
                            method: method.clone(),
                            slot: 0,
                            base: Box::new(typed_base),
                            arguments: typed_args,
                        },
                    });
                }

                // A method on a value of a type PARAMETER: the bound says which methods
                // exist, so the body is checked against the interface rather than against each
                // instantiation. That is what makes the signature the contract.
                if let Type::Param(p) = &typed_base.ty {
                    let bound = self.param_bounds.get(p).cloned().flatten();
                    let Some(interface_name) = bound else {
                        return Err(unbounded(p, &format!("asked for `.{}(...)`", method)));
                    };
                    let Some(sigs) = self.interfaces.get(&interface_name) else {
                        return Err(format!(
                            "`{}: {}` is not an interface bound with methods — `Ordered` and \
                             `Equatable` allow comparison, not calls.",
                            p, interface_name
                        ));
                    };
                    let Some(signature) = sigs.iter().find(|s| &s.name == method) else {
                        return Err(format!(
                            "`{}: {}` has no method `{}`. `{}` declares: {}.",
                            p,
                            interface_name,
                            method,
                            interface_name,
                            sigs.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", ")
                        ));
                    };
                    if arguments.len() != signature.parameters.len() {
                        return Err(format!(
                            "`{}.{}` takes {} argument(s), but {} were given",
                            p,
                            method,
                            signature.parameters.len(),
                            arguments.len()
                        ));
                    }
                    let mut typed_args = Vec::new();
                    for (argument, want) in arguments.iter().zip(&signature.parameters) {
                        let t = self.check_expr(argument, Some(&want.ty))?;
                        if !self.storable(&t.ty, &want.ty) {
                            self.blame(argument.span);
                            return Err(format!(
                                "`{}.{}` expects {} here, but this argument has type {}",
                                p, method, want.ty, t.ty
                            ));
                        }
                        typed_args.push(t);
                    }
                    // The same rule for a value of a type parameter, asked of the BOUND:
                    // `largest<T: Priced>` calls `T`'s methods through the interface exactly as a
                    // trait object does, and any implementation may allocate. `interface_name`
                    // here is the bound, never `p` — `p` is the parameter's own name.
                    let leaks = self.impls.iter().any(|(implemented, concrete)| {
                        *implemented == interface_name
                            && self.alloc_methods.contains(&(concrete.clone(), method.clone()))
                    });
                    if leaks && !self.has_region() {
                        return Err(format!(
                            "`{}.{}` builds its result in the caller's region — `{}: {}` is \
                             satisfied by an implementation that allocates. There is no \
                             region open here: wrap the call in `region name {{ ... }}`, or \
                             declare the enclosing function `allocates` too.",
                            p, method, p, interface_name
                        ));
                    }
                    return Ok(TypedExpr {
                        ty: signature.ret.clone(),
                        kind: TypedExprKind::MethodCall {
                            receiver: p.clone(),
                            method: method.clone(),
                            receiver_mut: signature.receiver_mut,
                            base: Box::new(typed_base),
                            arguments: typed_args,
                        },
                    });
                }
                let receiver = match &typed_base.ty {
                    Type::Named(n) => n.clone(),
                    other => {
                        return Err(format!(
                            "`.{}(...)` needs a class value, but this has type {}.",
                            method, other
                        ))
                    }
                };
                let (receiver_mut, param_tys, ret) = self
                    .methods
                    .get(&(receiver.clone(), method.clone()))
                    .cloned()
                    .ok_or_else(|| {
                        format!(
                            "`{}` has no method named `{}`.",
                            self.shown_type_name(&receiver),
                            method
                        )
                    })?;
                // A4: purity is transitive through a method call. A pure thing — a `pure`
                // function, a `pure` method, or a CONTRACT CLAUSE, all of which set `in_pure` —
                // may call a method only if that method is `pure` too.
                //
                // This is the rule that makes A4 worth having, and the reason is the contract
                // half: `typeck.rs` has always checked a method's clauses under the pure rule, so
                // `requires self.sum() > 0` was refused by the blanket "no method is pure" branch
                // that used to sit further up. Nothing about the contract machinery had to change
                // — the missing piece was a method that could answer yes.
                if self.in_pure.is_some()
                    && !self.pure_methods.contains(&(receiver.clone(), method.clone()))
                {
                    let holder = self.in_pure.clone().unwrap_or_default();
                    return Err(if self.in_contract {
                        format!(
                            "a contract clause on `{}` may not call `{}.{}()`, which is not \
                             `pure`: a clause has to be able to run without changing anything, \
                             and only `pure` says so. Declare it \
                             `pure function (self: {}) {}(...)`, or compare fields directly.",
                            holder, receiver, method, receiver, method
                        )
                    } else {
                        format!(
                            "`pure function {}` may not call `{}.{}()`, which is not `pure`: \
                             purity is transitive, so a pure answer may only be built from pure \
                             parts. Declare it `pure function (self: {}) {}(...)`, or drop `pure` \
                             from `{}`.",
                            holder, receiver, method, receiver, method, holder
                        )
                    });
                }
                // `private` on a method, same rule and same boundary as a private field.
                if self.current_receiver.as_deref() != Some(receiver.as_str())
                    && self.private_methods.contains(&(receiver.clone(), method.clone()))
                {
                    return Err(format!(
                        "`{}.{}()` is private: it is callable only from `{}`'s own methods. \
                         It is an implementation detail of `{}`, not part of its API.",
                        receiver, method, receiver, receiver
                    ));
                }

                if receiver_mut {
                    // A mutating method is passed a true reference, so the
                    // base MUST be the actual mutable binding — exactly the
                    // rule AssignField already enforces for `item.field = v`.
                    let ExprKind::Var(name) = &base.as_ref().kind else {
                        return Err(format!(
                            "`{}` is a mutating method (`function (mutable self: {}) ...`); \
                             it can only be called on a variable, not an \
                             expression.",
                            method, receiver
                        ));
                    };
                    let (_, mutable) = self
                        .env
                        .get(name)
                        .ok_or_else(|| format!("unknown variable: {}", name))?;
                    if !*mutable {
                        // Through `how_to_make_writable` like the other seven, and it was NOT
                        // before: this site hardcoded the `let mutable` form, so a mutating
                        // method on an immutable PARAMETER advised changing a `let` that does
                        // not exist. The dyn path A11 adds asks the same question, and one
                        // question with two answers is how the two drift.
                        let ty = self
                            .env
                            .get(name)
                            .map(|(t, _)| t.clone())
                            .unwrap_or_else(|| Type::Named(receiver.clone()));
                        return Err(format!(
                            "cannot call the mutating method `{}` on `{}`: it was \
                             declared immutable. {}",
                            method,
                            name,
                            self.how_to_make_writable(name, &ty)
                        ));
                    }
                    // B25 through `self`, which is the same hole and had to be measured rather
                    // than assumed: `class Log { lines: [Int] }` with
                    // `function (mutable self: Log) add(v: Int) { push(self.lines, v); }`, called
                    // five times on an outer `l` inside a region, printed 777 for `l.lines[0]`.
                    // A `mutable self` receiver is always a plain variable — the check just above
                    // is what guarantees it — so the root is the name, with no place to walk.
                    if self.grow_self.contains(&(receiver.clone(), method.clone())) {
                        self.record_param_growth(name);
                        if let Some(open) = self.declared_outside_open_region(name) {
                            return Err(Self::growing_an_outer_binding(
                                name,
                                &open,
                                &format!(
                                    "{}.{}(...)",
                                    self.shown_type_name(&receiver),
                                    method
                                ),
                            ));
                        }
                    }
                }

                // What the method REACHES, checked against what this signature admits — the same
                // rule the free-function form has had since v0.0.159, and it was missing here.
                //
                // `method_effects` was populated and read in exactly one place: setting
                // `allowed_effects` for a method's own BODY. So a method's declared effects were
                // enforced INWARD, permitting what its body does, and never OUTWARD at its callers.
                //
                // That made the property this feature exists for **false through a method call**. A
                // function declaring nothing could call `Loader.load`, which touches files, and the
                // effect vanished from the signature chain — so "a signature that says nothing about
                // the network is a signature you can trust about the network, however deep the call
                // goes" was not true, and `burxt review` reporting a GAINED effect could be evaded by
                // putting the call behind a method.
                //
                // Found in v0.0.183 by stage-1, which enforced effects for the first time and
                // immediately refused stage-1's own `load_program`. The differential running in that
                // direction is the third time this week.
                if self.in_pure.is_none() {
                    if let Some(reaches) =
                        self.method_effects.get(&(receiver.clone(), method.clone())).cloned()
                    {
                        for e in &reaches {
                            if !self.allowed_effects.contains(e) {
                                return Err(self
                                    .effect_refusal(&format!("{}.{}", receiver, method), *e));
                            }
                        }
                    }
                }
                // An `allocates` method builds in OUR region, same rule as the
                // free-function form.
                if self.alloc_methods.contains(&(receiver.clone(), method.clone()))
                    && !self.has_region()
                {
                    return Err(format!(
                        "`{}.{}` is declared `allocates`, so it builds its result in \
                         the caller's region — and there is none open here. Wrap the \
                         call in `region name {{ ... }}`, or declare the enclosing \
                         function `allocates` too.",
                        receiver, method
                    ));
                }
                if arguments.len() != param_tys.len() {
                    return Err(format!(
                        "method `{}.{}` takes {} argument(s), but {} were given",
                        self.shown_type_name(&receiver),
                        method,
                        param_tys.len(),
                        arguments.len()
                    ));
                }
                let mut typed_args = Vec::new();
                for (i, (argument, param_ty)) in arguments.iter().zip(&param_tys).enumerate() {
                    let typed = self.check_expr(argument, Some(param_ty))?;
                    if !self.storable(&typed.ty, param_ty) {
                        return Err(format!(
                            "in the call to `{}.{}`, argument {} must be {}, \
                             but it has type {}",
                            self.shown_type_name(&receiver),
                            method,
                            i + 1,
                            self.shown(param_ty),
                            self.shown(&typed.ty)
                        ));
                    }
                    typed_args.push(typed);
                }
                Ok(TypedExpr {
                    ty: ret,
                    kind: TypedExprKind::MethodCall {
                        receiver,
                        method: method.clone(),
                        receiver_mut,
                        base: Box::new(typed_base),
                        arguments: typed_args,
                    },
                })
            }

            ExprKind::ArrayLit(elems) => {
                if let Some(Type::Slice(elem_ty)) = expected {
                    let elem_ty = elem_ty.as_ref().clone();
                    let mut typed = Vec::new();
                    for (i, e) in elems.iter().enumerate() {
                        let t = self.check_expr(e, Some(&elem_ty))?;
                        if !self.storable(&t.ty, &elem_ty) {
                            return Err(format!(
                                "in this growable array, element {} must be {}, but it \
                                 has type {}",
                                i, elem_ty, t.ty
                            ));
                        }
                        typed.push(t);
                    }
                    return Ok(TypedExpr {
                        ty: Type::Slice(Box::new(elem_ty)),
                        kind: TypedExprKind::SliceLit(typed),
                    });
                }
                let (elem_ty, len) = match expected {
                    Some(Type::Array { elem, len }) => (elem.as_ref().clone(), *len),
                    // An array literal is the one thing local inference cannot serve, and
                    // not because of the element type: a list of values does not say
                    // whether the array is FIXED or GROWABLE, and that is a decision with
                    // different storage, different rules and different costs behind it.
                    // So an array binding names its type, and says which.
                    // See spec/1.0/M10-ERGONOMICS.md §1 Decision 2.
                    _ => {
                        return Err(
                            "an array literal does not say whether the array is fixed or \
                             growable, so an array binding names its type: \
                             `let xs: [Int; 3] = [1, 2, 3];` for a fixed one, or \
                             `let mutable xs: [Int] = [];` for one that grows."
                                .to_string(),
                        )
                    }
                };
                if elems.len() != len as usize {
                    return Err(format!(
                        "this literal has {} value(s), but [{}; {}] holds exactly {}",
                        elems.len(),
                        elem_ty,
                        len,
                        len
                    ));
                }
                let mut typed = Vec::new();
                for (i, e) in elems.iter().enumerate() {
                    let t = self.check_expr(e, Some(&elem_ty))?;
                    if !self.storable(&t.ty, &elem_ty) {
                        return Err(format!(
                            "in this array literal, element {} must be {}, but it \
                             has type {}",
                            i, elem_ty, t.ty
                        ));
                    }
                    typed.push(t);
                }
                Ok(TypedExpr {
                    ty: Type::Array { elem: Box::new(elem_ty), len },
                    kind: TypedExprKind::ArrayLit(typed),
                })
            }

            ExprKind::Index { base, index } => {
                let typed_base = self.check_expr(base, None)?;
                if let Type::Slice(elem) = typed_base.ty.clone() {
                    let idx = self.check_expr(index, None)?;
                    if idx.ty != Type::Int {
                        return Err(format!(
                            "an index must be an Int, but this one has type {}",
                            idx.ty
                        ));
                    }
                    return Ok(TypedExpr {
                        ty: elem.as_ref().clone(),
                        kind: TypedExprKind::SliceIndex {
                            base: Box::new(typed_base),
                            index: Box::new(idx),
                        },
                    });
                }
                let (elem, len) = match &typed_base.ty {
                    Type::Array { elem, len } => (elem.as_ref().clone(), *len),
                    other => {
                        return Err(format!(
                            "indexing with `[...]` needs an array, but this has type {}",
                            other
                        ))
                    }
                };
                let index =
                    self.check_index(&format!("{}", typed_base.ty), len, index)?;
                Ok(TypedExpr {
                    ty: elem,
                    kind: TypedExprKind::Index {
                        base: Box::new(typed_base),
                        len,
                        index: Box::new(index),
                    },
                })
            }
        }
    }

    /// Check an index expression: it must be an Int, and a LITERAL index
    /// that is provably out of range is refused at compile time — it would
    /// always fail at runtime, so it fails now instead.
    fn check_index(&self, what: &str, len: u32, index: &Expr) -> Result<TypedExpr, String> {
        let typed = self.check_expr(index, None)?;
        if typed.ty != Type::Int {
            return Err(format!(
                "an array index must be an Int, but this one has type {}",
                typed.ty
            ));
        }
        if let TypedExprKind::IntLit(n) = typed.kind {
            // `len == 0` means the bound is only known at run time — a growable array. A
            // FIXED array of length zero cannot exist (the language refuses one, because it
            // describes nothing), so the marker is unambiguous. A negative literal is still
            // always wrong, whatever the length turns out to be.
            if len == 0 {
                if n < 0 {
                    return Err(format!(
                        "index {} is negative, so it is out of bounds for {} whatever its \
                         length turns out to be.",
                        n, what
                    ));
                }
                return Ok(typed);
            }
            if n < 0 || n >= len as i64 {
                return Err(format!(
                    "index {} is out of bounds for {}: valid indexes are 0 to {}. \
                     This would always fail at runtime, so it is refused now.",
                    n,
                    what,
                    len - 1
                ));
            }
        }
        Ok(typed)
    }

    /// "unknown variable", but if the name is some enum's variant, say how to
    /// write it — bare `Plus` is a natural slip when `Token.Plus` is meant.
    fn unknown_name(&self, name: &str) -> String {
        for (en, variants) in &self.enums {
            if variants.iter().any(|(v, _)| v == name) {
                return format!(
                    "unknown variable: {} — did you mean `{}.{}`? Enum variants are \
                     always written with their enum.",
                    name, en, name
                );
            }
        }
        format!("unknown variable: {}", name)
    }

    /// Resolve `Enum.Variant` construction. Returns None when the base is not
    /// an enum name, so ordinary field access and method calls fall through
    /// unchanged.
    fn check_variant_lit(
        &self,
        base: &Expr,
        variant: &str,
        arguments: &[Expr],
        expected: Option<&Type>,
    ) -> Option<Result<TypedExpr, String>> {
        let ExprKind::Var(enum_name) = &base.kind else { return None };
        // A local binding wins over an enum of the same name: shadowing is
        // refused elsewhere, so this can only be a genuine variable.
        if self.env.contains_key(enum_name) {
            return None;
        }
        // A generic enum needs its arguments worked out first, and that is a different
        // question with a different answer, so it gets its own path.
        if self.generic_enums.contains_key(enum_name) {
            return Some(self.build_generic_variant(enum_name, variant, arguments, expected));
        }
        let variants = self.variants_of(enum_name)?;
        Some(self.build_variant(enum_name, variants, variant, arguments))
    }

    fn build_variant(
        &self,
        enum_name: &str,
        variants: Vec<(String, Vec<Type>)>,
        variant: &str,
        arguments: &[Expr],
    ) -> Result<TypedExpr, String> {
        let tag = variants
            .iter()
            .position(|(n, _)| n == variant)
            .ok_or_else(|| {
                format!(
                    "`{}` has no variant named `{}`. Its variants are: {}.",
                    enum_name,
                    variant,
                    variants.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", ")
                )
            })?;
        let payload = &variants[tag].1;
        if arguments.len() != payload.len() {
            return Err(format!(
                "`{}.{}` carries {} value(s), but {} were given",
                enum_name,
                variant,
                payload.len(),
                arguments.len()
            ));
        }
        let mut typed_args = Vec::new();
        for (i, (argument, want)) in arguments.iter().zip(payload).enumerate() {
            let t = self.check_expr(argument, Some(want))?;
            if !self.storable(&t.ty, want) {
                return Err(format!(
                    "in `{}.{}`, payload {} must be {}, but it has type {}",
                    enum_name,
                    variant,
                    i + 1,
                    want,
                    t.ty
                ));
            }
            typed_args.push(t);
        }
        Ok(TypedExpr {
            ty: Type::Named(enum_name.to_string()),
            kind: TypedExprKind::VariantLit {
                enum_name: enum_name.to_string(),
                tag: tag as u32,
                arguments: typed_args,
            },
        })
    }

    /// Resolve `.field` on a value of type `ty` to (positional index, type).
    fn resolve_field(&self, ty: &Type, field: &str) -> Result<(u32, Type), String> {
        // A tuple whose elements still mention a type parameter — `p.0` inside `zip<T, U>`.
        // It has no anonymous class yet, so there is nothing for the `Named` path to look up,
        // but the POSITION is known regardless of what `T` turns out to be. Answering it here
        // is what lets a generic's body read the tuple it built, which is the whole of what
        // `zip` and `enumerate` need. `private` cannot apply: a tuple has no declaration to
        // declare it in.
        if let Type::Tuple(elements) = ty {
            return match field.parse::<usize>() {
                Ok(i) if i < elements.len() => Ok((i as u32, elements[i].clone())),
                _ => Err(format!(
                    "`{}` is a tuple of {} values, so it has no `.{}`. Its positions are {}, \
                     counting from zero.",
                    ty,
                    elements.len(),
                    field,
                    (0..elements.len())
                        .map(|i| format!("`.{}`", i))
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
            };
        }
        let name = match ty {
            Type::Named(n) => n,
            other => {
                return Err(format!(
                    "`.{}` needs a class value, but the value has type {}.",
                    field, other
                ))
            }
        };
        // `private` — and this is the only place a field is resolved, which is why the rule
        // needs no sweep. A class's own methods may reach its private fields; nothing else can.
        if self.current_receiver.as_deref() != Some(name.as_str()) {
            if let Some(hidden) = self.private_fields.get(name) {
                if hidden.iter().any(|h| h == field) {
                    return Err(format!(
                        "`{}.{}` is private: it is reachable only from `{}`'s own methods. \
                         Read it through a method that `{}` provides, or drop `private` from \
                         the field if it is part of the type's API.",
                        name, field, name, name
                    ));
                }
            }
        }
        let fields = self
            .fields_of(name)
            .ok_or_else(|| format!("unknown type `{}`", name))?;
        fields
            .iter()
            .position(|(n, _)| n == field)
            .map(|i| (i as u32, fields[i].1.clone()))
            .ok_or_else(|| {
                // A tuple has POSITIONS, not names, and the ordinary message would send a
                // reader looking for a `class` declaration that was never written. It also
                // has to answer the reader who wrote `pair.first` — the word `field` alone
                // would not tell them that a name is the wrong shape of thing here.
                if Self::is_tuple_symbol(name) {
                    return format!(
                        "`{}` is a tuple of {} values, so it has no `.{}`. Its positions \
                         are {}, counting from zero.",
                        name,
                        fields.len(),
                        field,
                        (0..fields.len())
                            .map(|i| format!("`.{}`", i))
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
                format!(
                    "`{}` has no field named `{}`. Its fields are: {}.",
                    self.shown_type_name(name),
                    field,
                    self.field_list(name)
                )
            })
    }

    /// Every field of `name` can be compared, or the first one that cannot, named.
    ///
    /// Recursive, because a class holding a class compares field by field all the way down —
    /// and cycle-guarded, because `embeds_by_value` proved that a walk over types has to be. A class
    /// cannot contain itself by value (that rule refuses it), so the guard is belt and braces rather
    /// than load-bearing; it costs one Vec and removes a class of hang.
    fn class_is_comparable(&self, name: &str, seen: &mut Vec<String>) -> Result<(), String> {
        if seen.iter().any(|s| s == name) {
            return Ok(());
        }
        seen.push(name.to_string());
        let Some(fields) = self.fields_of(name) else {
            return Err(format!("unknown type `{}`", name));
        };
        for (field, ty) in fields {
            let why = match &ty {
                Type::Int | Type::Bool | Type::String | Type::Decimal { .. } => None,
                Type::Named(inner) if self.is_enum(inner) => Some(format!(
                    "the enum `{}`, and `==` on an enum is not available yet",
                    inner
                )),
                Type::Named(inner) => {
                    self.class_is_comparable(inner, seen)?;
                    None
                }
                // A slice is a pointer, a length and a capacity. Comparing it element-wise is a
                // reasonable thing to want and a separate decision — two slices of equal contents but
                // different capacity would have to be equal, and nothing says so yet.
                Type::Slice(_) => Some("a growable array, and `==` on one is a separate question — two arrays with equal contents and different capacity would have to be equal, and nothing has decided that"
                    .to_string()),
                Type::Array { .. } => Some(
                    "a fixed array, and element-wise `==` on one is not available yet".to_string(),
                ),
                Type::Dyn(t) => Some(format!(
                    "a `dynamic {}`, which is a pointer pair — comparing it would compare addresses, not values",
                    t
                )),
                other => Some(format!("{}, which cannot be compared", other)),
            };
            if let Some(why) = why {
                return Err(format!(
                    "`==` on `{}` needs every field to be comparable, and `{}.{}` is {}.",
                    name, name, field, why
                ));
            }
        }
        Ok(())
    }

    /// Comparisons are always exact, and both sides must have the SAME type —
    /// comparing money of different scales (or contracts) is refused just like
    /// adding it would be.
    fn check_compare(&self, op: CmpOp, lhs: &Type, rhs: &Type) -> Result<(), String> {
        use Type::*;
        match (lhs, rhs) {
            (Int, Int) => Ok(()),
            (Named(a), Named(b)) if a == b => {
                // A CLASS compares field by field, and needs no `derive` to do it.
                //
                // Burxt can get away with that where Rust cannot, and the reason is the language's
                // own restrictions paying off: a class has value semantics, no interior pointers and
                // a fixed cell layout, so field-by-field is not *a* definition of equality — it is
                // the only one available. Nothing is being chosen on the programmer's behalf, which
                // is what a `derive` exists to make explicit.
                //
                // NOT memcmp, and that distinction is the whole of the work: a class holding a
                // String holds a POINTER, and two equal strings need not live at the same address.
                // Comparing the bytes of the struct would answer `false` for two accounts with the
                // same owner built separately — a wrong answer that looks like a working program.
                //
                // Ordering is refused: `<` on a class would have to pick which field dominates, and
                // that is a decision nobody wrote down.
                if !matches!(op, CmpOp::Eq | CmpOp::Ne) {
                    return Err(format!(
                        "`{}` on `{}` would have to decide which field comes first, and nothing says which. Compare the field you mean, or give `{}` a method that answers the question you are really asking.",
                        op, a, a
                    ));
                }
                if self.is_enum(a) {
                    return Err(format!(
                        "`==` on the enum `{}` is not available yet: two variants can carry different payloads, so equality has to compare the TAG first and then only the payload that variant holds. Use `match`.",
                        a
                    ));
                }
                self.class_is_comparable(a, &mut Vec::new())
            }
            (Named(a), Named(b)) => Err(format!(
                "cannot compare `{}` with `{}`: one equality, no coercion. Both sides of `==` must be the same type.",
                a, b
            )),
            // Strings compare by BYTES, and only for equality. This is the
            // same `==` every other type uses — not a parallel string-equals
            // path — so a cross-type comparison involving a String falls
            // through to the shared catch-all below and reads identically to
            // any other type mismatch.
            // Strings compare by BYTES, for equality and for order alike (v0.0.202).
            //
            // **Byte order, and the documentation says so rather than implying it.** This is not
            // alphabetical order in any language: "Zebra" sorts before "apple" because 'Z' is 90 and
            // 'a' is 97, and "ä" sorts after both because it is two bytes. Locale collation is a
            // real thing people want and it is a DECISION — which language, which of that language's
            // several orders — so a `<` that quietly picked one would be exactly the silent choice
            // this language refuses. Byte order is the one ordering that needs no decision, is the
            // same on every machine, and is what a sort needs to be reproducible.
            //
            // It was refused until now with "byte ordering arrives with collections". Collections
            // arrived; `lib/array.bx` could sort numbers and not names, which is the more common
            // want.
            (String, String) => Ok(()),
            // Two pointers being equal says nothing a program can act on, and pointer ORDERING
            // says even less. The question people actually mean is "did the call fail", and that
            // has a name.
            (CPointer, _) | (_, CPointer) => Err(format!(
                "a CPointer has no `{}`: it is a token to hand back to C, not a value to \
                 compare. To test whether the call failed, use `c_is_null(p)`; to read what \
                 it points at, `c_string_at(p)`, which copies.",
                op
            )),
            (Bool, Bool) => match op {
                CmpOp::Eq | CmpOp::Ne => Ok(()),
                _ => Err(format!(
                    "Bools have no order: `{}` does not apply; only `==` and `!=` do",
                    op
                )),
            },
            (Decimal { .. }, Decimal { .. }) => {
                if lhs == rhs {
                    Ok(())
                } else {
                    self.matching_decimal(format!("compare ({})", op), lhs, rhs).map(|_| ())
                }
            }
            // Two values of the same type PARAMETER. Whether this is allowed is entirely
            // what the bound says, which is the point of bounds: the body is checked
            // against the signature's promise, not against whatever the instantiations
            // happen to permit. See spec/1.0/M7-GENERICS.md Decision 2.
            (Param(a), Param(b)) if a == b => {
                let bound = self.param_bounds.get(a).cloned().flatten();
                match (bound.as_deref(), op) {
                    (Some("Ordered"), _) => Ok(()),
                    (Some("Equatable"), CmpOp::Eq | CmpOp::Ne) => Ok(()),
                    (Some("Equatable"), _) => Err(format!(
                        "`{}: Equatable` says two values of it can be compared for \
                         equality, not ordered — `{}` needs `{}: Ordered`.",
                        a, op, a
                    )),
                    (Some(other), _) => Err(format!(
                        "`{}: {}` says a value of it has {}'s methods, not an order. \
                         Comparing with `{}` needs `{}: Ordered`, or `{}: Equatable` for \
                         `==` and `!=`.",
                        a, other, other, op, a, a
                    )),
                    (None, _) => Err(unbounded_compare(a, op)),
                }
            }
            _ => Err(format!(
                "type error: cannot compare {} and {} — the types must match exactly",
                lhs, rhs
            )),
        }
    }

    /// The core thesis rules for arithmetic result types.
    fn check_binop(
        &self,
        op: BinOp,
        lhs: &Type,
        rhs: &Type,
        expected: Option<&Type>,
    ) -> Result<Type, String> {
        use Type::*;
        match (op, lhs, rhs) {
            // Integer division truncates — that is silent rounding. Refused
            // until integers get explicit division semantics.
            (BinOp::Div, Int, Int) => Err(
                "`/` on two Ints would have to round, and one operator cannot say \
                 which way: -7 divided by 2 is -3 rounding toward zero and -4 \
                 rounding down. Say which you mean — `divide_floor(a, b)`, \
                 `divide_toward_zero(a, b)`, or `remainder(a, b)` for the remainder."
                    .to_string(),
            ),

            // Integer arithmetic.
            (_, Int, Int) => Ok(Int),

            // String + String concatenates into the enclosing region. The
            // result's bytes are region-allocated, so the same escape rules
            // apply — enforced where the value is bound, not here.
            (BinOp::Add, String, String) => {
                if !self.has_region() {
                    return Err(self.needs_region("joining strings with `+` allocates"));
                }
                Ok(String)
            }

            // Decimal +/- Decimal: exact, but the types must match exactly
            // (same scale AND same rounding contract).
            (BinOp::Add, Decimal { .. }, Decimal { .. })
            | (BinOp::Sub, Decimal { .. }, Decimal { .. }) => {
                self.matching_decimal(op, lhs, rhs)
            }

            // Decimal * Int (or Int * Decimal): scale the money value by a count.
            // Always exact, so no rounding contract needed. This is `price * qty`.
            (BinOp::Mul, dec @ Decimal { .. }, Int) | (BinOp::Mul, Int, dec @ Decimal { .. }) => {
                Ok(dec.clone())
            }

            // Decimal * Decimal: the product's natural scale is the SUM of the
            // operand scales, so landing it always needs a rounding contract.
            //
            // Mixed operand scales are legal here — and only here. Addition
            // combines like quantities, so its scales must match; multiplication
            // combines a quantity with a RATE, whose scales differ by nature
            // (a price is scale-2, a tax rate is finer). What keeps that safe is
            // that the RESULT's contract, which is mandatory, says how the
            // sum-of-scales product narrows. Never optional: a silently rounded
            // product would break the thesis.
            (BinOp::Mul, Decimal { scale: ls, .. }, Decimal { scale: rs, .. }) => {
                // The product's TRUE scale is the sum of the operands'. At that scale
                // nothing rounds, so nothing needs declaring — and a contract demanded
                // where no rounding happens teaches the reader that contracts are
                // ceremony. They are not: one appears exactly where a value narrows.
                //
                // A scaled i64 holds 18 fractional digits, so a sum past that cannot be
                // represented and the result must narrow whatever the author wanted.
                let exact = ls + rs;
                let representable = exact <= 18;
                let exact_ty = Decimal { scale: exact, rounding: None };
                match expected {
                    // A contract was asked for: it says how the product narrows, and to
                    // what. This is the only arm that can round.
                    Some(t @ Decimal { rounding: Some(_), .. }) => Ok(t.clone()),
                    // Exactly the product's own width: nothing rounds, nothing to declare.
                    Some(Decimal { scale, rounding: None })
                        if representable && *scale == exact =>
                    {
                        Ok(exact_ty)
                    }
                    // Some other width, with no contract to say how it gets there.
                    Some(Decimal { scale, .. }) => Err(format!(
                        "this multiplication of {} by {} has an exact product with {} \
                         decimal places, and reaching Decimal<{}> means rounding it. \
                         Say how — Decimal<{}, RoundHalfEven> — or take the exact \
                         answer with Decimal<{}>{}.",
                        lhs,
                        rhs,
                        exact,
                        scale,
                        scale,
                        exact,
                        if representable { "" } else { ", which does not fit an i64" }
                    )),
                    // Nothing asked for it. Identical operands that carry a contract land
                    // the product at their OWN scale — the context does not have to repeat
                    // what the operands already say, so `print(a * b)` on money answers in
                    // money. (v0.0.86; kept, because it is the common case.)
                    _ if lhs == rhs && matches!(lhs, Decimal { rounding: Some(_), .. }) => {
                        Ok(lhs.clone())
                    }
                    // Otherwise the exact product, and the decision about narrowing it
                    // belongs wherever this value is finally stored. A caller expecting
                    // something else reports that mismatch itself rather than having this
                    // rule guess at it. See spec/1.0/M10-ERGONOMICS.md §1 Decision 5.
                    _ if representable => Ok(exact_ty),
                    _ => Err(format!(
                        "this multiplication of {} by {} has an exact product with {} \
                         decimal places, which does not fit a scaled i64. Bind it to a \
                         Decimal with a rounding contract, e.g. \
                         `let x: Decimal<2, RoundHalfEven> = ...`.",
                        lhs, rhs, exact
                    )),
                }
            }

            // Decimal / Decimal still requires matching scales: this decision
            // covered multiplication only.
            (BinOp::Div, Decimal { .. }, Decimal { .. }) => {
                let result = self.matching_decimal(op, lhs, rhs)?;
                self.require_rounding(op, &result)
            }

            // Decimal / Int: the quotient can also fall between representable
            // values (1.00 / 3), so a rounding contract is required too.
            (BinOp::Div, dec @ Decimal { .. }, Int) => self.require_rounding(op, dec),

            // Decimal +/- Int and friends: refuse. No silent int->decimal.
            (_, a, b) => Err(format!(
                "type error: cannot apply `{}` to {} and {}",
                op, a, b
            )),
        }
    }

    /// Both operands must be the SAME decimal type: equal scale and equal
    /// rounding contract. Burxt never reconciles differing money types.
    /// Whether a value of type `have` may go where `want` was declared — a `let`, a call argument, a
    /// field initializer, a variant payload, an assignment, a `return`.
    ///
    /// Equality, plus **one** widening: a rounding contract may be ADDED to a value that
    /// has none. The representation is byte-identical — both are a scaled i64 holding the
    /// same integer — and a contract does not reinterpret the value, it constrains what
    /// future operations may do to it. That is strictly more information, not different
    /// information.
    ///
    /// Why this matters more than it looks (v0.0.86): without it, a contract could only be
    /// declared where money ENTERS the program, so `let tax: Decimal<2, RoundHalfEven> =
    /// price * qty;` failed and the fix was not where the error was — you had to walk back
    /// to the binding of `price` and change that. Dropping a contract stays refused: that
    /// loses a declared intention, and losing one silently is what this language is for.
    ///
    /// **Used at every position, since v0.0.194.** The history is worth keeping, because the same
    /// mistake was made twice and the second time was hidden by the note about the first.
    ///
    /// Until v0.0.181 this had ONE call site — the `let` — so `json_money(tax)` was refused where
    /// `let m: Decimal<2, RoundHalfEven> = tax;` was fine, and the rule read as arbitrary because it
    /// WAS arbitrary: a relaxation implemented in one place. v0.0.181 extended it, and this comment
    /// then claimed "every position" while **seven positions still compared types with `==`**:
    /// `return`, a field assignment, an index assignment on either path, `push`, a method argument,
    /// and an array literal's elements — growable or fixed. The list two paragraphs up even *names*
    /// `return`, and `return` was one of the seven.
    ///
    /// So the rule for this codebase, restated, because stating it once did not make it true:
    /// **a relaxation that applies at a binding applies wherever a declared type meets a value, and
    /// finding it at one site means the others were not checked.** The way to know it holds is a
    /// fixture that exercises every position in one program — `tests/pass/a_contract_may_be_added.bx`
    /// — and not a comment claiming it does. A comment cannot be run.
    ///
    /// Same shape as the `dynamic` coercion gap closed in v0.0.175, where the binding path coerced and
    /// the argument path did not.
    fn storable(&self, have: &Type, want: &Type) -> bool {
        if have == want {
            return true;
        }
        matches!(
            (have, want),
            (
                Type::Decimal { scale: a, rounding: None },
                Type::Decimal { scale: b, rounding: Some(_) },
            ) if a == b
        )
    }

    fn matching_decimal(
        &self,
        op: impl std::fmt::Display,
        lhs: &Type,
        rhs: &Type,
    ) -> Result<Type, String> {
        if lhs == rhs {
            return Ok(lhs.clone());
        }
        if let (
            Type::Decimal { scale: a, rounding: ra },
            Type::Decimal { scale: b, rounding: rb },
        ) = (lhs, rhs)
        {
            if a != b {
                return Err(format!(
                    "cannot {} {} and {}: scales must match. \
                     Burxt does not silently rescale money.",
                    op, lhs, rhs
                ));
            }
            // Addition and subtraction NEVER round, so a rounding contract on one side
            // and none on the other is not a conflict: there is exactly one answer to
            // "if this ever rounds, which way", and the result carries it.
            //
            // Two DIFFERENT contracts still conflict — that is a genuine ambiguity, and
            // picking one would be the silent decision this language exists to refuse.
            // (Relaxed in v0.0.86: the old rule cost three attempts on a seven-line
            // invoice, and the scale rule is the one that protects money.)
            match (ra, rb) {
                (Some(x), Some(y)) if x != y => {
                    return Err(format!(
                        "cannot {} {} and {}: these are two different rounding contracts, \
                         and picking one would be a decision nobody wrote down. Make them \
                         the same, or drop one to a plain Decimal<{}>.",
                        op, lhs, rhs, a
                    ));
                }
                (Some(x), _) => return Ok(Type::Decimal { scale: *a, rounding: Some(*x) }),
                (_, Some(y)) => return Ok(Type::Decimal { scale: *a, rounding: Some(*y) }),
                _ => return Ok(Type::Decimal { scale: *a, rounding: None }),
            }
        }
        Err(format!(
            "cannot {} {} and {}: rounding contracts must match. \
             Burxt does not silently pick one.",
            op, lhs, rhs
        ))
    }

    /// The operation rounds, so the decimal type must carry a rounding contract.
    fn require_rounding(&self, op: BinOp, dec: &Type) -> Result<Type, String> {
        match dec {
            Type::Decimal { rounding: Some(_), .. } => Ok(dec.clone()),
            Type::Decimal { scale, rounding: None } => Err(format!(
                "`{}` on {} needs an explicit rounding contract, because the exact \
                 result can have more than {} decimal places. Declare one in the \
                 type, e.g. Decimal<{}, RoundHalfEven> or Decimal<{}, RoundHalfUp>.",
                op,
                dec,
                scale,
                scale,
                scale
            )),
            other => unreachable!("require_rounding called on {}", other),
        }
    }
}

/// Does this body contain a call to `name`? Used to refuse a `decreases` measure on
/// a function that never recurses — a claim with nothing to check reads as if it
/// meant something.
fn calls_itself(body: &[Stmt], name: &str) -> bool {
    fn in_expr(e: &Expr, name: &str) -> bool {
        let any = |list: &[Expr]| list.iter().any(|x| in_expr(x, name));
        match &e.kind {
            ExprKind::Call { name: callee, arguments } => callee == name || any(arguments),
            ExprKind::Neg(i) | ExprKind::Not(i) => in_expr(i, name),
            ExprKind::Logical { lhs, rhs, .. }
            | ExprKind::Binary { lhs, rhs, .. }
            | ExprKind::Compare { lhs, rhs, .. } => in_expr(lhs, name) || in_expr(rhs, name),
            ExprKind::MethodCall { base, arguments, .. } => in_expr(base, name) || any(arguments),
            ExprKind::StructLit { fields, .. } => fields.iter().any(|(_, v)| in_expr(v, name)),
            ExprKind::Field { base, .. } => in_expr(base, name),
            ExprKind::ArrayLit(items) => any(items),
            ExprKind::Try(inner) => in_expr(inner, name),
            ExprKind::Index { base, index } => in_expr(base, name) || in_expr(index, name),
            ExprKind::InterpStr(parts) => parts.iter().any(|p| match p {
                InterpPart::Expr(x) => in_expr(x, name),
                InterpPart::Lit(_) => false,
            }),
            _ => false,
        }
    }

    fn in_stmt(s: &Stmt, name: &str) -> bool {
        let block = |b: &[Stmt]| b.iter().any(|x| in_stmt(x, name));
        match &s.kind {
            StmtKind::Let { value, .. }
            | StmtKind::Print { value, .. }
            | StmtKind::Return(value)
            | StmtKind::ExprStmt(value)
            | StmtKind::Assign { value, .. } => in_expr(value, name),
            StmtKind::TailReturn(value) => in_expr(value, name),
            StmtKind::AssignField { value, .. } => in_expr(value, name),
            StmtKind::AssignIndex { index, value, .. } => {
                in_expr(index, name) || in_expr(value, name)
            }
            StmtKind::AssignFieldIndex { index, value, .. } => {
                in_expr(index, name) || in_expr(value, name)
            }
            StmtKind::If { cond, then_block, else_block } => {
                in_expr(cond, name)
                    || block(then_block)
                    || else_block.as_deref().is_some_and(block)
            }
            StmtKind::While { cond, body } => in_expr(cond, name) || block(body),
            StmtKind::For { iterable, body, .. } => in_expr(iterable, name) || block(body),
            StmtKind::ForRange { start, end, body, .. } => {
                in_expr(start, name) || in_expr(end, name) || block(body)
            }
            StmtKind::Region { body, .. } => block(body),
            StmtKind::Match { value, arms } => {
                in_expr(value, name) || arms.iter().any(|a| block(&a.body))
            }
            StmtKind::Break | StmtKind::Continue => false,
        }
    }

    body.iter().any(|s| in_stmt(s, name))
}

/// Does this expression mention a name anywhere inside it?
///
/// Used for one thing: refusing `old(result)` with an explanation instead of a
/// name-resolution failure. Worth a walker rather than a message that reads like a
/// bug report.
fn mentions(e: &Expr, name: &str) -> bool {
    let any = |list: &[Expr]| list.iter().any(|x| mentions(x, name));
    match &e.kind {
        ExprKind::Var(n) => n == name,
        ExprKind::Neg(i) | ExprKind::Not(i) | ExprKind::Try(i) => mentions(i, name),
        ExprKind::Logical { lhs, rhs, .. }
        | ExprKind::Binary { lhs, rhs, .. }
        | ExprKind::Compare { lhs, rhs, .. } => mentions(lhs, name) || mentions(rhs, name),
        ExprKind::Call { arguments, .. } => any(arguments),
        ExprKind::MethodCall { base, arguments, .. } => mentions(base, name) || any(arguments),
        ExprKind::StructLit { fields, .. } => fields.iter().any(|(_, v)| mentions(v, name)),
        ExprKind::Field { base, .. } => mentions(base, name),
        ExprKind::ArrayLit(items) | ExprKind::TupleLit(items) => any(items),
        ExprKind::Index { base, index } => mentions(base, name) || mentions(index, name),
        ExprKind::InterpStr(parts) => parts.iter().any(|p| match p {
            InterpPart::Expr(x) => mentions(x, name),
            InterpPart::Lit(_) => false,
        }),
        ExprKind::IntLit(_)
        | ExprKind::DecimalLit { .. }
        | ExprKind::BoolLit(_)
        | ExprKind::StrLit(_) => false,
    }
}

/// Does control leave this statement without falling through — by returning, or by
/// jumping out of a loop? Used for the unreachable-code check.
///
/// Deliberately NOT the same question as `stmt_returns`: a `break` ends a block but
/// does not satisfy a function's obligation to return a value, and conflating the two
/// would let a function end in `break` and be accepted.
fn stmt_diverges(s: &TypedStmt) -> bool {
    match &s.kind {
        TypedStmtKind::Break | TypedStmtKind::Continue => true,
        TypedStmtKind::If { then_block, else_block: Some(e), .. } => {
            block_diverges(then_block) && block_diverges(e)
        }
        TypedStmtKind::Match { arms, .. } => {
            !arms.is_empty() && arms.iter().all(|a| block_diverges(&a.body))
        }
        TypedStmtKind::Region { body, .. } | TypedStmtKind::Release { body } => block_diverges(body),
        // A `for` over an empty array — or an empty range, `0..0` — runs zero times, so
        // neither form can be what makes control leave a block.
        TypedStmtKind::For { .. } | TypedStmtKind::ForRange { .. } => false,
        _ => stmt_returns(s),
    }
}

fn block_diverges(stmts: &[TypedStmt]) -> bool {
    stmts.last().is_some_and(stmt_diverges)
}

/// Does this statement return on every path through it?
fn stmt_returns(s: &TypedStmt) -> bool {
    match &s.kind {
        TypedStmtKind::Return(_) | TypedStmtKind::TailReturn { .. } => true,
        TypedStmtKind::If { then_block, else_block: Some(e), .. } => {
            block_returns(then_block) && block_returns(e)
        }
        // An exhaustive match is a return when every arm is. Exhaustiveness is
        // already proven, so the arms ARE all the paths — the same reasoning as
        // an if/else where both branches return. A `while` never counts, since
        // its condition may be false at entry.
        TypedStmtKind::Match { arms, .. } => {
            !arms.is_empty() && arms.iter().all(|a| block_returns(&a.body))
        }
        // A region is a lexical scope, not a branch: if its body returns on
        // every path, so does the region. Without this the prover asked for a
        // second `return` after the block and then called it unreachable —
        // there was no way to write a function that returns from inside a
        // region.
        TypedStmtKind::Region { body, .. } | TypedStmtKind::Release { body } => block_returns(body),
        // **`For` used to answer `block_returns(body)` here, and that was a CRASH.** A `for` whose
        // body returns was treated as returning on every path — but a `for` over an EMPTY array runs
        // zero times, so the path that skips the loop falls out of the function with no `return`.
        //
        // Measured at v0.0.241, and the shape is the worst one available:
        //
        //     function f(xs: [Int]) -> Int { for x in xs { return x; } }
        //     ...typechecks, then: error: LLVM module verification failed:
        //                          Basic Block in function 'bx.f' does not have terminator!
        //
        // The checker said yes and LLVM said no, so the user is told the COMPILER is broken rather
        // than that their program is missing a `return`. `stmt_diverges`, two functions above, had
        // already said the true thing out loud about the same construct — the two disagreed and
        // nothing compared them.
        //
        // `while` was always `_ => false` for exactly this reason; `For` is now the same. Found by a
        // subagent implementing ranges, because deciding what a RANGE should answer here made the
        // inconsistency worth measuring — it did not want to write `false` for one loop form and
        // `true` for the other. A question about new work exposing old work is the cheapest kind of
        // audit there is.
        _ => false,
    }
}

/// A block returns on every path iff its last statement does (the typechecker
/// refuses statements after one that always returns, so "last" is enough).
fn block_returns(stmts: &[TypedStmt]) -> bool {
    stmts.last().is_some_and(stmt_returns)
}

/// The value of an expression that IS an integer literal, and nothing more. Deliberately
/// not a constant folder: it answers `None` for `2 + 1`, for a binding, and for a call.
/// Used only to catch `for i in 3..0`, where a wrong answer in either direction would be
/// worse than no answer — a false `Some` would refuse a correct program.
fn literal_int(e: &TypedExpr) -> Option<i64> {
    match &e.kind {
        TypedExprKind::IntLit(n) => Some(*n),
        _ => None,
    }
}

/// Rescale a decimal's unscaled integer from `from_scale` to `to_scale`,
/// exactly. Widening (e.g. 2 -> 3) multiplies by a power of ten. Narrowing is
/// only allowed if it loses no information (trailing zeros); otherwise it's an
/// error, because silently dropping precision on money is exactly what Burxt
/// exists to prevent.
fn normalize_decimal(unscaled: i64, from_scale: u32, to_scale: u32) -> Result<i64, String> {
    if from_scale == to_scale || unscaled == 0 {
        // zero is exactly representable at every scale
        return Ok(if unscaled == 0 { 0 } else { unscaled });
    }
    if to_scale > from_scale {
        // to_scale is capped at 18, so this power always fits — but stay
        // checked rather than trusting the cap from a distance.
        let factor = 10i64
            .checked_pow(to_scale - from_scale)
            .ok_or_else(|| "decimal overflow while widening scale".to_string())?;
        unscaled
            .checked_mul(factor)
            .ok_or_else(|| "decimal overflow while widening scale".to_string())
    } else {
        // Narrowing: only ok if exactly divisible. A literal can carry more
        // fractional digits than any i64 power of ten (e.g. 24 of them) —
        // if the factor itself overflows, a nonzero value certainly loses
        // digits, so it's the same refusal.
        let lose = || {
            format!(
                "this literal needs {} decimal places but the context has only {}, \
                 and dropping digits from money is refused. Either widen the \
                 binding to Decimal<{}>, or write a literal that fits in {}. \
                 (A percent literal like `8.25%` needs 2 more places than the \
                 percentage itself: `8.25%` is exactly 0.0825, so it is a \
                 Decimal<4>.)",
                from_scale, to_scale, from_scale, to_scale
            )
        };
        let factor = 10i64.checked_pow(from_scale - to_scale).ok_or_else(lose)?;
        if unscaled % factor == 0 {
            Ok(unscaled / factor)
        } else {
            Err(lose())
        }
    }
}

// ---- generics: substitution, unification, and the name of an instantiation -----
//
// Monomorphisation, per spec/1.0/M7-GENERICS.md Decision 1: each `(generic, type arguments)`
// pair becomes its own function at compile time, so a `T` in memory is whatever the
// caller's type is rather than a pointer to it. Erasure would put a pointer where the
// value was and quietly undo everything else this language promises about what a value
// IS.

/// A type with every parameter replaced. Recursive, because a parameter can be nested:
/// `[T]`, `[T; 3]`, and eventually `List<T>`.
pub fn substitute(ty: &Type, map: &HashMap<String, Type>) -> Type {
    match ty {
        Type::Param(name) => map.get(name).cloned().unwrap_or_else(|| ty.clone()),
        Type::Array { elem, len } => Type::Array {
            elem: Box::new(substitute(elem, map)),
            len: *len,
        },
        Type::Slice(elem) => Type::Slice(Box::new(substitute(elem, map))),
        Type::Generic { name, arguments } => Type::Generic {
            name: name.clone(),
            arguments: arguments.iter().map(|a| substitute(a, map)).collect(),
        },
        // A tuple written inside a generic — `function pair_up<T>(t: T) -> (T, Int)`. The
        // `other` arm below would have copied it unchanged and left the `T` in the return
        // type, so the instantiation would have had a parameter in a signature that
        // `expand` then refused. Neither this nor `mentions_param` below is flagged by the
        // compiler, because both end in a catch-all: they are the two silent holes A8 had
        // to be looked for rather than told about.
        Type::Tuple(elements) => {
            Type::Tuple(elements.iter().map(|e| substitute(e, map)).collect())
        }
        // `function (self: List<T>) mapped(f: dynamic Mapper<T>) -> [T]`. A9 walked into the
        // hole the comment above had already named: without this arm the catch-all copied
        // `dynamic Mapper<T>` unchanged into every instantiation, so `List<Int>.mapped` asked
        // for a `dynamic Mapper<T>`, the argument `Doubler` did not match it, and the body's
        // `f.apply(x)` was refused with "needs a class value". Three messages, none of which
        // mentioned substitution. Measured on `tests/pass/a_generic_interface_through_a_
        // generic_method.bx`, which is that program.
        Type::DynGeneric { name, arguments } => Type::DynGeneric {
            name: name.clone(),
            arguments: arguments.iter().map(|a| substitute(a, map)).collect(),
        },
        other => other.clone(),
    }
}

/// Does this type mention a parameter at all? The cheap test that keeps every
/// non-generic call on exactly the path it was on before generics existed.
pub fn mentions_param(ty: &Type) -> bool {
    match ty {
        Type::Param(_) => true,
        Type::Array { elem, .. } | Type::Slice(elem) => mentions_param(elem),
        // `DynGeneric` sits with the other two for the reason the `substitute` comment above
        // gives: the catch-all answers NO in silence, and a wrong NO here means a signature
        // still holding a `T` is treated as fully concrete.
        Type::Generic { arguments, .. }
        | Type::Tuple(arguments)
        | Type::DynGeneric { arguments, .. } => arguments.iter().any(mentions_param),
        _ => false,
    }
}

/// Match a declared parameter type against an argument's actual type, binding type
/// parameters as it goes. This is the whole of Burxt's inference for type arguments:
/// structural, one direction, no unification variables and no backtracking — which is
/// why `largest(xs)` needs no turbofish and why the rule fits in one function.
/// A type as the author would write it: `Option<String>` rather than `Option$String`.
///
/// Monomorphisation names an instantiation by mangling, and that name must never be what a
/// message shows — a reader did not write `Option$String` and should not have to learn that
/// it exists.
/// A type as the author wrote it, undoing monomorphisation's renaming.
///
/// The `Type` counterpart of `show`, which answers a String. Both read `instance_of`, and both
/// exist for the same reason: a mangled symbol must never reach a reader. This one is for the
/// places that need a TYPE back rather than text — the language server, which renders the type
/// itself and then asks `explain` about it.
pub fn written_form(ty: &Type, instances: &HashMap<String, (String, Vec<Type>)>) -> Type {
    match ty {
        Type::Named(n) => match instances.get(n) {
            Some((of, arguments)) => Type::Generic {
                name: of.clone(),
                arguments: arguments.iter().map(|a| written_form(a, instances)).collect(),
            },
            None => ty.clone(),
        },
        Type::Dyn(n) => match instances.get(n) {
            Some((of, arguments)) => Type::DynGeneric {
                name: of.clone(),
                arguments: arguments.iter().map(|a| written_form(a, instances)).collect(),
            },
            None => ty.clone(),
        },
        Type::Slice(elem) => Type::Slice(Box::new(written_form(elem, instances))),
        Type::Array { elem, len } => {
            Type::Array { elem: Box::new(written_form(elem, instances)), len: *len }
        }
        other => other.clone(),
    }
}

pub fn show(ty: &Type, instances: &HashMap<String, (String, Vec<Type>)>) -> String {
    match ty {
        Type::Named(n) => match instances.get(n) {
            Some((of, arguments)) => {
                let inner: Vec<String> = arguments.iter().map(|a| show(a, instances)).collect();
                format!("{}<{}>", of, inner.join(", "))
            }
            None => n.clone(),
        },
        // The same lookup the `Named` arm makes, and it was missing until A9 put an
        // instantiation behind a `Dyn`. Without it every message about a generic interface
        // says `dynamic Mapper$Int` — a name the reader never wrote, which is the exact thing
        // this function exists to prevent.
        Type::Dyn(n) => match instances.get(n) {
            Some((of, arguments)) => {
                let inner: Vec<String> = arguments.iter().map(|a| show(a, instances)).collect();
                format!("dynamic {}<{}>", of, inner.join(", "))
            }
            None => format!("dynamic {}", n),
        },
        Type::Array { elem, len } => format!("[{}; {}]", show(elem, instances), len),
        Type::Slice(elem) => format!("[{}]", show(elem, instances)),
        Type::Generic { name, arguments } => {
            let inner: Vec<String> = arguments.iter().map(|a| show(a, instances)).collect();
            format!("{}<{}>", name, inner.join(", "))
        }
        other => other.to_string(),
    }
}

type Instances = HashMap<String, (String, Vec<Type>)>;
type Tuples = HashMap<String, Vec<Type>>;

fn unify(
    declared: &Type,
    actual: &Type,
    map: &mut HashMap<String, Type>,
    instances: &Instances,
    tuples: &Tuples,
) -> Result<(), String> {
    match (declared, actual) {
        (Type::Param(name), concrete) => {
            if let Some(already) = map.get(name) {
                if already != concrete {
                    return Err(format!(
                        "`{}` would have to be both {} and {} in this call — a type \
                         parameter stands for one type per call",
                        name, already, concrete
                    ));
                }
                return Ok(());
            }
            // `concrete` may itself be a parameter — that is a generic calling a
            // generic, where `T` stands for the OUTER function's `T`. Binding it is
            // right; what must not happen is emitting a copy for it, and the caller
            // below decides that.
            map.insert(name.clone(), concrete.clone());
            Ok(())
        }
        (Type::Array { elem: d, len: dl }, Type::Array { elem: a, len: al }) if dl == al => {
            unify(d, a, map, instances, tuples)
        }
        (Type::Slice(d), Type::Slice(a)) => unify(d, a, map, instances, tuples),
        // `Option<T>` against `Option$String`: the instantiation remembers what it was
        // made from, so the arguments line up and `T` binds to String.
        (Type::Generic { name: dn, arguments: dargs }, Type::Named(m)) => {
            match instances.get(m) {
                Some((of, aargs)) if of == dn && aargs.len() == dargs.len() => {
                    for (d, a) in dargs.iter().zip(aargs) {
                        unify(d, a, map, instances, tuples)?;
                    }
                    Ok(())
                }
                _ => Err(format!(
                    "expected {}, but this is {}",
                    show(declared, instances),
                    show(actual, instances)
                )),
            }
        }
        // `dynamic Mapper<T>` against the `Dyn("Mapper$Int")` an argument already has — the
        // interface counterpart of the `Generic`-against-`Named` arm above, and it reads
        // `instances` for exactly the same reason: `expand` mangled the instantiation, and
        // `instance_of` is the only route from `Mapper$Int` back to `(Mapper, [Int])`.
        //
        // Without it, `relay<T>(m: dynamic Mapper<T>, x: T)` called with a `dynamic Mapper<Int>`
        // cannot bind `T`, and the call is refused with a message about a type parameter the
        // caller never wrote.
        (Type::DynGeneric { name: dn, arguments: dargs }, Type::Dyn(m)) => {
            match instances.get(m) {
                Some((of, aargs)) if of == dn && aargs.len() == dargs.len() => {
                    for (d, a) in dargs.iter().zip(aargs) {
                        unify(d, a, map, instances, tuples)?;
                    }
                    Ok(())
                }
                _ => Err(format!(
                    "expected {}, but this is {}",
                    show(declared, instances),
                    show(actual, instances)
                )),
            }
        }
        // Two tuples that are both still written as tuples: a generic calling a generic,
        // where neither side has been expanded yet. Element by element, exactly as the
        // slice and array arms above.
        (Type::Tuple(d), Type::Tuple(a)) if d.len() == a.len() => {
            for (d, a) in d.iter().zip(a) {
                unify(d, a, map, instances, tuples)?;
            }
            Ok(())
        }
        // `(T, Int)` against the anonymous class the argument's tuple became — the tuple
        // counterpart of the `Generic`-against-`Named` arm above, and it needs `tuples` for
        // exactly the reason that one needs `instances`: `expand` has already turned
        // `(String, Int)` into `Named("(String, Int)")`, so the elements have to be looked
        // up rather than read off.
        //
        // **This arm exists because the alternative was a divergence.** Stage-1 never
        // monomorphises — a tuple stays a tuple with its elements beside it — so it can
        // bind `T` here with no lookup at all, and it does. Leaving stage-0 to refuse what
        // stage-1 accepts would have been a difference in what is ACCEPTED, which is §B15's
        // direction and the one this project has paid for most. Threading one more map was
        // cheaper than that.
        (Type::Tuple(d), Type::Named(m)) => match tuples.get(m) {
            Some(a) if a.len() == d.len() => {
                for (d, a) in d.iter().zip(a) {
                    unify(d, a, map, instances, tuples)?;
                }
                Ok(())
            }
            _ => Err(format!(
                "expected {}, but this is {}",
                show(declared, instances),
                show(actual, instances)
            )),
        },
        (d, a) if d == a => Ok(()),
        (d, a) => Err(format!(
            "expected {}, but this is {}",
            show(d, instances),
            show(a, instances)
        )),
    }
}

/// The symbol one instantiation gets: `identity$Int`, `largest$Decimal_2`. `$` and `_`
/// are both legal in an LLVM symbol and neither can appear in a Burxt identifier, so a
/// mangled name can never collide with a name the program wrote.
/// The way a mangled instantiation was SPELLED, for a message a reader has to act on.
///
/// `Option$Node` is not a name anyone wrote, and a diagnostic naming it sends the reader
/// looking for a declaration that does not exist. The mangling is not fully reversible —
/// `mangle` flattens `[Node]` to `_Node_` — so this reconstructs the shape, not the bytes,
/// which is the right trade for prose: `Option<Node>` is what the reader typed.
pub fn as_written(symbol: &str) -> String {
    match symbol.split_once('$') {
        None => symbol.to_string(),
        Some((base, args)) => format!("{}<{}>", base, args.split('$').collect::<Vec<_>>().join(", ")),
    }
}

pub fn mangle(name: &str, arguments: &[Type]) -> String {
    let mut out = String::from(name);
    for a in arguments {
        out.push('$');
        for c in a.to_string().chars() {
            match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' => out.push(c),
                _ => out.push('_'),
            }
        }
    }
    out
}

/// One instantiation of a generic, as an ordinary function: every type parameter replaced
/// by the caller's type, the parameter list emptied, and the mangled symbol as its name.
///
/// Substituting in the AST rather than threading a map through the checker is deliberate.
/// It means an instantiation is checked by exactly the code that checks every other
/// function — no second path that can disagree with the first, and no rule that has to
/// remember it might be looking at a parameter.
fn specialise(f: &FnDef, map: &HashMap<String, Type>, symbol: &str) -> FnDef {
    let mut out = f.clone();
    out.name = symbol.to_string();
    out.type_parameters.clear();
    for p in &mut out.parameters {
        p.ty = substitute(&p.ty, map);
    }
    out.ret = substitute(&out.ret, map);
    substitute_in_block(&mut out.body, map);
    out
}

/// A declared type can appear inside a body too — `let best: T = xs[0];` — so the walk
/// has to reach every block a statement can hold.
fn substitute_in_block(stmts: &mut [Stmt], map: &HashMap<String, Type>) {
    for s in stmts {
        match &mut s.kind {
            StmtKind::Let { declared, .. } => {
                if let Some(t) = declared {
                    *t = substitute(t, map);
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::Region { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::ForRange { body, .. } => substitute_in_block(body, map),
            StmtKind::If { then_block, else_block, .. } => {
                substitute_in_block(then_block, map);
                if let Some(b) = else_block {
                    substitute_in_block(b, map);
                }
            }
            StmtKind::Match { arms, .. } => {
                for a in arms {
                    substitute_in_block(&mut a.body, map);
                }
            }
            _ => {}
        }
    }
}

/// What an unbounded type parameter cannot do, said the same way everywhere.
///
/// Per spec/1.0/M7-GENERICS.md Decision 2: a parameter with no bound can be stored, copied,
/// passed and returned, and nothing else. Anything more needs the signature to say so,
/// because a generic whose constraints are whatever its body happens to do is a generic
/// whose signature is a lie — adding a `>` inside it would silently narrow every caller.
fn unbounded(param: &str, what: &str) -> String {
    format!(
        "`{}` is a type parameter with no bound, so a value of it can be stored, copied, \
         passed and returned — not {}, which needs to know what the value IS. Say so in \
         the signature with a bound on `{}`.",
        param, what, param
    )
}

/// Comparing two values of an unbounded parameter: the same shape as `unbounded`, said in
/// terms of the operator, and naming the bound that would allow it.
fn unbounded_compare(param: &str, op: CmpOp) -> String {
    let needed = match op {
        CmpOp::Eq | CmpOp::Ne => "Equatable",
        _ => "Ordered",
    };
    format!(
        "`{}` is a type parameter with no bound, so two values of it cannot be compared — \
         `{}` needs to know what the values ARE. Write `<{}: {}>` and the signature says \
         so, which is what makes the body's rules the same for every caller.",
        param, op, param, needed
    )
}

/// One instantiation of a method on a generic record: every type parameter replaced, the
/// receiver renamed to the instantiation's symbol, and its own parameter list emptied.
///
/// Same argument `specialise` makes for functions — the result is checked by exactly the code
/// that checks every other method, so there is no second path to disagree with the first.
fn specialise_method(m: &MethodDef, map: &HashMap<String, Type>, receiver: &str) -> MethodDef {
    let mut out = m.clone();
    out.receiver = receiver.to_string();
    out.receiver_arguments.clear();
    for p in &mut out.parameters {
        p.ty = substitute(&p.ty, map);
    }
    out.ret = substitute(&out.ret, map);
    substitute_in_block(&mut out.body, map);
    out
}

// ===========================================================================
// M14 slice 3 / A12 — per-block release
// ===========================================================================
//
// One question, asked once per block instead of once per assignment:
//
//     does anything allocated inside this block reach a binding declared outside it?
//
// If the answer is no, the block gets a `TypedStmtKind::Release` wrapper and codegen
// puts the bump cursor back at its closing brace. If the answer is yes — or if this
// pass does not RECOGNISE the construct well enough to answer — the block is left
// exactly as it was, which is the behaviour before A12 existed. That asymmetry is
// the whole safety argument: a wrong guess costs memory, never correctness
// (spec/1.0/M14-IMPLICIT-REGIONS.md §10, "no guessing inward").
//
// ALL-OR-NOTHING PER BLOCK, per §9b Decision 3. A bump allocator is LIFO, so there is
// no way to place one value below the current mark while the block keeps allocating
// above it; the moment one value escapes, restoring the cursor would free it along
// with everything else. So a block that fails the proof does not release at all.
//
// This is a POST-pass over the already-checked body, and deliberately not a set of
// lines added to the checking arms. It cannot refuse anything, it cannot change a
// diagnostic, and it cannot change which programs compile — so acceptance item 3
// ("every existing `tests/pass` program compiles unchanged") is true by construction
// rather than by testing, and the tests then confirm it.

/// What is known about one name while the pass walks a body.
#[derive(Clone, Copy)]
struct Owned {
    /// Which block's lifetime the STORAGE behind this name belongs to. `None` means
    /// "outside this function" — a parameter, or anything reached through one.
    ///
    /// Not the same as where the name was DECLARED, and the difference is a
    /// use-after-free: copying a container copies its header and shares its buffer, so
    /// `let mine: [Int] = theirs;` declares a name here that grows storage owned there.
    owner: Option<usize>,
    /// Does this name hold storage built in this function (as opposed to a literal in
    /// `.rodata`, a scalar, or the caller's)? A PARAMETER never does — its storage is
    /// the caller's, and the caller outlives the call, which is why returning one is
    /// safe and must keep being.
    allocated: bool,
}

#[derive(Default)]
struct Frame {
    /// Names declared directly in this block, removed again when it closes — so two
    /// sibling blocks that use the same name cannot inherit each other's answers.
    declared: Vec<String>,
    /// Something allocated inside this block is reachable after it ends.
    leaks: bool,
    /// Anything at all was built in here. A block that allocates nothing has nothing
    /// to release, and wrapping it would be two stores for no reason.
    allocates: bool,
}

struct ReleasePass<'a> {
    tc: &'a TypeChecker,
    frames: Vec<Frame>,
    names: HashMap<String, Owned>,
}

impl<'a> ReleasePass<'a> {
    fn depth(&self) -> usize {
        self.frames.len() - 1
    }

    /// Mark every block INSIDE `owner` as unable to release. An allocation that lands
    /// in storage owned by block *k* has to survive until *k* ends, so every block
    /// nested in it must leave the cursor alone. `None` — the caller's storage, or a
    /// `return` — taints the whole function.
    fn taint(&mut self, owner: Option<usize>) {
        let from = match owner {
            None => 0,
            Some(d) => d + 1,
        };
        for f in self.frames.iter_mut().skip(from) {
            f.leaks = true;
        }
    }

    fn mark_allocates(&mut self) {
        for f in self.frames.iter_mut() {
            f.allocates = true;
        }
    }

    fn declare(&mut self, name: &str, owner: Option<usize>, allocated: bool) {
        if let Some(f) = self.frames.last_mut() {
            f.declared.push(name.to_string());
        }
        self.names.insert(name.to_string(), Owned { owner, allocated });
    }

    fn owner_of(&self, name: &str) -> Option<usize> {
        self.names.get(name).and_then(|o| o.owner)
    }

    fn allocated(&self, name: &str) -> bool {
        self.names.get(name).is_some_and(|o| o.allocated)
    }

    /// The shorter of two lifetimes, with `None` (outside the function) the shortest of
    /// all from this pass's point of view — it is the one that permits nothing.
    fn shorter(a: Option<usize>, b: Option<usize>) -> Option<usize> {
        match (a, b) {
            (Some(x), Some(y)) => Some(x.min(y)),
            _ => None,
        }
    }

    /// Whose storage could this expression be pointing INTO? The current block, unless
    /// it mentions a name that belongs further out — in which case it may well be an
    /// alias of that name's buffer, and growing it grows theirs.
    fn place_owner(&self, e: &TypedExpr) -> Option<usize> {
        let mut acc = Some(self.depth());
        let mut vars = Vec::new();
        self.collect_vars(e, &mut vars);
        for v in vars {
            acc = Self::shorter(acc, self.owner_of(&v));
        }
        acc
    }

    /// Every name this expression reads. Used only to place storage conservatively, so
    /// over-collecting costs a release opportunity and never correctness.
    fn collect_vars(&self, e: &TypedExpr, out: &mut Vec<String>) {
        use TypedExprKind as K;
        match &e.kind {
            K::Hold { value, .. } => self.collect_vars(value, out),
            K::Held { handle, .. } => self.collect_vars(handle, out),
            K::Var(n) => out.push(n.clone()),
            K::DynCoerce { var, .. } => out.push(var.clone()),
            K::IntLit(_)
            | K::DecimalLit { .. }
            | K::BoolLit(_)
            | K::StrLit(_)
            | K::ArgCount
            | K::Old(_) => {}
            K::Neg(i)
            | K::Not(i)
            | K::Arg(i)
            | K::ReadFile(i)
            | K::CIsNull(i)
            | K::CStringAt(i)
            | K::ByteAsString(i)
            | K::ToString(i)
            | K::Hash(i)
            | K::StrLen(i)
            | K::SliceLen(i)
            | K::Field { base: i, .. }
            | K::Try { value: i, .. } => self.collect_vars(i, out),
            K::Truncate { place: a, length: b }
            | K::WriteFile { path: a, contents: b }
            | K::WriteBytes { path: a, buffer: b }
            | K::CBytesAt { pointer: a, count: b }
            | K::CBytesTo { pointer: a, bytes: b }
            | K::ByteAt { s: a, index: b }
            | K::IntDiv { lhs: a, rhs: b, .. }
            | K::Logical { lhs: a, rhs: b, .. }
            | K::Binary { lhs: a, rhs: b, .. }
            | K::Compare { lhs: a, rhs: b, .. }
            | K::Push { place: a, value: b }
            | K::SliceIndex { base: a, index: b }
            | K::Index { base: a, index: b, .. } => {
                self.collect_vars(a, out);
                self.collect_vars(b, out);
            }
            K::Substring { source, at, len } => {
                self.collect_vars(source, out);
                self.collect_vars(at, out);
                self.collect_vars(len, out);
            }
            K::Bit { lhs, rhs, .. } => {
                self.collect_vars(lhs, out);
                if let Some(r) = rhs {
                    self.collect_vars(r, out);
                }
            }
            K::Call { arguments, .. }
            | K::VariantLit { arguments, .. }
            | K::StructLit { fields: arguments, .. }
            | K::ArrayLit(arguments)
            | K::SliceLit(arguments) => {
                for a in arguments {
                    self.collect_vars(a, out);
                }
            }
            K::MethodCall { base, arguments, .. } | K::DynCall { base, arguments, .. } => {
                self.collect_vars(base, out);
                for a in arguments {
                    self.collect_vars(a, out);
                }
            }
        }
    }

    /// Does this expression produce storage built in this function?
    ///
    /// The type is asked FIRST, and it settles most of it: a value whose type cannot
    /// hold region storage cannot carry an escape, whatever it was computed from. That
    /// is `may_be_region_storage`, the same predicate B27 uses, and it answers yes for
    /// a type parameter and a `dynamic` because neither says what the storage is.
    ///
    /// This pass carried its OWN copy of that predicate for a day, because the shared one
    /// answered "no" for a generic instantiation and `lib/json.bx` printed a truncated
    /// document. The copy is gone: B39 and B42 fixed the real one in both its arms, and a
    /// second answer to one question is how the two drift apart again.
    ///
    /// The match below has no `_` arm on purpose. A new expression kind should not
    /// silently inherit "does not allocate" — it should stop the build until someone
    /// says which it is.
    fn allocates(&self, e: &TypedExpr) -> bool {
        use TypedExprKind as K;
        if !self.tc.may_be_region_storage(&e.ty) {
            return false;
        }
        match &e.kind {
            // **`handle_of` ALLOCATES, and answering `false` here was a use-after-free** — in
            // the feature built to prevent them. It copies the value into the region, and that
            // copy has to outlive the block, because the table goes on pointing at it after the
            // block ends. Per-block release reasons about what a BLOCK keeps and cannot see the
            // table, so a handle taken inside a loop was storage the next iteration reclaimed:
            //
            //     while n < 500 { h = frame(h); if n == 250 { captured = h; } n += 1; }
            //     handle_value(captured)          -> SIGSEGV, 249 frames later
            //
            // which is star-burxt's real case exactly: a command issued on one frame resolves
            // several frames later, so the driver holds a handle it took mid-flight. Saying
            // `true` puts the copy under the same rule as any other escaping value.
            K::Hold { .. } => true,
            // `handle_value` builds nothing: it hands back storage the table already holds, and
            // whoever filed it is who kept it alive.
            K::Held { .. } => false,
            // A literal String lives in `.rodata`; nothing was built.
            K::StrLit(_) | K::IntLit(_) | K::DecimalLit { .. } | K::BoolLit(_) | K::ArgCount => {
                false
            }
            // An `old(...)` capture is a copy taken on entry, before this body ran.
            K::Old(_) => false,
            K::Var(n) => self.allocated(n),
            K::DynCoerce { var, .. } => self.allocated(var),
            // Built here, every time.
            K::SliceLit(_)
            | K::Push { .. }
            | K::ReadFile(_)
            | K::CStringAt(_)
            | K::CBytesAt { .. }
            | K::ByteAsString(_)
            | K::Substring { .. }
            | K::Arg(_) => true,
            K::ToString(v) => v.ty != Type::Bool,
            K::Binary { op: BinOp::Add, lhs, rhs }
                if lhs.ty == Type::String && rhs.ty == Type::String =>
            {
                true
            }
            // Writes into memory C already owns and answers a count. Nothing is built here —
            // the array it reads was built by whoever built it.
            K::CBytesTo { .. } => false,
            K::Binary { lhs, rhs, .. } => self.allocates(lhs) || self.allocates(rhs),
            K::Call { name, arguments } => {
                self.tc.alloc_fns.contains(name)
                    || arguments.iter().enumerate().any(|(i, a)| {
                        self.tc.relay_params.contains(&(name.clone(), i)) && self.allocates(a)
                    })
            }
            K::MethodCall { receiver, method, base, arguments, .. } => {
                self.tc.alloc_methods.contains(&(receiver.clone(), method.clone()))
                    || (self.tc.relay_methods.contains(&(
                        receiver.clone(),
                        method.clone(),
                        0,
                    )) && self.allocates(base))
                    || arguments.iter().enumerate().any(|(i, a)| {
                        self.tc.relay_methods.contains(&(
                            receiver.clone(),
                            method.clone(),
                            i + 1,
                        )) && self.allocates(a)
                    })
            }
            // Behind a vtable, so no call site can see which implementation runs: the
            // answer has to hold for all of them. Same rule §5 settled for `allocates`.
            K::DynCall { interface_name, method, base, arguments, .. } => {
                self.tc.dyn_call_allocates(interface_name, method)
                    || (self.tc.dyn_call_relays(interface_name, method, 0)
                        && self.allocates(base))
                    || arguments.iter().enumerate().any(|(i, a)| {
                        self.tc.dyn_call_relays(interface_name, method, i + 1)
                            && self.allocates(a)
                    })
            }
            K::Try { value, .. } => self.allocates(value),
            K::StructLit { fields, .. } => fields.iter().any(|f| self.allocates(f)),
            K::VariantLit { arguments, .. } => arguments.iter().any(|a| self.allocates(a)),
            K::ArrayLit(items) => items.iter().any(|i| self.allocates(i)),
            K::Field { base, .. } => self.allocates(base),
            K::Index { base, index, .. } | K::SliceIndex { base, index } => {
                self.allocates(base) || self.allocates(index)
            }
            // Reached only when the TYPE could hold storage, which for these means a
            // generic or a `dynamic` slipped through. Answer yes: §10 says a wrong
            // guess costs memory.
            K::Neg(_)
            | K::Not(_)
            | K::Truncate { .. }
            | K::WriteFile { .. }
            | K::WriteBytes { .. }
            | K::IntDiv { .. }
            | K::Bit { .. }
            | K::CIsNull(_)
            | K::ByteAt { .. }
            | K::Hash(_)
            | K::StrLen(_)
            | K::SliceLen(_)
            | K::Logical { .. }
            | K::Compare { .. } => true,
        }
    }

    /// Walk an expression for the ways it can hand storage to somebody who outlives
    /// this block, and class what it builds.
    fn scan(&mut self, e: &TypedExpr) {
        use TypedExprKind as K;
        if self.allocates(e) {
            self.mark_allocates();
        }
        match &e.kind {
            // `push` grows the buffer behind `place`. Whoever owns that buffer keeps
            // the growth, so the cursor cannot go back past it until they are done.
            K::Push { place, value } => {
                let owner = self.place_owner(place);
                self.taint(owner);
                if let Some(root) = Self::root_name(place) {
                    self.mark_allocated(&root);
                }
                self.mark_allocates();
                self.scan(place);
                self.scan(value);
            }
            K::Call { name, arguments } => {
                if self.tc.extern_names.contains(name) {
                    // Across the C boundary nothing can be proven: the callee may keep
                    // the pointer we handed it for as long as it likes.
                    self.taint(None);
                } else {
                    match self.fn_writable(name).cloned() {
                        Some(writable) => {
                            for (i, a) in arguments.iter().enumerate() {
                                if writable.get(i).copied().unwrap_or(true) {
                                    let owner = self.place_owner(a);
                                    self.taint(owner);
                                    if let Some(root) = Self::root_name(a) {
                                        self.mark_allocated(&root);
                                    }
                                }
                            }
                        }
                        // A callee whose signature this pass cannot find. Assume it
                        // writes into everything it was given.
                        None => self.taint(None),
                    }
                }
                for a in arguments {
                    self.scan(a);
                }
            }
            K::MethodCall { receiver, method, receiver_mut, base, arguments } => {
                if *receiver_mut {
                    let owner = self.place_owner(base);
                    self.taint(owner);
                    if let Some(root) = Self::root_name(base) {
                        self.mark_allocated(&root);
                    }
                }
                match self.tc.method_writable.get(&(receiver.clone(), method.clone())).cloned() {
                    Some(writable) => {
                        for (i, a) in arguments.iter().enumerate() {
                            if writable.get(i).copied().unwrap_or(true) {
                                let owner = self.place_owner(a);
                                self.taint(owner);
                                if let Some(root) = Self::root_name(a) {
                                    self.mark_allocated(&root);
                                }
                            }
                        }
                    }
                    None => self.taint(None),
                }
                self.scan(base);
                for a in arguments {
                    self.scan(a);
                }
            }
            K::DynCall { interface_name, method, base, arguments, .. } => {
                match self
                    .tc
                    .interfaces
                    .get(interface_name)
                    .and_then(|sigs| sigs.iter().find(|s| &s.name == method))
                    .cloned()
                {
                    Some(sig) => {
                        if sig.receiver_mut {
                            let owner = self.place_owner(base);
                            self.taint(owner);
                            if let Some(root) = Self::root_name(base) {
                                self.mark_allocated(&root);
                            }
                        }
                        for (i, a) in arguments.iter().enumerate() {
                            if sig.parameters.get(i).map(|p| p.writable).unwrap_or(true) {
                                let owner = self.place_owner(a);
                                self.taint(owner);
                                if let Some(root) = Self::root_name(a) {
                                    self.mark_allocated(&root);
                                }
                            }
                        }
                    }
                    None => self.taint(None),
                }
                self.scan(base);
                for a in arguments {
                    self.scan(a);
                }
            }
            // `?` leaves the function from the middle of a block, carrying an error
            // value that was built by the callee — which means built HERE, above this
            // block's mark. Releasing on the way out would hand back freed bytes.
            K::Try { value, .. } => {
                self.taint(None);
                self.scan(value);
            }
            _ => {
                let mut children = Vec::new();
                Self::children(e, &mut children);
                for c in children {
                    self.scan(c);
                }
            }
        }
    }

    fn mark_allocated(&mut self, name: &str) {
        if let Some(slot) = self.names.get_mut(name) {
            slot.allocated = true;
        }
    }

    /// A function's `mutable` flags, with the same fallback the call-site check uses:
    /// an instantiation `f$Int` inherits the generic's, because `mutable` is written on
    /// the declaration and substitution only replaces types.
    fn fn_writable(&self, name: &str) -> Option<&Vec<bool>> {
        self.tc.fn_writable.get(name).or_else(|| {
            name.split_once('$').and_then(|(generic, _)| self.tc.fn_writable.get(generic))
        })
    }

    /// The binding a place expression ultimately reaches through.
    fn root_name(e: &TypedExpr) -> Option<String> {
        match &e.kind {
            TypedExprKind::Var(n) => Some(n.clone()),
            TypedExprKind::DynCoerce { var, .. } => Some(var.clone()),
            TypedExprKind::Field { base, .. }
            | TypedExprKind::Index { base, .. }
            | TypedExprKind::SliceIndex { base, .. } => Self::root_name(base),
            _ => None,
        }
    }

    fn children<'e>(e: &'e TypedExpr, out: &mut Vec<&'e TypedExpr>) {
        use TypedExprKind as K;
        match &e.kind {
            K::Hold { value, .. } => out.push(value),
            K::Held { handle, .. } => out.push(handle),
            K::IntLit(_)
            | K::DecimalLit { .. }
            | K::BoolLit(_)
            | K::StrLit(_)
            | K::Var(_)
            | K::DynCoerce { .. }
            | K::ArgCount
            | K::Old(_) => {}
            K::Neg(i)
            | K::Not(i)
            | K::Arg(i)
            | K::ReadFile(i)
            | K::CIsNull(i)
            | K::CStringAt(i)
            | K::ByteAsString(i)
            | K::ToString(i)
            | K::Hash(i)
            | K::StrLen(i)
            | K::SliceLen(i)
            | K::Field { base: i, .. }
            | K::Try { value: i, .. } => out.push(i),
            K::Truncate { place: a, length: b }
            | K::WriteFile { path: a, contents: b }
            | K::WriteBytes { path: a, buffer: b }
            | K::CBytesAt { pointer: a, count: b }
            | K::CBytesTo { pointer: a, bytes: b }
            | K::ByteAt { s: a, index: b }
            | K::IntDiv { lhs: a, rhs: b, .. }
            | K::Logical { lhs: a, rhs: b, .. }
            | K::Binary { lhs: a, rhs: b, .. }
            | K::Compare { lhs: a, rhs: b, .. }
            | K::Push { place: a, value: b }
            | K::SliceIndex { base: a, index: b }
            | K::Index { base: a, index: b, .. } => {
                out.push(a);
                out.push(b);
            }
            K::Substring { source, at, len } => {
                out.push(source);
                out.push(at);
                out.push(len);
            }
            K::Bit { lhs, rhs, .. } => {
                out.push(lhs);
                if let Some(r) = rhs {
                    out.push(r);
                }
            }
            K::Call { arguments, .. }
            | K::VariantLit { arguments, .. }
            | K::StructLit { fields: arguments, .. }
            | K::ArrayLit(arguments)
            | K::SliceLit(arguments) => out.extend(arguments.iter()),
            K::MethodCall { base, arguments, .. } | K::DynCall { base, arguments, .. } => {
                out.push(base);
                out.extend(arguments.iter());
            }
        }
    }

    /// A store into `name`, reached however. If what is stored was built here, then
    /// whoever owns `name`'s storage keeps it, and no block inside them may release.
    fn store(&mut self, name: &str, value: &TypedExpr) {
        if self.allocates(value) {
            let owner = self.owner_of(name);
            self.taint(owner);
            self.mark_allocated(name);
        }
    }

    fn walk(&mut self, stmts: Vec<TypedStmt>) -> Vec<TypedStmt> {
        stmts.into_iter().map(|s| self.stmt(s)).collect()
    }

    /// A block, with the bindings the enclosing construct opens it with (a loop
    /// variable, a `match` arm's payload). Those bindings take their storage from the
    /// expression the construct evaluated BEFORE the block began, so their owner is the
    /// enclosing block — not this one.
    fn block(
        &mut self,
        stmts: Vec<TypedStmt>,
        may_release: bool,
        binds: &[(String, Option<usize>, bool)],
    ) -> Vec<TypedStmt> {
        self.frames.push(Frame::default());
        for (name, owner, allocated) in binds {
            self.declare(name, *owner, *allocated);
        }
        let body = self.walk(stmts);
        let frame = self.frames.pop().expect("a frame was pushed");
        for n in &frame.declared {
            self.names.remove(n);
        }
        // `allocates` needs no propagating: `mark_allocates` sets every open frame at
        // once, so an ancestor already knows about anything its children built.
        if may_release && frame.allocates && !frame.leaks {
            // A `Release` is the one statement in the typed tree the programmer did not
            // write, so it is also the one with no span of its own. It takes the span of
            // the FIRST statement it wraps, because that is where its code goes: the node
            // lowers to save-cursor, the body, restore-cursor, and the save sits at the
            // top. The restore inherits the last statement's location naturally, which is
            // where it belongs. An empty body is not reachable — a frame that allocates
            // has at least one statement — but the fallback is honest rather than zero.
            let span = body.first().map(|s| s.span).unwrap_or_else(|| Span::new(0, 0));
            vec![TypedStmt::new(TypedStmtKind::Release { body }, span)]
        } else {
            body
        }
    }

    /// Rebuilds the statement, keeping the position it was written at.
    ///
    /// This function is the reason `TypedStmt` carries its span as a field: the tree that
    /// reaches codegen is the one this pass returns, not the one the checker built, so a
    /// position recovered by walking the ORIGINAL `ast::Stmt` tree in parallel would be
    /// wrong wherever a `Release` was inserted — and wrong silently.
    fn stmt(&mut self, s: TypedStmt) -> TypedStmt {
        let here = self.depth();
        let span = s.span;
        let kind = match s.kind {
            TypedStmtKind::Let { name, ty, value } => {
                self.scan(&value);
                let owner = self.place_owner(&value);
                let allocated = self.allocates(&value);
                self.declare(&name, owner, allocated);
                TypedStmtKind::Let { name, ty, value }
            }
            TypedStmtKind::Assign { name, value } => {
                self.scan(&value);
                // The name may now alias something older than it is.
                let owner = Self::shorter(self.owner_of(&name), self.place_owner(&value));
                if let Some(slot) = self.names.get_mut(&name) {
                    slot.owner = owner;
                }
                self.store(&name, &value);
                TypedStmtKind::Assign { name, value }
            }
            TypedStmtKind::AssignField { name, indices, value } => {
                self.scan(&value);
                self.store(&name, &value);
                TypedStmtKind::AssignField { name, indices, value }
            }
            TypedStmtKind::AssignFieldIndex { name, indices, len, index, value } => {
                self.scan(&index);
                self.scan(&value);
                self.store(&name, &value);
                TypedStmtKind::AssignFieldIndex { name, indices, len, index, value }
            }
            TypedStmtKind::AssignIndex { name, len, index, value } => {
                self.scan(&index);
                self.scan(&value);
                self.store(&name, &value);
                TypedStmtKind::AssignIndex { name, len, index, value }
            }
            TypedStmtKind::ExprStmt(e) => {
                self.scan(&e);
                TypedStmtKind::ExprStmt(e)
            }
            TypedStmtKind::Exit(e) => {
                self.scan(&e);
                TypedStmtKind::Exit(e)
            }
            TypedStmtKind::Print { value, to_stderr } => {
                self.scan(&value);
                TypedStmtKind::Print { value, to_stderr }
            }
            TypedStmtKind::PrintInterp { parts, to_stderr } => {
                for p in &parts {
                    if let TypedInterpPart::Expr(e) = p {
                        self.scan(e);
                    }
                }
                TypedStmtKind::PrintInterp { parts, to_stderr }
            }
            // Whatever is handed back outlives every block in this function.
            TypedStmtKind::Return(e) => {
                self.scan(&e);
                if self.allocates(&e) {
                    self.taint(None);
                }
                TypedStmtKind::Return(e)
            }
            // `musttail` requires the call to sit immediately before the `ret`, with
            // nothing in between — and a release is something in between. So no block
            // containing one may release.
            TypedStmtKind::TailReturn { name, arguments } => {
                for a in &arguments {
                    self.scan(a);
                }
                self.taint(None);
                TypedStmtKind::TailReturn { name, arguments }
            }
            TypedStmtKind::Break => TypedStmtKind::Break,
            TypedStmtKind::Continue => TypedStmtKind::Continue,
            TypedStmtKind::While { cond, body } => {
                self.scan(&cond);
                let body = self.block(body, true, &[]);
                TypedStmtKind::While { cond, body }
            }
            TypedStmtKind::If { cond, then_block, else_block } => {
                self.scan(&cond);
                let then_block = self.block(then_block, true, &[]);
                let else_block = else_block.map(|b| self.block(b, true, &[]));
                TypedStmtKind::If { cond, then_block, else_block }
            }
            TypedStmtKind::For { name, elem, iterable, body } => {
                self.scan(&iterable);
                let owner = self.place_owner(&iterable);
                let allocated = self.allocates(&iterable);
                let body = self.block(body, true, &[(name.clone(), owner, allocated)]);
                TypedStmtKind::For { name, elem, iterable, body }
            }
            TypedStmtKind::ForRange { name, start, end, body } => {
                self.scan(&start);
                self.scan(&end);
                let body = self.block(body, true, &[(name.clone(), Some(here), false)]);
                TypedStmtKind::ForRange { name, start, end, body }
            }
            TypedStmtKind::Match { value, arms } => {
                self.scan(&value);
                let owner = self.place_owner(&value);
                let allocated = self.allocates(&value);
                let arms = arms
                    .into_iter()
                    .map(|a| {
                        let binds: Vec<(String, Option<usize>, bool)> = a
                            .bindings
                            .iter()
                            .map(|(n, _)| (n.clone(), owner, allocated))
                            .collect();
                        TypedArm {
                            tag: a.tag,
                            bindings: a.bindings,
                            body: self.block(a.body, true, &binds),
                        }
                    })
                    .collect();
                TypedStmtKind::Match { value, arms }
            }
            // A `region` the programmer wrote already releases at this exact point, so
            // there is nothing for a second wrapper to do — but its body is still a
            // block, and names declared in it still belong to it.
            TypedStmtKind::Region { name, body } => {
                let body = self.block(body, false, &[]);
                TypedStmtKind::Region { name, body }
            }
            // This pass runs once per body.
            TypedStmtKind::Release { body } => TypedStmtKind::Release { body },
        };
        TypedStmt { kind, span }
    }
}

impl TypeChecker {
    /// Place a `Release` on every block that provably keeps nothing. `params` are the
    /// names that arrived from outside — their storage is the caller's, so nothing this
    /// body does to them may be released here.
    ///
    /// `may_release` is false for the top level, which has nothing after it to release
    /// into, and true for a function or method body.
    fn place_releases(
        &self,
        params: &[String],
        body: Vec<TypedStmt>,
        may_release: bool,
    ) -> Vec<TypedStmt> {
        let mut pass = ReleasePass { tc: self, frames: Vec::new(), names: HashMap::new() };
        let binds: Vec<(String, Option<usize>, bool)> =
            params.iter().map(|p| (p.clone(), None, false)).collect();
        pass.block(body, may_release, &binds)
    }
}
