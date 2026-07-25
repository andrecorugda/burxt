//! Codegen: typed AST -> LLVM IR -> native object file.
//!
//! This is the ONLY file that knows about LLVM. Swapping backends (Cranelift,
//! an interpreter) would replace only this file.
//!
//! Representation choices for v0.0.1:
//!   * `Int`         -> LLVM i64
//!   * `Decimal<S>`  -> LLVM i64 holding the *scaled* value (value * 10^S).
//!                      e.g. 19.99 as Decimal<2> is the i64 1999. This is exact:
//!                      no float ever appears in the generated program.
//!
//! Printing:
//!   * Int prints via printf("%lld\n", v).
//!   * Decimal<S> prints exactly by splitting the scaled integer into its
//!     integer part (v / 10^S) and fractional part (v % 10^S), then printing
//!     "%lld.%0*lld" so 5997 with scale 2 prints "59.97" — never a float.
//!
//! Rounding:
//!   * Operations that round (Decimal*Decimal, division) call a tiny generated
//!     helper `@burxt.round.<mode>(p, d)` = p/d rounded per the mode, built
//!     from sdiv/srem plus a tie adjustment. One helper per mode per module,
//!     created lazily — keeps expression codegen simple and the IR readable.
//!   * Division by zero traps at runtime (SIGFPE), like C. A checked story
//!     comes later; silently producing a wrong number is not an option.

use crate::ast::{BinOp, Rounding, Type};
use crate::typeck::{TypedExpr, TypedExprKind, TypedProgram, TypedStmt};

use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::values::{BasicMetadataValueEnum, FunctionValue, IntValue, PointerValue};
use inkwell::AddressSpace;
use std::collections::HashMap;

pub struct CodeGen<'ctx> {
    ctx: &'ctx Context,
    module: Module<'ctx>,
    builder: Builder<'ctx>,
    /// name -> (stack slot, type)
    vars: HashMap<String, (PointerValue<'ctx>, Type)>,
    /// lazily created rounding helpers, one per mode used by the program
    round_fns: HashMap<Rounding, FunctionValue<'ctx>>,
}

impl<'ctx> CodeGen<'ctx> {
    pub fn new(ctx: &'ctx Context, module_name: &str) -> Self {
        let module = ctx.create_module(module_name);
        let builder = ctx.create_builder();
        CodeGen { ctx, module, builder, vars: HashMap::new(), round_fns: HashMap::new() }
    }

    /// Emit the whole program into a `main` function returning 0.
    pub fn compile(&mut self, prog: &TypedProgram) -> Result<(), String> {
        let i64t = self.ctx.i64_type();
        let i32t = self.ctx.i32_type();

        // declare: i32 @printf(i8*, ...)
        let i8ptr = self.ctx.ptr_type(AddressSpace::default());
        let printf_ty = i32t.fn_type(&[i8ptr.into()], true);
        let printf = self.module.add_function("printf", printf_ty, None);

        // define: i32 @main()
        let main_ty = i32t.fn_type(&[], false);
        let main_fn = self.module.add_function("main", main_ty, None);
        let entry = self.ctx.append_basic_block(main_fn, "entry");
        self.builder.position_at_end(entry);

        for stmt in &prog.stmts {
            self.gen_stmt(stmt, printf)?;
        }

        // return 0  (main returns i32)
        let _ = i64t; // (i64 type used elsewhere; silence unused if it were)
        self.builder
            .build_return(Some(&i32t.const_int(0, false)))
            .map_err(|e| e.to_string())?;

        // verify the module — catches malformed IR early
        self.module
            .verify()
            .map_err(|e| format!("LLVM module verification failed:\n{}", e.to_string()))?;

        Ok(())
    }

    fn gen_stmt(
        &mut self,
        stmt: &TypedStmt,
        printf: inkwell::values::FunctionValue<'ctx>,
    ) -> Result<(), String> {
        match stmt {
            TypedStmt::Let { name, ty, value } => {
                let val = self.gen_expr(value)?;
                let slot = self
                    .builder
                    .build_alloca(self.ctx.i64_type(), name)
                    .map_err(|e| e.to_string())?;
                self.builder.build_store(slot, val).map_err(|e| e.to_string())?;
                self.vars.insert(name.clone(), (slot, ty.clone()));
                Ok(())
            }
            TypedStmt::Print(e) => self.gen_print(e, printf),
        }
    }

