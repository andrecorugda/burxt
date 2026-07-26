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

use crate::ast::{BinOp, CmpOp, LogicalOp, Rounding, Type};
use crate::typeck::{TypedExpr, TypedExprKind, TypedFn, TypedMethod, TypedProgram, TypedStmt};
use inkwell::types::StructType;

use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::types::{AnyType, BasicMetadataTypeEnum, BasicType, BasicTypeEnum};
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
    /// lazily created string byte-scan helpers
    str_len_fn: Option<FunctionValue<'ctx>>,
    /// (heap base, bump cursor) globals for region allocation
    heap: Option<(
        inkwell::values::GlobalValue<'ctx>,
        inkwell::values::GlobalValue<'ctx>,
    )>,
    alloc_fn: Option<FunctionValue<'ctx>>,
    byte_index_check_fn: Option<FunctionValue<'ctx>>,
    str_eq_fn: Option<FunctionValue<'ctx>>,
    /// user fn name -> (param types, return type), for aggregate call lowering
    fn_sigs: HashMap<String, (Vec<Type>, Type)>,
    /// (receiver, method name) -> its LLVM function (mangled `bx.<Recv>.<method>`)
    methods: HashMap<(String, String), FunctionValue<'ctx>>,
    /// (trait, concrete) -> its static vtable global
    vtables: HashMap<(String, String), inkwell::values::GlobalValue<'ctx>>,
    /// trait name -> method return types in slot order, for indirect calls
    trait_slots: HashMap<String, Vec<(Vec<Type>, Type)>>,
    /// the hidden sret pointer of the function being generated, if it returns
    /// an aggregate
    current_sret: Option<PointerValue<'ctx>>,
    /// struct name -> its LLVM struct type (named `bx.<name>`)
    struct_types: HashMap<String, StructType<'ctx>>,
    /// struct name -> field types in declaration order (for GEP walks)
    struct_fields: HashMap<String, Vec<Type>>,
    /// enum name -> (its LLVM `{ i64 tag, [N x i64] payload }` type,
    /// payload types per variant in tag order)
    enum_types: HashMap<String, (StructType<'ctx>, Vec<Vec<Type>>)>,
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
            str_len_fn: None,
            heap: None,
            alloc_fn: None,
            byte_index_check_fn: None,
            str_eq_fn: None,
            fn_sigs: HashMap::new(),
            methods: HashMap::new(),
            vtables: HashMap::new(),
            trait_slots: HashMap::new(),
            current_sret: None,
            struct_types: HashMap::new(),
            struct_fields: HashMap::new(),
            enum_types: HashMap::new(),
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

        // Enums are a tag plus an inline payload area: `{ i64, [N x i64] }`,
        // where N is the largest payload slot count. Every payload field is 8
        // bytes (payloads are scalars), so slots need no size computation.
        for en in &prog.enums {
            let i64t = self.ctx.i64_type();
            let slots = en.variants.iter().map(|p| p.len()).max().unwrap_or(0) as u32;
            let st = self.ctx.opaque_struct_type(&format!("bx.enum.{}", en.name));
            if slots == 0 {
                st.set_body(&[i64t.into()], false);
            } else {
                st.set_body(&[i64t.into(), i64t.array_type(slots).into()], false);
            }
            self.enum_types
                .insert(en.name.clone(), (st, en.variants.clone()));
        }

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
            let param_tys: Vec<Type> = f.params.iter().map(|(_, t)| t.clone()).collect();
            let llf = self.declare_fn(&format!("bx.{}", f.name), &param_tys, &f.ret);
            self.user_fns.insert(f.name.clone(), llf);
            self.fn_sigs.insert(f.name.clone(), (param_tys, f.ret.clone()));
        }

        // Methods: namespaced by (receiver, name), mangled `bx.<Recv>.<name>`.
        for m in &prog.methods {
            let param_tys: Vec<Type> = m.params.iter().map(|(_, t)| t.clone()).collect();
            let mangled = format!("bx.{}.{}", m.receiver, m.name);
            let llf = self.declare_method(
                &mangled,
                &Type::Named(m.receiver.clone()),
                m.receiver_mut,
                &param_tys,
                &m.ret,
            );
            self.methods.insert((m.receiver.clone(), m.name.clone()), llf);
        }

        // Vtables: static, read-only tables of function pointers in
        // trait-declaration slot order, one per (Type, Trait) used as `dyn`.
        // Shared by every trait object of that pair — the instance carries
        // only two words. Emitted BEFORE any body, since a body may build a
        // trait object or dispatch through one.
        for vt in &prog.vtables {
            let ptr = self.ctx.ptr_type(AddressSpace::default());
            let fns: Vec<PointerValue> = vt
                .slots
                .iter()
                .map(|m| {
                    self.methods[&(vt.concrete.clone(), m.clone())]
                        .as_global_value()
                        .as_pointer_value()
                })
                .collect();
            let table_ty = ptr.array_type(fns.len() as u32);
            let global = self.module.add_global(
                table_ty,
                None,
                &format!("bx.vtable.{}.{}", vt.trait_name, vt.concrete),
            );
            global.set_initializer(&ptr.const_array(&fns));
            global.set_constant(true);
            self.vtables
                .insert((vt.trait_name.clone(), vt.concrete.clone()), global);

            // Record each slot's signature once, so an indirect call can build
            // the right function type. Every impl of a trait matches the
            // trait's signatures exactly, so the first one speaks for all.
            self.trait_slots.entry(vt.trait_name.clone()).or_insert_with(|| {
                vt.slots
                    .iter()
                    .map(|m| {
                        let f = prog
                            .methods
                            .iter()
                            .find(|tm| tm.receiver == vt.concrete && tm.name == *m)
                            .expect("vtable slot method must exist");
                        (
                            f.params.iter().map(|(_, t)| t.clone()).collect::<Vec<_>>(),
                            f.ret.clone(),
                        )
                    })
                    .collect()
            });
        }

        // Now the bodies, with every declaration and vtable already in place.
        for f in &prog.fns {
            self.gen_fn(f)?;
        }
        for m in &prog.methods {
            self.gen_method(m)?;
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

        // An aggregate return arrives as a hidden sret pointer in slot 0.
        let ret_is_agg = is_aggregate(&f.ret);
        self.current_sret = if ret_is_agg {
            Some(llf.get_nth_param(0).unwrap().into_pointer_value())
        } else {
            None
        };
        let offset = if ret_is_agg { 1 } else { 0 };

        for (i, (name, ty)) in f.params.iter().enumerate() {
            let arg = llf.get_nth_param((i + offset) as u32).unwrap();
            if is_aggregate(ty) {
                // byval already gave us a pointer to our OWN copy, so it is
                // the variable's slot directly — no second copy needed, and
                // writing through it cannot touch the caller's value.
                self.vars.insert(name.clone(), (arg.into_pointer_value(), ty.clone()));
            } else {
                // Spill scalars so they behave like any other binding.
                let slot = self.create_entry_alloca(name, ty)?;
                self.builder.build_store(slot, arg).map_err(|e| e.to_string())?;
                self.vars.insert(name.clone(), (slot, ty.clone()));
            }
        }

        for stmt in &f.body {
            self.gen_stmt(stmt)?;
        }
        self.current_sret = None;
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
                    self.store_array_elements(slot, ty, elems)?;
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
            TypedStmt::AssignFieldIndex { name, indices, len, index, value } => {
                let val = self.gen_expr(value)?;
                let (slot, ty) = self
                    .vars
                    .get(name)
                    .ok_or_else(|| format!("codegen: unknown variable {}", name))?
                    .clone();
                // Walk the field path to the array, then bounds-check and store.
                let mut cur_ptr = slot;
                let mut cur_ty = ty;
                for &idx in indices {
                    let sname = match &cur_ty {
                        Type::Named(n) => n.clone(),
                        other => {
                            return Err(format!(
                                "codegen bug: field path through non-struct {}",
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
                let i64t = self.ctx.i64_type();
                let idx_val = self.gen_expr(index)?.into_int_value();
                let n = i64t.const_int(*len as u64, false);
                let checked = self.build_checked_index(idx_val, n)?;
                let arr_ty = self.llvm_type(&cur_ty);
                let elem_ptr = unsafe {
                    self.builder.build_in_bounds_gep(
                        arr_ty,
                        cur_ptr,
                        &[i64t.const_zero(), checked],
                        "elem_ptr",
                    )
                }
                .map_err(|e| e.to_string())?;
                self.builder.build_store(elem_ptr, val).map_err(|e| e.to_string())?;
                Ok(())
            }
            TypedStmt::AssignIndex { name, len, index, value } => {
                let val = self.gen_expr(value)?;
                let ptr = self.gen_element_ptr(name, *len, index)?;
                self.builder.build_store(ptr, val).map_err(|e| e.to_string())?;
                Ok(())
            }
            TypedStmt::ExprStmt(e) => {
                self.gen_expr(e)?;
                Ok(())
            }
            TypedStmt::Region { name, body } => {
                // Mark where the bump pointer stands, run the body, then reset
                // to the mark — the whole region released in O(1), with no
                // per-object free, no refcount, and no collector.
                let _ = name;
                let mark = self.build_region_open()?;
                let saved = self.vars.clone();
                let r = body.iter().try_for_each(|s| self.gen_stmt(s));
                self.vars = saved;
                r?;
                if self.current_block_open() {
                    self.build_region_close(mark)?;
                }
                Ok(())
            }
            TypedStmt::Match { value, arms } => {
                let err = |e: inkwell::builder::BuilderError| e.to_string();
                let i64t = self.ctx.i64_type();
                let enum_name = match &value.ty {
                    Type::Named(n) => n.clone(),
                    other => {
                        return Err(format!("codegen bug: match on {}", other))
                    }
                };
                let (st, variants) = self.enum_types[enum_name.as_str()].clone();
                let slots =
                    variants.iter().map(|p| p.len()).max().unwrap_or(0) as u32;

                // The value needs an address so payload slots can be read out.
                let slot = self.gen_aggregate_addr(value)?;
                let tag_ptr =
                    self.builder.build_struct_gep(st, slot, 0, "tag_ptr").map_err(err)?;
                let tag = self
                    .builder
                    .build_load(i64t, tag_ptr, "tag")
                    .map_err(err)?
                    .into_int_value();

                let function = self
                    .builder
                    .get_insert_block()
                    .and_then(|b| b.get_parent())
                    .ok_or("codegen bug: match outside a function")?;
                let merge_bb = self.ctx.append_basic_block(function, "match.end");

                let mut cases = Vec::new();
                let mut bodies = Vec::new();
                for arm in arms {
                    let bb = self
                        .ctx
                        .append_basic_block(function, &format!("match.arm{}", arm.tag));
                    cases.push((i64t.const_int(arm.tag as u64, false), bb));
                    bodies.push((arm, bb));
                }

                // Exhaustiveness was proven by typeck, so no case can be
                // missing — but LLVM still needs a default block, and the only
                // honest one is unreachable.
                let default_bb = self.ctx.append_basic_block(function, "match.impossible");
                self.builder.build_switch(tag, default_bb, &cases).map_err(err)?;
                self.builder.position_at_end(default_bb);
                self.builder.build_unreachable().map_err(err)?;

                let mut any_fallthrough = false;
                for (arm, bb) in bodies {
                    self.builder.position_at_end(bb);
                    let saved = self.vars.clone();
                    if !arm.bindings.is_empty() {
                        let payload_ptr = self
                            .builder
                            .build_struct_gep(st, slot, 1, "payload_ptr")
                            .map_err(err)?;
                        let arr_ty = i64t.array_type(slots);
                        for (i, (name, ty)) in arm.bindings.iter().enumerate() {
                            let p = unsafe {
                                self.builder.build_in_bounds_gep(
                                    arr_ty,
                                    payload_ptr,
                                    &[i64t.const_zero(), i64t.const_int(i as u64, false)],
                                    "slot",
                                )
                            }
                            .map_err(err)?;
                            // The slot is 8 bytes; load it at the binding's own
                            // type so a String comes back as a pointer.
                            let v = self
                                .builder
                                .build_load(self.llvm_type(ty), p, name)
                                .map_err(err)?;
                            let local = self.create_entry_alloca(name, ty)?;
                            self.builder.build_store(local, v).map_err(err)?;
                            self.vars.insert(name.clone(), (local, ty.clone()));
                        }
                    }
                    let r = arm.body.iter().try_for_each(|s| self.gen_stmt(s));
                    self.vars = saved;
                    r?;
                    if self.current_block_open() {
                        self.builder.build_unconditional_branch(merge_bb).map_err(err)?;
                        any_fallthrough = true;
                    }
                }

                self.builder.position_at_end(merge_bb);
                if !any_fallthrough {
                    // every arm returned, so nothing reaches here
                    self.builder.build_unreachable().map_err(err)?;
                }
                Ok(())
            }
            TypedStmt::While { cond, body } => self.gen_while(cond, body),
            TypedStmt::Print(e) => self.gen_print(e),
            TypedStmt::PrintInterp(parts) => {
                // Emit each piece in order — no intermediate String is built,
                // so this needs no allocation.
                let printf = self.printf.ok_or("codegen bug: printf not declared")?;
                for p in parts {
                    match p {
                        crate::typeck::TypedInterpPart::Lit(text) => {
                            // Literal text is an ARGUMENT to %s, never the
                            // format string — a `%` in it must stay harmless.
                            let s = self.global_str(text, "interp_lit");
                            let fmt = self.global_str("%s", "fmt_interp");
                            self.builder
                                .build_call(printf, &[fmt.into(), s.into()], "printf_lit")
                                .map_err(|e| e.to_string())?;
                        }
                        crate::typeck::TypedInterpPart::Expr(e) => self.gen_print_value(e)?,
                    }
                }
                self.gen_newline()
            }
            TypedStmt::Return(e) => {
                if let Some(sret) = self.current_sret {
                    // Build the result directly in the caller's space, then
                    // return nothing: no aliasing question, no copy-elision
                    // subtlety.
                    let val = self.gen_expr(e)?;
                    self.builder.build_store(sret, val).map_err(|e| e.to_string())?;
                    self.builder.build_return(None).map_err(|e| e.to_string())?;
                } else {
                    let val = self.gen_expr(e)?;
                    self.builder.build_return(Some(&val)).map_err(|e| e.to_string())?;
                }
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

    /// Declare a function under the aggregate ABI:
    ///   * scalars pass and return in registers, as before;
    ///   * aggregate PARAMETERS pass as `byval(T)` — a pointer to a
    ///     caller-owned copy, so the callee can never alias caller storage
    ///     (LLVM guarantees the copy; hand-rolling it is where the aliasing
    ///     bugs live);
    ///   * aggregate RETURNS use an `sret(T)` hidden first pointer — one code
    ///     path on every target, and the only shape wasm can express.
    fn declare_fn(&self, name: &str, params: &[Type], ret: &Type) -> FunctionValue<'ctx> {
        let ptr = self.ctx.ptr_type(AddressSpace::default());
        let mut ll_params: Vec<BasicMetadataTypeEnum> = Vec::new();
        let ret_is_agg = is_aggregate(ret);
        if ret_is_agg {
            ll_params.push(ptr.into());
        }
        for p in params {
            if is_aggregate(p) {
                ll_params.push(ptr.into());
            } else {
                ll_params.push(self.llvm_type(p).into());
            }
        }
        let fn_ty = if ret_is_agg {
            self.ctx.void_type().fn_type(&ll_params, false)
        } else {
            self.llvm_type(ret).fn_type(&ll_params, false)
        };
        let f = self.module.add_function(name, fn_ty, None);

        // Attach the attributes that carry the ABI contract.
        use inkwell::attributes::AttributeLoc;
        if ret_is_agg {
            let attr = self.ctx.create_type_attribute(
                inkwell::attributes::Attribute::get_named_enum_kind_id("sret"),
                self.llvm_type(ret).as_any_type_enum(),
            );
            f.add_attribute(AttributeLoc::Param(0), attr);
        }
        let offset = if ret_is_agg { 1 } else { 0 };
        for (i, p) in params.iter().enumerate() {
            if is_aggregate(p) {
                let attr = self.ctx.create_type_attribute(
                    inkwell::attributes::Attribute::get_named_enum_kind_id("byval"),
                    self.llvm_type(p).as_any_type_enum(),
                );
                f.add_attribute(AttributeLoc::Param((i + offset) as u32), attr);
            }
        }
        f
    }

    /// Declare a method. Identical to `declare_fn` except for the receiver:
    /// `self` is always passed as a PLAIN pointer, never `byval`.
    ///
    /// Why this is sound, and why it has to be this way:
    ///   * A non-mutating `self` is read-only — the typechecker refuses
    ///     `self.field = ...` without `mut self` — so a pointer to the
    ///     caller's storage is indistinguishable from a pointer to a copy.
    ///     The A4.5 rule is that the mechanism must be invisible to the
    ///     semantics, and for a receiver nobody can write to, it is.
    ///   * A `mut self` receiver must be the caller's real storage anyway.
    ///   * Crucially, this makes methods VTABLE-COMPATIBLE. A vtable slot
    ///     cannot name a concrete type, so it cannot carry `byval(T)`; with
    ///     byval receivers a direct call (struct lowered into registers) and
    ///     an indirect call (pointer) would disagree about the ABI, which is
    ///     a silently wrong value — exactly what Burxt refuses.
    /// Ordinary aggregate PARAMETERS keep `byval`; only the receiver changes.
    fn declare_method(
        &self,
        name: &str,
        receiver_ty: &Type,
        receiver_mut: bool,
        params: &[Type],
        ret: &Type,
    ) -> FunctionValue<'ctx> {
        let ptr = self.ctx.ptr_type(AddressSpace::default());
        let mut ll_params: Vec<BasicMetadataTypeEnum> = Vec::new();
        let ret_is_agg = is_aggregate(ret);
        if ret_is_agg {
            ll_params.push(ptr.into());
        }
        ll_params.push(ptr.into()); // self, always by address
        for p in params {
            if is_aggregate(p) {
                ll_params.push(ptr.into());
            } else {
                ll_params.push(self.llvm_type(p).into());
            }
        }
        let fn_ty = if ret_is_agg {
            self.ctx.void_type().fn_type(&ll_params, false)
        } else {
            self.llvm_type(ret).fn_type(&ll_params, false)
        };
        let f = self.module.add_function(name, fn_ty, None);

        use inkwell::attributes::AttributeLoc;
        let mut idx = 0u32;
        if ret_is_agg {
            let attr = self.ctx.create_type_attribute(
                inkwell::attributes::Attribute::get_named_enum_kind_id("sret"),
                self.llvm_type(ret).as_any_type_enum(),
            );
            f.add_attribute(AttributeLoc::Param(0), attr);
            idx = 1;
        }
        // The receiver carries no byval attribute — see the doc comment.
        let _ = (receiver_mut, receiver_ty);
        idx += 1;
        for p in params {
            if is_aggregate(p) {
                let attr = self.ctx.create_type_attribute(
                    inkwell::attributes::Attribute::get_named_enum_kind_id("byval"),
                    self.llvm_type(p).as_any_type_enum(),
                );
                f.add_attribute(AttributeLoc::Param(idx), attr);
            }
            idx += 1;
        }
        f
    }

    /// Generate a method body. `self` binds to the incoming pointer directly
    /// in both cases: for a non-mutating method `byval` already gave us a
    /// private copy, so writes are safe; for a mutating method the pointer
    /// IS the caller's storage, and typeck already proved the call site holds
    /// a `let mut` binding, so writing through it is exactly the intended
    /// mutation.
    fn gen_method(&mut self, m: &TypedMethod) -> Result<(), String> {
        let llf = self.methods[&(m.receiver.clone(), m.name.clone())];
        let entry = self.ctx.append_basic_block(llf, "entry");
        self.builder.position_at_end(entry);
        self.vars.clear();

        let ret_is_agg = is_aggregate(&m.ret);
        self.current_sret = if ret_is_agg {
            Some(llf.get_nth_param(0).unwrap().into_pointer_value())
        } else {
            None
        };
        let self_idx = if ret_is_agg { 1 } else { 0 };
        let self_arg = llf.get_nth_param(self_idx as u32).unwrap();
        self.vars.insert(
            "self".to_string(),
            (self_arg.into_pointer_value(), Type::Named(m.receiver.clone())),
        );

        let param_offset = self_idx + 1;
        for (i, (name, ty)) in m.params.iter().enumerate() {
            let arg = llf.get_nth_param((param_offset + i) as u32).unwrap();
            if is_aggregate(ty) {
                self.vars.insert(name.clone(), (arg.into_pointer_value(), ty.clone()));
            } else {
                let slot = self.create_entry_alloca(name, ty)?;
                self.builder.build_store(slot, arg).map_err(|e| e.to_string())?;
                self.vars.insert(name.clone(), (slot, ty.clone()));
            }
        }

        for stmt in &m.body {
            self.gen_stmt(stmt)?;
        }
        self.current_sret = None;
        Ok(())
    }

    /// Compute the layout of a Burxt type per the no-hidden-header guarantee.
    /// Scalar alignments come from the target; field ORDER and logical shape
    /// are identical everywhere, so "field N" means the same field on every
    /// target.
    pub fn layout_of(&self, ty: &Type) -> Layout {
        match ty {
            Type::Slice(_) => Layout { size: 24, align: 8, field_offsets: vec![] },
            Type::Named(name) if self.enum_types.contains_key(name) => {
                // tag + payload slots, all 8-byte
                let slots = self.enum_types[name]
                    .1
                    .iter()
                    .map(|p| p.len())
                    .max()
                    .unwrap_or(0) as u64;
                Layout { size: 8 * (1 + slots), align: 8, field_offsets: vec![] }
            }
            Type::Named(name) => {
                let fields = &self.struct_fields[name];
                let mut offsets = Vec::with_capacity(fields.len());
                let mut size = 0u64;
                let mut align = 1u64;
                for f in fields {
                    let fl = self.layout_of(f);
                    // pad up to this field's natural alignment
                    size = (size + fl.align - 1) / fl.align * fl.align;
                    offsets.push(size);
                    size += fl.size;
                    align = align.max(fl.align);
                }
                // round the whole aggregate up to its own alignment
                size = (size + align - 1) / align * align;
                Layout { size, align, field_offsets: offsets }
            }
            Type::Array { elem, len } => {
                let el = self.layout_of(elem);
                let stride = (el.size + el.align - 1) / el.align * el.align;
                Layout {
                    size: stride * (*len as u64),
                    align: el.align,
                    // an array's "fields" are its elements, evenly strided
                    field_offsets: (0..*len as u64).map(|i| i * stride).collect(),
                }
            }
            // Scalars: i64-shaped, or a target-width pointer for String.
            Type::String => {
                let w = self.ctx.ptr_sized_int_type(&self.target_data(), None).get_bit_width() as u64 / 8;
                Layout { size: w, align: w, field_offsets: vec![] }
            }
            Type::CInt => Layout { size: 4, align: 4, field_offsets: vec![] },
            _ => Layout { size: 8, align: 8, field_offsets: vec![] },
        }
    }

    /// Target data for the host — the authority on pointer width.
    fn target_data(&self) -> inkwell::targets::TargetData {
        inkwell::targets::TargetData::create(&self.module.get_triple().as_str().to_string_lossy())
    }

    /// The LLVM type for a Burxt type. All scalars are i64; String is an
    /// opaque pointer — the TARGET decides pointer width, never this code.
    fn llvm_type(&self, ty: &Type) -> BasicTypeEnum<'ctx> {
        match ty {
            Type::Int | Type::Bool | Type::Decimal { .. } => self.ctx.i64_type().into(),
            Type::String => self.ctx.ptr_type(AddressSpace::default()).into(),
            Type::CInt => self.ctx.i32_type().into(),
            Type::Named(name) => match self.struct_types.get(name) {
                Some(st) => (*st).into(),
                None => self.enum_types[name].0.into(),
            },
            Type::Slice(_) => {
                let ptr = self.ctx.ptr_type(AddressSpace::default());
                let i64t = self.ctx.i64_type();
                self.ctx
                    .struct_type(&[ptr.into(), i64t.into(), i64t.into()], false)
                    .into()
            }
            Type::Array { elem, len } => self.llvm_type(elem).array_type(*len).into(),
            // A trait object is a fat pointer: { data, vtable }. The vtable
            // lives OUTSIDE the data, which is why becoming a trait object
            // never changes a struct's layout.
            Type::Dyn(_) => {
                let ptr = self.ctx.ptr_type(AddressSpace::default());
                self.ctx.struct_type(&[ptr.into(), ptr.into()], false).into()
            }
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
        self.gen_print_value(e)?;
        self.gen_newline()
    }

    /// Print a single trailing newline.
    fn gen_newline(&mut self) -> Result<(), String> {
        let printf = self.printf.ok_or("codegen bug: printf not declared")?;
        let fmt = self.global_str("\n", "fmt_nl");
        self.builder
            .build_call(printf, &[fmt.into()], "printf_nl")
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Print one value with NO trailing newline. `print` adds the newline
    /// itself, and interpolation must not put one between pieces.
    fn gen_print_value(&mut self, e: &TypedExpr) -> Result<(), String> {
        let printf = self.printf.ok_or("codegen bug: printf not declared")?;
        let val = self.gen_expr(e)?;
        match &e.ty {
            Type::Int => {
                let fmt = self.global_str("%lld", "fmt_int");
                self.builder
                    .build_call(printf, &[fmt.into(), val.into()], "printf_int")
                    .map_err(|e| e.to_string())?;
            }
            Type::String => {
                // User bytes are always an ARGUMENT, never the format string.
                let fmt = self.global_str("%s", "fmt_str");
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
                let t = self.global_str("true", "str_true");
                let f = self.global_str("false", "str_false");
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
            Type::Named(_) | Type::CInt | Type::Array { .. } | Type::Dyn(_)
            | Type::Slice(_) => {
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
                    let fmt = self.global_str("%s%llu", "fmt_dec0");
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
                    let fmt_str = format!("%s%llu.%0{}llu", scale);
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
            TypedExprKind::ByteAt { s, index } => {
                let sp = self.gen_expr(s)?.into_pointer_value();
                let i = self.gen_expr(index)?.into_int_value();
                let n = self.build_str_len(sp)?;
                // Reading past the end would hand back whatever byte follows —
                // silent garbage, so it is checked like every array index.
                let checked = self.build_checked_byte_index(i, n)?;
                let p = unsafe {
                    self.builder
                        .build_gep(self.ctx.i8_type(), sp, &[checked], "byte_ptr")
                }
                .map_err(|e| e.to_string())?;
                let b = self
                    .builder
                    .build_load(self.ctx.i8_type(), p, "byte")
                    .map_err(|e| e.to_string())?
                    .into_int_value();
                // Bytes are unsigned 0..255, so zero-extend — a sign-extend
                // would turn byte 200 into a negative number.
                self.builder
                    .build_int_z_extend(b, i64t, "byte_i64")
                    .map(Into::into)
                    .map_err(|e| e.to_string())
            }
            TypedExprKind::StrLen(inner) => {
                let s = self.gen_expr(inner)?.into_pointer_value();
                self.build_str_len(s).map(Into::into)
            }
            TypedExprKind::Not(inner) => {
                // Bool is 0/1, so `1 - v` flips it without any branch.
                let v = self.gen_expr(inner)?.into_int_value();
                self.builder
                    .build_int_sub(i64t.const_int(1, false), v, "not")
                    .map(Into::into)
                    .map_err(|e| e.to_string())
            }
            TypedExprKind::Logical { op, lhs, rhs } => {
                // Short-circuit with real control flow: the right side must NOT
                // execute when the left already decides the answer, because that
                // is observable (its side effects would show).
                let err = |e: inkwell::builder::BuilderError| e.to_string();
                let function = self
                    .builder
                    .get_insert_block()
                    .and_then(|b| b.get_parent())
                    .ok_or("codegen bug: logical operator outside a function")?;

                let l = self.gen_expr(lhs)?.into_int_value();
                let l_bit = self
                    .builder
                    .build_int_compare(inkwell::IntPredicate::NE, l, i64t.const_zero(), "lhs_bit")
                    .map_err(err)?;
                let lhs_block = self.builder.get_insert_block().unwrap();
                let rhs_bb = self.ctx.append_basic_block(function, "sc.rhs");
                let join_bb = self.ctx.append_basic_block(function, "sc.join");

                // `&&` evaluates the right side only when the left is true;
                // `||` only when the left is false.
                match op {
                    LogicalOp::And => {
                        self.builder
                            .build_conditional_branch(l_bit, rhs_bb, join_bb)
                            .map_err(err)?
                    }
                    LogicalOp::Or => {
                        self.builder
                            .build_conditional_branch(l_bit, join_bb, rhs_bb)
                            .map_err(err)?
                    }
                };

                self.builder.position_at_end(rhs_bb);
                let r = self.gen_expr(rhs)?.into_int_value();
                // The right side may itself have branched (a nested `&&`), so
                // take the block we actually end in, not rhs_bb.
                let rhs_end = self.builder.get_insert_block().unwrap();
                self.builder.build_unconditional_branch(join_bb).map_err(err)?;

                self.builder.position_at_end(join_bb);
                let phi = self.builder.build_phi(i64t, "sc").map_err(err)?;
                // Arriving from the left means the left decided it: false for
                // `&&`, true for `||`.
                let short = match op {
                    LogicalOp::And => i64t.const_zero(),
                    LogicalOp::Or => i64t.const_int(1, false),
                };
                phi.add_incoming(&[(&short, lhs_block), (&r, rhs_end)]);
                Ok(phi.as_basic_value())
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
                            // The exact product's scale is the SUM of the operand
                            // scales, so reaching the result's scale means shifting
                            // by (ls + rs - result). That one expression covers the
                            // same-scale case too (s + s - s = s), so mixed and
                            // matching scales share a single path rather than being
                            // special-cased against each other.
                            let (scale, mode) = decimal_with_rounding(&e.ty)?;
                            let ls = decimal_scale(&lhs.ty)?;
                            let rs = decimal_scale(&rhs.ty)?;
                            let l128 = self.widen(l)?;
                            let r128 = self.widen(r)?;
                            let raw = self
                                .builder
                                .build_int_mul(l128, r128, "mul_raw")
                                .map_err(|e| e.to_string())?;
                            let shift = ls as i64 + rs as i64 - scale as i64;
                            if shift >= 0 {
                                let pow = self.pow10_i128(shift as u32);
                                self.build_round_div(mode, raw, pow)
                            } else {
                                // The result is WIDER than the exact product, so
                                // widening is lossless and nothing rounds.
                                let pow = self.pow10_i128((-shift) as u32);
                                let widened = self
                                    .builder
                                    .build_int_mul(raw, pow, "mul_widen")
                                    .map_err(|e| e.to_string())?;
                                self.build_narrow_to_i64(widened)
                            }
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
                // Strings are pointers, so they compare by scanning bytes
                // rather than by comparing the pointers — two equal strings
                // must be equal regardless of where they live. Typeck allows
                // only `==`/`!=` here.
                if lhs.ty == Type::String {
                    let a = self.gen_expr(lhs)?.into_pointer_value();
                    let b = self.gen_expr(rhs)?.into_pointer_value();
                    let eq = self.build_str_eq(a, b)?;
                    return match op {
                        CmpOp::Eq => Ok(eq.into()),
                        CmpOp::Ne => self
                            .builder
                            .build_int_sub(i64t.const_int(1, false), eq, "str_ne")
                            .map(Into::into)
                            .map_err(|e| e.to_string()),
                        other => Err(format!(
                            "codegen bug: `{}` on String should have been refused",
                            other
                        )),
                    };
                }
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
                let user_sig = self.fn_sigs.get(name).cloned();
                let mut vals: Vec<BasicMetadataValueEnum> = Vec::new();

                // An aggregate result is written into space we own, then read
                // back as a value.
                let sret_slot = match &user_sig {
                    Some((_, ret)) if is_aggregate(ret) => {
                        let slot = self.create_entry_alloca("ret_tmp", ret)?;
                        vals.push(slot.into());
                        Some((slot, ret.clone()))
                    }
                    _ => None,
                };

                for (i, a) in args.iter().enumerate() {
                    // Aggregate arguments pass as an address; LLVM's byval
                    // makes the callee's copy.
                    if is_aggregate(&a.ty) {
                        let addr = self.gen_aggregate_addr(a)?;
                        vals.push(addr.into());
                        continue;
                    }
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

                // Mirror the declared ABI attributes onto the call site.
                use inkwell::attributes::{Attribute, AttributeLoc};
                if let Some((_, ret)) = &sret_slot {
                    let attr = self.ctx.create_type_attribute(
                        Attribute::get_named_enum_kind_id("sret"),
                        self.llvm_type(ret).as_any_type_enum(),
                    );
                    call.add_attribute(AttributeLoc::Param(0), attr);
                }
                if let Some((ptys, ret)) = &user_sig {
                    let offset = if is_aggregate(ret) { 1 } else { 0 };
                    for (i, p) in ptys.iter().enumerate() {
                        if is_aggregate(p) {
                            let attr = self.ctx.create_type_attribute(
                                Attribute::get_named_enum_kind_id("byval"),
                                self.llvm_type(p).as_any_type_enum(),
                            );
                            call.add_attribute(AttributeLoc::Param((i + offset) as u32), attr);
                        }
                    }
                }

                if let Some((slot, ret)) = sret_slot {
                    return self
                        .builder
                        .build_load(self.llvm_type(&ret), slot, "ret_val")
                        .map_err(|e| e.to_string());
                }
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
            TypedExprKind::VariantLit { enum_name, tag, args } => {
                let (st, _) = self.enum_types[enum_name.as_str()];
                let slot = self.create_entry_alloca("variant", &e.ty)?;
                // tag first
                let tag_ptr = self
                    .builder
                    .build_struct_gep(st, slot, 0, "tag_ptr")
                    .map_err(|e| e.to_string())?;
                self.builder
                    .build_store(tag_ptr, i64t.const_int(*tag as u64, false))
                    .map_err(|e| e.to_string())?;
                // then each payload value into its slot
                if !args.is_empty() {
                    let payload_ptr = self
                        .builder
                        .build_struct_gep(st, slot, 1, "payload_ptr")
                        .map_err(|e| e.to_string())?;
                    let arr_ty = i64t.array_type(
                        self.enum_types[enum_name.as_str()]
                            .1
                            .iter()
                            .map(|p| p.len())
                            .max()
                            .unwrap_or(0) as u32,
                    );
                    for (i, a) in args.iter().enumerate() {
                        let v = self.gen_expr(a)?;
                        let idx = i64t.const_int(i as u64, false);
                        let p = unsafe {
                            self.builder.build_in_bounds_gep(
                                arr_ty,
                                payload_ptr,
                                &[i64t.const_zero(), idx],
                                "slot",
                            )
                        }
                        .map_err(|e| e.to_string())?;
                        self.builder.build_store(p, v).map_err(|e| e.to_string())?;
                    }
                }
                self.builder
                    .build_load(st, slot, "variant_val")
                    .map_err(|e| e.to_string())
            }
            TypedExprKind::StructLit { name, fields } => {
                // Built in place rather than with insertvalue, so a field that
                // is itself an array literal can be filled element by element.
                let st = self.struct_types[name.as_str()];
                let slot = self.create_entry_alloca("struct_tmp", &e.ty)?;
                let field_tys = self.struct_fields[name.as_str()].clone();
                for (i, f) in fields.iter().enumerate() {
                    let fptr = self
                        .builder
                        .build_struct_gep(st, slot, i as u32, "fieldptr")
                        .map_err(|e| e.to_string())?;
                    if let TypedExprKind::ArrayLit(elems) = &f.kind {
                        self.store_array_elements(fptr, &field_tys[i], elems)?;
                    } else {
                        let v = self.gen_expr(f)?;
                        self.builder.build_store(fptr, v).map_err(|e| e.to_string())?;
                    }
                }
                self.builder
                    .build_load(st, slot, "struct_val")
                    .map_err(|e| e.to_string())
            }
            TypedExprKind::Field { base, index } => {
                let agg = self.gen_expr(base)?.into_struct_value();
                self.builder
                    .build_extract_value(agg, *index, "field")
                    .map_err(|e| e.to_string())
            }
            TypedExprKind::MethodCall { receiver, method, receiver_mut, base, args } => {
                let f = *self
                    .methods
                    .get(&(receiver.clone(), method.clone()))
                    .ok_or_else(|| {
                        format!("codegen bug: unknown method {}.{}", receiver, method)
                    })?;
                let mut vals: Vec<BasicMetadataValueEnum> = Vec::new();

                let sret_slot = if is_aggregate(&e.ty) {
                    let slot = self.create_entry_alloca("mret_tmp", &e.ty)?;
                    vals.push(slot.into());
                    Some(slot)
                } else {
                    None
                };

                // The receiver's address: for a mutating method typeck has
                // already proven `base` is a plain `let mut` binding, so this
                // yields the caller's real storage; for a non-mutating method
                // it may also be a materialized temporary — either way LLVM's
                // byval attribute (added below) makes the callee's copy when
                // one is needed.
                let recv_addr = self.gen_aggregate_addr(base)?;
                vals.push(recv_addr.into());

                for a in args {
                    if is_aggregate(&a.ty) {
                        vals.push(self.gen_aggregate_addr(a)?.into());
                    } else {
                        vals.push(self.gen_expr(a)?.into());
                    }
                }

                let call = self
                    .builder
                    .build_call(f, &vals, "mcall")
                    .map_err(|e| e.to_string())?;

                use inkwell::attributes::{Attribute, AttributeLoc};
                let mut idx = 0u32;
                if sret_slot.is_some() {
                    let attr = self.ctx.create_type_attribute(
                        Attribute::get_named_enum_kind_id("sret"),
                        self.llvm_type(&e.ty).as_any_type_enum(),
                    );
                    call.add_attribute(AttributeLoc::Param(0), attr);
                    idx = 1;
                }
                // The receiver is a plain pointer in both forms, so a direct
                // call and a vtable call agree on the ABI. (See declare_method.)
                let _ = receiver_mut;
                idx += 1;
                for a in args {
                    if is_aggregate(&a.ty) {
                        let attr = self.ctx.create_type_attribute(
                            Attribute::get_named_enum_kind_id("byval"),
                            self.llvm_type(&a.ty).as_any_type_enum(),
                        );
                        call.add_attribute(AttributeLoc::Param(idx), attr);
                    }
                    idx += 1;
                }

                if let Some(slot) = sret_slot {
                    return self
                        .builder
                        .build_load(self.llvm_type(&e.ty), slot, "mret_val")
                        .map_err(|e| e.to_string());
                }
                match call.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(v) => Ok(v),
                    _ => Err(format!(
                        "codegen bug: call to {}.{} returned void",
                        receiver, method
                    )),
                }
            }
            TypedExprKind::DynCoerce { trait_name, concrete, var } => {
                // Pair the binding's storage with the static vtable. No copy:
                // the data half IS the borrowed value (and the layout it points
                // at is unchanged by being viewed as a trait object).
                let (slot, _) = *self
                    .vars
                    .get(var)
                    .ok_or_else(|| format!("codegen: unknown variable {}", var))?;
                let vtable = self
                    .vtables
                    .get(&(trait_name.clone(), concrete.clone()))
                    .ok_or_else(|| {
                        format!("codegen bug: no vtable for {}/{}", concrete, trait_name)
                    })?
                    .as_pointer_value();
                let fat_ty = self.llvm_type(&e.ty).into_struct_type();
                let mut fat = fat_ty.get_undef();
                fat = self
                    .builder
                    .build_insert_value(fat, slot, 0, "dyn_data")
                    .map_err(|e| e.to_string())?
                    .into_struct_value();
                fat = self
                    .builder
                    .build_insert_value(fat, vtable, 1, "dyn_vtable")
                    .map_err(|e| e.to_string())?
                    .into_struct_value();
                Ok(fat.into())
            }
            TypedExprKind::DynCall { trait_name, method, slot, base, args } => {
                let fat = self.gen_expr(base)?.into_struct_value();
                let data = self
                    .builder
                    .build_extract_value(fat, 0, "dyn_data")
                    .map_err(|e| e.to_string())?
                    .into_pointer_value();
                let vtable = self
                    .builder
                    .build_extract_value(fat, 1, "dyn_vtable")
                    .map_err(|e| e.to_string())?
                    .into_pointer_value();

                // Load the slot's function pointer: the index is fixed at
                // compile time by trait-declaration order.
                let ptr_ty = self.ctx.ptr_type(AddressSpace::default());
                let fn_ptr_addr = unsafe {
                    self.builder.build_gep(
                        ptr_ty,
                        vtable,
                        &[self.ctx.i32_type().const_int(*slot as u64, false)],
                        "slot_addr",
                    )
                }
                .map_err(|e| e.to_string())?;
                let fn_ptr = self
                    .builder
                    .build_load(ptr_ty, fn_ptr_addr, "slot_fn")
                    .map_err(|e| e.to_string())?
                    .into_pointer_value();

                // Rebuild the callee's type: (receiver ptr, params...) -> ret.
                // Typeck refused mutating methods here, so the receiver is the
                // byval form — the callee copies from `data`.
                let (param_tys, ret_ty) = self
                    .trait_slots
                    .get(trait_name)
                    .and_then(|s| s.get(*slot as usize))
                    .cloned()
                    .ok_or_else(|| {
                        format!("codegen bug: no signature for {}.{}", trait_name, method)
                    })?;
                let mut ll_params: Vec<BasicMetadataTypeEnum> = Vec::new();
                let ret_is_agg = is_aggregate(&ret_ty);
                if ret_is_agg {
                    ll_params.push(ptr_ty.into());
                }
                ll_params.push(ptr_ty.into()); // self
                for p in &param_tys {
                    if is_aggregate(p) {
                        ll_params.push(ptr_ty.into());
                    } else {
                        ll_params.push(self.llvm_type(p).into());
                    }
                }
                let fn_ty = if ret_is_agg {
                    self.ctx.void_type().fn_type(&ll_params, false)
                } else {
                    self.llvm_type(&ret_ty).fn_type(&ll_params, false)
                };

                let mut vals: Vec<BasicMetadataValueEnum> = Vec::new();
                let sret_slot = if ret_is_agg {
                    let s = self.create_entry_alloca("dret_tmp", &ret_ty)?;
                    vals.push(s.into());
                    Some(s)
                } else {
                    None
                };
                vals.push(data.into());
                for a in args {
                    if is_aggregate(&a.ty) {
                        vals.push(self.gen_aggregate_addr(a)?.into());
                    } else {
                        vals.push(self.gen_expr(a)?.into());
                    }
                }

                let call = self
                    .builder
                    .build_indirect_call(fn_ty, fn_ptr, &vals, "dyncall")
                    .map_err(|e| e.to_string())?;

                if let Some(s) = sret_slot {
                    return self
                        .builder
                        .build_load(self.llvm_type(&ret_ty), s, "dret_val")
                        .map_err(|e| e.to_string());
                }
                match call.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(v) => Ok(v),
                    _ => Err(format!(
                        "codegen bug: dyn call to {}.{} returned void",
                        trait_name, method
                    )),
                }
            }
            TypedExprKind::ArrayLit(_) => {
                Err("codegen bug: array literal outside a let initializer".to_string())
            }
            TypedExprKind::SliceLit(elems) => {
                // Allocate room in the region, fill it, and build the triple.
                let elem_ty = match &e.ty {
                    Type::Slice(t) => t.as_ref().clone(),
                    other => return Err(format!("codegen bug: slice literal of {}", other)),
                };
                let n = elems.len() as u64;
                let cap = if n == 0 { 4 } else { n };
                let data = self.build_alloc_array(&elem_ty, i64t.const_int(cap, false))?;
                let ll_elem = self.llvm_type(&elem_ty);
                for (i, el) in elems.iter().enumerate() {
                    let v = self.gen_expr(el)?;
                    let p = unsafe {
                        self.builder.build_gep(
                            ll_elem,
                            data,
                            &[i64t.const_int(i as u64, false)],
                            "init",
                        )
                    }
                    .map_err(|e| e.to_string())?;
                    self.builder.build_store(p, v).map_err(|e| e.to_string())?;
                }
                self.build_slice_value(&e.ty, data, i64t.const_int(n, false), i64t.const_int(cap, false))
            }
            TypedExprKind::SliceLen(inner) => {
                let sl = self.gen_expr(inner)?.into_struct_value();
                self.builder
                    .build_extract_value(sl, 1, "slice_len")
                    .map_err(|e| e.to_string())
            }
            TypedExprKind::SliceIndex { base, index } => {
                let elem_ty = match &base.ty {
                    Type::Slice(t) => t.as_ref().clone(),
                    other => return Err(format!("codegen bug: indexing {}", other)),
                };
                let sl = self.gen_expr(base)?.into_struct_value();
                let data = self
                    .builder
                    .build_extract_value(sl, 0, "data")
                    .map_err(|e| e.to_string())?
                    .into_pointer_value();
                let n = self
                    .builder
                    .build_extract_value(sl, 1, "len")
                    .map_err(|e| e.to_string())?
                    .into_int_value();
                let idx = self.gen_expr(index)?.into_int_value();
                // bounds are the RUNTIME length, not a static one
                let checked = self.build_checked_index(idx, n)?;
                let ll_elem = self.llvm_type(&elem_ty);
                let p = unsafe {
                    self.builder.build_gep(ll_elem, data, &[checked], "elem_ptr")
                }
                .map_err(|e| e.to_string())?;
                self.builder
                    .build_load(ll_elem, p, "elem")
                    .map_err(|e| e.to_string())
            }
            TypedExprKind::Push { place, value } => {
                let elem_ty = match &place.ty {
                    Type::Slice(t) => t.as_ref().clone(),
                    other => return Err(format!("codegen bug: push to {}", other)),
                };
                let slot = self.gen_place_addr(place)?;
                let v = self.gen_expr(value)?;
                self.build_push(&place.ty, &elem_ty, slot, v)
            }
            TypedExprKind::Index { base, len, index } => {
                let elem_ty = match &base.ty {
                    Type::Array { elem, .. } => self.llvm_type(elem),
                    other => return Err(format!("codegen bug: indexing a {}", other)),
                };
                let arr_ty = self.llvm_type(&base.ty);
                let base_ptr = self.gen_place_addr(base)?;
                let i64t2 = self.ctx.i64_type();
                let idx = self.gen_expr(index)?.into_int_value();
                let checked =
                    self.build_checked_index(idx, i64t2.const_int(*len as u64, false))?;
                let p = unsafe {
                    self.builder.build_in_bounds_gep(
                        arr_ty,
                        base_ptr,
                        &[i64t2.const_zero(), checked],
                        "elem_ptr",
                    )
                }
                .map_err(|e| e.to_string())?;
                self.builder
                    .build_load(elem_ty, p, "elem")
                    .map_err(|e| e.to_string())
            }
        }
    }

    /// The address of an aggregate value, for passing it by `byval`. A named
    /// binding lends its own slot (LLVM inserts the callee's copy); anything
    /// else is materialized into a temporary first.
    fn gen_aggregate_addr(&mut self, e: &TypedExpr) -> Result<PointerValue<'ctx>, String> {
        match &e.kind {
            TypedExprKind::Var(_) | TypedExprKind::Field { .. } => self.gen_place_addr(e),
            // An array literal has no home yet — build it in a temporary.
            TypedExprKind::ArrayLit(elems) => {
                let tmp = self.create_entry_alloca("arr_tmp", &e.ty)?;
                self.store_array_elements(tmp, &e.ty, elems)?;
                Ok(tmp)
            }
            _ => {
                let v = self.gen_expr(e)?;
                let tmp = self.create_entry_alloca("agg_tmp", &e.ty)?;
                self.builder.build_store(tmp, v).map_err(|e| e.to_string())?;
                Ok(tmp)
            }
        }
    }

    /// Store an array literal's elements into `slot`, one GEP per element.
    fn store_array_elements(
        &mut self,
        slot: PointerValue<'ctx>,
        ty: &Type,
        elems: &[TypedExpr],
    ) -> Result<(), String> {
        let arr_ty = self.llvm_type(ty);
        let i64t = self.ctx.i64_type();
        for (i, e) in elems.iter().enumerate() {
            let v = self.gen_expr(e)?;
            let idx = i64t.const_int(i as u64, false);
            let ptr = unsafe {
                self.builder.build_in_bounds_gep(
                    arr_ty,
                    slot,
                    &[i64t.const_zero(), idx],
                    "elem_init",
                )
            }
            .map_err(|e| e.to_string())?;
            self.builder.build_store(ptr, v).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// The ADDRESS of a place: a binding, or a field path through one. Both
    /// indexed reads and aggregate passing need this, so it lives in one spot.
    fn gen_place_addr(&mut self, e: &TypedExpr) -> Result<PointerValue<'ctx>, String> {
        match &e.kind {
            TypedExprKind::Var(name) => self
                .vars
                .get(name)
                .map(|(slot, _)| *slot)
                .ok_or_else(|| format!("codegen: unknown variable {}", name)),
            TypedExprKind::Field { base, index } => {
                let base_ptr = self.gen_place_addr(base)?;
                let sname = match &base.ty {
                    Type::Named(n) => n.clone(),
                    other => {
                        return Err(format!("codegen bug: field of non-struct {}", other))
                    }
                };
                let st = self.struct_types[&sname];
                self.builder
                    .build_struct_gep(st, base_ptr, *index, "fieldptr")
                    .map_err(|e| e.to_string())
            }
            // Anything else is an rvalue: materialize it so it has an address.
            _ => {
                let v = self.gen_expr(e)?;
                let tmp = self.create_entry_alloca("place_tmp", &e.ty)?;
                self.builder.build_store(tmp, v).map_err(|er| er.to_string())?;
                Ok(tmp)
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

    /// Lazily create the bump heap: one chunk, a cursor into it, and its size.
    /// This is NOT a runtime — no collector, no scheduler, no refcounts. Just a
    /// pointer that moves forward and resets when a region ends.
    fn heap_globals(
        &mut self,
    ) -> (
        inkwell::values::GlobalValue<'ctx>,
        inkwell::values::GlobalValue<'ctx>,
    ) {
        if let Some(g) = self.heap {
            return g;
        }
        let ptr = self.ctx.ptr_type(AddressSpace::default());
        let i64t = self.ctx.i64_type();
        let base = self.module.add_global(ptr, None, "burxt.heap.base");
        base.set_initializer(&ptr.const_null());
        let next = self.module.add_global(i64t, None, "burxt.heap.next");
        next.set_initializer(&i64t.const_zero());
        *self.heap.insert((base, next))
    }

    /// Allocate `count` elements of `elem` in the current region.
    fn build_alloc_array(
        &mut self,
        elem: &Type,
        count: IntValue<'ctx>,
    ) -> Result<PointerValue<'ctx>, String> {
        let f = self.alloc_fn()?;
        let i64t = self.ctx.i64_type();
        let size = self.layout_of(elem).size;
        let bytes = self
            .builder
            .build_int_mul(count, i64t.const_int(size, false), "bytes")
            .map_err(|e| e.to_string())?;
        let call = self
            .builder
            .build_call(f, &[bytes.into()], "region_alloc")
            .map_err(|e| e.to_string())?;
        match call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => Ok(v.into_pointer_value()),
            _ => Err("allocator returned void".to_string()),
        }
    }

    /// Build a `{ data, len, cap }` triple as a value.
    fn build_slice_value(
        &mut self,
        ty: &Type,
        data: PointerValue<'ctx>,
        len: IntValue<'ctx>,
        cap: IntValue<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let st = self.llvm_type(ty).into_struct_type();
        let mut v = st.get_undef();
        let parts: [BasicValueEnum<'ctx>; 3] = [data.into(), len.into(), cap.into()];
        for (i, part) in parts.into_iter().enumerate() {
            v = self
                .builder
                .build_insert_value(v, part, i as u32, "slice")
                .map_err(|e| e.to_string())?
                .into_struct_value();
        }
        Ok(v.into())
    }

    /// Append to a growable array in place, doubling its capacity in the region
    /// when it is full. Returns the new length.
    ///
    /// Honest note on arenas: growing copies into a fresh block and abandons the
    /// old one, because a bump allocator cannot free an individual object. That
    /// space is reclaimed when the region ends — the arena tradeoff, paid
    /// visibly rather than hidden.
    fn build_push(
        &mut self,
        slice_ty: &Type,
        elem_ty: &Type,
        slot: PointerValue<'ctx>,
        value: BasicValueEnum<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let i64t = self.ctx.i64_type();
        let st = self.llvm_type(slice_ty).into_struct_type();
        let ll_elem = self.llvm_type(elem_ty);

        let data_p = self.builder.build_struct_gep(st, slot, 0, "data_p").map_err(err)?;
        let len_p = self.builder.build_struct_gep(st, slot, 1, "len_p").map_err(err)?;
        let cap_p = self.builder.build_struct_gep(st, slot, 2, "cap_p").map_err(err)?;
        let ptr_ty = self.ctx.ptr_type(AddressSpace::default());
        let len = self.builder.build_load(i64t, len_p, "len").map_err(err)?.into_int_value();
        let cap = self.builder.build_load(i64t, cap_p, "cap").map_err(err)?.into_int_value();

        let function = self
            .builder
            .get_insert_block()
            .and_then(|b| b.get_parent())
            .ok_or("codegen bug: push outside a function")?;
        let grow_bb = self.ctx.append_basic_block(function, "push.grow");
        let store_bb = self.ctx.append_basic_block(function, "push.store");

        let full = self
            .builder
            .build_int_compare(inkwell::IntPredicate::SGE, len, cap, "full")
            .map_err(err)?;
        self.builder.build_conditional_branch(full, grow_bb, store_bb).map_err(err)?;

        // grow: double, allocate fresh, copy the live elements over
        self.builder.position_at_end(grow_bb);
        let two = i64t.const_int(2, false);
        let doubled = self.builder.build_int_mul(cap, two, "doubled").map_err(err)?;
        let min = i64t.const_int(4, false);
        let is_small = self
            .builder
            .build_int_compare(inkwell::IntPredicate::SLT, doubled, min, "small")
            .map_err(err)?;
        let new_cap = self
            .builder
            .build_select(is_small, min, doubled, "new_cap")
            .map_err(err)?
            .into_int_value();
        let fresh = self.build_alloc_array(elem_ty, new_cap)?;
        let old = self
            .builder
            .build_load(ptr_ty, data_p, "old_data")
            .map_err(err)?
            .into_pointer_value();
        let esize = self.layout_of(elem_ty).size;
        let copy_bytes = self
            .builder
            .build_int_mul(len, i64t.const_int(esize, false), "copy_bytes")
            .map_err(err)?;
        self.builder
            .build_memcpy(fresh, 8, old, 8, copy_bytes)
            .map_err(|e| e.to_string())?;
        self.builder.build_store(data_p, fresh).map_err(err)?;
        self.builder.build_store(cap_p, new_cap).map_err(err)?;
        self.builder.build_unconditional_branch(store_bb).map_err(err)?;

        // store the new element and bump the length
        self.builder.position_at_end(store_bb);
        let data = self
            .builder
            .build_load(ptr_ty, data_p, "data")
            .map_err(err)?
            .into_pointer_value();
        let at = unsafe { self.builder.build_gep(ll_elem, data, &[len], "at") }.map_err(err)?;
        self.builder.build_store(at, value).map_err(err)?;
        let new_len = self
            .builder
            .build_int_add(len, i64t.const_int(1, false), "new_len")
            .map_err(err)?;
        self.builder.build_store(len_p, new_len).map_err(err)?;
        Ok(new_len.into())
    }

    /// `open` is just "remember where the cursor is".
    fn build_region_open(&mut self) -> Result<IntValue<'ctx>, String> {
        // Opening a region brings its allocator into the module: a region and
        // the bump allocator are one mechanism, not two.
        self.alloc_fn()?;
        let (_, next) = self.heap_globals();
        self.builder
            .build_load(self.ctx.i64_type(), next.as_pointer_value(), "region_mark")
            .map(|v| v.into_int_value())
            .map_err(|e| e.to_string())
    }

    /// `close` is "put the cursor back" — that is the entire deallocation.
    fn build_region_close(&mut self, mark: IntValue<'ctx>) -> Result<(), String> {
        let (_, next) = self.heap_globals();
        self.builder
            .build_store(next.as_pointer_value(), mark)
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Get (or lazily define) `ptr @burxt.alloc(i64 bytes)`: bump the cursor,
    /// 8-byte aligned. Exhaustion is a named runtime error, never a silent
    /// overrun — the same standard every other check in Burxt meets.
    fn alloc_fn(&mut self) -> Result<FunctionValue<'ctx>, String> {
        if let Some(f) = self.alloc_fn {
            return Ok(f);
        }
        const CHUNK: u64 = 64 * 1024 * 1024;
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let saved = self.builder.get_insert_block();
        let (base, next) = self.heap_globals();

        let i64t = self.ctx.i64_type();
        let i8t = self.ctx.i8_type();
        let ptr = self.ctx.ptr_type(AddressSpace::default());
        let malloc = match self.module.get_function("malloc") {
            Some(f) => f,
            None => self
                .module
                .add_function("malloc", ptr.fn_type(&[i64t.into()], false), None),
        };

        let f = self
            .module
            .add_function("burxt.alloc", ptr.fn_type(&[i64t.into()], false), None);
        let entry = self.ctx.append_basic_block(f, "entry");
        let init_bb = self.ctx.append_basic_block(f, "init_chunk");
        let have_bb = self.ctx.append_basic_block(f, "have_chunk");
        let full_bb = self.ctx.append_basic_block(f, "exhausted");
        let ok_bb = self.ctx.append_basic_block(f, "ok");

        self.builder.position_at_end(entry);
        let want = f.get_nth_param(0).unwrap().into_int_value();
        // round the request up to 8 bytes so every value stays aligned
        let seven = i64t.const_int(7, false);
        let bumped = self.builder.build_int_add(want, seven, "bumped").map_err(err)?;
        let mask = i64t.const_int(!7u64, false);
        let size = self.builder.build_and(bumped, mask, "aligned").map_err(err)?;
        let cur_base = self
            .builder
            .build_load(ptr, base.as_pointer_value(), "base")
            .map_err(err)?
            .into_pointer_value();
        let is_null = self.builder.build_is_null(cur_base, "no_chunk").map_err(err)?;
        self.builder.build_conditional_branch(is_null, init_bb, have_bb).map_err(err)?;

        // one chunk, allocated on first use
        self.builder.position_at_end(init_bb);
        let chunk = self
            .builder
            .build_call(malloc, &[i64t.const_int(CHUNK, false).into()], "chunk")
            .map_err(err)?;
        let chunk_ptr = match chunk.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => v.into_pointer_value(),
            _ => return Err("malloc returned void".to_string()),
        };
        self.builder.build_store(base.as_pointer_value(), chunk_ptr).map_err(err)?;
        self.builder.build_unconditional_branch(have_bb).map_err(err)?;

        self.builder.position_at_end(have_bb);
        let real_base = self
            .builder
            .build_load(ptr, base.as_pointer_value(), "base2")
            .map_err(err)?
            .into_pointer_value();
        let cursor = self
            .builder
            .build_load(i64t, next.as_pointer_value(), "cursor")
            .map_err(err)?
            .into_int_value();
        let after = self.builder.build_int_add(cursor, size, "after").map_err(err)?;
        let over = self
            .builder
            .build_int_compare(
                inkwell::IntPredicate::UGT,
                after,
                i64t.const_int(CHUNK, false),
                "over",
            )
            .map_err(err)?;
        self.builder.build_conditional_branch(over, full_bb, ok_bb).map_err(err)?;

        self.builder.position_at_end(full_bb);
        self.build_panic(
            "burxt runtime error: region memory exhausted — this build reserves 64 MB \
             per process for region allocation\n",
        )?;

        self.builder.position_at_end(ok_bb);
        self.builder.build_store(next.as_pointer_value(), after).map_err(err)?;
        let out = unsafe { self.builder.build_gep(i8t, real_base, &[cursor], "cell") }
            .map_err(err)?;
        self.builder.build_return(Some(&out)).map_err(err)?;

        if let Some(bb) = saved {
            self.builder.position_at_end(bb);
        }
        self.alloc_fn = Some(f);
        Ok(f)
    }

    /// Emit a call to `@burxt.strlen(s)` — the byte length of a String.
    fn build_str_len(&mut self, s: PointerValue<'ctx>) -> Result<IntValue<'ctx>, String> {
        let f = self.str_len_fn()?;
        let call = self
            .builder
            .build_call(f, &[s.into()], "strlen")
            .map_err(|e| e.to_string())?;
        match call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => Ok(v.into_int_value()),
            _ => Err("string-length helper returned void".to_string()),
        }
    }

    /// Emit a call to `@burxt.streq(a, b)` — 1 if the bytes match, else 0.
    fn build_str_eq(
        &mut self,
        a: PointerValue<'ctx>,
        b: PointerValue<'ctx>,
    ) -> Result<IntValue<'ctx>, String> {
        let f = self.str_eq_fn()?;
        let call = self
            .builder
            .build_call(f, &[a.into(), b.into()], "streq")
            .map_err(|e| e.to_string())?;
        match call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => Ok(v.into_int_value()),
            _ => Err("string-equality helper returned void".to_string()),
        }
    }

    /// Emit a call to the string byte-index bounds check.
    fn build_checked_byte_index(
        &mut self,
        i: IntValue<'ctx>,
        n: IntValue<'ctx>,
    ) -> Result<IntValue<'ctx>, String> {
        let f = self.byte_index_fn()?;
        let call = self
            .builder
            .build_call(f, &[i.into(), n.into()], "checked_byte")
            .map_err(|e| e.to_string())?;
        match call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => Ok(v.into_int_value()),
            _ => Err("byte-index helper returned void".to_string()),
        }
    }

    /// Get (or lazily define) `i64 @burxt.checked.byte_index(i64 %i, i64 %n)`.
    /// Separate from the array check only so the message names BYTES — the same
    /// shape otherwise.
    fn byte_index_fn(&mut self) -> Result<FunctionValue<'ctx>, String> {
        if let Some(f) = self.byte_index_check_fn {
            return Ok(f);
        }
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let saved = self.builder.get_insert_block();

        let i64t = self.ctx.i64_type();
        let ptr = self.ctx.ptr_type(AddressSpace::default());
        let i32t = self.ctx.i32_type();
        let fprintf = match self.module.get_function("fprintf") {
            Some(f) => f,
            None => self
                .module
                .add_function("fprintf", i32t.fn_type(&[ptr.into(), ptr.into()], true), None),
        };
        let (stderr_g, _, exit) = self.panic_deps();

        let f = self.module.add_function(
            "burxt.checked.byte_index",
            i64t.fn_type(&[i64t.into(), i64t.into()], false),
            None,
        );
        let entry = self.ctx.append_basic_block(f, "entry");
        let oob_bb = self.ctx.append_basic_block(f, "out_of_bounds");
        let ok_bb = self.ctx.append_basic_block(f, "ok");

        self.builder.position_at_end(entry);
        let i = f.get_nth_param(0).unwrap().into_int_value();
        let n = f.get_nth_param(1).unwrap().into_int_value();
        use inkwell::IntPredicate::*;
        let neg = self.builder.build_int_compare(SLT, i, i64t.const_zero(), "neg").map_err(err)?;
        let big = self.builder.build_int_compare(SGE, i, n, "too_big").map_err(err)?;
        let oob = self.builder.build_or(neg, big, "oob").map_err(err)?;
        self.builder.build_conditional_branch(oob, oob_bb, ok_bb).map_err(err)?;

        self.builder.position_at_end(oob_bb);
        let fmt = self.global_str(
            "burxt runtime error: byte index %lld is out of bounds — this string has \
             %lld bytes (valid indexes 0 to %lld)\n",
            "fmt_byte_oob",
        );
        let stream = self.load_stderr(stderr_g)?;
        let last = self
            .builder
            .build_int_sub(n, i64t.const_int(1, false), "n_minus_1")
            .map_err(err)?;
        let args: Vec<BasicMetadataValueEnum> =
            vec![stream.into(), fmt.into(), i.into(), n.into(), last.into()];
        self.builder.build_call(fprintf, &args, "fprintf").map_err(err)?;
        self.build_exit70(exit)?;

        self.builder.position_at_end(ok_bb);
        self.builder.build_return(Some(&i)).map_err(err)?;

        if let Some(bb) = saved {
            self.builder.position_at_end(bb);
        }
        self.byte_index_check_fn = Some(f);
        Ok(f)
    }

    /// Get (or lazily define) `i64 @burxt.strlen(ptr)`: scan to the NUL.
    ///
    /// Burxt generates its own loop instead of calling libc `strlen` so that
    /// `extern fn strlen` stays available to user code — a builtin must not
    /// quietly consume a C symbol name — and so nothing here depends on libc
    /// for a future wasm target.
    fn str_len_fn(&mut self) -> Result<FunctionValue<'ctx>, String> {
        if let Some(f) = self.str_len_fn {
            return Ok(f);
        }
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let saved = self.builder.get_insert_block();

        let i64t = self.ctx.i64_type();
        let i8t = self.ctx.i8_type();
        let ptr = self.ctx.ptr_type(AddressSpace::default());
        let f = self
            .module
            .add_function("burxt.strlen", i64t.fn_type(&[ptr.into()], false), None);
        let entry = self.ctx.append_basic_block(f, "entry");
        let loop_bb = self.ctx.append_basic_block(f, "scan");
        let next_bb = self.ctx.append_basic_block(f, "next");
        let done_bb = self.ctx.append_basic_block(f, "done");

        let s = f.get_nth_param(0).unwrap().into_pointer_value();
        self.builder.position_at_end(entry);
        self.builder.build_unconditional_branch(loop_bb).map_err(err)?;

        self.builder.position_at_end(loop_bb);
        let i = self.builder.build_phi(i64t, "i").map_err(err)?;
        i.add_incoming(&[(&i64t.const_zero(), entry)]);
        let idx = i.as_basic_value().into_int_value();
        let p = unsafe { self.builder.build_gep(i8t, s, &[idx], "byte_ptr") }.map_err(err)?;
        let c = self.builder.build_load(i8t, p, "byte").map_err(err)?.into_int_value();
        let is_nul = self
            .builder
            .build_int_compare(inkwell::IntPredicate::EQ, c, i8t.const_zero(), "is_nul")
            .map_err(err)?;
        self.builder.build_conditional_branch(is_nul, done_bb, next_bb).map_err(err)?;

        self.builder.position_at_end(next_bb);
        let bumped = self
            .builder
            .build_int_add(idx, i64t.const_int(1, false), "i_next")
            .map_err(err)?;
        i.add_incoming(&[(&bumped, next_bb)]);
        self.builder.build_unconditional_branch(loop_bb).map_err(err)?;

        self.builder.position_at_end(done_bb);
        self.builder.build_return(Some(&idx)).map_err(err)?;

        if let Some(bb) = saved {
            self.builder.position_at_end(bb);
        }
        self.str_len_fn = Some(f);
        Ok(f)
    }

    /// Get (or lazily define) `i64 @burxt.streq(ptr, ptr)`: byte equality,
    /// returning Burxt's 0/1 Bool. Two strings are equal when their bytes are
    /// equal, never because their pointers happen to match.
    fn str_eq_fn(&mut self) -> Result<FunctionValue<'ctx>, String> {
        if let Some(f) = self.str_eq_fn {
            return Ok(f);
        }
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let saved = self.builder.get_insert_block();

        let i64t = self.ctx.i64_type();
        let i8t = self.ctx.i8_type();
        let ptr = self.ctx.ptr_type(AddressSpace::default());
        let f = self.module.add_function(
            "burxt.streq",
            i64t.fn_type(&[ptr.into(), ptr.into()], false),
            None,
        );
        let entry = self.ctx.append_basic_block(f, "entry");
        let loop_bb = self.ctx.append_basic_block(f, "scan");
        let same_bb = self.ctx.append_basic_block(f, "same_byte");
        let next_bb = self.ctx.append_basic_block(f, "next");
        let eq_bb = self.ctx.append_basic_block(f, "equal");
        let ne_bb = self.ctx.append_basic_block(f, "not_equal");

        let a = f.get_nth_param(0).unwrap().into_pointer_value();
        let b = f.get_nth_param(1).unwrap().into_pointer_value();
        self.builder.position_at_end(entry);
        self.builder.build_unconditional_branch(loop_bb).map_err(err)?;

        self.builder.position_at_end(loop_bb);
        let i = self.builder.build_phi(i64t, "i").map_err(err)?;
        i.add_incoming(&[(&i64t.const_zero(), entry)]);
        let idx = i.as_basic_value().into_int_value();
        let pa = unsafe { self.builder.build_gep(i8t, a, &[idx], "a_ptr") }.map_err(err)?;
        let pb = unsafe { self.builder.build_gep(i8t, b, &[idx], "b_ptr") }.map_err(err)?;
        let ca = self.builder.build_load(i8t, pa, "a_byte").map_err(err)?.into_int_value();
        let cb = self.builder.build_load(i8t, pb, "b_byte").map_err(err)?.into_int_value();
        let differs = self
            .builder
            .build_int_compare(inkwell::IntPredicate::NE, ca, cb, "differs")
            .map_err(err)?;
        self.builder.build_conditional_branch(differs, ne_bb, same_bb).map_err(err)?;

        // Bytes match: if this is the terminator, both strings ended together.
        self.builder.position_at_end(same_bb);
        let at_end = self
            .builder
            .build_int_compare(inkwell::IntPredicate::EQ, ca, i8t.const_zero(), "at_end")
            .map_err(err)?;
        self.builder.build_conditional_branch(at_end, eq_bb, next_bb).map_err(err)?;

        self.builder.position_at_end(next_bb);
        let bumped = self
            .builder
            .build_int_add(idx, i64t.const_int(1, false), "i_next")
            .map_err(err)?;
        i.add_incoming(&[(&bumped, next_bb)]);
        self.builder.build_unconditional_branch(loop_bb).map_err(err)?;

        self.builder.position_at_end(eq_bb);
        self.builder.build_return(Some(&i64t.const_int(1, false))).map_err(err)?;
        self.builder.position_at_end(ne_bb);
        self.builder.build_return(Some(&i64t.const_zero())).map_err(err)?;

        if let Some(bb) = saved {
            self.builder.position_at_end(bb);
        }
        self.str_eq_fn = Some(f);
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
    fn pow10_i128(&self, exp: u32) -> IntValue<'ctx> {
        self.ctx
            .i128_type()
            .const_int_arbitrary_precision(&pow10_words(exp))
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

    /// Create a global null-terminated string constant and return an i8* to it.
    fn global_str(&self, s: &str, name: &str) -> PointerValue<'ctx> {
        let gv = self
            .builder
            .build_global_string_ptr(s, name)
            .expect("global string");
        gv.as_pointer_value()
    }

    // (rounding helpers above; printing/IO below)

    /// Report every declared struct's layout: size, alignment and field
    /// offsets. This makes the no-hidden-header guarantee OBSERVABLE — the
    /// object model depends on it, so it is worth being able to check.
    pub fn layout_report(&self, prog: &TypedProgram) -> String {
        let mut out = String::new();
        for s in &prog.structs {
            let ty = Type::Named(s.name.clone());
            let l = self.layout_of(&ty);
            out.push_str(&format!(
                "{}: size {} align {}\n",
                s.name, l.size, l.align
            ));
            for (i, ft) in s.fields.iter().enumerate() {
                out.push_str(&format!(
                    "  +{} {} ({} bytes)\n",
                    l.field_offsets[i],
                    ft,
                    self.layout_of(ft).size
                ));
            }
        }
        out
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

/// Is this type an aggregate (multi-field or multi-element)?
/// The boundary is decided by the TYPE, never by size, so it is identical on
/// every target.
pub fn is_aggregate(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Named(_) | Type::Array { .. } | Type::Dyn(_) | Type::Slice(_)
    )
}

/// The memory layout of an aggregate: exactly its declared fields, in
/// declaration order, standard alignment padding between them, and NOTHING
/// else — no type tag, no vtable pointer, no refcount, no hidden header.
///
/// This is the forward guarantee the object model depends on: a field's offset
/// is a pure function of the declared field types and order, so adding a trait
/// implementation later can never move a field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub size: u64,
    pub align: u64,
    pub field_offsets: Vec<u64>,
}

/// 10^scale as the 64-bit words LLVM wants for an i128 constant.
/// scale is capped at 18, so the value always fits in the low word.
fn pow10_words(exp: u32) -> [u64; 2] {
    // Mixed-scale multiplication can need 10^(s1+s2), up to 10^36, which no
    // longer fits one 64-bit word.
    let v: u128 = 10u128.pow(exp);
    [(v & u64::MAX as u128) as u64, (v >> 64) as u64]
}

/// The scale of a Decimal type; a codegen bug anywhere else.
fn decimal_scale(ty: &Type) -> Result<u32, String> {
    match ty {
        Type::Decimal { scale, .. } => Ok(*scale),
        other => Err(format!("codegen bug: expected a Decimal, got {}", other)),
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
