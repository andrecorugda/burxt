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
    /// `arg_count()` and `arg(n)` — the command line. A compiler needs to know which
    /// file it was asked to compile.
    ArgCount,
    Arg(Box<TypedExpr>),
    /// `write_file(path, contents)` — how a backend emits anything.
    WriteFile { path: Box<TypedExpr>, contents: Box<TypedExpr> },
    /// `substring(s, at, len)` — a copy of part of a String, in the current region.
    Substring { source: Box<TypedExpr>, at: Box<TypedExpr>, len: Box<TypedExpr> },
    /// `div_floor`, `div_trunc` or `rem` on two Ints. Three names rather than one
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
    Call { name: String, args: Vec<TypedExpr> },
    /// Method call, resolved to its receiver type. `receiver_mut` decides how
    /// codegen passes `base`: a true reference (mutating) or a value copy.
    MethodCall {
        receiver: String,
        method: String,
        receiver_mut: bool,
        base: Box<TypedExpr>,
        args: Vec<TypedExpr>,
    },
    /// Build a trait object from a concrete binding: a fat pointer pairing the
    /// binding's storage with the static (Type, Trait) vtable.
    DynCoerce { trait_name: String, concrete: String, var: String },
    /// A dynamically dispatched call: load slot `slot` from the receiver's
    /// vtable and call it with the data pointer.
    DynCall {
        trait_name: String,
        method: String,
        slot: u32,
        base: Box<TypedExpr>,
        args: Vec<TypedExpr>,
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
    /// Enum construction: the variant's index plus its payload values.
    VariantLit { enum_name: String, tag: u32, args: Vec<TypedExpr> },
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
    TailReturn { name: String, args: Vec<TypedExpr> },
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
    pub params: Vec<(String, Type)>,
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
    pub params: Vec<(String, Type)>,
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
    pub params: Vec<Type>,
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
    pub trait_name: String,
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
    extern_params: HashMap<String, Vec<(Type, Option<Marshal>)>>,
    /// struct name -> fields (name, type) in declaration order; hoisted first.
    structs: HashMap<String, Vec<(String, Type)>>,
    /// enum name -> variants (name, payload types) in declaration order, which
    /// is what fixes each variant's tag.
    enums: HashMap<String, Vec<(String, Vec<Type>)>>,
    /// (receiver, method name) -> (is mutating, param types, return type)
    methods: HashMap<(String, String), (bool, Vec<Type>, Type)>,
    /// trait name -> its method signatures, in declaration order (slot order)
    traits: HashMap<String, Vec<TraitSig>>,
    /// which (trait, concrete type) pairs have an explicit impl
    impls: HashSet<(String, String)>,
    /// (trait, concrete) pairs that need a vtable because the trait is used
    /// as `dyn` somewhere — pay for what you use.
    dyn_traits: HashSet<String>,
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
    /// checker: it records the problem and moves to the next statement, so one
    /// mistake does not hide the other five.
    errors: Vec<Diagnostic>,
    current_ret: Option<Type>,
    /// The enclosing function's name and parameter types. A guaranteed tail
    /// call needs them: LLVM only guarantees the call when caller and callee
    /// prototypes match, so that has to be checked before promising it.
    current_sig: Option<(String, Vec<Type>)>,
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
}

impl TypeChecker {
    pub fn new() -> Self {
        TypeChecker {
            env: HashMap::new(),
            fns: HashMap::new(),
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
            extern_params: HashMap::new(),
            structs: HashMap::new(),
            enums: HashMap::new(),
            methods: HashMap::new(),
            traits: HashMap::new(),
            impls: HashSet::new(),
            dyn_traits: HashSet::new(),
            current_span: Cell::new(Span::default()),
            error_located: Cell::new(false),
            expr_types: RefCell::new(Vec::new()),
            errors: Vec::new(),
            current_ret: None,
            current_sig: None,
            current_region: None,
        }
    }

