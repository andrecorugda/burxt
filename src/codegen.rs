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

use crate::ast::{BinOp, Type};
use crate::typeck::{TypedExpr, TypedExprKind, TypedProgram, TypedStmt};

use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::values::{BasicMetadataValueEnum, IntValue, PointerValue};
use inkwell::AddressSpace;
use std::collections::HashMap;

pub struct CodeGen<'ctx> {
    ctx: &'ctx Context,
    module: Module<'ctx>,
    builder: Builder<'ctx>,
    /// name -> (stack slot, type)
    vars: HashMap<String, (PointerValue<'ctx>, Type)>,
}

impl<'ctx> CodeGen<'ctx> {
    pub fn new(ctx: &'ctx Context, module_name: &str) -> Self {
        let module = ctx.create_module(module_name);
        let builder = ctx.create_builder();
        CodeGen { ctx, module, builder, vars: HashMap::new() }
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
            Type::Decimal { scale } => {
                // Split scaled value into integer and fractional parts, exactly.
                let i64t = self.ctx.i64_type();
                let pow = i64t.const_int(10u64.pow(*scale), false);

                let int_part = self
                    .builder
                    .build_int_signed_div(val, pow, "int_part")
                    .map_err(|e| e.to_string())?;
                let frac_part = self
                    .builder
                    .build_int_signed_rem(val, pow, "frac_part")
                    .map_err(|e| e.to_string())?;
                // fractional part must be non-negative for printing
                let frac_abs = self.build_abs(frac_part)?;

                // "%lld.%0<scale>lld\n" — zero-pad the fractional digits.
                let fmt_str = format!("%lld.%0{}lld\n", scale);
                let fmt = self.global_str(&fmt_str, "fmt_dec");

                let args: Vec<BasicMetadataValueEnum> =
                    vec![fmt.into(), int_part.into(), frac_abs.into()];
                self.builder
                    .build_call(printf, &args, "printf_dec")
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }

    fn gen_expr(&self, e: &TypedExpr) -> Result<IntValue<'ctx>, String> {
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
                // For our representation (scaled i64), Add/Sub/Mul map directly to
                // integer ops. Decimal<S> * Int works because the scaled value
                // times a plain count keeps the same scale.
                let res = match op {
                    BinOp::Add => self.builder.build_int_add(l, r, "add"),
                    BinOp::Sub => self.builder.build_int_sub(l, r, "sub"),
                    BinOp::Mul => self.builder.build_int_mul(l, r, "mul"),
                }
                .map_err(|e| e.to_string())?;
                Ok(res)
            }
        }
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
