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

use crate::ast::*;
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
    /// Bounds-checked indexed read; `len` carried for the runtime check.
    Index { name: String, len: u32, index: Box<TypedExpr> },
}

#[derive(Debug, Clone)]
pub enum TypedStmt {
    Let { name: String, ty: Type, value: TypedExpr },
    Assign { name: String, value: TypedExpr },
    /// Field assignment, path resolved to positional indices.
    AssignField { name: String, indices: Vec<u32>, value: TypedExpr },
    /// A call kept for its side effect; the result is evaluated and discarded.
    ExprStmt(TypedExpr),
    /// Bounds-checked element assignment.
    AssignIndex { name: String, len: u32, index: TypedExpr, value: TypedExpr },
    Print(TypedExpr),
    /// `print` of an interpolated string: emit each piece in order.
    PrintInterp(Vec<TypedInterpPart>),
    Return(TypedExpr),
    While { cond: TypedExpr, body: Vec<TypedStmt> },
    If {
        cond: TypedExpr,
        then_block: Vec<TypedStmt>,
        else_block: Option<Vec<TypedStmt>>,
    },
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
    /// struct name -> fields (name, type) in declaration order; hoisted first.
    structs: HashMap<String, Vec<(String, Type)>>,
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
    current_ret: Option<Type>,
}

impl TypeChecker {
    pub fn new() -> Self {
        TypeChecker {
            env: HashMap::new(),
            fns: HashMap::new(),
            structs: HashMap::new(),
            methods: HashMap::new(),
            traits: HashMap::new(),
            impls: HashSet::new(),
            dyn_traits: HashSet::new(),
            current_ret: None,
        }
    }

