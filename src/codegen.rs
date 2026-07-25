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
//!   * Those helpers work in **i128**, and so do the hidden intermediates
//!     (the double-scale product A*B and the pre-scaled dividend A*10^S).
//!     Values are i64; only intermediates need the extra headroom, and this
//!     is where the old compiler reported "overflow" for results that fit
//!     perfectly. The final narrowing back to i64 is checked, so the
//!     overflow error now fires only when the RESULT genuinely doesn't fit.
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
    /// extern name -> its declared C signature (for CInt width conversions)
    extern_sigs: HashMap<String, (Vec<Type>, Type)>,
    /// lazily created i64 -> i32 range-checked truncation helper
    cint_fn: Option<FunctionValue<'ctx>>,
    /// lazily created array bounds-check helper
    index_check_fn: Option<FunctionValue<'ctx>>,
    /// lazily created i128 -> i64 checked narrowing helper
    narrow_check_fn: Option<FunctionValue<'ctx>>,
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
            extern_sigs: HashMap::new(),
            cint_fn: None,
            index_check_fn: None,
            narrow_check_fn: None,
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
            self.extern_sigs.insert(e.name.clone(), (e.params.clone(), e.ret.clone()));
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
                // An array is built in place: alloca once, store per element.
                if let TypedExprKind::ArrayLit(elems) = &value.kind {
                    let slot = self.create_entry_alloca(name, ty)?;
                    let arr_ty = self.llvm_type(ty);
                    for (i, e) in elems.iter().enumerate() {
                        let v = self.gen_expr(e)?;
                        let idx = self.ctx.i64_type().const_int(i as u64, false);
                        let ptr = unsafe {
                            self.builder.build_in_bounds_gep(
                                arr_ty,
                                slot,
                                &[self.ctx.i64_type().const_zero(), idx],
                                "elem_init",
                            )
                        }
                        .map_err(|e| e.to_string())?;
                        self.builder.build_store(ptr, v).map_err(|e| e.to_string())?;
                    }
                    self.vars.insert(name.clone(), (slot, ty.clone()));
                    return Ok(());
                }
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
            TypedStmt::AssignIndex { name, len, index, value } => {
                let val = self.gen_expr(value)?;
                let ptr = self.gen_element_ptr(name, *len, index)?;
                self.builder.build_store(ptr, val).map_err(|e| e.to_string())?;
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
            Type::CInt => self.ctx.i32_type().into(),
            Type::Named(name) => self.struct_types[name].into(),
            Type::Array { elem, len } => self.llvm_type(elem).array_type(*len).into(),
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
            Type::Named(_) | Type::CInt | Type::Array { .. } => {
                return Err(format!(
                    "codegen bug: print on {} should have been refused by typeck",
                    e.ty
                ))
            }
            Type::Decimal { scale, .. } => {
                // Split |scaled value| into integer and fractional parts, exactly.
                // The sign is printed separately: deriving it from int_part alone
                // would drop it for values like -0.50, where int_part is 0.
                // The split happens in i128 so that the most negative
                // representable value stays printable (|i64::MIN| needs 64
                // unsigned bits), and the parts print with %llu — they are
                // magnitudes, never negative.
                let val = val.into_int_value();
                let i64t = self.ctx.i64_type();
                let i128t = self.ctx.i128_type();

                let is_neg = self
                    .builder
                    .build_int_compare(inkwell::IntPredicate::SLT, val, i64t.const_zero(), "is_neg")
                    .map_err(|e| e.to_string())?;
                let wide = self.widen(val)?;
                let abs = self.build_abs_wide(wide)?;

                let minus = self.global_str("-", "str_minus");
                let empty = self.global_str("", "str_empty");
                let sign = self
                    .builder
                    .build_select(is_neg, minus, empty, "sign")
                    .map_err(|e| e.to_string())?;

                let narrow = |b: &Builder<'ctx>, v: IntValue<'ctx>, n: &str| {
                    b.build_int_truncate(v, i64t, n).map_err(|e| e.to_string())
                };

                if *scale == 0 {
                    // Scale 0 has NO fractional digits — printing ".0" would
                    // show a digit that does not exist.
                    let int_part = narrow(&self.builder, abs, "int_part")?;
                    let fmt = self.global_str("%s%llu\n", "fmt_dec0");
                    let args: Vec<BasicMetadataValueEnum> =
                        vec![fmt.into(), sign.into(), int_part.into()];
                    self.builder
                        .build_call(printf, &args, "printf_dec")
                        .map_err(|e| e.to_string())?;
                } else {
                    let pow = self.pow10_i128(*scale);
                    let int_wide = self
                        .builder
                        .build_int_unsigned_div(abs, pow, "int_wide")
                        .map_err(|e| e.to_string())?;
                    let frac_wide = self
                        .builder
                        .build_int_unsigned_rem(abs, pow, "frac_wide")
                        .map_err(|e| e.to_string())?;
                    let int_part = narrow(&self.builder, int_wide, "int_part")?;
                    let frac_part = narrow(&self.builder, frac_wide, "frac_part")?;
                    let _ = i128t;

                    // "%s%llu.%0<scale>llu\n" — sign, then zero-padded digits.
                    let fmt_str = format!("%s%llu.%0{}llu\n", scale);
                    let fmt = self.global_str(&fmt_str, "fmt_dec");
                    let args: Vec<BasicMetadataValueEnum> =
                        vec![fmt.into(), sign.into(), int_part.into(), frac_part.into()];
                    self.builder
                        .build_call(printf, &args, "printf_dec")
                        .map_err(|e| e.to_string())?;
                }
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
            TypedExprKind::Neg(inner) => {
                // 0 - v, overflow-checked like any subtraction (there is no
                // negation of the most negative value).
                let v = self.gen_expr(inner)?.into_int_value();
                let zero = i64t.const_zero();
                self.build_checked(BinOp::Sub, zero, v).map(Into::into)
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
                            // Both the product and the division happen in i128,
                            // so a representable result is never refused.
                            let (scale, mode) = decimal_with_rounding(&e.ty)?;
                            let l128 = self.widen(l)?;
                            let r128 = self.widen(r)?;
                            let raw = self
                                .builder
                                .build_int_mul(l128, r128, "mul_raw")
                                .map_err(|e| e.to_string())?;
                            let pow = self.pow10_i128(scale);
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
                                let l128 = self.widen(l)?;
                                let pow = self.pow10_i128(scale);
                                let scaled = self
                                    .builder
                                    .build_int_mul(l128, pow, "div_prescale")
                                    .map_err(|e| e.to_string())?;
                                let r128 = self.widen(r)?;
                                self.build_round_div(mode, scaled, r128)
                            }
                            // A/n keeps scale S: round(A / n).
                            _ => {
                                let l128 = self.widen(l)?;
                                let r128 = self.widen(r)?;
                                self.build_round_div(mode, l128, r128)
                            }
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
                let extern_sig = self.extern_sigs.get(name).cloned();
                let mut vals: Vec<BasicMetadataValueEnum> = Vec::new();
                for (i, a) in args.iter().enumerate() {
                    let mut v = self.gen_expr(a)?;
                    // A CInt parameter is 32-bit on the C side: range-check
                    // and truncate — a value that doesn't fit is a loud
                    // runtime error, never a silent wrap.
                    if let Some((ptys, _)) = &extern_sig {
                        if ptys.get(i) == Some(&Type::CInt) {
                            v = self.build_to_cint(v.into_int_value())?.into();
                        }
                    }
                    vals.push(v.into());
                }
                let call = self
                    .builder
                    .build_call(f, &vals, "call")
                    .map_err(|e| e.to_string())?;
                let result = match call.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(v) => v,
                    _ => return Err(format!("codegen bug: call to {} returned void", name)),
                };
                // A CInt return is C's 32-bit int: sign-extend so the sign
                // survives into Burxt's Int (strcmp's -1 stays -1).
                if matches!(extern_sig, Some((_, Type::CInt))) {
                    return self
                        .builder
                        .build_int_s_extend(result.into_int_value(), i64t, "cint_ret")
                        .map(Into::into)
                        .map_err(|e| e.to_string());
                }
                Ok(result)
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
            TypedExprKind::ArrayLit(_) => {
                Err("codegen bug: array literal outside a let initializer".to_string())
            }
            TypedExprKind::Index { name, len, index } => {
                let (_, ty) = self
                    .vars
                    .get(name)
                    .ok_or_else(|| format!("codegen: unknown variable {}", name))?
                    .clone();
                let elem_ty = match &ty {
                    Type::Array { elem, .. } => self.llvm_type(elem),
                    other => return Err(format!("codegen bug: indexing a {}", other)),
                };
                let ptr = self.gen_element_ptr(name, *len, index)?;
                self.builder
                    .build_load(elem_ty, ptr, "elem")
                    .map_err(|e| e.to_string())
            }
        }
    }

    /// Bounds-check `index` against `len`, then GEP to the element. Every
    /// indexed access — read or write — goes through here.
    fn gen_element_ptr(
        &mut self,
        name: &str,
        len: u32,
        index: &TypedExpr,
    ) -> Result<PointerValue<'ctx>, String> {
        let (slot, ty) = self
            .vars
            .get(name)
            .ok_or_else(|| format!("codegen: unknown variable {}", name))?
            .clone();
        let i64t = self.ctx.i64_type();
        let idx_val = self.gen_expr(index)?.into_int_value();
        let n = i64t.const_int(len as u64, false);
        let checked = self.build_checked_index(idx_val, n)?;
        let arr_ty = self.llvm_type(&ty);
        unsafe {
            self.builder.build_in_bounds_gep(
                arr_ty,
                slot,
                &[i64t.const_zero(), checked],
                "elem_ptr",
            )
        }
        .map_err(|e| e.to_string())
    }

    /// Declare (once) the libc pieces every runtime error needs.
    fn panic_deps(
        &mut self,
    ) -> (
        inkwell::values::GlobalValue<'ctx>,
        FunctionValue<'ctx>,
        FunctionValue<'ctx>,
    ) {
        match self.panic_deps {
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
        }
    }

    /// Load the current stderr FILE* (a libc global).
    fn load_stderr(
        &mut self,
        stderr_g: inkwell::values::GlobalValue<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        self.builder
            .build_load(
                self.ctx.ptr_type(AddressSpace::default()),
                stderr_g.as_pointer_value(),
                "stderr",
            )
            .map_err(|e| e.to_string())
    }

    /// Emit `exit(70); unreachable` — the tail of every runtime error.
    fn build_exit70(&mut self, exit: FunctionValue<'ctx>) -> Result<(), String> {
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let code = self.ctx.i32_type().const_int(70, false);
        self.builder.build_call(exit, &[code.into()], "exit").map_err(err)?;
        self.builder.build_unreachable().map_err(err)?;
        Ok(())
    }

    /// Emit the runtime-error tail: fputs(msg, stderr); exit(70); unreachable.
    /// The builder must be positioned inside the (never-returning) panic path.
    fn build_panic(&mut self, msg: &str) -> Result<(), String> {
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let (stderr_g, fputs, exit) = self.panic_deps();
        let msg_ptr = self.global_str(msg, "panic_msg");
        let stream = self.load_stderr(stderr_g)?;
        self.builder
            .build_call(fputs, &[msg_ptr.into(), stream.into()], "fputs")
            .map_err(err)?;
        self.build_exit70(exit)
    }

    /// Emit a call to `checked_index(i, n)` — returns i when 0 <= i < n,
    /// otherwise dies with a message that NAMES the offending index and the
    /// valid range (advice, not just an alarm).
    fn build_checked_index(
        &mut self,
        i: IntValue<'ctx>,
        n: IntValue<'ctx>,
    ) -> Result<IntValue<'ctx>, String> {
        let f = self.index_fn()?;
        let call = self
            .builder
            .build_call(f, &[i.into(), n.into()], "checked_index")
            .map_err(|e| e.to_string())?;
        match call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => Ok(v.into_int_value()),
            _ => Err("index helper returned void".to_string()),
        }
    }

    /// Get (or lazily define) `i64 @burxt.checked.index(i64 %i, i64 %n)`.
    fn index_fn(&mut self) -> Result<FunctionValue<'ctx>, String> {
        if let Some(f) = self.index_check_fn {
            return Ok(f);
        }
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let saved_block = self.builder.get_insert_block();

        let i64t = self.ctx.i64_type();
        let ptr = self.ctx.ptr_type(AddressSpace::default());
        let i32t = self.ctx.i32_type();
        // i32 @fprintf(ptr, ptr, ...)
        let fprintf_ty = i32t.fn_type(&[ptr.into(), ptr.into()], true);
        let fprintf = self.module.add_function("fprintf", fprintf_ty, None);
        let (stderr_g, _, exit) = self.panic_deps();

        let fn_ty = i64t.fn_type(&[i64t.into(), i64t.into()], false);
        let f = self.module.add_function("burxt.checked.index", fn_ty, None);
        let entry = self.ctx.append_basic_block(f, "entry");
        let oob_bb = self.ctx.append_basic_block(f, "out_of_bounds");
        let ok_bb = self.ctx.append_basic_block(f, "ok");

        self.builder.position_at_end(entry);
        let i = f.get_nth_param(0).unwrap().into_int_value();
        let n = f.get_nth_param(1).unwrap().into_int_value();
        use inkwell::IntPredicate::*;
        let neg = self.builder.build_int_compare(SLT, i, i64t.const_zero(), "neg").map_err(err)?;
        let too_big = self.builder.build_int_compare(SGE, i, n, "too_big").map_err(err)?;
        let oob = self.builder.build_or(neg, too_big, "oob").map_err(err)?;
        self.builder.build_conditional_branch(oob, oob_bb, ok_bb).map_err(err)?;

        self.builder.position_at_end(oob_bb);
        let fmt = self.global_str(
            "burxt runtime error: index %lld is out of bounds — this array holds \
             %lld values (valid indexes 0 to %lld)\n",
            "fmt_oob",
        );
        let stream = self.load_stderr(stderr_g)?;
        let n_minus_1 = self
            .builder
            .build_int_sub(n, i64t.const_int(1, false), "n_minus_1")
            .map_err(err)?;
        let args: Vec<BasicMetadataValueEnum> =
            vec![stream.into(), fmt.into(), i.into(), n.into(), n_minus_1.into()];
        self.builder.build_call(fprintf, &args, "fprintf").map_err(err)?;
        self.build_exit70(exit)?;

        self.builder.position_at_end(ok_bb);
        self.builder.build_return(Some(&i)).map_err(err)?;

        if let Some(bb) = saved_block {
            self.builder.position_at_end(bb);
        }
        self.index_check_fn = Some(f);
        Ok(f)
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

    /// Emit a call to the range-checked i64 -> i32 truncation used for CInt
    /// extern parameters.
    fn build_to_cint(&mut self, v: IntValue<'ctx>) -> Result<IntValue<'ctx>, String> {
        let f = self.to_cint_fn()?;
        let call = self
            .builder
            .build_call(f, &[v.into()], "to_cint")
            .map_err(|e| e.to_string())?;
        match call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => Ok(v.into_int_value()),
            _ => Err("CInt helper returned void".to_string()),
        }
    }

    /// Get (or lazily define) `i32 @burxt.checked.cint(i64)`: returns the
    /// value as C's 32-bit int, or panics if it doesn't fit — passing a
    /// silently wrapped number to C is still a silently wrong number.
    fn to_cint_fn(&mut self) -> Result<FunctionValue<'ctx>, String> {
        if let Some(f) = self.cint_fn {
            return Ok(f);
        }
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let saved_block = self.builder.get_insert_block();

        let i64t = self.ctx.i64_type();
        let i32t = self.ctx.i32_type();
        let fn_ty = i32t.fn_type(&[i64t.into()], false);
        let f = self.module.add_function("burxt.checked.cint", fn_ty, None);
        let entry = self.ctx.append_basic_block(f, "entry");
        let panic_bb = self.ctx.append_basic_block(f, "doesnt_fit");
        let ok_bb = self.ctx.append_basic_block(f, "ok");

        self.builder.position_at_end(entry);
        let v = f.get_nth_param(0).unwrap().into_int_value();
        use inkwell::IntPredicate::*;
        let max = i64t.const_int(i32::MAX as u64, true);
        let min = i64t.const_int(i32::MIN as i64 as u64, true);
        let too_big = self.builder.build_int_compare(SGT, v, max, "too_big").map_err(err)?;
        let too_small = self.builder.build_int_compare(SLT, v, min, "too_small").map_err(err)?;
        let out = self.builder.build_or(too_big, too_small, "out_of_range").map_err(err)?;
        self.builder.build_conditional_branch(out, panic_bb, ok_bb).map_err(err)?;

        self.builder.position_at_end(panic_bb);
        self.build_panic(
            "burxt runtime error: this value does not fit in a C int — the extern \
             parameter is 32-bit\n",
        )?;

        self.builder.position_at_end(ok_bb);
        let truncated = self.builder.build_int_truncate(v, i32t, "cint").map_err(err)?;
        self.builder.build_return(Some(&truncated)).map_err(err)?;

        if let Some(bb) = saved_block {
            self.builder.position_at_end(bb);
        }
        self.cint_fn = Some(f);
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
        let wide = match call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
            _ => return Err("rounding helper returned void".to_string()),
        };
        self.build_narrow_to_i64(wide)
    }

    /// Sign-extend an i64 value to i128 for intermediate arithmetic.
    fn widen(&self, v: IntValue<'ctx>) -> Result<IntValue<'ctx>, String> {
        self.builder
            .build_int_s_extend(v, self.ctx.i128_type(), "widen")
            .map_err(|e| e.to_string())
    }

    /// 10^scale as an i128 constant (scale <= 18, so this always fits).
    fn pow10_i128(&self, scale: u32) -> IntValue<'ctx> {
        self.ctx
            .i128_type()
            .const_int_arbitrary_precision(&pow10_words(scale))
    }

    /// Narrow i128 -> i64, dying loudly if the value does not fit. This is the
    /// ONLY place the overflow error can now come from for decimal
    /// multiplication and division — so when it fires, the RESULT really
    /// doesn't fit, not merely an intermediate.
    fn build_narrow_to_i64(&mut self, v: IntValue<'ctx>) -> Result<IntValue<'ctx>, String> {
        let f = self.narrow_fn()?;
        let call = self
            .builder
            .build_call(f, &[v.into()], "narrow")
            .map_err(|e| e.to_string())?;
        match call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => Ok(v.into_int_value()),
            _ => Err("narrowing helper returned void".to_string()),
        }
    }

    /// Get (or lazily define) `i64 @burxt.checked.narrow(i128)`.
    fn narrow_fn(&mut self) -> Result<FunctionValue<'ctx>, String> {
        if let Some(f) = self.narrow_check_fn {
            return Ok(f);
        }
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let saved_block = self.builder.get_insert_block();

        let i64t = self.ctx.i64_type();
        let i128t = self.ctx.i128_type();
        let fn_ty = i64t.fn_type(&[i128t.into()], false);
        let f = self.module.add_function("burxt.checked.narrow", fn_ty, None);
        let entry = self.ctx.append_basic_block(f, "entry");
        let panic_bb = self.ctx.append_basic_block(f, "doesnt_fit");
        let ok_bb = self.ctx.append_basic_block(f, "ok");

        self.builder.position_at_end(entry);
        let v = f.get_nth_param(0).unwrap().into_int_value();
        // Round-trip through i64: if truncating then sign-extending changes
        // the value, it never fitted.
        let trunc = self.builder.build_int_truncate(v, i64t, "trunc").map_err(err)?;
        let back = self.builder.build_int_s_extend(trunc, i128t, "back").map_err(err)?;
        let fits = self
            .builder
            .build_int_compare(inkwell::IntPredicate::EQ, v, back, "fits")
            .map_err(err)?;
        self.builder.build_conditional_branch(fits, ok_bb, panic_bb).map_err(err)?;

        self.builder.position_at_end(panic_bb);
        self.build_panic(
            "burxt runtime error: arithmetic overflow — the exact result no \
             longer fits in the value range\n",
        )?;

        self.builder.position_at_end(ok_bb);
        self.builder.build_return(Some(&trunc)).map_err(err)?;

        if let Some(bb) = saved_block {
            self.builder.position_at_end(bb);
        }
        self.narrow_check_fn = Some(f);
        Ok(f)
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

        // The helper works in i128: the caller's operands are widened i64s or
        // a pre-scaled product, so no intermediate here can overflow.
        let i128t = self.ctx.i128_type();
        let fn_ty = i128t.fn_type(&[i128t.into(), i128t.into()], false);
        let name = match mode {
            Rounding::HalfEven => "burxt.round.half_even",
            Rounding::HalfUp => "burxt.round.half_up",
        };
        let f = self.module.add_function(name, fn_ty, None);
        let entry = self.ctx.append_basic_block(f, "entry");
        let div0_bb = self.ctx.append_basic_block(f, "div_by_zero");
        let main_bb = self.ctx.append_basic_block(f, "main");
        self.builder.position_at_end(entry);

        let p = f.get_nth_param(0).unwrap().into_int_value();
        let d = f.get_nth_param(1).unwrap().into_int_value();

        let err = |e: inkwell::builder::BuilderError| e.to_string();

        // The only division that cannot produce a value is d == 0 (i128 has
        // room for every quotient of our i64-derived operands). It becomes a
        // named runtime error instead of a raw SIGFPE.
        let is_zero = self
            .builder
            .build_int_compare(inkwell::IntPredicate::EQ, d, i128t.const_zero(), "d_is_zero")
            .map_err(err)?;
        self.builder.build_conditional_branch(is_zero, div0_bb, main_bb).map_err(err)?;

        self.builder.position_at_end(div0_bb);
        self.build_panic("burxt runtime error: division by zero\n")?;

        self.builder.position_at_end(main_bb);
        let q = self.builder.build_int_signed_div(p, d, "q").map_err(err)?;
        let r = self.builder.build_int_signed_rem(p, d, "r").map_err(err)?;
        let abs_r = self.build_abs_wide(r)?;
        let abs_d = self.build_abs_wide(d)?;
        let two = i128t.const_int(2, false);
        // In i128 the tie test cannot overflow: |r| < |d| <= 10^36 or so.
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
                    .build_and(q, i128t.const_int(1, false), "q_lsb")
                    .map_err(err)?;
                let q_odd = self
                    .builder
                    .build_int_compare(NE, q_lsb, i128t.const_zero(), "q_odd")
                    .map_err(err)?;
                let tie_to_even = self.builder.build_and(eq, q_odd, "tie_to_even").map_err(err)?;
                self.builder.build_or(gt, tie_to_even, "need_bump").map_err(err)?
            }
        };

        // "away from zero" = in the direction of the true quotient's sign,
        // which is sign(p) * sign(d).
        let p_neg = self.builder.build_int_compare(SLT, p, i128t.const_zero(), "p_neg").map_err(err)?;
        let d_neg = self.builder.build_int_compare(SLT, d, i128t.const_zero(), "d_neg").map_err(err)?;
        let opposite = self.builder.build_xor(p_neg, d_neg, "opposite_signs").map_err(err)?;
        let minus_one = i128t.const_all_ones(); // -1
        let one = i128t.const_int(1, false);
        let bump = self
            .builder
            .build_select(opposite, minus_one, one, "bump_dir")
            .map_err(err)?
            .into_int_value();
        let delta = self
            .builder
            .build_select(need_bump, bump, i128t.const_zero(), "delta")
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

    /// abs(x) for a WIDE (i128) value. No overflow check is needed: every
    /// value reaching here came from i64s, so its negation always fits.
    fn build_abs_wide(&mut self, x: IntValue<'ctx>) -> Result<IntValue<'ctx>, String> {
        let zero = x.get_type().const_zero();
        let neg = self.builder.build_int_neg(x, "neg").map_err(|e| e.to_string())?;
        let is_neg = self
            .builder
            .build_int_compare(inkwell::IntPredicate::SLT, x, zero, "is_neg")
            .map_err(|e| e.to_string())?;
        Ok(self
            .builder
            .build_select(is_neg, neg, x, "abs")
            .map_err(|e| e.to_string())?
            .into_int_value())
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

/// 10^scale as the 64-bit words LLVM wants for an i128 constant.
/// scale is capped at 18, so the value always fits in the low word.
fn pow10_words(scale: u32) -> [u64; 2] {
    [10u64.pow(scale), 0]
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