    fn gen_print(
        &mut self,
        e: &TypedExpr,
        printf: inkwell::values::FunctionValue<'ctx>,
    ) -> Result<(), String> {
        let val = self.gen_expr(e)?;
        match &e.ty {
            Type::Int => {
                let fmt = self.global_str("%lld\n", "fmt_int");
                self.builder
                    .build_call(printf, &[fmt.into(), val.into()], "printf_int")
                    .map_err(|e| e.to_string())?;
            }
            Type::Decimal { scale, .. } => {
                // Split |scaled value| into integer and fractional parts, exactly.
                // The sign is printed separately: deriving it from int_part alone
                // would drop it for values like -0.50, where int_part is 0.
                let i64t = self.ctx.i64_type();
                let pow = i64t.const_int(10u64.pow(*scale), false);

                let is_neg = self
                    .builder
                    .build_int_compare(inkwell::IntPredicate::SLT, val, i64t.const_zero(), "is_neg")
                    .map_err(|e| e.to_string())?;
                let abs = self.build_abs(val)?;
                let int_part = self
                    .builder
                    .build_int_unsigned_div(abs, pow, "int_part")
                    .map_err(|e| e.to_string())?;
                let frac_part = self
                    .builder
                    .build_int_unsigned_rem(abs, pow, "frac_part")
                    .map_err(|e| e.to_string())?;

                let minus = self.global_str("-", "str_minus");
                let empty = self.global_str("", "str_empty");
                let sign = self
                    .builder
                    .build_select(is_neg, minus, empty, "sign")
                    .map_err(|e| e.to_string())?;

                // "%s%lld.%0<scale>lld\n" — sign, then zero-padded fractional digits.
                let fmt_str = format!("%s%lld.%0{}lld\n", scale);
                let fmt = self.global_str(&fmt_str, "fmt_dec");

                let args: Vec<BasicMetadataValueEnum> =
                    vec![fmt.into(), sign.into(), int_part.into(), frac_part.into()];
                self.builder
                    .build_call(printf, &args, "printf_dec")
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }

    fn gen_expr(&mut self, e: &TypedExpr) -> Result<IntValue<'ctx>, String> {
        let i64t = self.ctx.i64_type();
        match &e.kind {
            TypedExprKind::IntLit(n) => Ok(i64t.const_int(*n as u64, true)),
            TypedExprKind::DecimalLit { unscaled } => Ok(i64t.const_int(*unscaled as u64, true)),
            TypedExprKind::Var(name) => {
                let (slot, _) = self
                    .vars
                    .get(name)
                    .ok_or_else(|| format!("codegen: unknown variable {}", name))?;
                let loaded = self
                    .builder
                    .build_load(i64t, *slot, name)
                    .map_err(|e| e.to_string())?;
                Ok(loaded.into_int_value())
            }
            TypedExprKind::Binary { op, lhs, rhs } => {
                let l = self.gen_expr(lhs)?;
                let r = self.gen_expr(rhs)?;
                // For our representation (scaled i64), Add/Sub map directly to
                // integer ops, and so does Mul when at most one operand is a
                // decimal (Decimal<S> * Int keeps the scale, exactly).
                // Decimal*Decimal and Div produce extra digits and go through
                // the rounding helper; typeck guarantees a contract is present.
                match op {
                    BinOp::Add => self.builder.build_int_add(l, r, "add").map_err(|e| e.to_string()),
                    BinOp::Sub => self.builder.build_int_sub(l, r, "sub").map_err(|e| e.to_string()),
                    BinOp::Mul => {
                        let both_decimal = matches!(lhs.ty, Type::Decimal { .. })
                            && matches!(rhs.ty, Type::Decimal { .. });
                        if both_decimal {
                            // (A * B) has scale 2S; divide by 10^S, rounding.
                            let raw = self
                                .builder
                                .build_int_mul(l, r, "mul_raw")
                                .map_err(|e| e.to_string())?;
                            let (scale, mode) = decimal_with_rounding(&e.ty)?;
                            let pow = i64t.const_int(10u64.pow(scale), false);
                            self.build_round_div(mode, raw, pow)
                        } else {
                            self.builder.build_int_mul(l, r, "mul").map_err(|e| e.to_string())
                        }
                    }
                    BinOp::Div => {
                        let (scale, mode) = decimal_with_rounding(&e.ty)?;
                        match rhs.ty {
                            // A/B has scale 0; pre-scale by 10^S: round(A*10^S / B).
                            Type::Decimal { .. } => {
                                let pow = i64t.const_int(10u64.pow(scale), false);
                                let scaled = self
                                    .builder
                                    .build_int_mul(l, pow, "div_prescale")
                                    .map_err(|e| e.to_string())?;
                                self.build_round_div(mode, scaled, r)
                            }
                            // A/n keeps scale S: round(A / n).
                            Type::Int => self.build_round_div(mode, l, r),
                        }
                    }
                }
            }
        }
    }

    /// Emit a call to `round(p / d)` under the given mode.
    fn build_round_div(
        &mut self,
        mode: Rounding,
        p: IntValue<'ctx>,
        d: IntValue<'ctx>,
    ) -> Result<IntValue<'ctx>, String> {
        let f = self.round_fn(mode)?;
        let call = self
            .builder
            .build_call(f, &[p.into(), d.into()], "round")
            .map_err(|e| e.to_string())?;
        match call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => Ok(v.into_int_value()),
            _ => Err("rounding helper returned void".to_string()),
        }
    }

    /// Get (or lazily define) the rounding helper for `mode`:
    ///   i64 @burxt.round.<mode>(i64 %p, i64 %d) = p/d rounded per the mode.
    ///
    /// Both modes start from truncating division (q = sdiv, r = srem) and then
    /// decide whether to bump q one step away from zero:
    ///   RoundHalfUp:   bump when 2|r| >= |d|            (ties away from zero)
    ///   RoundHalfEven: bump when 2|r| >  |d|, or on an
    ///                  exact tie (2|r| == |d|) when q is odd (ties to even)
    fn round_fn(&mut self, mode: Rounding) -> Result<FunctionValue<'ctx>, String> {
        if let Some(f) = self.round_fns.get(&mode) {
            return Ok(*f);
        }
        let saved_block = self.builder.get_insert_block();

        let i64t = self.ctx.i64_type();
        let fn_ty = i64t.fn_type(&[i64t.into(), i64t.into()], false);
        let name = match mode {
            Rounding::HalfEven => "burxt.round.half_even",
            Rounding::HalfUp => "burxt.round.half_up",
        };
        let f = self.module.add_function(name, fn_ty, None);
        let entry = self.ctx.append_basic_block(f, "entry");
        self.builder.position_at_end(entry);

        let p = f.get_nth_param(0).unwrap().into_int_value();
        let d = f.get_nth_param(1).unwrap().into_int_value();

        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let q = self.builder.build_int_signed_div(p, d, "q").map_err(err)?;
        let r = self.builder.build_int_signed_rem(p, d, "r").map_err(err)?;
        let abs_r = self.build_abs(r)?;
        let abs_d = self.build_abs(d)?;
        let two = i64t.const_int(2, false);
        let r2 = self.builder.build_int_mul(abs_r, two, "r2").map_err(err)?;

        use inkwell::IntPredicate::*;
        let need_bump = match mode {
            Rounding::HalfUp => self
                .builder
                .build_int_compare(SGE, r2, abs_d, "half_or_more")
                .map_err(err)?,
            Rounding::HalfEven => {
                let gt = self.builder.build_int_compare(SGT, r2, abs_d, "over_half").map_err(err)?;
                let eq = self.builder.build_int_compare(EQ, r2, abs_d, "exact_tie").map_err(err)?;
                let q_lsb = self
                    .builder
                    .build_and(q, i64t.const_int(1, false), "q_lsb")
                    .map_err(err)?;
                let q_odd = self
                    .builder
                    .build_int_compare(NE, q_lsb, i64t.const_zero(), "q_odd")
                    .map_err(err)?;
                let tie_to_even = self.builder.build_and(eq, q_odd, "tie_to_even").map_err(err)?;
                self.builder.build_or(gt, tie_to_even, "need_bump").map_err(err)?
            }
        };

        // "away from zero" = in the direction of the true quotient's sign,
        // which is sign(p) * sign(d).
        let p_neg = self.builder.build_int_compare(SLT, p, i64t.const_zero(), "p_neg").map_err(err)?;
        let d_neg = self.builder.build_int_compare(SLT, d, i64t.const_zero(), "d_neg").map_err(err)?;
        let opposite = self.builder.build_xor(p_neg, d_neg, "opposite_signs").map_err(err)?;
        let minus_one = i64t.const_int(u64::MAX, true); // -1
        let one = i64t.const_int(1, false);
        let bump = self
            .builder
            .build_select(opposite, minus_one, one, "bump_dir")
            .map_err(err)?
            .into_int_value();
        let delta = self
            .builder
            .build_select(need_bump, bump, i64t.const_zero(), "delta")
            .map_err(err)?
            .into_int_value();
        let rounded = self.builder.build_int_add(q, delta, "rounded").map_err(err)?;
        self.builder.build_return(Some(&rounded)).map_err(err)?;

        if let Some(b) = saved_block {
            self.builder.position_at_end(b);
        }
        self.round_fns.insert(mode, f);
        Ok(f)
    }

    /// abs(x) for i64 via select — keeps fractional digits positive when the
    /// whole value is negative.
    fn build_abs(&self, x: IntValue<'ctx>) -> Result<IntValue<'ctx>, String> {
        let i64t = self.ctx.i64_type();
        let zero = i64t.const_zero();
        let neg = self.builder.build_int_neg(x, "neg").map_err(|e| e.to_string())?;
        let is_neg = self
            .builder
            .build_int_compare(inkwell::IntPredicate::SLT, x, zero, "is_neg")
            .map_err(|e| e.to_string())?;
        let sel = self
            .builder
            .build_select(is_neg, neg, x, "abs")
            .map_err(|e| e.to_string())?;
        Ok(sel.into_int_value())
    }

    /// Create a global null-terminated string constant and return an i8* to it.
    fn global_str(&self, s: &str, name: &str) -> PointerValue<'ctx> {
        let gv = self
            .builder
            .build_global_string_ptr(s, name)
            .expect("global string");
        gv.as_pointer_value()
    }

    // (rounding helpers above; printing/IO below)

    /// Write the LLVM IR to a file (for inspection / debugging).
    pub fn write_ir(&self, path: &str) -> Result<(), String> {
        self.module.print_to_file(path).map_err(|e| e.to_string())
    }

    /// Emit a native object file using the host target machine.
    pub fn write_object(&self, path: &str) -> Result<(), String> {
        use inkwell::targets::{
            CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine,
        };
        use inkwell::OptimizationLevel;

        Target::initialize_native(&InitializationConfig::default())
            .map_err(|e| format!("failed to init native target: {}", e))?;

        let triple = TargetMachine::get_default_triple();
        let target = Target::from_triple(&triple).map_err(|e| e.to_string())?;
        let cpu = TargetMachine::get_host_cpu_name();
        let features = TargetMachine::get_host_cpu_features();
        let tm = target
            .create_target_machine(
                &triple,
                cpu.to_str().unwrap(),
                features.to_str().unwrap(),
                OptimizationLevel::Default,
                RelocMode::PIC,
                CodeModel::Default,
            )
            .ok_or("failed to create target machine")?;

        tm.write_to_file(&self.module, FileType::Object, std::path::Path::new(path))
            .map_err(|e| e.to_string())
    }
}

/// A rounding operation's result type must be a Decimal carrying a contract —
/// the typechecker guarantees this; violating it is a compiler bug.
fn decimal_with_rounding(ty: &Type) -> Result<(u32, Rounding), String> {
    match ty {
        Type::Decimal { scale, rounding: Some(mode) } => Ok((*scale, *mode)),
        other => Err(format!(
            "codegen bug: expected a Decimal with a rounding contract, got {}",
            other
        )),
    }
}
