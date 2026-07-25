//! Typechecker: the stage where Burxt's *thesis* is enforced.
//!
//! Rules for v0.0.1 (deliberately strict — correctness by construction):
//!   * Every `let` must declare a type, and the expression's inferred type must
//!     match it exactly. No implicit widening, no surprises.
//!   * `Int + Int = Int`, `Int * Int = Int`.
//!   * `Decimal<S> + Decimal<S> = Decimal<S>` — scales MUST match. Adding
//!     `Decimal<2>` to `Decimal<3>` is a compile error, not a silent coercion.
//!   * `Decimal<S> * Int = Decimal<S>` (scaling a money value by a count).
//!     This is the `price * qty` case. Multiplying two decimals (which would
//!     change the scale) is intentionally deferred — it needs a rounding
//!     contract, which is the next milestone.
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
    Var(String),
    Binary {
        op: BinOp,
        lhs: Box<TypedExpr>,
        rhs: Box<TypedExpr>,
    },
}

#[derive(Debug, Clone)]
pub enum TypedStmt {
    Let { name: String, ty: Type, value: TypedExpr },
    Print(TypedExpr),
}

#[derive(Debug, Clone)]
pub struct TypedProgram {
    pub stmts: Vec<TypedStmt>,
}

pub struct TypeChecker {
    env: HashMap<String, Type>,
}

impl TypeChecker {
    pub fn new() -> Self {
        TypeChecker { env: HashMap::new() }
    }

    pub fn check_program(mut self, prog: &Program) -> Result<TypedProgram, String> {
        let mut stmts = Vec::new();
        for s in &prog.stmts {
            stmts.push(self.check_stmt(s)?);
        }
        Ok(TypedProgram { stmts })
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
        }
    }

    /// `expected` is the type context (the declared type of the enclosing
    /// `let`), used to normalize decimal literals to the right scale.
    fn check_expr(&self, e: &Expr, expected: Option<&Type>) -> Result<TypedExpr, String> {
        match e {
            Expr::IntLit(n) => Ok(TypedExpr { ty: Type::Int, kind: TypedExprKind::IntLit(*n) }),

            Expr::DecimalLit { unscaled, scale } => {
                // Determine the target scale from context if available.
                let target_scale = match expected {
                    Some(Type::Decimal { scale: s }) => *s,
                    // No decimal context: the literal's own scale is its type.
                    _ => *scale,
                };
                let normalized = normalize_decimal(*unscaled, *scale, target_scale)?;
                Ok(TypedExpr {
                    ty: Type::Decimal { scale: target_scale },
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
        }
    }

    /// The core thesis rules for arithmetic result types.
    fn check_binop(&self, op: BinOp, lhs: &Type, rhs: &Type) -> Result<Type, String> {
        use Type::*;
        match (op, lhs, rhs) {
            // Integer arithmetic.
            (_, Int, Int) => Ok(Int),

            // Decimal +/- Decimal: scales MUST match exactly.
            (BinOp::Add, Decimal { scale: a }, Decimal { scale: b })
            | (BinOp::Sub, Decimal { scale: a }, Decimal { scale: b }) => {
                if a == b {
                    Ok(Decimal { scale: *a })
                } else {
                    Err(format!(
                        "cannot {} Decimal<{}> and Decimal<{}>: scales must match. \
                         Burxt does not silently rescale money.",
                        op, a, b
                    ))
                }
            }

            // Decimal * Int (or Int * Decimal): scale the money value by a count.
            // Result keeps the decimal's scale. This is `price * qty`.
            (BinOp::Mul, Decimal { scale }, Int) | (BinOp::Mul, Int, Decimal { scale }) => {
                Ok(Decimal { scale: *scale })
            }

            // Decimal * Decimal changes scale and needs a rounding contract.
            // Deferred to the next milestone — refuse rather than guess.
            (BinOp::Mul, Decimal { .. }, Decimal { .. }) => Err(
                "multiplying two Decimals changes the scale and requires an explicit \
                 rounding contract (coming in the next milestone). Not allowed yet."
                    .to_string(),
            ),

            // Decimal +/- Int and friends: refuse. No silent int->decimal.
            (_, a, b) => Err(format!(
                "type error: cannot apply `{}` to {} and {}",
                op, a, b
            )),
        }
    }
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
