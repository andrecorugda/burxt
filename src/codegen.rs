//! Codegen: typed AST -> LLVM IR -> native object file.
//!
//! This is the ONLY file that knows about LLVM. Swapping backends (Cranelift,
//! an interpreter) would replace only this file.
//!
//! Representation choices:
//!   * `Int`         -> LLVM i64
//!   * `Bool`        -> LLVM i64 holding 0 or 1 (i1 only transiently, at
//!                      comparisons and branches).
//!   * `Decimal<S>`  -> LLVM i64 holding the *scaled* value (value * 10^S).
//!                      e.g. 19.99 as Decimal<2> is the i64 1999. This is exact:
//!                      no float ever appears in the generated program.
//!   * `String`      -> LLVM opaque `ptr` to an immutable NUL-terminated byte
//!                      array in .rodata. Never ptrtoint'ed into an integer —
//!                      the target decides pointer width (wasm32 is coming).
//!   * user `fn f`   -> LLVM function `bx.f` (the prefix keeps user names from
//!                      ever colliding with libc symbols like printf/main).
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
//!
//! Runtime checks (silently wrong numbers are never an option):
//!   * Every +, -, * goes through `@burxt.checked.<op>`, built on LLVM's
//!     `llvm.s{add,sub,mul}.with.overflow` intrinsics. On overflow the program
//!     prints a runtime error to stderr and exits with code 70 — a loud stop
//!     instead of a silently wrapped money value.
//!   * Division checks for a zero divisor (and the i64::MIN / -1 edge) the
//!     same way, so you get a named error rather than a raw SIGFPE.

use crate::ast::{BinOp, CmpOp, Rounding, Type};
use crate::typeck::{TypedExpr, TypedExprKind, TypedFn, TypedProgram, TypedStmt};
use inkwell::types::StructType;

use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum};
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, FunctionValue, IntValue, PointerValue};
use inkwell::AddressSpace;
use std::collections::HashMap;

pub struct CodeGen<'ctx> {
    ctx: &'ctx Context,
    module: Module<'ctx>,
    builder: Builder<'ctx>,
    /// name -> (stack slot, type); reset per function
    vars: HashMap<String, (PointerValue<'ctx>, Type)>,
    /// lazily created rounding helpers, one per mode used by the program
    round_fns: HashMap<Rounding, FunctionValue<'ctx>>,
    /// lazily created overflow-checked arithmetic helpers, one per operator
    checked_fns: HashMap<BinOp, FunctionValue<'ctx>>,
    /// user function name -> its LLVM function (mangled `bx.<name>`)
    user_fns: HashMap<String, FunctionValue<'ctx>>,
    /// struct name -> its LLVM struct type (named `bx.<name>`)
    struct_types: HashMap<String, StructType<'ctx>>,
    /// struct name -> field types in declaration order (for GEP walks)
    struct_fields: HashMap<String, Vec<Type>>,
    /// libc printf, declared once in compile()
    printf: Option<FunctionValue<'ctx>>,
    /// libc pieces for runtime errors: (stderr global, fputs, exit)
    panic_deps: Option<(
        inkwell::values::GlobalValue<'ctx>,
        FunctionValue<'ctx>,
        FunctionValue<'ctx>,
    )>,
}

impl<'ctx> CodeGen<'ctx> {
    pub fn new(ctx: &'ctx Context, module_name: &str) -> Self {
        let module = ctx.create_module(module_name);
        let builder = ctx.create_builder();
        CodeGen {
            ctx,
            module,
            builder,
            vars: HashMap::new(),
            round_fns: HashMap::new(),
            checked_fns: HashMap::new(),
            user_fns: HashMap::new(),
            struct_types: HashMap::new(),
            struct_fields: HashMap::new(),
            printf: None,
            panic_deps: None,
        }
    }