    pub fn check_program(mut self, prog: &Program) -> Result<TypedProgram, String> {
        // Pass 0: hoist struct declarations, then validate them (field types
        // must exist; no struct may contain itself, directly or transitively).
        for s in &prog.structs {
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
                fields.push((f.name.clone(), f.ty.clone()));
            }
            self.structs.insert(s.name.clone(), fields);
        }
        for s in &prog.structs {
            for f in &s.fields {
                if matches!(f.ty, Type::Array { .. }) {
                    return Err(format!(
                        "in struct `{}`: struct fields cannot hold arrays yet — \
                         coming with the aggregate ABI.",
                        s.name
                    ));
                }
                if matches!(f.ty, Type::Dyn(_)) {
                    return Err(format!(
                        "in struct `{}`: a field cannot hold a trait object — it \
                         borrows the value it refers to, and a struct may outlive \
                         it. Storing trait objects needs borrow tracking.",
                        s.name
                    ));
                }
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

        // Traits: signature sets only, hoisted so impls may precede them.
        for t in &prog.traits {
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
            for sig in &t.methods {
                for p in &sig.params {
                    self.validate_type(&p.ty)?;
                }
                self.validate_type(&sig.ret)?;
            }
        }

        // Pass 1: collect every signature, so order of definition never matters.
        let mut externs = Vec::new();
        for e in &prog.externs {
            self.check_extern(e)?;
            // Burxt code always sees CInt as Int; the width conversion is
            // codegen's job at the call site.
            let seen = |t: &Type| if *t == Type::CInt { Type::Int } else { t.clone() };
            let param_tys: Vec<Type> = e.params.iter().map(|p| seen(&p.ty)).collect();
            self.fns.insert(e.name.clone(), (param_tys, seen(&e.ret)));
            externs.push(TypedExtern {
                name: e.name.clone(),
                params: e.params.iter().map(|p| p.ty.clone()).collect(),
                ret: e.ret.clone(),
            });
        }
        for f in &prog.fns {
            if self.fns.contains_key(&f.name) {
                return Err(format!("function `{}` is defined twice", f.name));
            }
            if f.name == "len" {
                return Err(
                    "the name `len` is reserved for the built-in array length".to_string()
                );
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
            // A trait object borrows its data. Returning one would outlive the
            // storage it points at, and Burxt has no borrow tracking yet.
            if matches!(f.ret, Type::Dyn(_)) {
                return Err(format!(
                    "fn `{}` cannot return a trait object — it borrows the value \
                     it refers to, which would not outlive the call. Take one as a \
                     parameter instead.",
                    f.name
                ));
            }
            let param_tys = f.params.iter().map(|p| p.ty.clone()).collect();
            self.fns.insert(f.name.clone(), (param_tys, f.ret.clone()));
        }

        // Collect the methods declared inside impl blocks alongside the
        // free-standing ones: a trait method is just a method that also counts
        // toward a contract, so it uses the SAME machinery.
        let mut all_methods: Vec<&MethodDef> = prog.methods.iter().collect();
        for im in &prog.impls {
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
            self.methods.insert(key, (m.receiver_mut, param_tys, m.ret.clone()));
        }

        // Impls: satisfaction must be EXACT — every trait method present, with
        // matching receiver form and types. A partial or mismatched impl names
        // the offending method.
        for im in &prog.impls {
            self.check_impl(im)?;
            self.impls.insert((im.trait_name.clone(), im.type_name.clone()));
        }

        // Pass 2: check each function body.
        let mut fns = Vec::new();
        for f in &prog.fns {
            fns.push(self.check_fn(f)?);
        }
        let mut methods = Vec::new();
        for m in all_methods.iter().copied() {
            methods.push(self.check_method(m)?);
        }

        // Pass 3: top-level statements (the implicit main).
        let mut stmts = Vec::new();
        for s in &prog.stmts {
            stmts.push(self.check_stmt(s)?);
        }

        // A vtable is emitted only for impls of traits actually used as `dyn`
        // — if a type never becomes a trait object, it costs nothing.
        let mut vtables = Vec::new();
        for im in &prog.impls {
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

        Ok(TypedProgram { structs, externs, fns, methods, vtables, stmts })
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
            Type::Named(name) if !self.structs.contains_key(name) => Err(format!(
                "unknown type `{}` — declare it with `struct {} {{ ... }}`",
                name, name
            )),
            Type::CInt => Err(
                "CInt only exists at the C boundary (extern fn signatures) — \
                 use Int in Burxt code; values convert at the call."
                    .to_string(),
            ),
            Type::Array { elem, .. } => match elem.as_ref() {
                Type::Int | Type::Bool | Type::Decimal { .. } => Ok(()),
                other => Err(format!(
                    "arrays of {} are not available yet — elements must be Int, \
                     Bool or Decimal for now",
                    other
                )),
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
        if e.name == "len" {
            return Err(
                "the name `len` is reserved for the built-in array length".to_string()
            );
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
            if !matches!(p.ty, Type::Int | Type::String | Type::CInt) {
                return Err(format!(
                    "in extern fn `{}`, parameter `{}` has type {}, but only Int, \
                     CInt and String may cross the C boundary for now — C has no \
                     {}, and the raw value would silently lose its meaning.",
                    e.name, p.name, p.ty, p.ty
                ));
            }
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
        self.env.clear();
        let mut params = Vec::new();
        for p in &f.params {
            if self.env.insert(p.name.clone(), (p.ty.clone(), false)).is_some() {
                return Err(format!(
                    "function `{}` has two parameters named `{}`",
                    f.name, p.name
                ));
            }
            params.push((p.name.clone(), p.ty.clone()));
        }
        self.current_ret = Some(f.ret.clone());
        let body = self.check_block(&f.body)?;
        self.current_ret = None;
        self.env.clear();

        if !block_returns(&body) {
            return Err(format!(
                "function `{}` must end by returning a {} on every path \
                 (its last statement must be a `return`, or an if/else where \
                 both branches return)",
                f.name, f.ret
            ));
        }
        Ok(TypedFn { name: f.name.clone(), params, ret: f.ret.clone(), body })
    }

    /// Check a method body. `self` is bound like any parameter, with its
    /// mutability set from `receiver_mut` — so `self.field = ...` obeys the
    /// exact same AssignField rule an ordinary `let mut` binding would.
    fn check_method(&mut self, m: &MethodDef) -> Result<TypedMethod, String> {
        self.env.clear();
        self.env.insert(
            "self".to_string(),
            (Type::Named(m.receiver.clone()), m.receiver_mut),
        );
        let mut params = Vec::new();
        for p in &m.params {
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
        let body = self.check_block(&m.body)?;
        self.current_ret = None;
        self.env.clear();

        if !block_returns(&body) {
            return Err(format!(
                "method `{}.{}` must end by returning a {} on every path \
                 (its last statement must be a `return`, or an if/else where \
                 both branches return)",
                m.receiver, m.name, m.ret
            ));
        }
        Ok(TypedMethod {
            receiver: m.receiver.clone(),
            receiver_mut: m.receiver_mut,
            name: m.name.clone(),
            params,
            ret: m.ret.clone(),
            body,
        })
    }

    /// Check a block's statements in a child scope: names declared inside are
    /// gone after the closing brace. Also refuses unreachable code — anything
    /// following a statement that always returns.
    fn check_block(&mut self, stmts: &[Stmt]) -> Result<Vec<TypedStmt>, String> {
        let saved = self.env.clone();
        let mut out: Vec<TypedStmt> = Vec::new();
        for s in stmts {
            if out.last().is_some_and(stmt_returns) {
                self.env = saved;
                return Err(
                    "unreachable statement: this code comes after a `return`".to_string()
                );
            }
            match self.check_stmt(s) {
                Ok(t) => out.push(t),
                Err(e) => {
                    self.env = saved;
                    return Err(e);
                }
            }
        }
        self.env = saved;
        Ok(out)
    }

    fn check_stmt(&mut self, s: &Stmt) -> Result<TypedStmt, String> {
        match s {
            Stmt::Let { name, mutable, declared, value } => {
                if self.env.contains_key(name) {
                    return Err(format!(
                        "`{}` is already declared — Burxt does not allow shadowing; \
                         a second `let {}` would silently hide the first. Use a new \
                         name, or `{} = ...` if it was declared `let mut`.",
                        name, name, name
                    ));
                }
                self.validate_type(declared)?;
                // An array exists only behind a binding: it must be created
                // right here, from a literal (whole-array copies are deferred).
                if matches!(declared, Type::Array { .. }) && !matches!(value, Expr::ArrayLit(_))
                {
                    return Err(format!(
                        "`let {}: {}` must be initialized with an array literal, \
                         e.g. [1, 2, 3] — copying a whole array is deferred.",
                        name, declared
                    ));
                }
                // `let d: dyn Trait = concrete;` builds a trait object. The
                // source must be a plain variable: the fat pointer borrows its
                // storage, and an expression has none.
                if let Type::Dyn(trait_name) = declared {
                    let Expr::Var(var) = value else {
                        return Err(format!(
                            "`let {}: {}` must be initialized from a variable — a \
                             trait object borrows the storage of the value it \
                             refers to, and an expression has none.",
                            name, declared
                        ));
                    };
                    let (src_ty, _) = self
                        .env
                        .get(var)
                        .ok_or_else(|| format!("unknown variable: {}", var))?
                        .clone();
                    let concrete = match &src_ty {
                        Type::Named(c) => c.clone(),
                        Type::Dyn(_) => {
                            return Err(format!(
                                "`{}` is already a trait object; re-borrowing one \
                                 is deferred until Burxt tracks borrows.",
                                var
                            ))
                        }
                        other => {
                            return Err(format!(
                                "`{}` has type {}, which cannot be a `{}` — only a \
                                 struct that implements the trait can.",
                                var, other, declared
                            ))
                        }
                    };
                    if !self.impls.contains(&(trait_name.clone(), concrete.clone())) {
                        return Err(format!(
                            "`{}` does not implement `{}` — add `impl {} for {} \
                             {{ ... }}`.",
                            concrete, trait_name, trait_name, concrete
                        ));
                    }
                    self.env.insert(name.clone(), (declared.clone(), *mutable));
                    return Ok(TypedStmt::Let {
                        name: name.clone(),
                        ty: declared.clone(),
                        value: TypedExpr {
                            ty: declared.clone(),
                            kind: TypedExprKind::DynCoerce {
                                trait_name: trait_name.clone(),
                                concrete,
                                var: var.clone(),
                            },
                        },
                    });
                }
                let typed = self.check_expr(value, Some(declared))?;
                if &typed.ty != declared {
                    return Err(format!(
                        "type mismatch in `let {}`: declared {}, but expression has type {}",
                        name, declared, typed.ty
                    ));
                }
                self.env.insert(name.clone(), (declared.clone(), *mutable));
                Ok(TypedStmt::Let { name: name.clone(), ty: declared.clone(), value: typed })
            }
            Stmt::Assign { name, value } => {
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
            Stmt::AssignField { name, path, value } => {
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
            Stmt::AssignIndex { name, index, value } => {
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
                let index = self.check_index(name, len, index)?;
                let typed = self.check_expr(value, Some(&elem))?;
                if typed.ty != elem {
                    return Err(format!(
                        "cannot assign {} {} to `{}[...]`, which holds {}",
                        typed.ty.article(), typed.ty, name, elem
                    ));
                }
                Ok(TypedStmt::AssignIndex { name: name.clone(), len, index, value: typed })
            }
            Stmt::ExprStmt(e) => {
                let typed = self.check_expr(e, None)?;
                Ok(TypedStmt::ExprStmt(typed))
            }
            Stmt::While { cond, body } => {
                let cond = self.check_expr(cond, None)?;
                if cond.ty != Type::Bool {
                    return Err(format!(
                        "a `while` condition must be a Bool (e.g. a comparison), \
                         but this one has type {}",
                        cond.ty
                    ));
                }
                let body = self.check_block(body)?;
                Ok(TypedStmt::While { cond, body })
            }
            Stmt::Print(e) => {
                // An interpolated string prints its pieces in order, which
                // needs no allocation — so it is handled here rather than as a
                // String-valued expression.
                if let Expr::InterpStr(parts) = e {
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
            Stmt::Return(e) => {
                let ret = self.current_ret.clone().ok_or_else(|| {
                    "`return` only makes sense inside a function".to_string()
                })?;
                let typed = self.check_expr(e, Some(&ret))?;
                if typed.ty != ret {
                    return Err(format!(
                        "this function returns {}, but the `return` expression has type {}",
                        ret, typed.ty
                    ));
                }
                Ok(TypedStmt::Return(typed))
            }
            Stmt::If { cond, then_block, else_block } => {
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
    fn check_expr(&self, e: &Expr, expected: Option<&Type>) -> Result<TypedExpr, String> {
        match e {
            Expr::IntLit(n) => Ok(TypedExpr { ty: Type::Int, kind: TypedExprKind::IntLit(*n) }),

            Expr::BoolLit(b) => Ok(TypedExpr { ty: Type::Bool, kind: TypedExprKind::BoolLit(*b) }),

            Expr::StrLit(s) => {
                Ok(TypedExpr { ty: Type::String, kind: TypedExprKind::StrLit(s.clone()) })
            }

            // Producing a String VALUE from interpolation means building new
            // bytes, which needs allocation — the same wall concatenation hits.
            // Printing the pieces in order needs none, so that is where it
            // works for now.
            Expr::InterpStr(_) => Err(
                "interpolation currently works only directly inside `print(...)` — \
                 producing a String value from it needs memory allocation, coming \
                 with the memory model."
                    .to_string(),
            ),

            Expr::DecimalLit { unscaled, scale } => {
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

            Expr::Var(name) => {
                let (ty, _) = self
                    .env
                    .get(name)
                    .ok_or_else(|| format!("unknown variable: {}", name))?
                    .clone();
                Ok(TypedExpr { ty, kind: TypedExprKind::Var(name.clone()) })
            }

            Expr::Neg(inner) => {
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

            Expr::Not(inner) => {
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

            Expr::Logical { op, lhs, rhs } => {
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

            Expr::Binary { op, lhs, rhs } => {
                let l = self.check_expr(lhs, expected)?;
                let r = self.check_expr(rhs, expected)?;
                let result_ty = self.check_binop(*op, &l.ty, &r.ty)?;
                Ok(TypedExpr {
                    ty: result_ty,
                    kind: TypedExprKind::Binary {
                        op: *op,
                        lhs: Box::new(l),
                        rhs: Box::new(r),
                    },
                })
            }

            Expr::Compare { op, lhs, rhs } => {
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

            Expr::Call { name, args } => {
                // `len` is a builtin over both arrays and strings, but the two
                // are different KINDS of length, and the difference is worth
                // keeping visible:
                //   * an array's length lives in its TYPE, so it folds to a
                //     constant and codegen never sees the call;
                //   * a string's length is a property of its DATA, so it is a
                //     byte scan at runtime.
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
                let mut typed_args = Vec::new();
                for (i, (arg, param_ty)) in args.iter().zip(&param_tys).enumerate() {
                    let typed = self.check_expr(arg, Some(param_ty))?;
                    if &typed.ty != param_ty {
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

            Expr::StructLit { name, fields } => {
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

            Expr::Field { base, field } => {
                let typed_base = self.check_expr(base, None)?;
                let (index, ty) = self.resolve_field(&typed_base.ty, field)?;
                Ok(TypedExpr {
                    ty,
                    kind: TypedExprKind::Field { base: Box::new(typed_base), index },
                })
            }

            Expr::MethodCall { base, method, args } => {
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
                             through a trait object is not available yet — the \
                             compiler cannot tell whether the borrowed value is \
                             itself mutable. Call it on the concrete type.",
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
                    let Expr::Var(name) = base.as_ref() else {
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

            Expr::ArrayLit(elems) => {
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

            Expr::Index { name, index } => {
                let (ty, _) = self
                    .env
                    .get(name)
                    .ok_or_else(|| format!("unknown variable: {}", name))?
                    .clone();
                let (elem, len) = match &ty {
                    Type::Array { elem, len } => (elem.as_ref().clone(), *len),
                    other => {
                        return Err(format!(
                            "`{}[...]` indexing needs an array, but `{}` has type {}",
                            name, name, other
                        ))
                    }
                };
                let index = self.check_index(name, len, index)?;
                Ok(TypedExpr {
                    ty: elem,
                    kind: TypedExprKind::Index { name: name.clone(), len, index: Box::new(index) },
                })
            }
        }
    }

    /// Check an index expression: it must be an Int, and a LITERAL index
    /// that is provably out of range is refused at compile time — it would
    /// always fail at runtime, so it fails now instead.
    fn check_index(&self, name: &str, len: u32, index: &Expr) -> Result<TypedExpr, String> {
        let typed = self.check_expr(index, None)?;
        if typed.ty != Type::Int {
            return Err(format!(
                "an array index must be an Int, but this one has type {}",
                typed.ty
            ));
        }
        if let TypedExprKind::IntLit(n) = typed.kind {
            if n < 0 || n >= len as i64 {
                let (ty, _) = self.env.get(name).cloned().unwrap_or((Type::Int, false));
                return Err(format!(
                    "index {} is out of bounds for {}: valid indexes are 0 to {}. \
                     This would always fail at runtime, so it is refused now.",
                    n,
                    ty,
                    len - 1
                ));
            }
        }
        Ok(typed)
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
    fn check_binop(&self, op: BinOp, lhs: &Type, rhs: &Type) -> Result<Type, String> {
        use Type::*;
        match (op, lhs, rhs) {
            // Integer division truncates — that is silent rounding. Refused
            // until integers get explicit division semantics.
            (BinOp::Div, Int, Int) => Err(
                "integer division truncates, which rounds silently. \
                 Burxt does not allow it (yet) — use Decimals with a rounding \
                 contract, e.g. Decimal<2, RoundHalfEven>."
                    .to_string(),
            ),

            // Integer arithmetic.
            (_, Int, Int) => Ok(Int),

            // String + String is concatenation — deferred until Burxt has an
            // allocation story. Refuse loudly with the reason.
            (BinOp::Add, String, String) => Err(
                "`+` on String is concatenation, which needs memory allocation — \
                 coming with collections (A4)."
                    .to_string(),
            ),

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

            // Decimal * Decimal and Decimal / Decimal produce digits beyond
            // the operands' scale, so they require matching types AND an
            // explicit rounding contract saying how to return to that scale.
            (BinOp::Mul, Decimal { .. }, Decimal { .. })
            | (BinOp::Div, Decimal { .. }, Decimal { .. }) => {
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

/// Does this statement return on every path through it?
fn stmt_returns(s: &TypedStmt) -> bool {
    match s {
        TypedStmt::Return(_) => true,
        TypedStmt::If { then_block, else_block: Some(e), .. } => {
            block_returns(then_block) && block_returns(e)
        }
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
                "literal has scale {} but context expects scale {}; \
                 narrowing would lose precision (refused).",
                from_scale, to_scale
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
