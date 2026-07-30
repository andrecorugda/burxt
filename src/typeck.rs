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
    /// `old(expr)` in an `ensures` clause: the value that expression had on
    /// ENTRY, by index into the function's hoisted list.
    Old(usize),
    /// `read_file(path)`: the file's bytes as a region-allocated String.
    ReadFile(Box<TypedExpr>),
    /// `to_string(v)`: the value's exact display form, region-allocated.
    ToString(Box<TypedExpr>),
    /// `byte_at(s, i)`: the i-th byte as an Int, bounds-checked at runtime.
    ByteAt { s: Box<TypedExpr>, index: Box<TypedExpr> },
    /// `hash(x)`: a deterministic, unseeded hash of an Equatable value.
    ///
    /// Unseeded on purpose. The same input hashes the same in every run on every machine, which
    /// is what lets a map iterate in a defined order and a program that contains one stay
    /// reproducible. The trade — no HashDoS protection — and the trigger that would change it are
    /// in spec/M11-MAPS.md Decision 4.
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
    /// succeeds, and what the caller's failure variant is. See spec/M8-ERRORS.md §1a.
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
pub enum TypedStmt {
    Let { name: String, ty: Type, value: TypedExpr },
    Assign { name: String, value: TypedExpr },
    /// Field assignment, path resolved to positional indices.
    AssignField { name: String, indices: Vec<u32>, value: TypedExpr },
    /// A call kept for its side effect; the result is evaluated and discarded.
    ExprStmt(TypedExpr),
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
    Print(TypedExpr),
    /// `region name { .. }`: open a region, run the body, release as a unit.
    Region { name: String, body: Vec<TypedStmt> },
    /// `for name in iterable { body }`. The element type and whether the array is fixed
    /// or growable are settled by the checker, so codegen only has to walk it.
    For { name: String, elem: Type, iterable: TypedExpr, body: Vec<TypedStmt> },
    /// `match` on an enum: arms in TAG order, each with the names bound to its
    /// payload slots. Exhaustiveness was proven by the typechecker.
    Match { value: TypedExpr, arms: Vec<TypedArm> },
    /// `print` of an interpolated string: emit each piece in order.
    PrintInterp(Vec<TypedInterpPart>),
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
    /// function name -> (parameter types, return type); collected up front so
    /// functions may be defined in any order and call each other.
    fns: HashMap<String, (Vec<Type>, Type)>,
    /// The type parameters of every generic function, by name. Empty for all the
    /// others, so the common path is one `is_empty` away. See spec/M7-GENERICS.md.
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
    /// Instantiations of generic enums, made on demand: mangled name -> variants, and
    /// mangled name -> what it was an instantiation OF, so a value's type can be read
    /// back into `(Option, [Int])` when a variant has no payload to infer from.
    made_enums: RefCell<HashMap<String, Vec<(String, Vec<Type>)>>>,
    made_order: RefCell<Vec<TypedEnum>>,
    instance_of: RefCell<HashMap<String, (String, Vec<Type>)>>,
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
    /// How many loops enclose the statement being checked. `break` and `continue`
    /// outside a loop have nothing to act on, and saying so beats generating a jump
    /// to nowhere.
    loop_depth: u32,
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
    /// Who is being probed: `(receiver, name)`, receiver empty for a free function.
    probe_owner: RefCell<(String, String)>,
    /// What the probe found. `RefCell` because `has_region` is a query — it answers a
    /// question about the checker and must not need `&mut` to do it.
    probe_fns: RefCell<HashSet<String>>,
    probe_methods: RefCell<HashSet<(String, String)>>,
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
    )
}