    /// Emit the whole program: user functions first, then the top-level
    /// statements as `main`.
    pub fn compile(&mut self, prog: &TypedProgram) -> Result<(), String> {
        let i32t = self.ctx.i32_type();

        // declare: i32 @printf(i8*, ...)
        let i8ptr = self.ctx.ptr_type(AddressSpace::default());
        let printf_ty = i32t.fn_type(&[i8ptr.into()], true);
        self.printf = Some(self.module.add_function("printf", printf_ty, None));

        // Create all struct types: opaque shells first so nested references
        // resolve in any order, then fill in the bodies.
        for s in &prog.structs {
            let st = self.ctx.opaque_struct_type(&format!("bx.{}", s.name));
            self.struct_types.insert(s.name.clone(), st);
            self.struct_fields.insert(s.name.clone(), s.fields.clone());
        }
        for s in &prog.structs {
            let body: Vec<BasicTypeEnum> =
                s.fields.iter().map(|t| self.llvm_type(t)).collect();
            self.struct_types[&s.name].set_body(&body, false);
        }

        // Declare extern fns under their real symbol names — no mangling is
        // the whole point of FFI. (Int -> i64, String -> const char*.)
        for e in &prog.externs {
            let param_tys: Vec<BasicMetadataTypeEnum> =
                e.params.iter().map(|t| self.llvm_type(t).into()).collect();
            let fn_ty = self.llvm_type(&e.ret).fn_type(&param_tys, false);
            let llf = self.module.add_function(&e.name, fn_ty, None);
            self.user_fns.insert(e.name.clone(), llf);
        }

        // Declare every user function up front (mutual recursion, any order).
        for f in &prog.fns {
            let param_tys: Vec<BasicMetadataTypeEnum> =
                f.params.iter().map(|(_, t)| self.llvm_type(t).into()).collect();
            let fn_ty = self.llvm_type(&f.ret).fn_type(&param_tys, false);
            let llf = self.module.add_function(&format!("bx.{}", f.name), fn_ty, None);
            self.user_fns.insert(f.name.clone(), llf);
        }

        // Define their bodies.
        for f in &prog.fns {
            self.gen_fn(f)?;
        }

        // define: i32 @main()
        let main_ty = i32t.fn_type(&[], false);
        let main_fn = self.module.add_function("main", main_ty, None);
        let entry = self.ctx.append_basic_block(main_fn, "entry");
        self.builder.position_at_end(entry);
        self.vars.clear();

        for stmt in &prog.stmts {
            self.gen_stmt(stmt)?;
        }

        // return 0  (main returns i32)
        self.builder
            .build_return(Some(&i32t.const_int(0, false)))
            .map_err(|e| e.to_string())?;

        // verify the module — catches malformed IR early
        self.module
            .verify()
            .map_err(|e| format!("LLVM module verification failed:\n{}", e.to_string()))?;

        Ok(())
    }

    fn gen_fn(&mut self, f: &TypedFn) -> Result<(), String> {
        let llf = self.user_fns[&f.name];
        let entry = self.ctx.append_basic_block(llf, "entry");
        self.builder.position_at_end(entry);
        self.vars.clear();

        // Spill each parameter to a stack slot so it behaves like any binding.
        for (i, (name, ty)) in f.params.iter().enumerate() {
            let slot = self.create_entry_alloca(name, ty)?;
            let arg = llf.get_nth_param(i as u32).unwrap();
            self.builder.build_store(slot, arg).map_err(|e| e.to_string())?;
            self.vars.insert(name.clone(), (slot, ty.clone()));
        }

        for stmt in &f.body {
            self.gen_stmt(stmt)?;
        }
        // The typechecker proved every path ends in `return`, so the current
        // block is already terminated — no fallthrough ret is needed.
        Ok(())
    }