    /// Check a program, reporting WHERE any problem is.
    ///
    /// The position is attached here, once, from wherever the checker had reached
    /// — so every one of the ~160 error sites inside stays a plain sentence, and
    /// a nested statement naturally yields the most precise position because it
    /// was the last thing entered.
    pub fn check(&mut self, prog: &Program) -> Result<TypedProgram, Vec<Diagnostic>> {
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
                    "`pure fn {}` may not {}: a pure function's result must depend \
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
        self.current_region.is_some() || self.in_caller_region
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
    /// This is where Burxt gets an unusual advantage: **every `let` declares its
    /// type**, so even when the initializer is wrong the binding's type is known.
    /// Binding it anyway means the rest of the function checks against the type
    /// the author asked for, instead of drowning the real error in a cascade of
    /// "unknown name" noise. In a language with inference this is the hard part;
    /// here the annotation was mandatory all along.
    fn recover_from(&mut self, s: &Stmt) {
        if let StmtKind::Let { name, mutable, declared, .. } = &s.kind {
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

    fn check_program_inner(&mut self, prog: &Program) -> Result<TypedProgram, String> {
        // Pass 0: hoist struct declarations, then validate them (field types
        // must exist; no struct may contain itself, directly or transitively).
        for s in &prog.structs {
            self.current_span.set(s.span);
            if self.structs.contains_key(&s.name) {
                return Err(format!("struct `{}` is defined twice", s.name));
            }
            let mut fields = Vec::new();
            for f in &s.fields {
                if fields.iter().any(|(n, _)| n == &f.name) {
                    return Err(format!(
                        "struct `{}` declares the field `{}` twice",
                        s.name, f.name
                    ));
                }
                if let Some(m) = f.marshal {
                    return Err(format!(
                        "struct `{}`: field `{}` is marked `as {}`, but marshalling \
                         describes how a value crosses a FOREIGN boundary, not how \
                         it is stored. Drop the `as {}`.",
                        s.name, f.name, m, m
                    ));
                }
                fields.push((f.name.clone(), f.ty.clone()));
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
            self.enums.insert(
                e.name.clone(),
                e.variants
                    .iter()
                    .map(|v| (v.name.clone(), v.payload.clone()))
                    .collect(),
            );
        }
        // Traits: signature sets only, hoisted so impls may precede them.
        for t in &prog.traits {
            self.current_span.set(t.span);
            if self.traits.contains_key(&t.name) {
                return Err(format!("trait `{}` is defined twice", t.name));
            }
            if self.structs.contains_key(&t.name) {
                return Err(format!(
                    "`{}` is already a struct — a trait cannot reuse the name",
                    t.name
                ));
            }
            let mut seen: Vec<&str> = Vec::new();
            for sig in &t.methods {
                if seen.contains(&sig.name.as_str()) {
                    return Err(format!(
                        "trait `{}` declares the method `{}` twice",
                        t.name, sig.name
                    ));
                }
                seen.push(&sig.name);
            }
            self.traits.insert(t.name.clone(), t.methods.clone());
        }
        // Validate the types inside the signatures only once every trait name
        // is known, so traits may reference each other in any order.
        for t in &prog.traits {
            self.current_span.set(t.span);
            for sig in &t.methods {
                for p in &sig.params {
                    self.validate_type(&p.ty)?;
                }
                self.validate_type(&sig.ret)?;
            }
        }

        // Payloads are scalars only in this cut: an aggregate payload reopens
        // the recursive-size question (an enum containing itself is infinite),
        // which needs indirection and therefore M1.
        for e in &prog.enums {
            self.current_span.set(e.span);
            for v in &e.variants {
                for (i, t) in v.payload.iter().enumerate() {
                    match t {
                        Type::Int | Type::Bool | Type::String | Type::Decimal { .. } => {}
                        Type::Named(n) if self.enums.contains_key(n) => {
                            return Err(format!(
                                "`{}.{}` payload {} is the enum `{}` — an enum inside \
                                 an enum needs indirection to have a finite size, \
                                 which arrives with the memory model. Carry the \
                                 parts as scalars for now.",
                                e.name,
                                v.name,
                                i + 1,
                                n
                            ))
                        }
                        other => {
                            return Err(format!(
                                "`{}.{}` payload {} is {} {} — variant payloads must \
                                 be Int, Bool, String or Decimal for now.",
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
        let structs = prog
            .structs
            .iter()
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
            let param_tys: Vec<Type> = e.params.iter().map(|p| seen(&p.ty)).collect();
            self.fns.insert(e.name.clone(), (param_tys, seen(&e.ret)));
            self.extern_names.insert(e.name.clone());
            self.extern_params.insert(
                e.name.clone(),
                e.params.iter().map(|p| (p.ty.clone(), p.marshal)).collect(),
            );
            externs.push(TypedExtern {
                name: e.name.clone(),
                params: e.params.iter().map(|p| p.ty.clone()).collect(),
                ret: e.ret.clone(),
            });
        }
        for f in &prog.fns {
            self.current_span.set(f.span);
            if self.fns.contains_key(&f.name) {
                return Err(format!("function `{}` is defined twice", f.name));
            }
            if f.name == "len" || f.name == "byte_at" || f.name == "push" || f.name == "read_file" || f.name == "to_string" || f.name == "old" || f.name == "substring" || f.name == "truncate" || f.name == "write_file" || f.name == "arg" || f.name == "arg_count" || f.name == "div_floor" || f.name == "div_trunc" || f.name == "rem" {
                return Err(format!(
                    "the name `{}` is reserved for a built-in",
                    f.name
                ));
            }
            for p in &f.params {
                self.validate_type(&p.ty)?;
            }
            self.validate_type(&f.ret)?;
            // Returning an array would need array-valued expressions to be
            // bindable (`let a: [Int; 3] = f();`), which is the whole-array
            // copy question deferred with collections. Parameters are fine.
            if matches!(f.ret, Type::Array { .. }) {
                return Err(format!(
                    "fn `{}` cannot return an array yet — returning one needs \
                     whole-array binding, which arrives with collections. Return \
                     a struct, or fill an array the caller owns.",
                    f.name
                ));
            }
            // RULE 2 of escape checking: returning region data would let it
            // outlive the region the caller opened.
            if self.region_allocated(&f.ret) {
                return Err(format!(
                    "fn `{}` cannot return {}, because its storage lives in a region \
                     and would not outlive it. Fill an array the caller owns, or \
                     return a scalar summary.",
                    f.name, f.ret
                ));
            }
            // Returning a trait object stays refused. A `dyn` borrows its source
            // BINDING, which is a local — so the borrow dangles on return
            // whether or not a region is involved. Regions bound
            // region-allocated data's lifetime; they do not change what a trait
            // object points at.
            if matches!(f.ret, Type::Dyn(_)) {
                return Err(format!(
                    "fn `{}` cannot return a trait object — it borrows the value it \
                     refers to, which would not outlive the call. Take one as a \
                     parameter instead.",
                    f.name
                ));
            }
            let param_tys = f.params.iter().map(|p| p.ty.clone()).collect();
            self.fns.insert(f.name.clone(), (param_tys, f.ret.clone()));
            if f.allocates {
                self.alloc_fns.insert(f.name.clone());
            }
            if f.is_pure {
                self.pure_fns.insert(f.name.clone());
            }
        }

        // Collect the methods declared inside impl blocks alongside the
        // free-standing ones: a trait method is just a method that also counts
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
            if !self.structs.contains_key(&m.receiver) {
                return Err(format!(
                    "method `{}` is declared for unknown type `{}` — declare it \
                     with `struct {} {{ ... }}`",
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
            for p in &m.params {
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
            let param_tys = m.params.iter().map(|p| p.ty.clone()).collect();
            if m.allocates {
                self.alloc_methods.insert(key.clone());
            }
            self.methods.insert(key, (m.receiver_mut, param_tys, m.ret.clone()));
        }

        // Impls: satisfaction must be EXACT — every trait method present, with
        // matching receiver form and types. A partial or mismatched impl names
        // the offending method.
        for im in &prog.impls {
            self.current_span.set(im.span);
            self.check_impl(im)?;
            self.impls.insert((im.trait_name.clone(), im.type_name.clone()));
        }

        // Pass 2: check each function body.
        let mut fns = Vec::new();
        for f in &prog.fns {
            self.current_span.set(f.span);
            fns.push(self.check_fn(f)?);
        }
        let mut methods = Vec::new();
        for m in all_methods.iter().copied() {
            methods.push(self.check_method(m)?);
        }

        // Pass 3: top-level statements (the implicit main).
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

        // A vtable is emitted only for impls of traits actually used as `dyn`
        // — if a type never becomes a trait object, it costs nothing.
        let mut vtables = Vec::new();
        for im in &prog.impls {
            self.current_span.set(im.span);
            if !self.dyn_traits.contains(&im.trait_name) {
                continue;
            }
            let sigs = &self.traits[&im.trait_name];
            vtables.push(TypedVTable {
                trait_name: im.trait_name.clone(),
                concrete: im.type_name.clone(),
                // trait-declaration order fixes each slot index
                slots: sigs.iter().map(|s| s.name.clone()).collect(),
            });
        }

        Ok(TypedProgram { structs, enums, externs, fns, methods, vtables, stmts })
    }

    /// An impl must satisfy its trait EXACTLY: every declared method present,
    /// same receiver form, same parameter types, same return type.
    fn check_impl(&self, im: &ImplBlock) -> Result<(), String> {
        let sigs = self.traits.get(&im.trait_name).ok_or_else(|| {
            format!(
                "unknown trait `{}` — declare it with `trait {} {{ ... }}`",
                im.trait_name, im.trait_name
            )
        })?;
        if !self.structs.contains_key(&im.type_name) {
            return Err(format!(
                "`impl {} for {}`: unknown type `{}` — declare it with \
                 `struct {} {{ ... }}`",
                im.trait_name, im.type_name, im.type_name, im.type_name
            ));
        }
        if self.impls.contains(&(im.trait_name.clone(), im.type_name.clone())) {
            return Err(format!(
                "`{}` already implements `{}`",
                im.type_name, im.trait_name
            ));
        }

        // Every method in the block must belong to the trait...
        for m in &im.methods {
            if !sigs.iter().any(|s| s.name == m.name) {
                return Err(format!(
                    "`impl {} for {}` defines `{}`, which is not a method of \
                     `{}`. Its methods are: {}.",
                    im.trait_name,
                    im.type_name,
                    m.name,
                    im.trait_name,
                    sigs.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", ")
                ));
            }
            if m.receiver != im.type_name {
                return Err(format!(
                    "in `impl {} for {}`, method `{}` has receiver `self: {}` — \
                     it must be `self: {}`.",
                    im.trait_name, im.type_name, m.name, m.receiver, im.type_name
                ));
            }
        }

        // ...and every trait method must be present, matching exactly.
        for sig in sigs {
            let found = im.methods.iter().find(|m| m.name == sig.name).ok_or_else(|| {
                format!(
                    "`impl {} for {}` is missing the method `{}`. Every trait \
                     method must be implemented — Burxt has no default bodies.",
                    im.trait_name, im.type_name, sig.name
                )
            })?;
            if found.receiver_mut != sig.receiver_mut {
                return Err(format!(
                    "in `impl {} for {}`, method `{}` declares `{}self` but the \
                     trait declares `{}self`.",
                    im.trait_name,
                    im.type_name,
                    sig.name,
                    if found.receiver_mut { "mut " } else { "" },
                    if sig.receiver_mut { "mut " } else { "" }
                ));
            }
            if found.params.len() != sig.params.len() {
                return Err(format!(
                    "in `impl {} for {}`, method `{}` takes {} parameter(s) but \
                     the trait declares {}.",
                    im.trait_name,
                    im.type_name,
                    sig.name,
                    found.params.len(),
                    sig.params.len()
                ));
            }
            for (i, (fp, sp)) in found.params.iter().zip(&sig.params).enumerate() {
                if fp.ty != sp.ty {
                    return Err(format!(
                        "in `impl {} for {}`, method `{}` parameter {} is {} but \
                         the trait declares {}.",
                        im.trait_name,
                        im.type_name,
                        sig.name,
                        i + 1,
                        fp.ty,
                        sp.ty
                    ));
                }
            }
            if found.ret != sig.ret {
                return Err(format!(
                    "in `impl {} for {}`, method `{}` returns {} but the trait \
                     declares {}.",
                    im.trait_name, im.type_name, sig.name, found.ret, sig.ret
                ));
            }
        }
        Ok(())
    }

    /// Coerce a concrete struct value to a trait object where one is expected.
    /// Lives here rather than in `let` so that struct fields, call arguments
    /// and returns all coerce too — every site that knows its expected type.
    /// The source must be a plain variable: the fat pointer borrows its storage,
    /// and an expression has none.
    fn coerce_dyn(
        &self,
        trait_name: &str,
        e: &Expr,
    ) -> Result<TypedExpr, String> {
        let ExprKind::Var(var) = &e.kind else {
            return Err(format!(
                "a `dyn {}` must come from a variable — a trait object borrows the \
                 storage of the value it refers to, and an expression has none.",
                trait_name
            ));
        };
        let (src_ty, _) = self
            .env
            .get(var)
            .ok_or_else(|| self.unknown_name(var))?
            .clone();
        let concrete = match &src_ty {
            Type::Named(c) if self.structs.contains_key(c) => c.clone(),
            Type::Dyn(_) => {
                return Err(format!(
                    "`{}` is already a trait object; re-borrowing one is deferred \
                     until Burxt tracks borrows.",
                    var
                ))
            }
            other => {
                return Err(format!(
                    "`{}` has type {}, which cannot be a `dyn {}` — only a struct \
                     that implements the trait can.",
                    var, other, trait_name
                ))
            }
        };
        if !self.impls.contains(&(trait_name.to_string(), concrete.clone())) {
            return Err(format!(
                "`{}` does not implement `{}` — add `impl {} for {} {{ ... }}`.",
                concrete, trait_name, trait_name, concrete
            ));
        }
        Ok(TypedExpr {
            ty: Type::Dyn(trait_name.to_string()),
            kind: TypedExprKind::DynCoerce {
                trait_name: trait_name.to_string(),
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
                             it `let mut {}: {}` to allow it.",
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
            TypedExprKind::VariantLit { args, .. } => {
                args.iter().any(|a| self.expr_allocates(a))
            }
            TypedExprKind::ArrayLit(items) => items.iter().any(|i| self.expr_allocates(i)),
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
    /// records that the trait needs vtables.
    fn validate_type(&mut self, ty: &Type) -> Result<(), String> {
        if let Type::Dyn(name) = ty {
            if !self.traits.contains_key(name) {
                return Err(format!(
                    "unknown trait `{}` — declare it with `trait {} {{ ... }}`",
                    name, name
                ));
            }
            self.dyn_traits.insert(name.clone());
            return Ok(());
        }
        match ty {
            Type::Named(name)
                if !self.structs.contains_key(name) && !self.enums.contains_key(name) =>
            {
                Err(format!(
                    "unknown type `{}` — declare it with `struct {} {{ ... }}` or \
                     `enum {} {{ ... }}`",
                    name, name, name
                ))
            }
            Type::CInt => Err(
                "CInt only exists at the C boundary (extern fn signatures) — \
                 use Int in Burxt code; values convert at the call."
                    .to_string(),
            ),
            // Elements may be scalars OR aggregates: a `[Node; 256]` is
            // stack-allocatable, which is what makes an arena-style AST
            // (children referenced by index, never by pointer) possible without
            // any heap. Refused: nested arrays, because `a[i][j]` cannot be
            // written — indexing takes a binding name, not an expression — and
            // trait objects, because they borrow and storing a borrow needs
            // tracking Burxt does not have.
            Type::Slice(elem) => match elem.as_ref() {
                Type::Slice(_) | Type::Array { .. } => Err(
                    "a growable array cannot hold another array yet — its element \
                     would need its own region reasoning. Use a struct element."
                        .to_string(),
                ),
                Type::Dyn(t) => Err(format!(
                    "a growable array cannot hold `dyn {}` yet — region-allocated \
                     trait objects arrive in a later slice.",
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
                     expression. Use one array of a struct instead."
                        .to_string(),
                ),
                Type::Dyn(t) => Err(format!(
                    "an array cannot hold `dyn {}` — a trait object borrows the value \
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
        if let Some(fields) = self.structs.get(name) {
            for (_, ty) in fields {
                if let Type::Named(inner) = ty {
                    self.check_struct_finite(inner, trail)?;
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
        if e.name == "len" || e.name == "byte_at" || e.name == "push" || e.name == "read_file" || e.name == "to_string" || e.name == "old" || e.name == "substring" || e.name == "truncate" || e.name == "write_file" || e.name == "arg" || e.name == "arg_count" || e.name == "div_floor" || e.name == "div_trunc" || e.name == "rem" {
            return Err(format!("the name `{}` is reserved for a built-in", e.name));
        }
        if RESERVED.contains(&e.name.as_str()) {
            return Err(format!(
                "extern fn `{}`: this symbol is used by the Burxt runtime itself. \
                 Call it through a differently-named C wrapper.",
                e.name
            ));
        }
        if self.fns.contains_key(&e.name) {
            return Err(format!("function `{}` is defined twice", e.name));
        }
        for p in &e.params {
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
                        "in extern fn `{}`, parameter `{}` is {} and C has no \
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
                        "in extern fn `{}`, parameter `{}` is {}, which C holds \
                         directly — `as {}` only means something for a Decimal, \
                         whose scale C has no way to carry.",
                        e.name, p.name, other, m
                    ))
                }
                (Type::Int | Type::String | Type::CInt | Type::CDouble, None) => {}
                (other, None) => {
                    return Err(format!(
                        "in extern fn `{}`, parameter `{}` has type {}, but only \
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
                "extern fn `{}` returns CDouble, but Burxt has no float type to \
                 receive it exactly — a double cannot hold most decimal amounts. \
                 Have the C function return the scaled integer (declare `-> Int`), \
                 or return it as text.",
                e.name
            ));
        }
        if !matches!(e.ret, Type::Int | Type::CInt) {
            return Err(format!(
                "extern fn `{}` returns {}, but only Int or CInt may cross the C \
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
        self.env.clear();
        let mut params = Vec::new();
        for p in &f.params {
            if let Some(m) = p.marshal {
                return Err(format!(
                    "in `fn {}`, parameter `{}` is marked `as {}`, but marshalling \
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
            params.push((p.name.clone(), p.ty.clone()));
        }
        self.current_ret = Some(f.ret.clone());
        self.in_caller_region = f.allocates;
        self.in_pure = if f.is_pure { Some(f.name.clone()) } else { None };
        self.current_sig =
            Some((f.name.clone(), f.params.iter().map(|p| p.ty.clone()).collect()));
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
        self.current_sig = None;
        self.env.clear();

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
        Ok(TypedFn { name: f.name.clone(), params, ret: f.ret.clone(), body, requires, ensures, decreases, olds })
    }

    /// Check a method body. `self` is bound like any parameter, with its
    /// mutability set from `receiver_mut` — so `self.field = ...` obeys the
    /// exact same AssignField rule an ordinary `let mut` binding would.
    fn check_method(&mut self, m: &MethodDef) -> Result<TypedMethod, String> {
        self.current_span.set(m.span);
        self.env.clear();
        self.env.insert(
            "self".to_string(),
            (Type::Named(m.receiver.clone()), m.receiver_mut),
        );
        let mut params = Vec::new();
        for p in &m.params {
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
            params.push((p.name.clone(), p.ty.clone()));
        }
        self.current_ret = Some(m.ret.clone());
        self.in_caller_region = m.allocates;

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
        Ok(TypedMethod {
            receiver: m.receiver.clone(),
            receiver_mut: m.receiver_mut,
            name: m.name.clone(),
            params,
            ret: m.ret.clone(),
            body,
            requires,
            ensures,
            olds,
        })
    }

    /// `return tail f(args)` — the guarantee, checked.
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
        let (caller, caller_params) = self.current_sig.clone().ok_or_else(|| {
            "a guaranteed tail call only makes sense inside a function".to_string()
        })?;
        let (name, args) = match &e.kind {
            ExprKind::Call { name, args } => (name.clone(), args),
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
                "`{}` is an `extern fn`, so Burxt cannot guarantee a tail call \
                 into it: the C side owns that ABI, and the width conversion \
                 Burxt does on the result has to happen after the call returns.",
                name
            ));
        }
        let (params, callee_ret) = self.fns.get(&name).cloned().ok_or_else(|| {
            format!(
                "unknown function `{}` — a guaranteed tail call needs a `fn` \
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
        if callee_ret != ret || params != caller_params {
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
                Self::type_list(&params),
                callee_ret
            ));
        }
        if !params.iter().all(scalar) || !scalar(&ret) {
            return Err(format!(
                "a guaranteed tail call is limited to scalar parameters and \
                 returns for now — `{}` passes or returns an aggregate, which \
                 travels by hidden pointer into storage this frame owns. Use an \
                 ordinary `return`.",
                name
            ));
        }

        // Ordinary argument checking: a tail call is still a call.
        if args.len() != params.len() {
            return Err(format!(
                "`{}` takes {} argument{}, but {} {} given",
                name,
                params.len(),
                if params.len() == 1 { "" } else { "s" },
                args.len(),
                if args.len() == 1 { "was" } else { "were" }
            ));
        }
        let mut typed_args = Vec::new();
        for (arg, want) in args.iter().zip(params.iter()) {
            let t = self.check_expr(arg, Some(want))?;
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
        Ok(TypedStmt::TailReturn { name, args: typed_args })
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
                         name, or `{} = ...` if it was declared `let mut`.",
                        name, name, name
                    ));
                }
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
                if matches!(declared, Type::Array { .. }) && !matches!(value.kind, ExprKind::ArrayLit(_))
                {
                    return Err(format!(
                        "`let {}: {}` must be initialized with an array literal, \
                         e.g. [1, 2, 3] — copying a whole array is deferred.",
                        name, declared
                    ));
                }
                let typed = self.check_expr(value, Some(declared))?;
                if &typed.ty != declared {
                    // The declaration is fine; it is the value that disagrees.
                    self.blame(value.span);
                    return Err(format!(
                        "type mismatch in `let {}`: declared {}, but expression has type {}",
                        name, declared, typed.ty
                    ));
                }
                self.env.insert(name.clone(), (declared.clone(), *mutable));
                Ok(TypedStmt::Let { name: name.clone(), ty: declared.clone(), value: typed })
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
                         Declare it `let mut {}: {}` to allow reassignment.",
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
                         Declare it `let mut {}: {}` to allow it.",
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
                         Declare it `let mut {}: {}` to allow it.",
                        lvalue, name, name, cur_ty
                    ));
                }
                let mut indices = Vec::new();
                for field in path {
                    let (i, t) = self.resolve_field(&cur_ty, field)?;
                    indices.push(i);
                    cur_ty = t;
                }
                let (elem, len) = match &cur_ty {
                    Type::Array { elem, len } => (elem.as_ref().clone(), *len),
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
                let (elem, len) = match &declared {
                    Type::Array { elem, len } => (elem.as_ref().clone(), *len),
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
                         Declare it `let mut {}: {}` to allow it.",
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
                let enum_name = match &scrutinee.ty {
                    Type::Named(n) if self.enums.contains_key(n) => n.clone(),
                    other => {
                        return Err(format!(
                            "`match` needs an enum value, but this has type {}. \
                             Use `if` to branch on other types.",
                            other
                        ))
                    }
                };
                let variants = self.enums[&enum_name].clone();

                let mut typed_arms: Vec<TypedArm> = Vec::new();
                for arm in arms {
                    // Checking the previous arm's body moved the position; an error
                    // about THIS arm's pattern belongs to the match, not to the arm
                    // above it. (Found by shadowing a name in examples/lexer.bx and
                    // being pointed at the wrong line.)
                    self.current_span.set(s.span);
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
                                enum_name,
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
                            enum_name,
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
                        enum_name,
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
                    Type::Named(n) if self.enums.contains_key(n) => {
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
                            "print does not know how to show a `dyn {}` — a trait \
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
                if self.expr_allocates(&typed) && !(self.in_caller_region && self.current_region.is_none())
                {
                    return Err(format!(
                        "cannot return this {}: it was built in a region, so its \
                         storage would not outlive it. Return a scalar summary, fill \
                         storage the caller owns, or move the allocation out of the \
                         `region` block and declare the function `allocates`.",
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
        // A concrete value becomes a trait object wherever one is expected.
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

            ExprKind::Call { name, args } => {
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
                    if args.len() != 2 {
                        return Err(
                            "truncate(...) takes a growable array and a length: \
                             truncate(xs, n)"
                                .to_string(),
                        );
                    }
                    let place = self.check_expr(&args[0], None)?;
                    if !matches!(place.ty, Type::Slice(_)) {
                        return Err(format!(
                            "truncate(...) needs a growable array `[T]`, but this has \
                             type {}",
                            place.ty
                        ));
                    }
                    self.require_mutable_place(&args[0])?;
                    let length = self.check_expr(&args[1], Some(&Type::Int))?;
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
                    if args.len() != 2 {
                        return Err(
                            "push(...) takes a growable array and a value: \
                             push(xs, v)"
                                .to_string(),
                        );
                    }
                    let place = self.check_expr(&args[0], None)?;
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
                    self.require_mutable_place(&args[0])?;
                    let value = self.check_expr(&args[1], Some(&elem))?;
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
                if name == "arg_count" {
                    if !args.is_empty() {
                        return Err("arg_count() takes no arguments".to_string());
                    }
                    return Ok(TypedExpr { ty: Type::Int, kind: TypedExprKind::ArgCount });
                }
                if name == "arg" {
                    if args.len() != 1 {
                        return Err("arg(n) takes one Int".to_string());
                    }
                    let index = self.check_expr(&args[0], Some(&Type::Int))?;
                    if index.ty != Type::Int {
                        return Err(format!(
                            "arg(n) takes an Int, but this has type {}",
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
                    if args.len() != 2 {
                        return Err("write_file(path, contents) takes two Strings".to_string());
                    }
                    let path = self.check_expr(&args[0], Some(&Type::String))?;
                    let contents = self.check_expr(&args[1], Some(&Type::String))?;
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
                // `substring(s, at, len)` — the primitive a symbol table needs. A
                // lexer can already compare a span against a literal byte by byte;
                // what it could not do was KEEP the text, which is what a table of
                // names is made of.
                if name == "substring" {
                    if args.len() != 3 {
                        return Err(
                            "substring(...) takes a String, a start offset and a length"
                                .to_string(),
                        );
                    }
                    let source = self.check_expr(&args[0], Some(&Type::String))?;
                    if source.ty != Type::String {
                        return Err(format!(
                            "substring(...) reads a String, but the first argument has \
                             type {}",
                            source.ty
                        ));
                    }
                    let at = self.check_expr(&args[1], Some(&Type::Int))?;
                    let len = self.check_expr(&args[2], Some(&Type::Int))?;
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
                    "div_floor" => Some(crate::codegen::IntDiv::Floor),
                    "div_trunc" => Some(crate::codegen::IntDiv::Trunc),
                    "rem" => Some(crate::codegen::IntDiv::Rem),
                    _ => None,
                } {
                    if args.len() != 2 {
                        return Err(format!("{}(...) takes two Ints", name));
                    }
                    let lhs = self.check_expr(&args[0], Some(&Type::Int))?;
                    let rhs = self.check_expr(&args[1], Some(&Type::Int))?;
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
                    if args.len() != 1 {
                        return Err("old(...) takes one expression".to_string());
                    }
                    // `result` has no meaning inside `old`: the point of `old` is the
                    // state BEFORE the call, and there was no result then. Checked on
                    // the expression as written, which gives a better message than
                    // letting name resolution fail.
                    if mentions(&args[0], "result") {
                        return Err(
                            "`old(result)` is a contradiction: `old` is the state \
                             before the call, and there was no result then."
                                .to_string(),
                        );
                    }
                    let inner = self.check_expr(&args[0], None)?;
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
                    if args.len() != 1 {
                        return Err("read_file(...) takes one path".to_string());
                    }
                    let path = self.check_expr(&args[0], Some(&Type::String))?;
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
                    if args.len() != 1 {
                        return Err("to_string(...) takes one value".to_string());
                    }
                    let v = self.check_expr(&args[0], None)?;
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
                if name == "byte_at" {
                    if args.len() != 2 {
                        return Err(
                            "byte_at(...) takes a string and an index: byte_at(s, i)"
                                .to_string(),
                        );
                    }
                    let s = self.check_expr(&args[0], None)?;
                    if s.ty != Type::String {
                        return Err(format!(
                            "byte_at(...) reads a String, but the first argument has \
                             type {}",
                            s.ty
                        ));
                    }
                    let idx = self.check_expr(&args[1], None)?;
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
                    if args.len() != 1 {
                        return Err(
                            "len(...) takes exactly one array or string".to_string()
                        );
                    }
                    let arg = self.check_expr(&args[0], None)?;
                    return match arg.ty {
                        Type::Array { len, .. } => Ok(TypedExpr {
                            ty: Type::Int,
                            kind: TypedExprKind::IntLit(len as i64),
                        }),
                        Type::String => Ok(TypedExpr {
                            ty: Type::Int,
                            kind: TypedExprKind::StrLen(Box::new(arg)),
                        }),
                        // a growable array knows its length at runtime
                        Type::Slice(_) => Ok(TypedExpr {
                            ty: Type::Int,
                            kind: TypedExprKind::SliceLen(Box::new(arg)),
                        }),
                        other => Err(format!(
                            "len(...) needs an array or a string, but this has type {}",
                            other
                        )),
                    };
                }
                let (param_tys, ret) = self
                    .fns
                    .get(name)
                    .ok_or_else(|| format!("unknown function: {}", name))?
                    .clone();
                if args.len() != param_tys.len() {
                    return Err(format!(
                        "function `{}` takes {} argument(s), but {} were given",
                        name,
                        param_tys.len(),
                        args.len()
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
                                 change the program it checks. Declare `pure fn {}`.",
                                holder, name, name
                            )
                        } else {
                            format!(
                                "`pure fn {}` may not call `{}`, which is not declared \
                                 `pure`: the guarantee cannot rest on a function that \
                                 does not make it. Declare `pure fn {}` too, or drop \
                                 `pure` from `{}`.",
                                holder, name, name, holder
                            )
                        });
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
                let declared = self.extern_params.get(name).cloned();
                let mut typed_args = Vec::new();
                for (i, (arg, param_ty)) in args.iter().zip(&param_tys).enumerate() {
                    let typed = self.check_expr(arg, Some(param_ty))?;
                    if &typed.ty != param_ty {
                        // Point at the argument, not at the whole call.
                        self.blame(arg.span);
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
                            name,
                            i + 1,
                            param_ty,
                            typed.ty
                        ));
                    }
                    typed_args.push(typed);
                }
                Ok(TypedExpr { ty: ret, kind: TypedExprKind::Call { name: name.clone(), args: typed_args } })
            }

            ExprKind::StructLit { name, fields } => {
                let declared = self
                    .structs
                    .get(name)
                    .ok_or_else(|| {
                        format!(
                            "unknown type `{}` — declare it with `struct {} {{ ... }}`",
                            name, name
                        )
                    })?
                    .clone();
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
                    ty: Type::Named(name.clone()),
                    kind: TypedExprKind::StructLit { name: name.clone(), fields: typed_fields },
                })
            }

            ExprKind::Field { base, field } => {
                if let Some(r) = self.check_variant_lit(base, field, &[]) {
                    return r;
                }
                let typed_base = self.check_expr(base, None)?;
                let (index, ty) = self.resolve_field(&typed_base.ty, field)?;
                Ok(TypedExpr {
                    ty,
                    kind: TypedExprKind::Field { base: Box::new(typed_base), index },
                })
            }

            ExprKind::MethodCall { base, method, args } => {
                // Methods cannot carry the marker yet, so a pure function cannot
                // call one. Said plainly, with the reason, rather than by letting
                // some later check produce something confusing.
                if let Some(name) = &self.in_pure {
                    return Err(format!(
                        "`pure fn {}` may not call the method `.{}()`: a method cannot \
                         be declared `pure` yet, so there is no promise to rely on. \
                         Move the calculation into a `pure fn`, passing the fields it \
                         needs.",
                        name, method
                    ));
                }
                if let Some(r) = self.check_variant_lit(base, method, args) {
                    return r;
                }
                let typed_base = self.check_expr(base, None)?;

                // A call on a `dyn Trait` is the ONE place dispatch happens at
                // runtime: find the method's slot from trait-declaration order.
                if let Type::Dyn(trait_name) = typed_base.ty.clone() {
                    let sigs = &self.traits[&trait_name];
                    let slot = sigs
                        .iter()
                        .position(|s| s.name == *method)
                        .ok_or_else(|| {
                            format!(
                                "`dyn {}` has no method named `{}`. Its methods \
                                 are: {}.",
                                trait_name,
                                method,
                                sigs.iter()
                                    .map(|s| s.name.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            )
                        })?;
                    let sig = sigs[slot].clone();
                    if sig.receiver_mut {
                        return Err(format!(
                            "`{}` takes `mut self`, and calling a mutating method \
                             through a trait object is not available yet: the \
                             compiler still cannot tell whether the value behind \
                             the object was declared mutable. Regions bound its \
                             LIFETIME, not its mutability. Call it on the concrete \
                             type.",
                            method
                        ));
                    }
                    if args.len() != sig.params.len() {
                        return Err(format!(
                            "`dyn {}.{}` takes {} argument(s), but {} were given",
                            trait_name,
                            method,
                            sig.params.len(),
                            args.len()
                        ));
                    }
                    let mut typed_args = Vec::new();
                    for (i, (arg, p)) in args.iter().zip(&sig.params).enumerate() {
                        let typed = self.check_expr(arg, Some(&p.ty))?;
                        if typed.ty != p.ty {
                            return Err(format!(
                                "in the call to `dyn {}.{}`, argument {} must be \
                                 {}, but it has type {}",
                                trait_name,
                                method,
                                i + 1,
                                p.ty,
                                typed.ty
                            ));
                        }
                        typed_args.push(typed);
                    }
                    return Ok(TypedExpr {
                        ty: sig.ret.clone(),
                        kind: TypedExprKind::DynCall {
                            trait_name,
                            method: method.clone(),
                            slot: slot as u32,
                            base: Box::new(typed_base),
                            args: typed_args,
                        },
                    });
                }

                let receiver = match &typed_base.ty {
                    Type::Named(n) => n.clone(),
                    other => {
                        return Err(format!(
                            "`.{}(...)` needs a struct value, but this has type {}.",
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

                if receiver_mut {
                    // A mutating method is passed a true reference, so the
                    // base MUST be the actual mutable binding — exactly the
                    // rule AssignField already enforces for `item.field = v`.
                    let ExprKind::Var(name) = &base.as_ref().kind else {
                        return Err(format!(
                            "`{}` is a mutating method (`fn (mut self: {}) ...`); \
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
                             declared immutable. Declare it `let mut {}: {}` to \
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
                if args.len() != param_tys.len() {
                    return Err(format!(
                        "method `{}.{}` takes {} argument(s), but {} were given",
                        receiver,
                        method,
                        param_tys.len(),
                        args.len()
                    ));
                }
                let mut typed_args = Vec::new();
                for (i, (arg, param_ty)) in args.iter().zip(&param_tys).enumerate() {
                    let typed = self.check_expr(arg, Some(param_ty))?;
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
                        args: typed_args,
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
                    _ => {
                        return Err(
                            "an array literal needs a declared array type — write it \
                             as a `let` initializer: let a: [Int; 3] = [...];"
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
        args: &[Expr],
    ) -> Option<Result<TypedExpr, String>> {
        let ExprKind::Var(enum_name) = &base.kind else { return None };
        // A local binding wins over an enum of the same name: shadowing is
        // refused elsewhere, so this can only be a genuine variable.
        if self.env.contains_key(enum_name) {
            return None;
        }
        let variants = self.enums.get(enum_name)?;
        Some(self.build_variant(enum_name, variants.clone(), variant, args))
    }

    fn build_variant(
        &self,
        enum_name: &str,
        variants: Vec<(String, Vec<Type>)>,
        variant: &str,
        args: &[Expr],
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
        if args.len() != payload.len() {
            return Err(format!(
                "`{}.{}` carries {} value(s), but {} were given",
                enum_name,
                variant,
                payload.len(),
                args.len()
            ));
        }
        let mut typed_args = Vec::new();
        for (i, (arg, want)) in args.iter().zip(payload).enumerate() {
            let t = self.check_expr(arg, Some(want))?;
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
                args: typed_args,
            },
        })
    }

    /// Resolve `.field` on a value of type `ty` to (positional index, type).
    fn resolve_field(&self, ty: &Type, field: &str) -> Result<(u32, Type), String> {
        let name = match ty {
            Type::Named(n) => n,
            other => {
                return Err(format!(
                    "`.{}` needs a struct value, but the value has type {}.",
                    field, other
                ))
            }
        };
        let fields = self
            .structs
            .get(name)
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
                "struct comparison is not available yet — compare fields individually."
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
                 rounding down. Say which you mean — `div_floor(a, b)`, \
                 `div_trunc(a, b)`, or `rem(a, b)` for the remainder."
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
                if lhs == rhs {
                    // identical operand types: the long-standing rule
                    return self.require_rounding(op, lhs);
                }
                match expected {
                    Some(t @ Decimal { rounding: Some(_), .. }) => Ok(t.clone()),
                    Some(Decimal { scale, rounding: None }) => Err(format!(
                        "this multiplication mixes scales {} and {}, so its exact \
                         product has {} decimal places and must be rounded to reach \
                         {}. Give the result a rounding contract to say how, e.g. \
                         Decimal<{}, RoundHalfEven>.",
                        ls,
                        rs,
                        ls + rs,
                        scale,
                        scale
                    )),
                    _ => Err(format!(
                        "this multiplication mixes scales {} and {}, so its exact \
                         product has {} decimal places and must be rounded. Bind it \
                         to a Decimal with a rounding contract, e.g. \
                         `let x: Decimal<2, RoundHalfEven> = ...`, so the rounding \
                         is declared rather than guessed.",
                        ls,
                        rs,
                        ls + rs
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
    fn matching_decimal(
        &self,
        op: impl std::fmt::Display,
        lhs: &Type,
        rhs: &Type,
    ) -> Result<Type, String> {
        if lhs == rhs {
            return Ok(lhs.clone());
        }
        if let (Type::Decimal { scale: a, .. }, Type::Decimal { scale: b, .. }) = (lhs, rhs) {
            if a != b {
                return Err(format!(
                    "cannot {} {} and {}: scales must match. \
                     Burxt does not silently rescale money.",
                    op, lhs, rhs
                ));
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
            ExprKind::Call { name: callee, args } => callee == name || any(args),
            ExprKind::Neg(i) | ExprKind::Not(i) => in_expr(i, name),
            ExprKind::Logical { lhs, rhs, .. }
            | ExprKind::Binary { lhs, rhs, .. }
            | ExprKind::Compare { lhs, rhs, .. } => in_expr(lhs, name) || in_expr(rhs, name),
            ExprKind::MethodCall { base, args, .. } => in_expr(base, name) || any(args),
            ExprKind::StructLit { fields, .. } => fields.iter().any(|(_, v)| in_expr(v, name)),
            ExprKind::Field { base, .. } => in_expr(base, name),
            ExprKind::ArrayLit(items) => any(items),
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
        ExprKind::Neg(i) | ExprKind::Not(i) => mentions(i, name),
        ExprKind::Logical { lhs, rhs, .. }
        | ExprKind::Binary { lhs, rhs, .. }
        | ExprKind::Compare { lhs, rhs, .. } => mentions(lhs, name) || mentions(rhs, name),
        ExprKind::Call { args, .. } => any(args),
        ExprKind::MethodCall { base, args, .. } => mentions(base, name) || any(args),
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