impl TypeChecker {
    pub fn new() -> Self {
        TypeChecker {
            env: HashMap::new(),
            fns: HashMap::new(),
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
            fn_effects: HashMap::new(),
            method_effects: HashMap::new(),
            allowed_effects: Vec::new(),
            effects_owner: String::new(),
            made_enums: RefCell::new(HashMap::new()),
            made_order: RefCell::new(Vec::new()),
            instance_of: RefCell::new(HashMap::new()),
            wanted: RefCell::new(Vec::new()),
            seen_instantiations: RefCell::new(HashSet::new()),
            alloc_fns: HashSet::new(),
            alloc_methods: HashSet::new(),
            pure_fns: HashSet::new(),
            in_pure: None,
            in_contract: false,
            loop_depth: 0,
            in_ensures: false,
            olds: RefCell::new(Vec::new()),
            in_caller_region: false,
            extern_names: HashSet::new(),
            extern_parameters: HashMap::new(),
            structs: HashMap::new(),
            enums: HashMap::new(),
            methods: HashMap::new(),
            interfaces: HashMap::new(),
            impls: HashSet::new(),
            dyn_interfaces: HashSet::new(),
            current_span: Cell::new(Span::default()),
            error_located: Cell::new(false),
            expr_types: RefCell::new(Vec::new()),
            errors: Vec::new(),
            current_ret: None,
            current_signature: None,
            current_region: None,
            region_locals: HashSet::new(),
            private_fields: HashMap::new(),
            private_methods: HashSet::new(),
            current_receiver: None,
            probing: false,
            probe_owner: RefCell::new((String::new(), String::new())),
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
    pub fn check(&mut self, prog: &Program) -> Result<TypedProgram, Vec<Diagnostic>> {
        // M14: work out which functions allocate before checking anything, so `allocates`
        // need not be written. See `probing` on the struct for why this needs a fixpoint
        // and not a pass.
        let (fns, methods) = Self::infer_allocates(prog);
        self.alloc_fns.extend(fns);
        self.alloc_methods.extend(methods);

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
            out.push(TypedContract { cond, text: clause.text.clone() });
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
                    name, what
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
    fn infer_allocates(prog: &Program) -> (HashSet<String>, HashSet<(String, String)>) {
        let mut fns: HashSet<String> = HashSet::new();
        let mut methods: HashSet<(String, String)> = HashSet::new();
        // One round per link in the longest call chain. The bound is the number of
        // functions, which no chain can exceed without repeating a name, and it is a
        // backstop rather than an expectation — real programs settle in two or three.
        let ceiling = prog.fns.len() + prog.methods.len() + 1;
        for _ in 0..ceiling {
            let mut probe = TypeChecker::new();
            probe.probing = true;
            probe.alloc_fns = fns.clone();
            probe.alloc_methods = methods.clone();
            let _ = probe.check_program_inner(prog);
            let found_fns = probe.probe_fns.borrow().clone();
            let found_methods = probe.probe_methods.borrow().clone();
            let grew = !found_fns.is_subset(&fns) || !found_methods.is_subset(&methods);
            fns.extend(found_fns);
            methods.extend(found_methods);
            if !grew {
                break;
            }
        }
        (fns, methods)
    }

    /// Does this function build its answer in the caller's region?
    ///
    /// One question, one answer, whether the programmer wrote `allocates` or the probe
    /// worked it out. Everything below asks through here rather than reading the AST flag,
    /// so there is no way for the two to disagree.
    fn allocates_fn(&self, name: &str) -> bool {
        self.alloc_fns.contains(name)
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
    /// nothing is guessed. That is the stated cost of spec/M10-ERGONOMICS.md §1 — half
    /// of an advantage Burxt used to have for free — and it is a real argument for
    /// annotating bindings in a long function.
    /// Record that this `(generic, type arguments)` pair is needed, and answer the symbol
    /// it will have. Recording is idempotent: a generic called in fifty places is emitted
    /// once, and a generic called nowhere is emitted never — which is what lets a library
    /// declare generics at no cost. See spec/M7-GENERICS.md Decision 4.
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
                | StmtKind::For { body, .. } => self.expand_block(body)?,
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
    /// generic is instantiated. See spec/M7-GENERICS.md Decision 4.
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
        for (i, (declared, argument)) in payload.iter().zip(arguments).enumerate() {
            if !mentions_param(declared) {
                continue;
            }
            let actual = self.check_expr(argument, None)?.ty;
            let instances = self.instance_of.borrow().clone();
            unify(declared, &actual, &mut map, &instances).map_err(|why| {
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
                    return Err(format!(
                        "`{}.{}` does not say what `{}` is, and nothing here does. Write \
                         the type where the value lands — `let x: {}<...> = {};` — or \
                         pass it somewhere that names it.",
                        enum_name, variant, p.name, enum_name, call
                    ))
                }
            }
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
                Type::Int | Type::Decimal { .. } => Ok(()),
                _ => Err(format!(
                    "`{}` needs `{}: Ordered`, and {} has no order. Ordered is Int and \
                     Decimal — the types `<` works on.",
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
    /// generic enum's variant uses. See spec/M7-GENERICS.md.
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
            unify(declared, &typed.ty, &mut map, &instances)
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
    pub fn expr_types(&self) -> Vec<(Span, Type)> {
        self.expr_types.borrow().clone()
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
            self.expand_fn_types(
                &mut concrete.parameters,
                &mut concrete.ret,
                &mut concrete.body,
            )?;
            self.current_span.set(concrete.span);
            let key = (symbol.clone(), concrete.name.clone());
            if self.methods.contains_key(&key) {
                continue;      // made already, for an earlier use of the same type
            }
            let param_tys: Vec<Type> =
                concrete.parameters.iter().map(|p| p.ty.clone()).collect();
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

    fn check_program_inner(&mut self, prog: &Program) -> Result<TypedProgram, String> {
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
        for t in &prog.interfaces {
            self.current_span.set(t.span);
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
                    match t {
                        Type::Int | Type::Bool | Type::String | Type::Decimal { .. } => {}
                        // An enum payload is fine when its width is FINITE, which is the rule this
                        // used to approximate by refusing every enum payload. What actually makes a
                        // width unbounded is a type containing ITSELF by value; recursion through a
                        // slice is a pointer and always terminates. See `embeds_by_value`.
                        Type::Named(n)
                            if self.is_enum(n)
                                && self.embeds_by_value(t, &e.name, &mut Vec::new()) =>
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
            let seen = |t: &Type| match t {
                Type::CInt | Type::CDouble => Type::Int,
                other => other.clone(),
            };
            let param_tys: Vec<Type> = e.parameters.iter().map(|p| seen(&p.ty)).collect();
            self.fns.insert(e.name.clone(), (param_tys, seen(&e.ret)));
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
                self.validate_type(&p.ty)?;
            }
            self.validate_type(&f.ret)?;
            // Returning an array would need array-valued expressions to be
            // bindable (`let a: [Int; 3] = f();`), which is the whole-array
            // copy question deferred with collections. Parameters are fine.
            if matches!(f.ret, Type::Array { .. }) {
                return Err(format!(
                    "function `{}` cannot return an array yet — returning one needs \
                     whole-array binding, which arrives with collections. Return \
                     a class, or fill an array the caller owns.",
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
            if !self.probing && self.region_allocated(&f.ret) && !self.allocates_fn(&f.name) {
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
            // A method on a GENERIC record is held back: its receiver has no layout until a
            // use says what the arguments are. One copy is registered per instantiation, in
            // the drain loop below, so `Stack<Int>` and `Stack<String>` get their own.
            if !m.receiver_arguments.is_empty() {
                self.current_span.set(m.span);
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
            self.methods.insert(key, (m.receiver_mut, param_tys, m.ret.clone()));
        }

        // Impls: satisfaction must be EXACT — every interface method present, with
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
                Ok(t) => stmts.push(t),
                Err(message) => {
                    self.record(message);
                    self.recover_from(s);
                }
            }
        }

        // Pass 2b: one copy of each generic per `(generic, type arguments)` pair the
        // program actually reached. Checking an instantiation can discover more — a
        // generic calling a generic — so this drains to a fixpoint rather than iterating
        // a fixed list. See spec/M7-GENERICS.md Decision 4.
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
                // Substituting can make a generic application concrete — `Option<T>`
                // becomes `Option<Int>` — so the instantiation is expanded again here.
                self.expand_fn_types(
                    &mut concrete.parameters,
                    &mut concrete.ret,
                    &mut concrete.body,
                )?;
                self.current_span.set(concrete.span);
                // Registered under its mangled name so a recursive generic call inside
                // the body resolves, and so `allocates`/`pure` carry over.
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
        Ok(TypedProgram { structs, enums, externs, fns, methods, vtables, stmts })
    }

    /// An impl must satisfy its trait EXACTLY: every declared method present,
    /// same receiver form, same parameter types, same return type.
    fn check_impl(&self, im: &ImplBlock) -> Result<(), String> {
        let sigs = self.interfaces.get(&im.interface_name).ok_or_else(|| {
            format!(
                "unknown interface `{}` — declare it with `interface {} {{ ... }}`",
                im.interface_name, im.interface_name
            )
        })?;
        if !self.structs.contains_key(&im.type_name) {
            return Err(format!(
                "`implement {} for {}`: unknown type `{}` — declare it with \
                 `class {} {{ ... }}`",
                im.interface_name, im.type_name, im.type_name, im.type_name
            ));
        }
        if self.impls.contains(&(im.interface_name.clone(), im.type_name.clone())) {
            return Err(format!(
                "`{}` already implements `{}`",
                im.type_name, im.interface_name
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
                        im.type_name, im.interface_name, signature.name
                    )
                })?;
                if receiver_mut != signature.receiver_mut {
                    return Err(format!(
                        "in `class {} implements {}`, method `{}` declares `{}self` but the \
                         interface declares `{}self`.",
                        im.type_name,
                        im.interface_name,
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
                        im.interface_name,
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
                            im.type_name, im.interface_name, signature.name, i + 1, have, want.ty
                        ));
                    }
                }
                if ret != signature.ret {
                    return Err(format!(
                        "in `class {} implements {}`, method `{}` returns {} but the interface \
                         declares {}.",
                        im.type_name, im.interface_name, signature.name, ret, signature.ret
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
                    im.interface_name,
                    im.type_name,
                    m.name,
                    im.interface_name,
                    sigs.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", ")
                ));
            }
            if m.receiver != im.type_name {
                return Err(format!(
                    "in `implement {} for {}`, method `{}` has receiver `self: {}` — \
                     it must be `self: {}`.",
                    im.interface_name, im.type_name, m.name, m.receiver, im.type_name
                ));
            }
        }

        // ...and every interface method must be present, matching exactly.
        for signature in sigs {
            let found = im.methods.iter().find(|m| m.name == signature.name).ok_or_else(|| {
                format!(
                    "`implement {} for {}` is missing the method `{}`. Every interface \
                     method must be implemented — Burxt has no default bodies.",
                    im.interface_name, im.type_name, signature.name
                )
            })?;
            if found.receiver_mut != signature.receiver_mut {
                return Err(format!(
                    "in `implement {} for {}`, method `{}` declares `{}self` but the \
                     interface declares `{}self`.",
                    im.interface_name,
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
                    im.interface_name,
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
                        im.interface_name,
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
                    im.interface_name, im.type_name, signature.name, found.ret, signature.ret
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
                interface_name
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
                concrete, interface_name, interface_name, concrete
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
                            "cannot modify `{}`: it was declared immutable. Declare \
                             it `let mutable {}: {}` to allow it.",
                            name, name, ty
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

    /// Does evaluating this expression produce region-allocated storage? Needed
    /// because a concatenated String lives in a region while a literal lives in
    /// .rodata, and the two share one type — so the type alone cannot say.
    fn expr_allocates(&self, e: &TypedExpr) -> bool {
        match &e.kind {
            TypedExprKind::SliceLit(_)
            | TypedExprKind::Push { .. }
            | TypedExprKind::ReadFile(_)
            | TypedExprKind::Substring { .. } => true,
            // A call to an `allocates` function or method built its result in OUR
            // region, so it is region storage here and the same escape rules apply.
            TypedExprKind::Call { name, .. } => self.alloc_fns.contains(name),
            TypedExprKind::MethodCall { receiver, method, .. } => {
                self.alloc_methods.contains(&(receiver.clone(), method.clone()))
            }
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
            TypedExprKind::Field { base, .. } => self.expr_allocates(base),
            TypedExprKind::Index { base, index, .. } => {
                self.expr_allocates(base) || self.expr_allocates(index)
            }
            TypedExprKind::SliceIndex { base, index } => {
                self.expr_allocates(base) || self.expr_allocates(index)
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

    /// A Named type must refer to a declared struct; CInt never leaves the
    /// C boundary. `dyn Trait` must name a declared trait — and using one
    /// classes that the interface needs vtables.
    fn validate_type(&mut self, ty: &Type) -> Result<(), String> {
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
                Err(format!(
                    "unknown type `{}` — declare it with `class {} {{ ... }}` or \
                     `enum {} {{ ... }}`",
                    name, name, name
                ))
            }
            Type::CInt => Err(
                "CInt only exists at the C boundary (external function signatures) — \
                 use Int in Burxt code; values convert at the call."
                    .to_string(),
            ),
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
                 (containment cycle: {} -> {})",
                trail[0],
                trail[0],
                trail.join(" -> "),
                name
            ));
        }
        trail.push(name.to_string());
        if let Some(fields) = self.fields_of(name) {
            for (_, ty) in fields {
                if let Type::Named(inner) = ty {
                    self.check_struct_finite(&inner, trail)?;
                }
            }
        }
        trail.pop();
        Ok(())
    }

    /// Render a struct's fields as `name: Type, ...` for error messages.
    fn field_list(&self, name: &str) -> String {
        self.structs
            .get(name)
            .map(|fs| {
                fs.iter()
                    .map(|(n, t)| format!("{}: {}", n, t))
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
        if e.name == "len" || e.name == "byte_at" || e.name == "push" || e.name == "read_file" || e.name == "to_string" || e.name == "old" || e.name == "substring" || e.name == "truncate" || e.name == "write_file" || e.name == "argument" || e.name == "argument_count" || e.name == "divide_floor" || e.name == "divide_toward_zero" || e.name == "remainder" || e.name == "write_bytes" || e.name == "hash" {
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
            let seen = |t: &Type| match t {
                Type::CInt | Type::CDouble => Type::Int,
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
                (Type::Int | Type::String | Type::CInt | Type::CDouble, None) => {}
                (other, None) => {
                    return Err(format!(
                        "in external function `{}`, parameter `{}` has type {}, but only \
                         Int, CInt, CDouble, String and a marshalled Decimal may \
                         cross the C boundary for now — C has no {}, and the raw \
                         value would silently lose its meaning.",
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
        if !matches!(e.ret, Type::Int | Type::CInt) {
            return Err(format!(
                "external function `{}` returns {}, but only Int or CInt may cross the C \
                 boundary as a return for now — Burxt cannot yet track who owns \
                 memory a C function returns. (If the C function returns a 32-bit \
                 `int`, declare `-> CInt` so the sign survives.)",
                e.name, e.ret
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
        self.region_locals.clear();
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
            if self.env.insert(p.name.clone(), (p.ty.clone(), false)).is_some() {
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
                        f.name, clause.text
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
                Some(TypedContract { cond: measure, text: clause.text.clone() })
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
        Ok(TypedFn { name: f.name.clone(), parameters, ret: f.ret.clone(), body, requires, ensures, decreases, olds })
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
        self.region_locals.clear();
        self.env.insert(
            "self".to_string(),
            (Type::Named(m.receiver.clone()), m.receiver_mut),
        );
        let mut parameters = Vec::new();
        for p in &m.parameters {
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

        let body = self.check_block(&m.body)?;
        self.current_ret = None;
        self.in_caller_region = false;
        self.env.clear();
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
    fn check_tail_return(&mut self, e: &Expr) -> Result<TypedStmt, String> {
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
            if t.ty != *want {
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
        Ok(TypedStmt::TailReturn { name, arguments: typed_args })
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
    ) -> Result<TypedStmt, String> {
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
            chain = vec![TypedStmt::If { cond, then_block, else_block: Some(chain) }];
        }
        // A chain of one is the wildcard alone, which is a block and not an `if`.
        Ok(match chain.len() {
            1 => chain.into_iter().next().unwrap(),
            _ => TypedStmt::If {
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
    ) -> Result<TypedStmt, String> {
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
                for (name, ty) in arm.bindings.iter().zip(payload) {
                    if self.env.contains_key(name) {
                        self.env = saved;
                        return Err(format!(
                            "`{}` is already declared — a pattern binding may not \
                             shadow it, the same rule `let` follows.",
                            name
                        ));
                    }
                    self.env.insert(name.clone(), (ty.clone(), false));
                    bindings.push((name.clone(), ty.clone()));
                }
                let body = self.check_block(&arm.body);
                self.env = saved;
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
            Ok(TypedStmt::Match { value: scrutinee, arms: typed_arms })
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
        let mut out: Vec<TypedStmt> = Vec::new();
        let errors_before = self.errors.len();
        for s in stmts {
            if out.last().is_some_and(stmt_diverges) {
                self.current_span.set(s.span);
                let after = match out.last().map(|p| &*p) {
                    Some(TypedStmt::Break) => "`break`",
                    Some(TypedStmt::Continue) => "`continue`",
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
                Ok(t) => out.push(t),
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
        let _ = errors_before;
        Ok(out)
    }

    fn check_stmt(&mut self, s: &Stmt) -> Result<TypedStmt, String> {
        // Remember where we are. Errors below are returned as plain messages and
        // the position is attached once, at the boundary — so a nested statement
        // naturally reports the innermost (most precise) position, and no error
        // site has to thread a span through.
        self.current_span.set(s.span);
        // A fresh statement, so the next error is free to claim its own position.
        self.error_located.set(false);
        match &s.kind {
            StmtKind::Let { name, mutable, declared, value } => {
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
                // typing, not checking. See spec/M10-ERGONOMICS.md §1 Decision 3.
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
                                name, declared, typed.ty
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
                self.env.insert(name.clone(), (bound.clone(), *mutable));
                Ok(TypedStmt::Let { name: name.clone(), ty: bound, value: typed })
            }
            StmtKind::Assign { name, value } => {
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
                    return Err(format!(
                        "cannot assign to `{}`: it was declared immutable. \
                         Declare it `let mutable {}: {}` to allow reassignment.",
                        name, name, declared
                    ));
                }
                let typed = self.check_expr(value, Some(&declared))?;
                if typed.ty != declared {
                    return Err(format!(
                        "cannot assign {} {} to `{}`, which was declared {}",
                        typed.ty.article(), typed.ty, name, declared
                    ));
                }
                // Assignment can put region storage into a binding that did not hold any:
                // `let mutable s: String = "x"; region r { s = "a" + "b"; }`. The `let`
                // saw a literal, so only this can know.
                if self.current_region.is_some() && self.expr_allocates(&typed) {
                    self.region_locals.insert(name.clone());
                }
                Ok(TypedStmt::Assign { name: name.clone(), value: typed })
            }
            StmtKind::AssignField { name, path, value } => {
                let lvalue = format!("{}.{}", name, path.join("."));
                let (mut cur_ty, mutable) = self
                    .env
                    .get(name)
                    .ok_or_else(|| format!("unknown variable: {}", name))?
                    .clone();
                if !mutable {
                    return Err(format!(
                        "cannot assign to `{}`: `{}` was declared immutable. \
                         Declare it `let mutable {}: {}` to allow it.",
                        lvalue, name, name, cur_ty
                    ));
                }
                let mut indices = Vec::new();
                for field in path {
                    let (index, field_ty) = self.resolve_field(&cur_ty, field)?;
                    indices.push(index);
                    cur_ty = field_ty;
                }
                let typed = self.check_expr(value, Some(&cur_ty))?;
                if typed.ty != cur_ty {
                    return Err(format!(
                        "cannot assign {} {} to `{}`, which was declared {}",
                        typed.ty.article(), typed.ty, lvalue, cur_ty
                    ));
                }
                Ok(TypedStmt::AssignField { name: name.clone(), indices, value: typed })
            }
            StmtKind::AssignFieldIndex { name, path, index, value } => {
                let lvalue = format!("{}.{}", name, path.join("."));
                let (mut cur_ty, mutable) = self
                    .env
                    .get(name)
                    .ok_or_else(|| self.unknown_name(name))?
                    .clone();
                if !mutable {
                    return Err(format!(
                        "cannot assign to `{}[...]`: `{}` was declared immutable. \
                         Declare it `let mutable {}: {}` to allow it.",
                        lvalue, name, name, cur_ty
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
                if typed.ty != elem {
                    return Err(format!(
                        "cannot assign {} {} to `{}[...]`, which holds {}",
                        typed.ty.article(),
                        typed.ty,
                        lvalue,
                        elem
                    ));
                }
                Ok(TypedStmt::AssignFieldIndex {
                    name: name.clone(),
                    indices,
                    len,
                    index,
                    value: typed,
                })
            }
            StmtKind::AssignIndex { name, index, value } => {
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
                        "cannot assign to `{}[...]`: `{}` was declared immutable. \
                         Declare it `let mutable {}: {}` to allow it.",
                        name, name, name, declared
                    ));
                }
                let index = self.check_index(&format!("{}", declared), len, index)?;
                let typed = self.check_expr(value, Some(&elem))?;
                if typed.ty != elem {
                    return Err(format!(
                        "cannot assign {} {} to `{}[...]`, which holds {}",
                        typed.ty.article(), typed.ty, name, elem
                    ));
                }
                Ok(TypedStmt::AssignIndex { name: name.clone(), len, index, value: typed })
            }
            StmtKind::ExprStmt(e) => {
                let typed = self.check_expr(e, None)?;
                Ok(TypedStmt::ExprStmt(typed))
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
                let checked = self.check_block(body);
                self.current_region = None;
                Ok(TypedStmt::Region { name: name.clone(), body: checked? })
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
                Ok(if word == "break" { TypedStmt::Break } else { TypedStmt::Continue })
            }
            StmtKind::For { name, iterable, body } => {
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
                self.loop_depth += 1;
                let body = self.check_block(body);
                self.loop_depth -= 1;
                self.env = saved;
                Ok(TypedStmt::For {
                    name: name.clone(),
                    elem,
                    iterable,
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
                Ok(TypedStmt::While { cond, body: body? })
            }
            StmtKind::Print(e) => {
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
                    return Ok(TypedStmt::PrintInterp(typed_parts));
                }
                let typed = self.check_expr(e, None)?;
                match &typed.ty {
                    Type::Param(p) => return Err(unbounded(p, "printed")),
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
                            t
                        ))
                    }
                    _ => {}
                }
                Ok(TypedStmt::Print(typed))
            }
            StmtKind::Return(e) => {
                let ret = self.current_ret.clone().ok_or_else(|| {
                    "`return` only makes sense inside a function".to_string()
                })?;
                let typed = self.check_expr(e, Some(&ret))?;
                if typed.ty != ret {
                    self.blame(e.span);
                    return Err(format!(
                        "this function returns {}, but the `return` expression has type {}",
                        ret, typed.ty
                    ));
                }
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
                if self.expr_allocates(&typed) && self.current_region.is_some() {
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
                        return Ok(TypedStmt::Return(typed));
                    }
                    return Err(format!(
                        "cannot return this {}: it was built inside a `region` block, which \
                         releases at its closing brace, so its storage would not outlive the \
                         call. Move the allocation out of the `region` block, or return a \
                         scalar summary.",
                        typed.ty
                    ));
                }
                Ok(TypedStmt::Return(typed))
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
                Ok(TypedStmt::If { cond, then_block, else_block })
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
            // here, both from spec/M8-ERRORS.md §1a: the failure variant is recognised by
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
                    let value = self.check_expr(&arguments[1], Some(&elem))?;
                    if value.ty != elem {
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
                    // No region: the C runtime's argument strings outlive the program,
                    // so this borrows rather than copies.
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
                let (mut param_tys, mut ret) = self
                    .fns
                    .get(name)
                    .ok_or_else(|| format!("unknown function: {}", name))?
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
                        unify(declared, &actual, &mut map, &instances).map_err(|why| {
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
                            // A failure is not an error here. The expectation may legitimately be
                            // unrelated — a call whose result is discarded, or one inside a bigger
                            // expression — and the real complaint is the one below, which names the
                            // parameter that is still unknown.
                            let _ = unify(&ret, want, &mut map, &instances);
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
                let mut typed_args = Vec::new();
                for (i, (argument, param_ty)) in arguments.iter().zip(&param_tys).enumerate() {
                    let typed = self.check_expr(argument, Some(param_ty))?;
                    if &typed.ty != param_ty {
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
                        return Err(format!(
                            "in the call to `{}`, argument {} must be {}, \
                             but it has type {}",
                            written,
                            i + 1,
                            param_ty,
                            typed.ty
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
                            name,
                            given,
                            self.field_list(name)
                        ));
                    }
                    if fields.iter().filter(|(g, _)| g == given).count() > 1 {
                        return Err(format!(
                            "in `{} {{ ... }}`, the field `{}` is given twice",
                            name, given
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
                    if &typed.ty != fty {
                        return Err(format!(
                            "in `{} {{ ... }}`, the field `{}` must be {}, but its \
                             value has type {}",
                            name, fname, fty, typed.ty
                        ));
                    }
                    typed_fields.push(typed);
                }
                Ok(TypedExpr {
                    ty: literal_ty,
                    kind: TypedExprKind::StructLit { name: name.clone(), fields: typed_fields },
                })
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
                // Methods cannot carry the marker yet, so a pure function cannot
                // call one. Said plainly, with the reason, rather than by letting
                // some later check produce something confusing.
                if let Some(name) = &self.in_pure {
                    return Err(format!(
                        "`pure function {}` may not call the method `.{}()`: a method cannot \
                         be declared `pure` yet, so there is no promise to rely on. \
                         Move the calculation into a `pure function`, passing the fields it \
                         needs.",
                        name, method
                    ));
                }
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
                                interface_name,
                                method,
                                sigs.iter()
                                    .map(|s| s.name.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            )
                        })?;
                    let signature = sigs[slot].clone();
                    if signature.receiver_mut {
                        return Err(format!(
                            "`{}` takes `mutable self`, and calling a mutating method \
                             through an interface object is not available yet: the \
                             compiler still cannot tell whether the value behind \
                             the object was declared mutable. Regions bound its \
                             LIFETIME, not its mutability. Call it on the concrete \
                             type.",
                            method
                        ));
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
                        if typed.ty != p.ty {
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
                    // further up the stack. spec/M14-IMPLICIT-REGIONS.md §5.
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
                    let leaks = self.impls.iter().any(|(implemented, concrete)| {
                        *implemented == interface_name
                            && self.alloc_methods.contains(&(concrete.clone(), method.clone()))
                    });
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
                        if t.ty != want.ty {
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
                            receiver, method
                        )
                    })?;
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
                        return Err(format!(
                            "cannot call the mutating method `{}` on `{}`: it was \
                             declared immutable. Declare it `let mutable {}: {}` to \
                             allow it.",
                            method, name, name, receiver
                        ));
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
                        receiver,
                        method,
                        param_tys.len(),
                        arguments.len()
                    ));
                }
                let mut typed_args = Vec::new();
                for (i, (argument, param_ty)) in arguments.iter().zip(&param_tys).enumerate() {
                    let typed = self.check_expr(argument, Some(param_ty))?;
                    if &typed.ty != param_ty {
                        return Err(format!(
                            "in the call to `{}.{}`, argument {} must be {}, \
                             but it has type {}",
                            receiver,
                            method,
                            i + 1,
                            param_ty,
                            typed.ty
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
                        if t.ty != elem_ty {
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
                    // See spec/M10-ERGONOMICS.md §1 Decision 2.
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
                    if t.ty != elem_ty {
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
            if &t.ty != want {
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
                format!(
                    "`{}` has no field named `{}`. Its fields are: {}.",
                    name,
                    field,
                    self.field_list(name)
                )
            })
    }

    /// Comparisons are always exact, and both sides must have the SAME type —
    /// comparing money of different scales (or contracts) is refused just like
    /// adding it would be.
    fn check_compare(&self, op: CmpOp, lhs: &Type, rhs: &Type) -> Result<(), String> {
        use Type::*;
        match (lhs, rhs) {
            (Int, Int) => Ok(()),
            (Named(_), Named(_)) => Err(
                "record comparison is not available yet — compare fields individually."
                    .to_string(),
            ),
            // Strings compare by BYTES, and only for equality. This is the
            // same `==` every other type uses — not a parallel string-equals
            // path — so a cross-type comparison involving a String falls
            // through to the shared catch-all below and reads identically to
            // any other type mismatch.
            (String, String) => match op {
                CmpOp::Eq | CmpOp::Ne => Ok(()),
                _ => Err(
                    "Strings have no ordering yet — byte ordering arrives with \
                     collections. (For C's ordering, call strcmp through FFI.)"
                        .to_string(),
                ),
            },
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
            // happen to permit. See spec/M7-GENERICS.md Decision 2.
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
                    // rule guess at it. See spec/M10-ERGONOMICS.md §1 Decision 5.
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
    /// Whether a value of type `have` may be stored where `want` was declared.
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
            | StmtKind::Print(value)
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
        ExprKind::ArrayLit(items) => any(items),
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
    match s {
        TypedStmt::Break | TypedStmt::Continue => true,
        TypedStmt::If { then_block, else_block: Some(e), .. } => {
            block_diverges(then_block) && block_diverges(e)
        }
        TypedStmt::Match { arms, .. } => {
            !arms.is_empty() && arms.iter().all(|a| block_diverges(&a.body))
        }
        TypedStmt::Region { body, .. } => block_diverges(body),
        TypedStmt::For { .. } => false,   // a `for` over an empty array runs zero times
        other => stmt_returns(other),
    }
}

fn block_diverges(stmts: &[TypedStmt]) -> bool {
    stmts.last().is_some_and(stmt_diverges)
}

/// Does this statement return on every path through it?
fn stmt_returns(s: &TypedStmt) -> bool {
    match s {
        TypedStmt::Return(_) | TypedStmt::TailReturn { .. } => true,
        TypedStmt::If { then_block, else_block: Some(e), .. } => {
            block_returns(then_block) && block_returns(e)
        }
        // An exhaustive match is a return when every arm is. Exhaustiveness is
        // already proven, so the arms ARE all the paths — the same reasoning as
        // an if/else where both branches return. A `while` never counts, since
        // its condition may be false at entry.
        TypedStmt::Match { arms, .. } => {
            !arms.is_empty() && arms.iter().all(|a| block_returns(&a.body))
        }
        // A region is a lexical scope, not a branch: if its body returns on
        // every path, so does the region. Without this the prover asked for a
        // second `return` after the block and then called it unreachable —
        // there was no way to write a function that returns from inside a
        // region.
        TypedStmt::Region { body, .. } => block_returns(body),
        TypedStmt::For { body, .. } => block_returns(body),
        _ => false,
    }
}

/// A block returns on every path iff its last statement does (the typechecker
/// refuses statements after one that always returns, so "last" is enough).
fn block_returns(stmts: &[TypedStmt]) -> bool {
    stmts.last().is_some_and(stmt_returns)
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
// Monomorphisation, per spec/M7-GENERICS.md Decision 1: each `(generic, type arguments)`
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
        other => other.clone(),
    }
}

/// Does this type mention a parameter at all? The cheap test that keeps every
/// non-generic call on exactly the path it was on before generics existed.
pub fn mentions_param(ty: &Type) -> bool {
    match ty {
        Type::Param(_) => true,
        Type::Array { elem, .. } | Type::Slice(elem) => mentions_param(elem),
        Type::Generic { arguments, .. } => arguments.iter().any(mentions_param),
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
pub fn show(ty: &Type, instances: &HashMap<String, (String, Vec<Type>)>) -> String {
    match ty {
        Type::Named(n) => match instances.get(n) {
            Some((of, arguments)) => {
                let inner: Vec<String> = arguments.iter().map(|a| show(a, instances)).collect();
                format!("{}<{}>", of, inner.join(", "))
            }
            None => n.clone(),
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

fn unify(
    declared: &Type,
    actual: &Type,
    map: &mut HashMap<String, Type>,
    instances: &Instances,
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
            unify(d, a, map, instances)
        }
        (Type::Slice(d), Type::Slice(a)) => unify(d, a, map, instances),
        // `Option<T>` against `Option$String`: the instantiation remembers what it was
        // made from, so the arguments line up and `T` binds to String.
        (Type::Generic { name: dn, arguments: dargs }, Type::Named(m)) => {
            match instances.get(m) {
                Some((of, aargs)) if of == dn && aargs.len() == dargs.len() => {
                    for (d, a) in dargs.iter().zip(aargs) {
                        unify(d, a, map, instances)?;
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
            | StmtKind::For { body, .. } => substitute_in_block(body, map),
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
/// Per spec/M7-GENERICS.md Decision 2: a parameter with no bound can be stored, copied,
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