    fn gen_stmt(&mut self, stmt: &TypedStmt) -> Result<(), String> {
        match stmt {
            TypedStmt::Let { name, ty, value } => {
                let val = self.gen_expr(value)?;
                let slot = self.create_entry_alloca(name, ty)?;
                self.builder.build_store(slot, val).map_err(|e| e.to_string())?;
                self.vars.insert(name.clone(), (slot, ty.clone()));
                Ok(())
            }
            TypedStmt::Assign { name, value } => {
                let val = self.gen_expr(value)?;
                let (slot, _) = *self
                    .vars
                    .get(name)
                    .ok_or_else(|| format!("codegen: unknown variable {}", name))?;
                self.builder.build_store(slot, val).map_err(|e| e.to_string())?;
                Ok(())
            }
            TypedStmt::AssignField { name, indices, value } => {
                let val = self.gen_expr(value)?;
                let (slot, ty) = self
                    .vars
                    .get(name)
                    .ok_or_else(|| format!("codegen: unknown variable {}", name))?
                    .clone();
                // Walk the field path with struct GEPs, tracking the type at
                // each hop (typeck resolved the indices).
                let mut cur_ptr = slot;
                let mut cur_ty = ty;
                for &idx in indices {
                    let sname = match &cur_ty {
                        Type::Named(n) => n.clone(),
                        other => {
                            return Err(format!(
                                "codegen bug: field assignment through non-struct {}",
                                other
                            ))
                        }
                    };
                    let st = self.struct_types[&sname];
                    cur_ptr = self
                        .builder
                        .build_struct_gep(st, cur_ptr, idx, "fieldptr")
                        .map_err(|e| e.to_string())?;
                    cur_ty = self.struct_fields[&sname][idx as usize].clone();
                }
                self.builder.build_store(cur_ptr, val).map_err(|e| e.to_string())?;
                Ok(())
            }
            TypedStmt::While { cond, body } => self.gen_while(cond, body),
            TypedStmt::Print(e) => self.gen_print(e),
            TypedStmt::Return(e) => {
                let val = self.gen_expr(e)?;
                self.builder.build_return(Some(&val)).map_err(|e| e.to_string())?;
                Ok(())
            }
            TypedStmt::If { cond, then_block, else_block } => {
                self.gen_if(cond, then_block, else_block.as_deref())
            }
        }
    }

    fn gen_if(
        &mut self,
        cond: &TypedExpr,
        then_block: &[TypedStmt],
        else_block: Option<&[TypedStmt]>,
    ) -> Result<(), String> {
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let i64t = self.ctx.i64_type();

        let cond_val = self.gen_expr(cond)?.into_int_value();
        let cond_i1 = self
            .builder
            .build_int_compare(inkwell::IntPredicate::NE, cond_val, i64t.const_zero(), "ifcond")
            .map_err(err)?;

        let function = self
            .builder
            .get_insert_block()
            .and_then(|b| b.get_parent())
            .ok_or("codegen bug: `if` outside a function")?;
        let then_bb = self.ctx.append_basic_block(function, "then");
        let else_bb = else_block.map(|_| self.ctx.append_basic_block(function, "else"));
        let merge_bb = self.ctx.append_basic_block(function, "endif");

        self.builder
            .build_conditional_branch(cond_i1, then_bb, else_bb.unwrap_or(merge_bb))
            .map_err(err)?;

        // A branch "falls through" unless every path in it returned.
        let mut any_fallthrough = else_bb.is_none();

        self.builder.position_at_end(then_bb);
        self.gen_block(then_block)?;
        if self.current_block_open() {
            self.builder.build_unconditional_branch(merge_bb).map_err(err)?;
            any_fallthrough = true;
        }

        if let (Some(else_bb), Some(else_stmts)) = (else_bb, else_block) {
            self.builder.position_at_end(else_bb);
            self.gen_block(else_stmts)?;
            if self.current_block_open() {
                self.builder.build_unconditional_branch(merge_bb).map_err(err)?;
                any_fallthrough = true;
            }
        }

        self.builder.position_at_end(merge_bb);
        if !any_fallthrough {
            // Both branches returned; nothing ever reaches here (and the
            // typechecker refuses statements after such an `if`).
            self.builder.build_unreachable().map_err(err)?;
        }
        Ok(())
    }

    fn gen_while(&mut self, cond: &TypedExpr, body: &[TypedStmt]) -> Result<(), String> {
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let function = self
            .builder
            .get_insert_block()
            .and_then(|b| b.get_parent())
            .ok_or("codegen bug: `while` outside a function")?;
        let cond_bb = self.ctx.append_basic_block(function, "while.cond");
        let body_bb = self.ctx.append_basic_block(function, "while.body");
        let end_bb = self.ctx.append_basic_block(function, "while.end");

        self.builder.build_unconditional_branch(cond_bb).map_err(err)?;

        self.builder.position_at_end(cond_bb);
        let cond_val = self.gen_expr(cond)?.into_int_value();
        let cond_i1 = self
            .builder
            .build_int_compare(
                inkwell::IntPredicate::NE,
                cond_val,
                self.ctx.i64_type().const_zero(),
                "whilecond",
            )
            .map_err(err)?;
        self.builder
            .build_conditional_branch(cond_i1, body_bb, end_bb)
            .map_err(err)?;

        self.builder.position_at_end(body_bb);
        self.gen_block(body)?;
        if self.current_block_open() {
            self.builder.build_unconditional_branch(cond_bb).map_err(err)?;
        }

        self.builder.position_at_end(end_bb);
        Ok(())
    }

