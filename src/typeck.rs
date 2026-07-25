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
use std::collections::HashMap;

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
    Var(String),
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
}

#[derive(Debug, Clone)]
pub enum TypedStmt {
    Let { name: String, ty: Type, value: TypedExpr },
    Print(TypedExpr),
    Return(TypedExpr),
    If {
        cond: TypedExpr,
        then_block: Vec<TypedStmt>,
        else_block: Option<Vec<TypedStmt>>,
    },
}

#[derive(Debug, Clone)]
pub struct TypedFn {
    pub name: String,
    pub params: Vec<(String, Type)>,
    pub ret: Type,
    pub body: Vec<TypedStmt>,
}

#[derive(Debug, Clone)]
pub struct TypedProgram {
    pub fns: Vec<TypedFn>,
    pub stmts: Vec<TypedStmt>,
}

pub struct TypeChecker {
    env: HashMap<String, Type>,
    /// function name -> (parameter types, return type); collected up front so
    /// functions may be defined in any order and call each other.
    fns: HashMap<String, (Vec<Type>, Type)>,
    /// return type of the function currently being checked, if any.
    current_ret: Option<Type>,
}

impl TypeChecker {
    pub fn new() -> Self {
        TypeChecker { env: HashMap::new(), fns: HashMap::new(), current_ret: None }
    }

    pub fn check_program(mut self, prog: &Program) -> Result<TypedProgram, String> {
        // Pass 1: collect every signature, so order of definition never matters.
        for f in &prog.fns {
            if self.fns.contains_key(&f.name) {
                return Err(format!("function `{}` is defined twice", f.name));
            }
            let param_tys = f.params.iter().map(|p| p.ty.clone()).collect();
            self.fns.insert(f.name.clone(), (param_tys, f.ret.clone()));
        }

        // Pass 2: check each function body.
        let mut fns = Vec::new();
        for f in &prog.fns {
            fns.push(self.check_fn(f)?);
        }

        // Pass 3: top-level statements (the implicit main).
        let mut stmts = Vec::new();
        for s in &prog.stmts {
            stmts.push(self.check_stmt(s)?);
        }
        Ok(TypedProgram { fns, stmts })
    }

    fn check_fn(&mut self, f: &FnDef) -> Result<TypedFn, String> {
        self.env.clear();
        let mut params = Vec::new();
        for p in &f.params {
            if self.env.insert(p.name.clone(), p.ty.clone()).is_some() {
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
            Stmt::Let { name, declared, value } => {
                let typed = self.check_expr(value, Some(declared))?;
                if &typed.ty != declared {
                    return Err(format!(
                        "type mismatch in `let {}`: declared {}, but expression has type {}",
                        name, declared, typed.ty
                    ));
                }
                self.env.insert(name.clone(), declared.clone());
                Ok(TypedStmt::Let { name: name.clone(), ty: declared.clone(), value: typed })
            }
            Stmt::Print(e) => {
                let typed = self.check_expr(e, None)?;
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

            Expr::DecimalLit { unscaled, scale } => {
                // Determine the target scale (and rounding contract) from
                // context if available. The contract never rounds the literal
                // itself — literals must be exactly representable.
                let (target_scale, rounding) = match expected {
                    Some(Type::Decimal { scale: s, rounding }) => (*s, *rounding),
                    // No decimal context: the literal's own scale is its type.
                    _ => (*scale, None),
                };
                let normalized = normalize_decimal(*unscaled, *scale, target_scale)?;
                Ok(TypedExpr {
                    ty: Type::Decimal { scale: target_scale, rounding },
                    kind: TypedExprKind::DecimalLit { unscaled: normalized },
                })
            }

            Expr::Var(name) => {
                let ty = self
                    .env
                    .get(name)
                    .ok_or_else(|| format!("unknown variable: {}", name))?
                    .clone();
                Ok(TypedExpr { ty, kind: TypedExprKind::Var(name.clone()) })
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
        }
    }

    /// Comparisons are always exact, and both sides must have the SAME type —
    /// comparing money of different scales (or contracts) is refused just like
    /// adding it would be.
    fn check_compare(&self, op: CmpOp, lhs: &Type, rhs: &Type) -> Result<(), String> {
        use Type::*;
        match (lhs, rhs) {
            (Int, Int) => Ok(()),
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
    if from_scale == to_scale {
        return Ok(unscaled);
    }
    if to_scale > from_scale {
        let factor = 10i64.pow(to_scale - from_scale);
        unscaled
            .checked_mul(factor)
            .ok_or_else(|| "decimal overflow while widening scale".to_string())
    } else {
        // narrowing: only ok if exactly divisible
        let factor = 10i64.pow(from_scale - to_scale);
        if unscaled % factor == 0 {
            Ok(unscaled / factor)
        } else {
            Err(format!(
                "literal has scale {} but context expects scale {}; \
                 narrowing would lose precision (refused).",
                from_scale, to_scale
            ))
        }
    }
}