    /// The LLVM type for a Burxt type. All scalars are i64; String is an
    /// opaque pointer — the TARGET decides pointer width, never this code.
    fn llvm_type(&self, ty: &Type) -> BasicTypeEnum<'ctx> {
        match ty {
            Type::Int | Type::Bool | Type::Decimal { .. } => self.ctx.i64_type().into(),
            Type::String => self.ctx.ptr_type(AddressSpace::default()).into(),
            Type::Named(name) => self.struct_types[name].into(),
        }
    }

    /// Put every alloca in the function's ENTRY block, not wherever the
    /// builder happens to be: an alloca inside a loop body would otherwise
    /// grow the stack on every iteration.
    fn create_entry_alloca(&self, name: &str, ty: &Type) -> Result<PointerValue<'ctx>, String> {
        let function = self
            .builder
            .get_insert_block()
            .and_then(|b| b.get_parent())
            .ok_or("codegen bug: alloca outside a function")?;
        let entry = function
            .get_first_basic_block()
            .ok_or("codegen bug: function has no entry block")?;
        let tmp = self.ctx.create_builder();
        match entry.get_first_instruction() {
            Some(first) => tmp.position_before(&first),
            None => tmp.position_at_end(entry),
        }
        tmp.build_alloca(self.llvm_type(ty), name).map_err(|e| e.to_string())
    }

    /// Generate a block's statements in a child scope, mirroring the
    /// typechecker: bindings made inside vanish at the closing brace.
    fn gen_block(&mut self, stmts: &[TypedStmt]) -> Result<(), String> {
        let saved = self.vars.clone();
        let result = stmts.iter().try_for_each(|s| self.gen_stmt(s));
        self.vars = saved;
        result
    }

    /// Is the builder's current block still missing a terminator?
    fn current_block_open(&self) -> bool {
        self.builder
            .get_insert_block()
            .is_some_and(|b| b.get_terminator().is_none())
    }

    fn gen_print(&mut self, e: &TypedExpr) -> Result<(), String> {
        let printf = self.printf.ok_or("codegen bug: printf not declared")?;
        let val = self.gen_expr(e)?;
        match &e.ty {
            Type::Int => {
                let fmt = self.global_str("%lld\n", "fmt_int");
                self.builder
                    .build_call(printf, &[fmt.into(), val.into()], "printf_int")
                    .map_err(|e| e.to_string())?;
            }
            Type::String => {
                // User bytes are always an ARGUMENT, never the format string.
                let fmt = self.global_str("%s\n", "fmt_str");
                self.builder
                    .build_call(printf, &[fmt.into(), val.into()], "printf_str")
                    .map_err(|e| e.to_string())?;
            }
            Type::Bool => {
                let is_true = self
                    .builder
                    .build_int_compare(
                        inkwell::IntPredicate::NE,
                        val.into_int_value(),
                        self.ctx.i64_type().const_zero(),
                        "is_true",
                    )
                    .map_err(|e| e.to_string())?;
                let t = self.global_str("true\n", "str_true");
                let f = self.global_str("false\n", "str_false");
                let s = self
                    .builder
                    .build_select(is_true, t, f, "bool_str")
                    .map_err(|e| e.to_string())?;
                let fmt = self.global_str("%s", "fmt_bool");
                let args: Vec<BasicMetadataValueEnum> = vec![fmt.into(), s.into()];
                self.builder
                    .build_call(printf, &args, "printf_bool")
                    .map_err(|e| e.to_string())?;
            }
            Type::Named(n) => {
                return Err(format!(
                    "codegen bug: print on struct {} should have been refused by typeck",
                    n
                ))
            }
            Type::Decimal { scale, .. } => {
                // Split |scaled value| into integer and fractional parts, exactly.
                // The sign is printed separately: deriving it from int_part alone
                // would drop it for values like -0.50, where int_part is 0.
                let val = val.into_int_value();
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

    fn gen_expr(&mut self, e: &TypedExpr) -> Result<BasicValueEnum<'ctx>, String> {
        let i64t = self.ctx.i64_type();
        match &e.kind {
            TypedExprKind::IntLit(n) => Ok(i64t.const_int(*n as u64, true).into()),
            TypedExprKind::DecimalLit { unscaled } => {
                Ok(i64t.const_int(*unscaled as u64, true).into())
            }
            TypedExprKind::BoolLit(b) => Ok(i64t.const_int(*b as u64, false).into()),
            TypedExprKind::StrLit(s) => Ok(self.global_str(s, "str").into()),
            TypedExprKind::Var(name) => {
                let (slot, ty) = self
                    .vars
                    .get(name)
                    .ok_or_else(|| format!("codegen: unknown variable {}", name))?
                    .clone();
                self.builder
                    .build_load(self.llvm_type(&ty), slot, name)
                    .map_err(|e| e.to_string())
            }
            TypedExprKind::Binary { op, lhs, rhs } => {
                let l = self.gen_expr(lhs)?.into_int_value();
                let r = self.gen_expr(rhs)?.into_int_value();
                // For our representation (scaled i64), Add/Sub map directly to
                // integer ops, and so does Mul when at most one operand is a
                // decimal (Decimal<S> * Int keeps the scale, exactly).
                // Decimal*Decimal and Div produce extra digits and go through
                // the rounding helper; typeck guarantees a contract is present.
                let res = match op {
                    BinOp::Add | BinOp::Sub => self.build_checked(*op, l, r),
                    BinOp::Mul => {
                        let both_decimal = matches!(lhs.ty, Type::Decimal { .. })
                            && matches!(rhs.ty, Type::Decimal { .. });
                        if both_decimal {
                            // (A * B) has scale 2S; divide by 10^S, rounding.
                            let raw = self.build_checked(BinOp::Mul, l, r)?;
                            let (scale, mode) = decimal_with_rounding(&e.ty)?;
                            let pow = i64t.const_int(10u64.pow(scale), false);
                            self.build_round_div(mode, raw, pow)
                        } else {
                            self.build_checked(BinOp::Mul, l, r)
                        }
                    }
                    BinOp::Div => {
                        let (scale, mode) = decimal_with_rounding(&e.ty)?;
                        match rhs.ty {
                            // A/B has scale 0; pre-scale by 10^S: round(A*10^S / B).
                            Type::Decimal { .. } => {
                                let pow = i64t.const_int(10u64.pow(scale), false);
                                let scaled = self.build_checked(BinOp::Mul, l, pow)?;
                                self.build_round_div(mode, scaled, r)
                            }
                            // A/n keeps scale S: round(A / n).
                            _ => self.build_round_div(mode, l, r),
                        }
                    }
                }?;
                Ok(res.into())
            }
            TypedExprKind::Compare { op, lhs, rhs } => {
                let l = self.gen_expr(lhs)?.into_int_value();
                let r = self.gen_expr(rhs)?.into_int_value();
                // Scaled decimals of equal scale compare exactly as plain
                // integers — no rescaling, no rounding, no float.
                use inkwell::IntPredicate::*;
                let pred = match op {
                    CmpOp::Eq => EQ,
                    CmpOp::Ne => NE,
                    CmpOp::Lt => SLT,
                    CmpOp::Le => SLE,
                    CmpOp::Gt => SGT,
                    CmpOp::Ge => SGE,
                };
                let bit = self
                    .builder
                    .build_int_compare(pred, l, r, "cmp")
                    .map_err(|e| e.to_string())?;
                self.builder
                    .build_int_z_extend(bit, i64t, "cmp_i64")
                    .map(Into::into)
                    .map_err(|e| e.to_string())
            }
            TypedExprKind::Call { name, args } => {
                let f = *self
                    .user_fns
                    .get(name)
                    .ok_or_else(|| format!("codegen bug: unknown function {}", name))?;
                let mut vals: Vec<BasicMetadataValueEnum> = Vec::new();
                for a in args {
                    vals.push(self.gen_expr(a)?.into());
                }
                let call = self
                    .builder
                    .build_call(f, &vals, "call")
                    .map_err(|e| e.to_string())?;
                match call.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(v) => Ok(v),
                    _ => Err(format!("codegen bug: call to {} returned void", name)),
                }
            }
            TypedExprKind::StructLit { name, fields } => {
                // Build the aggregate value field by field; storing it (in
                // Let/Assign) is one whole-struct store — value semantics.
                let st = self.struct_types[name.as_str()];
                let mut agg = st.get_undef();
                for (i, f) in fields.iter().enumerate() {
                    let v = self.gen_expr(f)?;
                    agg = self
                        .builder
                        .build_insert_value(agg, v, i as u32, "field")
                        .map_err(|e| e.to_string())?
                        .into_struct_value();
                }
                Ok(agg.into())
            }
            TypedExprKind::Field { base, index } => {
                let agg = self.gen_expr(base)?.into_struct_value();
                self.builder
                    .build_extract_value(agg, *index, "field")
                    .map_err(|e| e.to_string())
            }
        }
    }

    /// Emit the runtime-error tail: fputs(msg, stderr); exit(70); unreachable.
    /// The builder must be positioned inside the (never-returning) panic path.
    fn build_panic(&mut self, msg: &str) -> Result<(), String> {
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let (stderr_g, fputs, exit) = match self.panic_deps {
            Some(deps) => deps,
            None => {
                let ptr = self.ctx.ptr_type(AddressSpace::default());
                let i32t = self.ctx.i32_type();
                let stderr_g = self.module.add_global(ptr, None, "stderr");
                let fputs_ty = i32t.fn_type(&[ptr.into(), ptr.into()], false);
                let fputs = self.module.add_function("fputs", fputs_ty, None);
                let exit_ty = self.ctx.void_type().fn_type(&[i32t.into()], false);
                let exit = self.module.add_function("exit", exit_ty, None);
                *self.panic_deps.insert((stderr_g, fputs, exit))
            }
        };
        let msg_ptr = self.global_str(msg, "panic_msg");
        let stream = self
            .builder
            .build_load(
                self.ctx.ptr_type(AddressSpace::default()),
                stderr_g.as_pointer_value(),
                "stderr",
            )
            .map_err(err)?;
        self.builder
            .build_call(fputs, &[msg_ptr.into(), stream.into()], "fputs")
            .map_err(err)?;
        let code = self.ctx.i32_type().const_int(70, false);
        self.builder.build_call(exit, &[code.into()], "exit").map_err(err)?;
        self.builder.build_unreachable().map_err(err)?;
        Ok(())
    }

    /// Emit a call to `checked_op(a, b)` — the overflow-trapping version of
    /// +, - or *.
    fn build_checked(
        &mut self,
        op: BinOp,
        a: IntValue<'ctx>,
        b: IntValue<'ctx>,
    ) -> Result<IntValue<'ctx>, String> {
        let f = self.checked_fn(op)?;
        let call = self
            .builder
            .build_call(f, &[a.into(), b.into()], "checked")
            .map_err(|e| e.to_string())?;
        match call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => Ok(v.into_int_value()),
            _ => Err("checked-arithmetic helper returned void".to_string()),
        }
    }

    /// Get (or lazily define) `i64 @burxt.checked.<op>(i64, i64)`: performs the
    /// operation via LLVM's overflow-reporting intrinsic and panics on overflow
    /// instead of wrapping — a money value must never silently corrupt.
    fn checked_fn(&mut self, op: BinOp) -> Result<FunctionValue<'ctx>, String> {
        if let Some(f) = self.checked_fns.get(&op) {
            return Ok(*f);
        }
        let (intrinsic_name, fn_name) = match op {
            BinOp::Add => ("llvm.sadd.with.overflow", "burxt.checked.add"),
            BinOp::Sub => ("llvm.ssub.with.overflow", "burxt.checked.sub"),
            BinOp::Mul => ("llvm.smul.with.overflow", "burxt.checked.mul"),
            BinOp::Div => return Err("checked_fn: division is handled by round_fn".to_string()),
        };
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let saved_block = self.builder.get_insert_block();

        let i64t = self.ctx.i64_type();
        let intrinsic = inkwell::intrinsics::Intrinsic::find(intrinsic_name)
            .ok_or_else(|| format!("LLVM intrinsic {} not found", intrinsic_name))?;
        let intr_fn = intrinsic
            .get_declaration(&self.module, &[i64t.into()])
            .ok_or_else(|| format!("cannot declare {}", intrinsic_name))?;

        let fn_ty = i64t.fn_type(&[i64t.into(), i64t.into()], false);
        let f = self.module.add_function(fn_name, fn_ty, None);
        let entry = self.ctx.append_basic_block(f, "entry");
        let panic_bb = self.ctx.append_basic_block(f, "overflow");
        let ok_bb = self.ctx.append_basic_block(f, "ok");

        self.builder.position_at_end(entry);
        let a = f.get_nth_param(0).unwrap().into();
        let b = f.get_nth_param(1).unwrap().into();
        let call = self
            .builder
            .build_call(intr_fn, &[a, b], "op")
            .map_err(err)?;
        let pair = match call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => v.into_struct_value(),
            _ => return Err("overflow intrinsic returned void".to_string()),
        };
        let value = self
            .builder
            .build_extract_value(pair, 0, "value")
            .map_err(err)?
            .into_int_value();
        let overflowed = self
            .builder
            .build_extract_value(pair, 1, "overflowed")
            .map_err(err)?
            .into_int_value();
        self.builder
            .build_conditional_branch(overflowed, panic_bb, ok_bb)
            .map_err(err)?;

        self.builder.position_at_end(panic_bb);
        self.build_panic(
            "burxt runtime error: arithmetic overflow — the exact result no \
             longer fits in the value range\n",
        )?;

        self.builder.position_at_end(ok_bb);
        self.builder.build_return(Some(&value)).map_err(err)?;

        if let Some(bb) = saved_block {
            self.builder.position_at_end(bb);
        }
        self.checked_fns.insert(op, f);
        Ok(f)
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
        let div0_bb = self.ctx.append_basic_block(f, "div_by_zero");
        let ovf_bb = self.ctx.append_basic_block(f, "quot_overflow");
        let main_bb = self.ctx.append_basic_block(f, "main");
        self.builder.position_at_end(entry);

        let p = f.get_nth_param(0).unwrap().into_int_value();
        let d = f.get_nth_param(1).unwrap().into_int_value();

        let err = |e: inkwell::builder::BuilderError| e.to_string();

        // Guard the two divisions that cannot produce a value: d == 0, and
        // the lone overflowing quotient i64::MIN / -1. Both become a named
        // runtime error instead of a raw SIGFPE.
        let is_zero = self
            .builder
            .build_int_compare(inkwell::IntPredicate::EQ, d, i64t.const_zero(), "d_is_zero")
            .map_err(err)?;
        let min = i64t.const_int(i64::MIN as u64, true);
        let minus_one_c = i64t.const_int(u64::MAX, true);
        let p_min = self
            .builder
            .build_int_compare(inkwell::IntPredicate::EQ, p, min, "p_is_min")
            .map_err(err)?;
        let d_m1 = self
            .builder
            .build_int_compare(inkwell::IntPredicate::EQ, d, minus_one_c, "d_is_minus_one")
            .map_err(err)?;
        let ovf = self.builder.build_and(p_min, d_m1, "quot_overflows").map_err(err)?;
        let cont_bb = self.ctx.append_basic_block(f, "nonzero");
        self.builder.build_conditional_branch(is_zero, div0_bb, cont_bb).map_err(err)?;
        self.builder.position_at_end(cont_bb);
        self.builder.build_conditional_branch(ovf, ovf_bb, main_bb).map_err(err)?;

        self.builder.position_at_end(div0_bb);
        self.build_panic("burxt runtime error: division by zero\n")?;
        self.builder.position_at_end(ovf_bb);
        self.build_panic(
            "burxt runtime error: arithmetic overflow — the exact result no \
             longer fits in the value range\n",
        )?;

        self.builder.position_at_end(main_bb);
        let q = self.builder.build_int_signed_div(p, d, "q").map_err(err)?;
        let r = self.builder.build_int_signed_rem(p, d, "r").map_err(err)?;
        let abs_r = self.build_abs(r)?;
        let abs_d = self.build_abs(d)?;
        let two = i64t.const_int(2, false);
        let r2 = self.build_checked(BinOp::Mul, abs_r, two)?;

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
    /// whole value is negative. The negation is overflow-checked: abs(i64::MIN)
    /// does not exist, and pretending it does would print a wrong number.
    fn build_abs(&mut self, x: IntValue<'ctx>) -> Result<IntValue<'ctx>, String> {
        let i64t = self.ctx.i64_type();
        let zero = i64t.const_zero();
        let neg = self.build_checked(BinOp::Sub, zero, x)?;
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
