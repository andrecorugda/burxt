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
use crate::typeck::{
    TypedExpr, TypedExprKind, TypedFn, TypedMethod, TypedProgram, TypedStmt, TypedStmtKind,
};
use inkwell::types::StructType;

use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::types::{AnyType, BasicMetadataTypeEnum, BasicType, BasicTypeEnum};
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, FloatValue, FunctionValue, IntValue, PointerValue};
use inkwell::debug_info::AsDIScope;
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
    /// the range-checked i64 -> double conversion, emitted only if used
    cdouble_fn: Option<FunctionValue<'ctx>>,
    /// lazily created array bounds-check helper
    index_check_fn: Option<FunctionValue<'ctx>>,
    /// lazily created i128 -> i64 checked narrowing helper
    narrow_check_fn: Option<FunctionValue<'ctx>>,
    /// lazily created string byte-scan helpers
    /// the bump cursor for region allocation — ONE logical offset across every chunk, so a
    /// region mark is an integer and the chunk table is `alloc_fn`'s business alone
    heap: Option<inkwell::values::GlobalValue<'ctx>>,
    alloc_fn: Option<FunctionValue<'ctx>>,
    /// M17's two runtime functions and the fingerprint their tags mix in, all lazily made —
    /// a program that never holds a value pays for none of it.
    hold_fn: Option<FunctionValue<'ctx>>,
    held_fn: Option<FunctionValue<'ctx>>,
    module_fingerprint: Option<u64>,
    byte_index_check_fn: Option<FunctionValue<'ctx>>,
    str_eq_fn: Option<FunctionValue<'ctx>>,
    /// lazily created UTF-8 validator, B5 — emitted only into programs that let text IN
    utf8_check_fn: Option<FunctionValue<'ctx>>,
    /// B50. C symbols a program declared with a signature the compiler disagrees with.
    libc_conflicts: Vec<String>,
    /// user fn name -> (param types, return type), for aggregate call lowering
    fn_sigs: HashMap<String, (Vec<Type>, Type)>,
    /// Which of each function's parameters were declared `mutable`, by name.
    ///
    /// Needed at the CALL SITE and not only at the declaration, because LLVM requires `byval` on
    /// both — and mirroring it from `fn_sigs`, which carries only types, is exactly how the first
    /// version of `mutable` parameters silently kept copying. The declaration said pointer, the call
    /// said `byval`, and the caller saw nothing change.
    fn_writable: HashMap<String, Vec<bool>>,
    /// Which stream the print statement being emitted goes to.
    ///
    /// A flag rather than a parameter threaded through seven call sites, because every one of those
    /// sites is inside the per-type formatter and none of them has any other reason to know. Saved
    /// and restored around each statement, so nothing leaks into the next one.
    print_to_stderr: bool,
    /// (receiver, method name) -> its LLVM function (mangled `bx.<Recv>.<method>`)
    methods: HashMap<(String, String), FunctionValue<'ctx>>,
    /// (trait, concrete) -> its static vtable global
    vtables: HashMap<(String, String), inkwell::values::GlobalValue<'ctx>>,
    /// interface name -> method return types in slot order, for indirect calls
    interface_slots: HashMap<String, Vec<(Vec<Type>, Type)>>,
    /// the hidden sret pointer of the function being generated, if it returns
    /// an aggregate
    current_sret: Option<PointerValue<'ctx>>,
    /// The termination measure of the function being generated: its slot (holding
    /// this invocation's value), the measure expression, the parameter names it is
    /// written in terms of, the clause text, and the function's name.
    ///
    /// The parameter names are the whole trick: at a recursive call, binding them to
    /// the ARGUMENTS and re-evaluating the measure gives the callee's measure without
    /// rewriting a single expression.
    current_measure: Option<MeasureState<'ctx>>,
    /// One slot per `old(...)` expression in the function being generated, filled
    /// on entry. A clause reads the slot rather than re-evaluating, which is the
    /// point: the value has to be the one from BEFORE the body ran.
    old_slots: Vec<(PointerValue<'ctx>, Type)>,
    /// The enclosing loops of the statement being generated: where `continue` goes,
    /// where `break` goes, and how deep `region_marks` stood when the loop started —
    /// which is what lets a jump tell a block opened INSIDE the loop from one that
    /// encloses it.
    loop_stack: Vec<(inkwell::basic_block::BasicBlock<'ctx>, inkwell::basic_block::BasicBlock<'ctx>, usize)>,
    /// How many `for` loops have been lowered, so each hidden index gets its own name.
    desugared_loops: usize,
    /// The postconditions of the function being generated, with the name of that
    /// function: every `return` has to check them, and the check needs both the
    /// clause and the name to write its message.
    current_ensures: Vec<(crate::typeck::TypedContract, String)>,
    /// argc and argv, stashed by `main` so any function can read them
    arguments: Option<(inkwell::values::GlobalValue<'ctx>, inkwell::values::GlobalValue<'ctx>)>,
    /// The bump-cursor marks of the regions open here, outermost first, so a `return`
    /// from inside them releases every one on the way out.
    ///
    /// A Vec since A12, and one entry per releasing block rather than the single slot
    /// M1 needed for a `region`. The allocator is unchanged and so is the mechanism:
    /// each entry is where the cursor stood, and putting it back IS the deallocation.
    /// Because the cursor is LIFO, restoring the OUTERMOST of several marks releases
    /// all of them at once — which is why leaving by `return` needs one store and not
    /// one per level.
    ///
    /// Per function: emptied on entry to each body, so a mark can never be restored
    /// from inside a different frame.
    region_marks: Vec<IntValue<'ctx>>,
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
    /// DWARF, present only when `-g` was asked for. C1.
    ///
    /// `Option` rather than a flag, so there is no way to emit half a line table: every
    /// call site is `if let Some(d) = &self.debug`, and a build without `-g` produces
    /// exactly the module it produced before this existed. That is what keeps the
    /// self-hosting fixpoint and `the_ir_is_the_same_for_every_target` untouched.
    debug: Option<DebugInfo<'ctx>>,
}

/// Everything DWARF needs, built once per module when `-g` is given.
struct DebugInfo<'ctx> {
    builder: inkwell::debug_info::DebugInfoBuilder<'ctx>,
    unit: inkwell::debug_info::DICompileUnit<'ctx>,
    /// One `DIFile` per source file, in the order `main.rs` loaded them — so a
    /// breakpoint in a `use`d module names THAT module rather than an offset into a
    /// concatenated buffer nobody wrote.
    files: Vec<inkwell::debug_info::DIFile<'ctx>>,
    /// Per file, parallel to `files`: where it starts in the buffer, where it ends, and
    /// the buffer offset of each of its lines. Binary-searched, because a compile of
    /// `lib/json.bx` asks this question once per statement.
    extents: Vec<(usize, usize, Vec<usize>)>,
    /// Printed Burxt type -> its `DIType`. A program with four hundred `Int` locals
    /// should describe `Int` once.
    ///
    /// A `RefCell` because building a type needs `&self` for `layout_of` and
    /// `struct_fields` at the same time as it needs the cache.
    types: std::cell::RefCell<HashMap<String, inkwell::debug_info::DIType<'ctx>>>,
    /// Where each function was DECLARED, by name — `Recv.name` for a method. Built by
    /// `main.rs` from the untyped AST, which is the only tree that still knows: the
    /// typed `TypedFn` carries a body and no position of its own.
    decls: HashMap<String, crate::diag::Span>,
    /// The function being generated: its subprogram, and which file it is written in.
    current: Option<(inkwell::debug_info::DISubprogram<'ctx>, usize)>,
    /// The lexical scopes open here, outermost first — the subprogram, then one per
    /// nested block. Locations and variables hang off the innermost.
    ///
    /// Without this every local in a function belongs to the function, and `info locals`
    /// inside the second of two sibling blocks shows BOTH of their `x`es, one of them
    /// holding whatever the stack had. Burxt forbids shadowing, so this cannot happen
    /// between a block and its parent — only between siblings, which is exactly the case
    /// nobody writes a fixture for.
    scopes: Vec<inkwell::debug_info::DIScope<'ctx>>,
    /// Was the IR pipeline going to run? DWARF records it, and a debugger tells the user
    /// when the code it is stepping through has been optimised.
    optimised: bool,
}

/// How many values a host may hold at once. A power of two so the slot is a mask, and reused
/// round-robin — a UI builds a model per keystroke, so what matters is that reuse is SAFE
/// (the generation moves) rather than that the table is large.
const HANDLE_SLOTS: u64 = 1024;

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
            cdouble_fn: None,
            index_check_fn: None,
            narrow_check_fn: None,
            heap: None,
            alloc_fn: None,
            hold_fn: None,
            held_fn: None,
            module_fingerprint: None,
            byte_index_check_fn: None,
            str_eq_fn: None,
            utf8_check_fn: None,
            libc_conflicts: Vec::new(),
            fn_sigs: HashMap::new(),
            fn_writable: HashMap::new(),
            print_to_stderr: false,
            methods: HashMap::new(),
            vtables: HashMap::new(),
            interface_slots: HashMap::new(),
            current_sret: None,
            arguments: None,
            region_marks: Vec::new(),
            current_ensures: Vec::new(),
            loop_stack: Vec::new(),
            desugared_loops: 0,
            old_slots: Vec::new(),
            current_measure: None,
            struct_types: HashMap::new(),
            struct_fields: HashMap::new(),
            enum_types: HashMap::new(),
            printf: None,
            panic_deps: None,
            debug: None,
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

        // Enums are a tag plus an inline payload area: `{ i64, [N x i64] }`, where N is the
        // widest variant measured in CELLS.
        //
        // It used to be the payload COUNT, on the stated assumption that every payload is 8 bytes
        // because payloads are scalars. Once a class could be a payload that assumption became a
        // bug with a precise shape: `Line(Point, Point)` gave each Point one cell, so the second
        // overlapped the first's second field and the area was half the size it needed.
        //
        // Two passes, because a payload may be a class or another enum whose own width has to be
        // known first — `payload_cells` reads `struct_fields` and `enum_types`, so every shell must
        // exist before any body is sized.
        for en in &prog.enums {
            let st = self.ctx.opaque_struct_type(&format!("bx.enum.{}", en.name));
            self.enum_types.insert(en.name.clone(), (st, en.variants.clone()));
        }
        for s in &prog.structs {
            self.struct_fields.insert(s.name.clone(), s.fields.clone());
        }
        for en in &prog.enums {
            let i64t = self.ctx.i64_type();
            let slots = self.payload_area(&en.variants);
            let st = self.enum_types[en.name.as_str()].0;
            if slots == 0 {
                st.set_body(&[i64t.into()], false);
            } else {
                st.set_body(&[i64t.into(), i64t.array_type(slots).into()], false);
            }
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
                e.parameters.iter().map(|t| self.llvm_type(t).into()).collect();
            let fn_ty = self.llvm_type(&e.ret).fn_type(&param_tys, false);
            // Two modules may declare the same extern — `lib/fs.bx` and `lib/os.bx` both
            // need `system` — and the typechecker allows it when the signatures match.
            // Adding it twice here would let LLVM rename the second to `system.1`, and
            // the linker would ask for a symbol nobody has.
            let llf = match self.module.get_function(&e.name) {
                Some(existing) => existing,
                None => self.module.add_function(&e.name, fn_ty, None),
            };
            self.user_fns.insert(e.name.clone(), llf);
            self.extern_sigs.insert(e.name.clone(), (e.parameters.clone(), e.ret.clone()));
        }

        // Declare every user function up front (mutual recursion, any order).
        for f in &prog.fns {
            let param_tys: Vec<Type> = f.parameters.iter().map(|(_, t)| t.clone()).collect();
            let llf = self.declare_fn(
                &format!("bx.{}", f.name),
                &param_tys,
                &f.ret,
                &f.writable,
            );
            self.user_fns.insert(f.name.clone(), llf);
            self.fn_sigs.insert(f.name.clone(), (param_tys, f.ret.clone()));
            self.fn_writable.insert(f.name.clone(), f.writable.clone());
        }

        // Methods: namespaced by (receiver, name), mangled `bx.<Recv>.<name>`.
        for m in &prog.methods {
            let param_tys: Vec<Type> = m.parameters.iter().map(|(_, t)| t.clone()).collect();
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
        // Shared by every interface object of that pair — the instance carries
        // only two words. Emitted BEFORE any body, since a body may build a
        // interface object or dispatch through one.
        // **The signature of an interface method comes from the INTERFACE, not from whoever
        // happened to implement it.** This used to be read off a vtable, which meant a program
        // that declared an interface, took a `dynamic` of it, and implemented it NOWHERE had no
        // signature to build an indirect call from — and said so as `codegen bug: no signature
        // for Greeter.greet`, an internal error reaching a user for writing eight ordinary lines.
        //
        // It is reachable through the standard library: `lib/http.bx` declares `Handler` and
        // `http_serve` takes one, so any program using only the CLIENT half hit it. Present in
        // v1.3.0 and every release before it, because nothing had declared an interface a
        // consumer might not implement until a library did.
        //
        // The declaration is also the right source on its own terms: an impl matches the
        // signature, so reading it from an impl is reading a copy.
        self.interface_slots = prog.interface_slots.clone();

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
                &format!("bx.vtable.{}.{}", vt.interface_name, vt.concrete),
            );
            global.set_initializer(&ptr.const_array(&fns));
            global.set_constant(true);
            self.vtables
                .insert((vt.interface_name.clone(), vt.concrete.clone()), global);

            // Record each slot's signature once, so an indirect call can build the right
            // function type. Every impl of an interface matches the trait's signatures exactly,
            // so the first one speaks for all — **and when there are NO impls, none of them
            // does.** That is filled in from the declarations below, before any of this runs.
            self.interface_slots.entry(vt.interface_name.clone()).or_insert_with(|| {
                vt.slots
                    .iter()
                    .map(|m| {
                        let f = prog
                            .methods
                            .iter()
                            .find(|tm| tm.receiver == vt.concrete && tm.name == *m)
                            .expect("vtable slot method must exist");
                        (
                            f.parameters.iter().map(|(_, t)| t.clone()).collect::<Vec<_>>(),
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

        // define: i32 @main(i32 %argc, ptr %argv)
        //
        // Taken even by programs that never look at them: a compiler needs to know
        // which file it was asked to compile, and the C runtime only offers that here.
        let ptr_ty = self.ctx.ptr_type(AddressSpace::default());
        let main_ty = i32t.fn_type(&[i32t.into(), ptr_ty.into()], false);
        let main_fn = self.module.add_function("main", main_ty, None);
        // The top level has no declaration to point at — it IS the file — so `main`'s
        // subprogram starts at the first statement the programmer wrote. That is also
        // where `break main` should land, and it is a real line rather than line 1 of a
        // file whose first lines are `use` declarations.
        let main_at = prog.stmts.first().map(|s| s.span).unwrap_or_else(|| crate::diag::Span::new(0, 0));
        self.begin_subprogram(main_fn, "main", main_at, &[], &Type::Int);
        let entry = self.ctx.append_basic_block(main_fn, "entry");
        self.builder.position_at_end(entry);
        self.set_debug_location(main_at);
        // B7. Before anything else runs, so every function that checks has something to check.
        self.build_stack_floor()?;
        self.vars.clear();
        // A body starts with no mark of its own: a region is released inside the frame
        // that opened it, never across a call.
        self.region_marks.clear();

        let (argc_g, argv_g) = self.args_globals();
        let argc = main_fn.get_nth_param(0).unwrap().into_int_value();
        let argc64 = self
            .builder
            .build_int_s_extend(argc, self.ctx.i64_type(), "argc64")
            .map_err(|e| e.to_string())?;
        self.builder
            .build_store(argc_g.as_pointer_value(), argc64)
            .map_err(|e| e.to_string())?;
        self.builder
            .build_store(argv_g.as_pointer_value(), main_fn.get_nth_param(1).unwrap())
            .map_err(|e| e.to_string())?;

        for stmt in &prog.stmts {
            self.gen_stmt(stmt)?;
        }

        // return 0  (main returns i32)
        self.builder
            .build_return(Some(&i32t.const_int(0, false)))
            .map_err(|e| e.to_string())?;
        self.end_subprogram();

        // BEFORE the verifier, not after: `finalize` resolves the temporary metadata
        // nodes the builder left behind, and the verifier rejects a module that still
        // holds one. Getting this order wrong fails loudly, which is the good case.
        self.finalize_debug_info();

        // B50. Reported BEFORE the verifier, because the verifier is exactly what would report it
        // otherwise — in its own words, about a call the programmer did not write. A conflict here
        // is a fact about their `external function` declaration, and it should read like one.
        if !self.libc_conflicts.is_empty() {
            self.libc_conflicts.sort();
            self.libc_conflicts.dedup();
            return Err(format!(
                "`external function {}` declares a signature the compiler disagrees with. The \
                 Burxt runtime calls `{}` itself, and one C symbol cannot have two signatures — so \
                 either the declaration is wrong about what `{}` takes, or it wants a different \
                 function and needs a differently-named C wrapper. Declaring it identically to the \
                 runtime's own use is fine and is what `lib/os.bx` does with `malloc`.",
                self.libc_conflicts.join("`, `external function "),
                self.libc_conflicts[0],
                self.libc_conflicts[0]
            ));
        }

        // verify the module — catches malformed IR early
        self.module
            .verify()
            .map_err(|e| format!("LLVM module verification failed:\n{}", e.to_string()))?;

        Ok(())
    }

    fn gen_fn(&mut self, f: &TypedFn) -> Result<(), String> {
        let llf = self.user_fns[&f.name];
        let at = self.decl_span(&f.name, &f.body);
        self.begin_subprogram(llf, &f.name, at, &f.parameters, &f.ret);
        let entry = self.ctx.append_basic_block(llf, "entry");
        self.builder.position_at_end(entry);
        // The prologue — parameter spills, `old(...)` captures, contract checks — is
        // attributed to the declaration until a clause or a statement says otherwise.
        // Not cosmetic: LLVM's verifier REFUSES a call to a function with debug info
        // from a function with debug info when the call carries no location, so an
        // unlocated prologue is a build failure waiting for the first `pure` call in a
        // contract. It was found that way.
        self.set_debug_location(at);
        // B7. First thing in the function, before the parameter spills, so a frame that is about
        // to run out of stack says so instead of faulting inside its own prologue.
        self.build_stack_guard()?;
        self.vars.clear();
        // A body starts with no mark of its own: a region is released inside the frame
        // that opened it, never across a call.
        self.region_marks.clear();

        // An aggregate return arrives as a hidden sret pointer in slot 0.
        let ret_is_agg = is_aggregate(&f.ret);
        self.current_sret = if ret_is_agg {
            Some(llf.get_nth_param(0).unwrap().into_pointer_value())
        } else {
            None
        };
        let offset = if ret_is_agg { 1 } else { 0 };

        for (i, (name, ty)) in f.parameters.iter().enumerate() {
            let argument = llf.get_nth_param((i + offset) as u32).unwrap();
            if is_aggregate(ty) {
                // byval already gave us a pointer to our OWN copy, so it is
                // the variable's slot directly — no second copy needed, and
                // writing through it cannot touch the caller's value.
                self.vars.insert(name.clone(), (argument.into_pointer_value(), ty.clone()));
            } else {
                // Spill scalars so they behave like any other binding.
                let slot = self.create_entry_alloca(name, ty)?;
                self.builder.build_store(slot, argument).map_err(|e| e.to_string())?;
                self.vars.insert(name.clone(), (slot, ty.clone()));
            }
            // An aggregate parameter's slot IS the caller's storage (byval or `mutable`),
            // and a scalar's is the spill above — either way the name now has an address,
            // which is the only thing `dbg.declare` needs. `i + 1`: DWARF numbers
            // arguments from one, and the hidden sret pointer is not one of the
            // programmer's.
            let (slot, _) = self.vars[name];
            self.declare_variable(name, ty, slot, at, Some((i + 1) as u32));
        }

        self.gen_contract_prologue(&f.requires, &f.ensures, &f.olds, &f.name, at)?;
        self.gen_measure_prologue(f)?;

        for stmt in &f.body {
            self.gen_stmt(stmt)?;
        }
        self.current_sret = None;
        self.current_ensures.clear();
        self.old_slots.clear();
        self.current_measure = None;
        self.end_subprogram();
        // The typechecker proved every path ends in `return`, so the current
        // block is already terminated — no fallthrough ret is needed.
        Ok(())
    }

    fn gen_stmt(&mut self, stmt: &TypedStmt) -> Result<(), String> {
        // Every instruction from here down is attributed to this statement, until the next
        // one moves it. That is the whole line table: LLVM stamps whatever location the
        // builder is carrying onto each instruction it creates. Off unless `-g` was asked
        // for, in which case it is a no-op.
        self.set_debug_location(stmt.span);
        // The `for` lowerings below build statements the programmer never wrote. Each is
        // blamed on the `for` it came from rather than given a position of its own, so a
        // hidden counter update reports the loop's line instead of somewhere it isn't.
        let syn = |kind| TypedStmt::new(kind, stmt.span);
        match &stmt.kind {
            // `exit(code)` — the status a shell reads, and the reason this is not an
            // `external function exit`: the runtime owns that symbol (it is what a contract failure
            // calls), so declaring it was refused and a CLI had no way to report failure at all.
            //
            // The range check is the interesting part. POSIX hands the shell only the LOW EIGHT
            // BITS, so `exit(256)` arrives as 0 — a program reporting SUCCESS while trying to report
            // failure, which is the worst possible direction for this particular mistake. A literal
            // is refused by the checker; anything computed dies here, naming the range.
            TypedStmtKind::Exit(code) => {
                let value = self.gen_expr(code)?.into_int_value();
                let i64t = self.ctx.i64_type();
                let i32t = self.ctx.i32_type();
                let err = |e: inkwell::builder::BuilderError| e.to_string();
                // One unsigned comparison catches both ends: a negative status wraps to something
                // enormous, so `code as u64 > 255` is `code < 0 || code > 255`.
                let out_of_range = self
                    .builder
                    .build_int_compare(
                        inkwell::IntPredicate::UGT,
                        value,
                        i64t.const_int(255, false),
                        "status_out_of_range",
                    )
                    .map_err(err)?;
                let function = self
                    .builder
                    .get_insert_block()
                    .and_then(|b| b.get_parent())
                    .ok_or("codegen bug: exit outside a function")?;
                let bad = self.ctx.append_basic_block(function, "status_bad");
                let ok = self.ctx.append_basic_block(function, "status_ok");
                self.builder.build_conditional_branch(out_of_range, bad, ok).map_err(err)?;
                self.builder.position_at_end(bad);
                self.build_panic(
                    "burxt runtime error: a process status is 0 to 255 — a shell reads only the \
                     low eight bits\n",
                )?;
                self.builder.position_at_end(ok);
                let (_, _, exit) = self.panic_deps();
                let narrow = self
                    .builder
                    .build_int_truncate(value, i32t, "status")
                    .map_err(err)?;
                self.builder.build_call(exit, &[narrow.into()], "exit").map_err(err)?;
                // `exit` does not return, and LLVM has to be told or it assumes the next block is
                // reachable and the function falls off its end.
                self.builder.build_unreachable().map_err(err)?;
                // Anything after `exit(...)` is dead, but the emitter still walks it, so it needs a
                // block to walk into.
                let after = self.ctx.append_basic_block(function, "after_exit");
                self.builder.position_at_end(after);
                Ok(())
            }
            TypedStmtKind::Let { name, ty, value } => {
                // An array is built in place: alloca once, store per element.
                if let TypedExprKind::ArrayLit(elems) = &value.kind {
                    let slot = self.create_entry_alloca(name, ty)?;
                    self.store_array_elements(slot, ty, elems)?;
                    self.vars.insert(name.clone(), (slot, ty.clone()));
                    self.declare_variable(name, ty, slot, stmt.span, None);
                    return Ok(());
                }
                let val = self.gen_expr(value)?;
                let slot = self.create_entry_alloca(name, ty)?;
                self.builder.build_store(slot, val).map_err(|e| e.to_string())?;
                self.vars.insert(name.clone(), (slot, ty.clone()));
                // AFTER the store, so the declaration marks the point the name means
                // something. Declaring it at the alloca would let a debugger stopped on
                // this line print whatever the stack happened to hold.
                self.declare_variable(name, ty, slot, stmt.span, None);
                Ok(())
            }
            TypedStmtKind::Assign { name, value } => {
                let val = self.gen_expr(value)?;
                let (slot, _) = *self
                    .vars
                    .get(name)
                    .ok_or_else(|| format!("codegen: unknown variable {}", name))?;
                self.builder.build_store(slot, val).map_err(|e| e.to_string())?;
                Ok(())
            }
            TypedStmtKind::AssignField { name, indices, value } => {
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
                                "codegen bug: field assignment through non-class {}",
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
            TypedStmtKind::AssignFieldIndex { name, indices, len, index, value } => {
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
                                "codegen bug: field path through non-class {}",
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
                let err = |e: inkwell::builder::BuilderError| e.to_string();
                // A growable array's bound is its header's length, and its elements live in
                // the region rather than in the struct.
                if let Type::Slice(elem) = &cur_ty {
                    let st = self.llvm_type(&cur_ty).into_struct_type();
                    let ptr_ty = self.ctx.ptr_type(AddressSpace::default());
                    let data_p =
                        self.builder.build_struct_gep(st, cur_ptr, 0, "data_p").map_err(err)?;
                    let len_p =
                        self.builder.build_struct_gep(st, cur_ptr, 1, "len_p").map_err(err)?;
                    let data = self.builder.build_load(ptr_ty, data_p, "data").map_err(err)?;
                    let live =
                        self.builder.build_load(i64t, len_p, "live").map_err(err)?.into_int_value();
                    let checked = self.build_checked_index(idx_val, live)?;
                    let ll_elem = self.llvm_type(elem);
                    let elem_ptr = unsafe {
                        self.builder.build_gep(
                            ll_elem,
                            data.into_pointer_value(),
                            &[checked],
                            "elem_ptr",
                        )
                    }
                    .map_err(err)?;
                    self.builder.build_store(elem_ptr, val).map_err(err)?;
                    return Ok(());
                }
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
            TypedStmtKind::AssignIndex { name, len, index, value } => {
                let val = self.gen_expr(value)?;
                let ptr = self.gen_element_ptr(name, *len, index)?;
                self.builder.build_store(ptr, val).map_err(|e| e.to_string())?;
                Ok(())
            }
            TypedStmtKind::ExprStmt(e) => {
                self.gen_expr(e)?;
                Ok(())
            }
            TypedStmtKind::For { name, elem, iterable, body } => {
                // Lowered HERE rather than in the parser, so the checker could give `for`
                // its own errors instead of complaining about a `len` call the author
                // never wrote. The index is named `for$N`; `$` is not a byte an
                // identifier may contain, so it cannot collide, and N lets loops nest.
                let index = format!("for${}", self.desugared_loops);
                self.desugared_loops += 1;
                let int = |kind| TypedExpr { ty: Type::Int, kind };
                let idx = || int(TypedExprKind::Var(index.clone()));
                let (bound, read) = match &iterable.ty {
                    Type::Array { len, .. } => (
                        int(TypedExprKind::IntLit(*len as i64)),
                        TypedExprKind::Index {
                            base: Box::new(iterable.clone()),
                            len: *len,
                            index: Box::new(idx()),
                        },
                    ),
                    Type::Slice(_) => (
                        int(TypedExprKind::SliceLen(Box::new(iterable.clone()))),
                        TypedExprKind::SliceIndex {
                            base: Box::new(iterable.clone()),
                            index: Box::new(idx()),
                        },
                    ),
                    other => return Err(format!("codegen bug: `for` over {}", other)),
                };

                let saved = self.vars.clone();
                self.gen_stmt(&syn(TypedStmtKind::Let {
                    name: index.clone(),
                    ty: Type::Int,
                    value: int(TypedExprKind::IntLit(0)),
                }))?;
                let mut inner = Vec::with_capacity(body.len() + 2);
                inner.push(syn(TypedStmtKind::Let {
                    name: name.clone(),
                    ty: elem.clone(),
                    value: TypedExpr { ty: elem.clone(), kind: read },
                }));
                // The advance comes BEFORE the body. `continue` jumps to the condition,
                // so an increment at the bottom is skipped and the loop never ends — one
                // hung test taught me that, and it is why a lowering has to be read
                // against every control-flow statement the language has.
                inner.push(syn(TypedStmtKind::Assign {
                    name: index.clone(),
                    value: int(TypedExprKind::Binary {
                        op: BinOp::Add,
                        lhs: Box::new(idx()),
                        rhs: Box::new(int(TypedExprKind::IntLit(1))),
                    }),
                }));
                inner.extend(body.iter().cloned());
                let cond = TypedExpr {
                    ty: Type::Bool,
                    kind: TypedExprKind::Compare {
                        op: CmpOp::Lt,
                        lhs: Box::new(idx()),
                        rhs: Box::new(bound),
                    },
                };
                let r = self.gen_while(&cond, &inner);
                self.vars = saved;
                r
            }
            TypedStmtKind::ForRange { name, start, end, body } => {
                // Lowered to EXACTLY the hand-written idiom it replaces:
                //
                //     let for$N_end = <end>;              // once, before the loop
                //     let for$N     = <start>;            // once, before the loop
                //     while for$N < for$N_end {
                //         let i = for$N;                  // a fresh, immutable copy
                //         for$N = for$N + 1;
                //         <body>
                //     }
                //
                // Two i64 stack slots and an `icmp slt`. No allocation, no iterator, no
                // heap object — the same machine code as `while i < len(xs)`, which is the
                // whole reason A6 is worth doing as a lowering rather than as a library.
                //
                // The end is stored in a slot rather than re-emitted in the condition, and
                // that is the load-bearing difference from the array `for` directly above:
                // that one re-reads the array header every pass, deliberately, so a `push`
                // from the body is seen. A range's end may be any expression — `len(xs)`,
                // `a.b.c`, arithmetic — and re-emitting it would run it once per pass.
                // Evaluated once is also the only answer that makes `0..len(xs)` cost what
                // the reader thinks it costs. See ast::StmtKind::ForRange decision 4.
                //
                // `$` is not a byte an identifier may contain, so neither synthesized name
                // can collide with a program's own, and the counter lets loops nest.
                let index = format!("for${}", self.desugared_loops);
                let limit = format!("for${}$end", self.desugared_loops);
                self.desugared_loops += 1;
                let int = |kind| TypedExpr { ty: Type::Int, kind };
                let idx = || int(TypedExprKind::Var(index.clone()));

                let saved = self.vars.clone();
                self.gen_stmt(&syn(TypedStmtKind::Let {
                    name: limit.clone(),
                    ty: Type::Int,
                    value: end.clone(),
                }))?;
                self.gen_stmt(&syn(TypedStmtKind::Let {
                    name: index.clone(),
                    ty: Type::Int,
                    value: start.clone(),
                }))?;
                let mut inner = Vec::with_capacity(body.len() + 2);
                inner.push(syn(TypedStmtKind::Let {
                    name: name.clone(),
                    ty: Type::Int,
                    value: idx(),
                }));
                // The advance comes BEFORE the body, for the reason the array `for` above
                // records: `continue` jumps to the CONDITION, so an increment at the bottom
                // is skipped and the loop never ends. The counter is read into `name` first,
                // so the body still sees the pass it is on.
                inner.push(syn(TypedStmtKind::Assign {
                    name: index.clone(),
                    value: int(TypedExprKind::Binary {
                        op: BinOp::Add,
                        lhs: Box::new(idx()),
                        rhs: Box::new(int(TypedExprKind::IntLit(1))),
                    }),
                }));
                inner.extend(body.iter().cloned());
                let cond = TypedExpr {
                    ty: Type::Bool,
                    kind: TypedExprKind::Compare {
                        op: CmpOp::Lt,
                        lhs: Box::new(idx()),
                        rhs: Box::new(int(TypedExprKind::Var(limit.clone()))),
                    },
                };
                // `<`, not `<=`: the end is exclusive, and this one character is where that
                // decision actually lives. `0..0` and `3..0` both fail the test at entry and
                // run zero times — no guard needed, which is why the empty range needed no
                // code at all.
                let r = self.gen_while(&cond, &inner);
                self.vars = saved;
                r
            }
            // Mark where the bump pointer stands, run the body, then reset to the mark
            // — the whole block released in O(1), with no per-object free, no refcount,
            // and no collector.
            //
            // The two spellings are one mechanism. `Region` is the word the programmer
            // wrote; `Release` is an ordinary block the escape analysis proved keeps
            // nothing (A12). Neither knows anything the other does not, which is the
            // point: per-block release adds no runtime machinery, it only puts the
            // existing mark-and-restore somewhere else.
            TypedStmtKind::Region { name, body } => {
                let _ = name;
                self.gen_released_block(body)
            }
            TypedStmtKind::Release { body } => self.gen_released_block(body),
            TypedStmtKind::Match { value, arms } => {
                let err = |e: inkwell::builder::BuilderError| e.to_string();
                let i64t = self.ctx.i64_type();
                let enum_name = match &value.ty {
                    Type::Named(n) => n.clone(),
                    other => {
                        return Err(format!("codegen bug: match on {}", other))
                    }
                };
                let (st, variants) = self.enum_types[enum_name.as_str()].clone();
                let slots = self.payload_area(&variants);

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
                        // The SAME offsets construction used, which is why they come from one
                        // function: a variant that stored its payloads at cell offsets and read
                        // them back at indices would be a silent misread, not a crash.
                        let (offsets, _) = self.payload_offsets(&variants[arm.tag as usize]);
                        for (i, (name, ty)) in arm.bindings.iter().enumerate() {
                            let p = unsafe {
                                self.builder.build_in_bounds_gep(
                                    arr_ty,
                                    payload_ptr,
                                    &[i64t.const_zero(),
                                      i64t.const_int(offsets[i] as u64, false)],
                                    "slot",
                                )
                            }
                            .map_err(err)?;
                            // Loaded at the binding's own type, so a String comes back as a
                            // pointer and a class comes back as a whole record.
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
            // `break` and `continue` are jumps to blocks the enclosing loop set up.
            // Anything opened INSIDE the loop — a `region`, or the loop body itself
            // once A12 proved it keeps nothing — has to be released by either jump,
            // exactly as `return` does. Anything that ENCLOSES the loop must not be
            // touched, because the jump stays inside it. The loop records the depth it
            // started at, so the two cases are distinguishable rather than guessed.
            //
            // Releasing the body's own block here is what makes `continue` in a loop
            // that builds and discards constant-memory rather than merely usually so.
            TypedStmtKind::Break | TypedStmtKind::Continue => {
                let (cond_bb, end_bb, depth_at_entry) = *self
                    .loop_stack
                    .last()
                    .ok_or("codegen bug: `break` outside a loop")?;
                self.close_regions_below(depth_at_entry)?;
                let target = if matches!(stmt.kind, TypedStmtKind::Break) { end_bb } else { cond_bb };
                self.builder
                    .build_unconditional_branch(target)
                    .map_err(|e| e.to_string())?;
                Ok(())
            }
            TypedStmtKind::While { cond, body } => self.gen_while(cond, body),
            // One statement, two destinations. `to_stderr` is set for the whole statement and read
            // by `emit_print_call`, so every arm of the per-type formatter below serves both — which
            // is the point of not having a second statement: the first time one formatter learned
            // about a new type, the other would print something different for the same value.
            TypedStmtKind::Print { value, to_stderr, newline } => {
                let outer = std::mem::replace(&mut self.print_to_stderr, *to_stderr);
                let r = self.gen_print(value, *newline);
                self.print_to_stderr = outer;
                r
            }
            TypedStmtKind::PrintInterp { parts, to_stderr, newline } => {
                // Emit each piece in order — no intermediate String is built,
                // so this needs no allocation.
                let outer = std::mem::replace(&mut self.print_to_stderr, *to_stderr);
                let r = (|| -> Result<(), String> {
                    for p in parts {
                        match p {
                            crate::typeck::TypedInterpPart::Lit(text) => {
                                // Literal text is an ARGUMENT to %s, never the
                                // format string — a `%` in it must stay harmless.
                                let s = self.global_str(&text.clone(), "interp_lit");
                                let fmt = self.global_str("%s", "fmt_interp");
                                self.emit_print_call(&[fmt.into(), s.into()], "printf_lit")?;
                            }
                            crate::typeck::TypedInterpPart::Expr(e) => self.gen_print_value(e)?,
                        }
                    }
                    if *newline {
                        return self.gen_newline();
                    }
                    Ok(())
                })();
                self.print_to_stderr = outer;
                r
            }
            TypedStmtKind::Return(e) => {
                if let Some(sret) = self.current_sret {
                    // Build the result directly in the caller's space, then
                    // return nothing: no aliasing question, no copy-elision
                    // subtlety.
                    let val = self.gen_expr(e)?;
                    self.builder.build_store(sret, val).map_err(|e| e.to_string())?;
                    self.close_open_region()?;
                    self.builder.build_return(None).map_err(|e| e.to_string())?;
                } else {
                    let val = self.gen_expr(e)?;
                    // Postconditions run here, with `result` bound to the value
                    // about to be returned — before the region is released, so a
                    // clause may still read region storage.
                    if !self.current_ensures.is_empty() {
                        let slot = self.create_entry_alloca("result", &e.ty)?;
                        self.builder.build_store(slot, val).map_err(|e| e.to_string())?;
                        let shadowed = self.vars.insert(
                            "result".to_string(),
                            (slot, e.ty.clone()),
                        );
                        let clauses = self.current_ensures.clone();
                        for (clause, function) in &clauses {
                            self.gen_contract_check(clause, function, "ensures")?;
                        }
                        match shadowed {
                            Some(prev) => {
                                self.vars.insert("result".to_string(), prev);
                            }
                            None => {
                                self.vars.remove("result");
                            }
                        }
                    }
                    // Compute FIRST, then release: the expression may still be
                    // reading region storage (a slice's length, say).
                    self.close_open_region()?;
                    self.builder.build_return(Some(&val)).map_err(|e| e.to_string())?;
                }
                Ok(())
            }
            // A guaranteed tail call. `musttail` is the mechanism: LLVM either
            // emits a real tail call or fails the build — it never silently
            // falls back to a growing stack. Typeck has already proven the
            // prototypes match, so nothing here can surprise the verifier.
            //
            // The call must sit IMMEDIATELY before the `ret`, with nothing in
            // between. That is why a tail call inside a region is refused
            // earlier: the region release would land in that gap.
            TypedStmtKind::TailReturn { name, arguments } => {
                let f = *self
                    .user_fns
                    .get(name.as_str())
                    .ok_or_else(|| format!("codegen bug: unknown function {}", name))?;
                let mut plain: Vec<BasicValueEnum> = Vec::new();
                for a in arguments {
                    plain.push(self.gen_expr(a)?);
                }
                // Before the call, because after it there is no frame to be in: a
                // guaranteed tail call replaces this one.
                self.gen_measure_check(name, &plain)?;
                let vals: Vec<inkwell::values::BasicMetadataValueEnum> =
                    plain.iter().map(|v| (*v).into()).collect();
                let call = self
                    .builder
                    .build_call(f, &vals, "tailcall")
                    .map_err(|e| e.to_string())?;
                call.set_tail_call_kind(inkwell::values::LLVMTailCallKind::LLVMTailCallKindMustTail);
                match call.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(v) => {
                        self.builder.build_return(Some(&v)).map_err(|e| e.to_string())?;
                    }
                    _ => {
                        self.builder.build_return(None).map_err(|e| e.to_string())?;
                    }
                }
                Ok(())
            }
            TypedStmtKind::If { cond, then_block, else_block } => {
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
        // What `break` and `continue` inside this body will jump to, plus how many
        // regions were already open — so a jump can tell "opened inside the loop" from
        // "encloses the loop".
        self.loop_stack.push((cond_bb, end_bb, self.region_marks.len()));
        let generated = self.gen_block(body);
        self.loop_stack.pop();
        generated?;
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
    /// Plus which parameters were declared `mutable`.
    ///
    /// A `mutable` aggregate parameter does NOT get `byval`, so the callee receives a pointer to the
    /// CALLER's storage. Dropping the attribute is the whole mechanism, because the call site already
    /// passes an address — `byval` was what turned that address into a copy.
    ///
    /// This is exactly what `mutable self` has always done (see `declare_method`), which is why the
    /// soundness argument is not a new one: a value the callee may write is passed by pointer, and a
    /// value it may not is passed as a copy, so the mechanism is invisible either way.
    fn declare_fn(
        &self,
        name: &str,
        parameters: &[Type],
        ret: &Type,
        writable: &[bool],
    ) -> FunctionValue<'ctx> {
        let ptr = self.ctx.ptr_type(AddressSpace::default());
        let mut ll_params: Vec<BasicMetadataTypeEnum> = Vec::new();
        let ret_is_agg = is_aggregate(ret);
        if ret_is_agg {
            ll_params.push(ptr.into());
        }
        for p in parameters {
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
        for (i, p) in parameters.iter().enumerate() {
            // `byval` is what makes the copy. A `mutable` parameter must not have it, or the callee
            // writes to a copy and the caller sees nothing — which is the silent wrong answer this
            // feature was built to avoid rather than to introduce.
            if is_aggregate(p) && !writable.get(i).copied().unwrap_or(false) {
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
        parameters: &[Type],
        ret: &Type,
    ) -> FunctionValue<'ctx> {
        let ptr = self.ctx.ptr_type(AddressSpace::default());
        let mut ll_params: Vec<BasicMetadataTypeEnum> = Vec::new();
        let ret_is_agg = is_aggregate(ret);
        if ret_is_agg {
            ll_params.push(ptr.into());
        }
        ll_params.push(ptr.into()); // self, always by address
        for p in parameters {
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
        for p in parameters {
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
        let label = format!("{}.{}", m.receiver, m.name);
        let at = self.decl_span(&label, &m.body);
        self.begin_subprogram(llf, &label, at, &m.parameters, &m.ret);
        let entry = self.ctx.append_basic_block(llf, "entry");
        self.builder.position_at_end(entry);
        // See gen_fn: an unlocated prologue is a verifier failure, not a cosmetic gap.
        self.set_debug_location(at);
        // B7, and this line is the whole of the bug the first attempt had. The guard went into
        // `gen_fn` and methods are emitted HERE, by a separate function — so `function f()` was
        // guarded and `function (self) f()` was not. The suite went green: its recursion fixture is
        // a free function, and no fixture recurses through a method.
        //
        // It surfaced because stage-1's parser is recursive descent written as METHODS, so a
        // 30,000-deep expression segfaulted stage-1 while stage-0 parsed it fine. Found in one gdb
        // command — `bt` showed parse_primary -> parse_expr -> parse_primary with no guard between
        // them — which is a use for C1 the day after landing it, on a bug C1's own commit created.
        //
        // The general lesson is the one this codebase keeps paying for: a rule right about the case
        // someone wrote and silent about the case nobody did. The fix is not "remember methods", it
        // is that there are exactly two places a user-written body begins, and both are here.
        self.build_stack_guard()?;
        self.vars.clear();
        // A body starts with no mark of its own: a region is released inside the frame
        // that opened it, never across a call.
        self.region_marks.clear();

        let ret_is_agg = is_aggregate(&m.ret);
        self.current_sret = if ret_is_agg {
            Some(llf.get_nth_param(0).unwrap().into_pointer_value())
        } else {
            None
        };
        let self_idx = if ret_is_agg { 1 } else { 0 };
        let self_arg = llf.get_nth_param(self_idx as u32).unwrap();
        let self_ty = Type::Named(m.receiver.clone());
        self.vars.insert("self".to_string(), (self_arg.into_pointer_value(), self_ty.clone()));
        // `self` is argument one, so a debugger's `bt` shows the receiver a method was
        // called on rather than starting at its second parameter.
        self.declare_variable("self", &self_ty, self_arg.into_pointer_value(), at, Some(1));

        let param_offset = self_idx + 1;
        for (i, (name, ty)) in m.parameters.iter().enumerate() {
            let argument = llf.get_nth_param((param_offset + i) as u32).unwrap();
            if is_aggregate(ty) {
                self.vars.insert(name.clone(), (argument.into_pointer_value(), ty.clone()));
            } else {
                let slot = self.create_entry_alloca(name, ty)?;
                self.builder.build_store(slot, argument).map_err(|e| e.to_string())?;
                self.vars.insert(name.clone(), (slot, ty.clone()));
            }
            let (slot, _) = self.vars[name];
            self.declare_variable(name, ty, slot, at, Some((i + 2) as u32));
        }

        self.gen_contract_prologue(&m.requires, &m.ensures, &m.olds, &label, at)?;

        for stmt in &m.body {
            self.gen_stmt(stmt)?;
        }
        self.current_sret = None;
        self.current_ensures.clear();
        self.old_slots.clear();
        self.end_subprogram();
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
                // tag + payload slots, all 8-byte.
                //
                // `payload_area`, NOT `p.len()`. Counting payload TYPES answers how many things a
                // variant carries; the payload area needs how WIDE they are, and a slice is three
                // cells while a class is as many as it has fields.
                //
                // Counting was the bug, and it is the second time the same one: the note above
                // `enum_types` records `Line(Point, Point)` giving each Point one cell. That fix
                // built `payload_cells` / `payload_area` and taught `set_body` to use them — and
                // left this function counting. So the LLVM TYPE was right and the SIZE was wrong,
                // which is the worst possible split: `%bx.enum.Json` really is 32 bytes, and
                // `burxt.alloc` was asked for 16.
                //
                // What that did: `[json_field("n", json_int(1))]` allocated 24 bytes for a `Field`
                // that is 40, and the store ran 16 bytes past the end of the region block. Silent
                // corruption of whatever came next, then a wrong answer, then region exhaustion.
                //
                // One source of truth now. If a third size computation ever appears, it is this
                // bug again.
                let variants = self.enum_types[name].1.clone();
                let slots = self.payload_area(&variants) as u64;
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
            // Written out rather than left to the `_` fallback below, which would answer 8 for a
            // `u8`. A width is boundary-only, so it should never reach a layout walk at all — but a
            // fallback that is silently wrong for three of the four widths is the kind of thing that
            // only shows up once something else changes, and `bits / 8` is the honest answer.
            Type::Width { bits, .. } => {
                let w = (*bits / 8) as u64;
                Layout { size: w, align: w, field_offsets: vec![] }
            }
            _ => Layout { size: 8, align: 8, field_offsets: vec![] },
        }
    }

    /// Target data for the host — the authority on pointer width.
    fn target_data(&self) -> inkwell::targets::TargetData {
        inkwell::targets::TargetData::create(&self.module.get_triple().as_str().to_string_lossy())
    }

    /// The LLVM type for a Burxt type. All scalars are i64; String is an
    /// opaque pointer — the TARGET decides pointer width, never this code.
    /// How many 8-byte cells a value of this type occupies in an enum's payload area.
    ///
    /// Only enums need this. Everywhere else stage-0 uses real LLVM types and lets LLVM do the
    /// arithmetic; a variant payload is the one place where values of different types share one
    /// area and their positions have to be computed by hand.
    ///
    /// Kept deliberately parallel to `llvm_type`: every arm here answers for the type that arm
    /// builds. A `Slice` is stage-0's three-field {ptr, len, cap} struct, which is three cells and
    /// NOT the one cell stage-1 uses — the two compilers agree on behaviour, never on ABI.
    fn payload_cells(&self, ty: &Type) -> u32 {
        match ty {
            // A handle is one i64: a generation and an index, packed.
            Type::Handle(_) => 1,
            Type::Int | Type::Bool | Type::Decimal { .. } | Type::String => 1,
            // A width is boundary-only, so it can never be an enum variant's payload — the checker
            // refuses it in a field long before this runs. Answered anyway, and answered 1, because
            // every C scalar here is one cell and an arm that agrees with its neighbours cannot
            // become a wrong number if the boundary rule is ever widened.
            Type::CInt | Type::CDouble | Type::CPointer | Type::Width { .. } => 1,
            Type::Param(_) | Type::Generic { .. } => 1,
            // Same as `Generic` beside it: gone before codegen, answered anyway rather than
            // left to a `_`, so widening the boundary rule cannot make this a wrong number.
            Type::DynGeneric { .. } => 1,
            // A tuple is the anonymous class `expand` made of it long before codegen runs, so
            // a variant payload arrives here as `Named("(Int, String)")` and takes the arm
            // below. Answered by SUMMING rather than with a placeholder for the reason the
            // width arm above gives: an arm that agrees with its neighbours cannot become a
            // wrong number if something ever reaches it.
            Type::Tuple(elements) => elements.iter().map(|t| self.payload_cells(t)).sum(),
            Type::Slice(_) => 3,
            Type::Array { elem, len } => self.payload_cells(elem) * (*len as u32),
            Type::Dyn(_) => 2,
            Type::Named(name) => {
                if let Some(fields) = self.struct_fields.get(name) {
                    return fields.iter().map(|t| self.payload_cells(t)).sum();
                }
                if let Some((_, variants)) = self.enum_types.get(name) {
                    let widest = variants
                        .iter()
                        .map(|p| p.iter().map(|t| self.payload_cells(t)).sum::<u32>())
                        .max()
                        .unwrap_or(0);
                    return 1 + widest;
                }
                1
            }
        }
    }

    /// The cell offset of each payload within a variant, and the total the variant needs.
    fn payload_offsets(&self, payload: &[Type]) -> (Vec<u32>, u32) {
        let mut offsets = Vec::with_capacity(payload.len());
        let mut at = 0;
        for t in payload {
            offsets.push(at);
            at += self.payload_cells(t);
        }
        (offsets, at)
    }

    /// The payload area's width in cells: the widest variant.
    fn payload_area(&self, variants: &[Vec<Type>]) -> u32 {
        variants.iter().map(|p| self.payload_offsets(p).1).max().unwrap_or(0)
    }

    fn llvm_type(&self, ty: &Type) -> BasicTypeEnum<'ctx> {
        match ty {
            // Every type parameter is substituted before codegen runs — one copy of the
            // generic per instantiation — so reaching here is a compiler bug, not a
            // program error. Represented as an i64 so the panic is a diagnostic rather
            // than a crash; the checker is what guarantees it never happens.
            // All three are gone before codegen runs: a parameter is substituted, and a
            // generic application and a tuple both become the `Named` type of the class
            // `expand` made. Reaching here is a compiler bug, and the checker is what
            // guarantees it cannot.
            // `DynGeneric` joins the three for the same reason: `expand` turns
            // `dynamic Mapper<Int>` into `Dyn("Mapper$Int")`, which the `Dyn` arm below
            // builds, so reaching here is a compiler bug the checker guarantees cannot happen.
            Type::Param(_)
            | Type::Generic { .. }
            | Type::Tuple(_)
            | Type::DynGeneric { .. } => self.ctx.i64_type().into(),
            // A handle IS an i64 at the ABI, which is the point: a host sees an ordinary
            // number and needs no marshalling to hold it between calls.
            Type::Handle(_) => self.ctx.i64_type().into(),
            Type::Int | Type::Bool | Type::Decimal { .. } => self.ctx.i64_type().into(),
            Type::String => self.ctx.ptr_type(AddressSpace::default()).into(),
            Type::CInt => self.ctx.i32_type().into(),
            // The whole payoff of ONE variant: the bit count IS the LLVM type, so `u8` and `u64`
            // need no separate arms. Signedness does not appear here at all — LLVM integers carry
            // no sign, which is why it lives in the range check and the extension instead.
            // `custom_width_int_type` answers a Result — LLVM rejects a width of 0 or above 2^23 —
            // and the four widths the lexer admits are all valid, so the failure is unreachable.
            // Unwrapped to the 8-bit type rather than panicking, because `llvm_type` cannot report
            // an error and a crash here would be a worse diagnostic than a wrong integer that no
            // path can reach.
            Type::Width { bits, .. } => std::num::NonZeroU32::new(*bits)
                .and_then(|b| self.ctx.custom_width_int_type(b).ok())
                .unwrap_or_else(|| self.ctx.i8_type())
                .into(),
            // An opaque pointer, the same LLVM type a String uses — the TARGET decides the width,
            // never this code. What keeps it opaque is the checker, not the representation: nothing
            // in Burxt can load through it, and the only way to read what it points at is
            // `c_string_at`, which copies.
            Type::CPointer => self.ctx.ptr_type(AddressSpace::default()).into(),
            // FFI-only, so it appears in extern signatures and nowhere else.
            Type::CDouble => self.ctx.f64_type().into(),
            Type::Named(name) => match self.struct_types.get(name) {
                Some(st) => (*st).into(),
                // **Not a bare index, and the reason is a bug this line used to produce.** An enum
                // payload naming a type nobody declared reached here and the index panicked with
                // `no entry found for key` — a Rust backtrace naming this file, for a typo in the
                // author's own `enum`. `typeck.rs` now validates payload types, so the key is
                // guaranteed; the `expect` says WHY it is guaranteed, and turns a future regression
                // into a sentence naming the broken invariant rather than into this file again.
                // Nothing in this compiler should fail anonymously, and an index is the most
                // anonymous failure there is.
                None => self
                    .enum_types
                    .get(name)
                    .unwrap_or_else(|| {
                        panic!(
                            "`{}` reached codegen without being declared — typeck validates every \
                             type a declaration names, so this is a hole in that validation rather \
                             than a bad program",
                            name
                        )
                    })
                    .0
                    .into(),
            },
            Type::Slice(_) => {
                let ptr = self.ctx.ptr_type(AddressSpace::default());
                let i64t = self.ctx.i64_type();
                self.ctx
                    .struct_type(&[ptr.into(), i64t.into(), i64t.into()], false)
                    .into()
            }
            Type::Array { elem, len } => self.llvm_type(elem).array_type(*len).into(),
            // An interface object is a fat pointer: { data, vtable }. The vtable
            // lives OUTSIDE the data, which is why becoming an interface object
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

    // ------------------------------------------------------------ DWARF (C1)
    //
    // The whole of this section is inert unless `-g` was given. That is deliberate and
    // load-bearing rather than tidy: the self-hosting fixpoint compares stage-1's IR
    // against itself byte for byte, and `the_ir_is_the_same_for_every_target` compares
    // one machine's IR against another's. Debug info would break both — a `DIFile`
    // carries the directory the compiler ran in, and a compile unit carries a producer
    // string with a version in it. So debug info is opt-in, and a default build emits
    // the same module it emitted before this code existed.
    //
    // What a debugger gets: a line table at STATEMENT granularity, a subprogram per
    // function, and a `DILocalVariable` with a `llvm.dbg.declare` for every parameter
    // and every `let`. What it does not get: stage-1. See `main.rs`, which refuses
    // `-g` there rather than emitting something a debugger would read wrongly.

    /// Turn on DWARF for this module.
    ///
    /// `files` and `src` are `main.rs`'s program buffer and its map back to the files
    /// it was built from — a span is an offset into the buffer, so this is what turns
    /// one back into a place. `decls` maps a function's name (or `Recv.name` for a
    /// method) to where it was declared; anything missing from it — a monomorphised
    /// generic, whose mangled name no source line spells — falls back to the first
    /// statement of its body, which is a real position rather than an invented one.
    pub fn enable_debug_info(
        &mut self,
        files: &[crate::SourceFile],
        src: &str,
        decls: HashMap<String, crate::diag::Span>,
        optimised: bool,
    ) {
        use inkwell::debug_info::{DWARFEmissionKind, DWARFSourceLanguage};

        // Without this flag LLVM STRIPS every piece of debug info on its way out, and
        // says nothing. inkwell does not add it — its own documentation shows the caller
        // doing it. A build that silently emits no DWARF while reporting success is
        // exactly the failure this row exists to prevent, so
        // `a_debug_build_declares_its_debug_info_version` in tests/runner.rs asserts the
        // flag is present in the emitted IR.
        self.module.add_basic_value_flag(
            "Debug Info Version",
            inkwell::module::FlagBehavior::Warning,
            self.ctx.i32_type().const_int(inkwell::debug_info::debug_metadata_version() as u64, false),
        );
        // DWARF 4 rather than 5: it is what every debugger in the field reads without
        // argument, and Burxt uses nothing DWARF 5 added.
        self.module.add_basic_value_flag(
            "Dwarf Version",
            inkwell::module::FlagBehavior::Warning,
            self.ctx.i32_type().const_int(4, false),
        );

        // The compile unit is named for the ROOT file — the one the user typed — and the
        // `use`d modules become additional DIFiles below.
        let root = files.last().expect("a program has at least one file");
        let (root_name, root_dir) = split_path(&root.path);
        let (builder, unit) = self.module.create_debug_info_builder(
            true,
            // There is no DW_LANG for Burxt, and inventing one would make every tool
            // report "unknown". C is the closest honest lie: scalars in registers,
            // structs by offset, no runtime type model — which is what a debugger will
            // in fact find.
            DWARFSourceLanguage::C,
            &root_name,
            &root_dir,
            &format!("burxt {}", env!("CARGO_PKG_VERSION")),
            optimised,
            "",
            0,
            "",
            DWARFEmissionKind::Full,
            0,
            false,
            false,
            "",
            "",
        );

        let mut difiles = Vec::with_capacity(files.len());
        let mut extents = Vec::with_capacity(files.len());
        for f in files {
            let (name, dir) = split_path(&f.path);
            difiles.push(builder.create_file(&name, &dir));
            // Line starts in BUFFER coordinates, so a lookup is one binary search with
            // no subtraction to get wrong.
            let end = f.start + f.len;
            let mut starts = vec![f.start];
            starts.extend(
                src[f.start..end]
                    .bytes()
                    .enumerate()
                    .filter(|(_, b)| *b == b'\n')
                    .map(|(i, _)| f.start + i + 1),
            );
            extents.push((f.start, end, starts));
        }

        self.debug = Some(DebugInfo {
            builder,
            unit,
            files: difiles,
            extents,
            types: std::cell::RefCell::new(HashMap::new()),
            decls,
            current: None,
            scopes: Vec::new(),
            optimised,
        });
    }

    /// A buffer offset back to (which file, 1-based line, 1-based column).
    ///
    /// `None` when the offset belongs to no file, which happens for the one-byte
    /// separator `load_program` puts between files. Callers treat that as "no position"
    /// rather than guessing at a nearby one.
    fn locate(&self, offset: usize) -> Option<(usize, u32, u32)> {
        let d = self.debug.as_ref()?;
        let (ix, (start, _, starts)) = d
            .extents
            .iter()
            .enumerate()
            .find(|(_, (s, e, _))| offset >= *s && offset <= *e)?;
        let line_ix = match starts.binary_search(&offset) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        let _ = start;
        Some((ix, (line_ix + 1) as u32, (offset - starts[line_ix] + 1) as u32))
    }

    /// Point the builder at `span`, so every instruction it makes from here on is
    /// attributed there. Does nothing unless `-g` asked for debug info.
    fn set_debug_location(&self, span: crate::diag::Span) {
        let Some(d) = self.debug.as_ref() else { return };
        let Some(scope) = d.scopes.last().copied() else { return };
        let Some((_, line, col)) = self.locate(span.start as usize) else { return };
        let loc = d.builder.create_debug_location(self.ctx, line, col, scope, None);
        self.builder.set_current_debug_location(loc);
    }

    /// The `DIType` for a Burxt type, made once per program and cached.
    ///
    /// The mapping is deliberately literal about what is actually in the machine, not
    /// about what the language pretends. A `Decimal<2>` is described as a 64-bit signed
    /// integer NAMED `Decimal<2>`, because that is what a debugger will find in the slot
    /// — it holds 1999 for 19.99, and a reader who sees `Decimal<2> = 1999` has been told
    /// the truth, where a reader shown `19.99` would have been told a number no register
    /// contains. Same argument as the compiler's: never a float, anywhere.
    fn di_type(&self, ty: &Type) -> Option<inkwell::debug_info::DIType<'ctx>> {
        use inkwell::debug_info::{DIFlags, DIFlagsConstants};
        let d = self.debug.as_ref()?;
        let key = format!("{}", ty);
        if let Some(t) = d.types.borrow().get(&key) {
            return Some(*t);
        }

        // DWARF attribute encodings (DW_ATE_*), by number because inkwell takes the raw
        // value: 0x02 boolean, 0x05 signed, 0x06 signed char, 0x07 unsigned, 0x04 float.
        let made: inkwell::debug_info::DIType<'ctx> = match ty {
            Type::Int => d.builder.create_basic_type("Int", 64, 0x05, DIFlags::PUBLIC).ok()?.as_type(),
            // A Burxt Bool is an i64 holding 0 or 1 — see this file's header. Describing
            // it as one byte would make a debugger read the wrong seven.
            Type::Bool => d.builder.create_basic_type("Bool", 64, 0x02, DIFlags::PUBLIC).ok()?.as_type(),
            Type::CInt => d.builder.create_basic_type("CInt", 32, 0x05, DIFlags::PUBLIC).ok()?.as_type(),
            Type::CDouble => d.builder.create_basic_type("CDouble", 64, 0x04, DIFlags::PUBLIC).ok()?.as_type(),
            Type::Width { bits, signed } => d
                .builder
                .create_basic_type(
                    &format!("{}{}", if *signed { "i" } else { "u" }, bits),
                    *bits as u64,
                    if *signed { 0x05 } else { 0x07 },
                    DIFlags::PUBLIC,
                )
                .ok()?
                .as_type(),
            Type::Decimal { .. } => d
                .builder
                .create_basic_type(&key, 64, 0x05, DIFlags::PUBLIC)
                .ok()?
                .as_type(),
            // A String is a pointer to NUL-terminated bytes, and describing it as
            // `char*` is what makes a debugger PRINT it rather than show an address.
            // That is most of the value of `info locals` in a language whose errors are
            // sentences.
            Type::String => {
                let byte = d.builder.create_basic_type("char", 8, 0x06, DIFlags::PUBLIC).ok()?;
                d.builder
                    .create_pointer_type("String", byte.as_type(), 64, 64, AddressSpace::default())
                    .as_type()
            }
            // Opaque by design — the language allows exactly two operations on one and
            // looking inside is neither. `void*` is the honest description.
            Type::CPointer => {
                let byte = d.builder.create_basic_type("void", 8, 0x06, DIFlags::PUBLIC).ok()?;
                d.builder
                    .create_pointer_type("CPointer", byte.as_type(), 64, 64, AddressSpace::default())
                    .as_type()
            }
            Type::Array { elem, len } => {
                let inner = self.di_type(elem)?;
                let l = self.layout_of(ty);
                d.builder
                    .create_array_type(inner, l.size * 8, l.align as u32 * 8, &[0..(*len as i64)])
                    .as_type()
            }
            Type::Named(name) if self.struct_fields.contains_key(name) && !self.enum_types.contains_key(name) => {
                self.di_struct(name)?
            }
            // An enum is `{ i64 tag, [N x i64] payload }`. Described as a struct with
            // those two members rather than as a DWARF variant part: a debugger then
            // shows the tag, which is the thing a reader actually wants, and the payload
            // as the cells it really is. Claiming a discriminated union here would mean
            // claiming a member layout per variant, and the payload area is an overlay —
            // that would be a description the machine does not match.
            other => {
                let l = self.layout_of(other);
                let i64ty = d.builder.create_basic_type("Int", 64, 0x05, DIFlags::PUBLIC).ok()?;
                let root = d.files.last().copied()?;
                let members: Vec<inkwell::debug_info::DIType<'ctx>> = (0..l.size / 8)
                    .map(|i| {
                        d.builder
                            .create_member_type(
                                d.unit.as_debug_info_scope(),
                                &format!("cell{}", i),
                                root,
                                0,
                                64,
                                64,
                                i * 64,
                                DIFlags::PUBLIC,
                                i64ty.as_type(),
                            )
                            .as_type()
                    })
                    .collect();
                d.builder
                    .create_struct_type(
                        d.unit.as_debug_info_scope(),
                        &key,
                        root,
                        0,
                        l.size * 8,
                        l.align as u32 * 8,
                        DIFlags::PUBLIC,
                        None,
                        &members,
                        0,
                        None,
                        &key,
                    )
                    .as_type()
            }
        };
        d.types.borrow_mut().insert(key, made);
        Some(made)
    }

    /// A class, described field by field with the offsets `layout_of` already computes —
    /// so `print p` in a debugger shows the same field boundaries the compiler used.
    fn di_struct(&self, name: &str) -> Option<inkwell::debug_info::DIType<'ctx>> {
        use inkwell::debug_info::{DIFlags, DIFlagsConstants};
        let d = self.debug.as_ref()?;
        let fields = self.struct_fields.get(name)?.clone();
        let l = self.layout_of(&Type::Named(name.to_string()));
        let root = d.files.last().copied()?;
        // Placed in the cache BEFORE the members are built. A class may hold a pointer to
        // its own kind, and without this the walk would not terminate.
        let shell = d
            .builder
            .create_struct_type(
                d.unit.as_debug_info_scope(),
                name,
                root,
                0,
                l.size * 8,
                l.align as u32 * 8,
                DIFlags::PUBLIC,
                None,
                &[],
                0,
                None,
                name,
            )
            .as_type();
        d.types.borrow_mut().insert(format!("{}", Type::Named(name.to_string())), shell);

        let mut members = Vec::with_capacity(fields.len());
        for (i, f) in fields.iter().enumerate() {
            let fty = self.di_type(f)?;
            let fl = self.layout_of(f);
            members.push(
                d.builder
                    .create_member_type(
                        d.unit.as_debug_info_scope(),
                        &format!("f{}", i),
                        root,
                        0,
                        fl.size * 8,
                        fl.align as u32 * 8,
                        l.field_offsets.get(i).copied().unwrap_or(0) * 8,
                        DIFlags::PUBLIC,
                        fty,
                    )
                    .as_type(),
            );
        }
        let full = d
            .builder
            .create_struct_type(
                d.unit.as_debug_info_scope(),
                name,
                root,
                0,
                l.size * 8,
                l.align as u32 * 8,
                DIFlags::PUBLIC,
                None,
                &members,
                0,
                None,
                name,
            )
            .as_type();
        d.types.borrow_mut().insert(format!("{}", Type::Named(name.to_string())), full);
        Some(full)
    }

    /// Where a function was declared. A monomorphised generic has a mangled name no
    /// source line spells, so it falls back to the first statement of its body — a real
    /// position inside the right function, rather than line 1 of the wrong file.
    fn decl_span(&self, name: &str, body: &[TypedStmt]) -> crate::diag::Span {
        self.debug
            .as_ref()
            .and_then(|d| d.decls.get(name).copied())
            .or_else(|| body.first().map(|s| s.span))
            .unwrap_or_else(|| crate::diag::Span::new(0, 0))
    }

    /// Open a subprogram for the function about to be generated, and make it the scope
    /// every location below hangs off. `at` is where the function was declared.
    fn begin_subprogram(
        &mut self,
        llf: FunctionValue<'ctx>,
        name: &str,
        at: crate::diag::Span,
        parameters: &[(String, Type)],
        ret: &Type,
    ) {
        use inkwell::debug_info::{DIFlags, DIFlagsConstants};
        if self.debug.is_none() {
            return;
        }
        let (file_ix, line, _) = self.locate(at.start as usize).unwrap_or((0, 1, 1));
        // Burxt has no void type — a function that returns nothing still has a `ret` in
        // the typed tree — so the return type is always described rather than elided.
        let ret_di = self.di_type(ret);
        let param_di: Vec<_> = parameters.iter().filter_map(|(_, t)| self.di_type(t)).collect();
        let d = self.debug.as_ref().expect("checked above");
        let file = d.files.get(file_ix).copied().unwrap_or_else(|| d.files[0]);
        let sub_ty = d.builder.create_subroutine_type(file, ret_di, &param_di, DIFlags::PUBLIC);
        let sp = d.builder.create_function(
            file.as_debug_info_scope(),
            name,
            Some(llf.get_name().to_str().unwrap_or(name)),
            file,
            line,
            sub_ty,
            false,
            true,
            line,
            DIFlags::PUBLIC,
            d.optimised,
        );
        llf.set_subprogram(sp);
        {
            let d = self.debug.as_mut().expect("checked above");
            d.current = Some((sp, file_ix));
            // Cleared and re-seeded rather than pushed onto: a body that returned early
            // out of a nested block would otherwise leave its scope open for the next
            // function, and every location after it would name the wrong one.
            d.scopes.clear();
            d.scopes.push(sp.as_debug_info_scope());
        }
        // Cleared, not left over: an instruction built between two functions with a
        // stale location attached is an LLVM verifier error, and a confusing one.
        self.builder.unset_current_debug_location();
    }

    /// Close the current subprogram. Every function must do this, or the next one's
    /// instructions hang off the previous one's scope.
    fn end_subprogram(&mut self) {
        if let Some(d) = self.debug.as_mut() {
            d.current = None;
            d.scopes.clear();
        }
        self.builder.unset_current_debug_location();
    }

    /// Record a binding so a debugger can name it and read it.
    ///
    /// `arg_no` is `Some(n)` (1-based) for a parameter and `None` for a `let`: DWARF
    /// keeps the two apart, and it is why `bt` can print the arguments a frame was
    /// called with rather than only the locals it went on to make.
    fn declare_variable(
        &self,
        name: &str,
        ty: &Type,
        slot: PointerValue<'ctx>,
        at: crate::diag::Span,
        arg_no: Option<u32>,
    ) {
        use inkwell::debug_info::{DIFlags, DIFlagsConstants};
        let Some(dty) = self.di_type(ty) else { return };
        let Some(d) = self.debug.as_ref() else { return };
        let Some((_, file_ix)) = d.current else { return };
        let Some(scope) = d.scopes.last().copied() else { return };
        let Some(block) = self.builder.get_insert_block() else { return };
        let (loc_file, line, col) = self.locate(at.start as usize).unwrap_or((file_ix, 1, 1));
        let file = d.files.get(loc_file).copied().unwrap_or_else(|| d.files[0]);
        let var = match arg_no {
            Some(n) => d.builder.create_parameter_variable(scope, name, n, file, line, dty, true, DIFlags::PUBLIC),
            None => d.builder.create_auto_variable(scope, name, file, line, dty, true, DIFlags::PUBLIC, 0),
        };
        let loc = d.builder.create_debug_location(self.ctx, line, col, scope, None);
        d.builder.insert_declare_at_end(slot, Some(var), None, loc, block);
    }

    /// Resolve the metadata graph. Nothing below this is optional: LLVM's verifier
    /// rejects a module with unresolved temporary debug nodes, so a missing `finalize`
    /// is a hard failure rather than a quiet one.
    fn finalize_debug_info(&self) {
        if let Some(d) = &self.debug {
            d.builder.finalize();
        }
    }

    /// Generate a block's statements in a child scope, mirroring the
    /// typechecker: bindings made inside vanish at the closing brace.
    fn gen_block(&mut self, stmts: &[TypedStmt]) -> Result<(), String> {
        let saved = self.vars.clone();
        // A DWARF lexical block, mirroring exactly what `vars` is doing on the line
        // above: bindings made inside vanish at the closing brace, for the debugger as
        // well as for the compiler. Pushed only when there is a statement to take a
        // position from.
        let opened = self.push_lexical_block(stmts.first().map(|s| s.span));
        let result = stmts.iter().try_for_each(|s| self.gen_stmt(s));
        if opened {
            self.pop_lexical_block();
        }
        self.vars = saved;
        result
    }

    /// Open a debug scope for a block. Answers whether one was actually pushed, so the
    /// pop is never unbalanced — an extra pop would silently reparent the rest of the
    /// function onto the wrong scope.
    fn push_lexical_block(&mut self, at: Option<crate::diag::Span>) -> bool {
        let Some(at) = at else { return false };
        let Some((file_ix, line, col)) = self.locate(at.start as usize) else { return false };
        let Some(d) = self.debug.as_mut() else { return false };
        let Some(parent) = d.scopes.last().copied() else { return false };
        let file = match d.files.get(file_ix).copied() {
            Some(f) => f,
            None => return false,
        };
        let block = d.builder.create_lexical_block(parent, file, line, col);
        d.scopes.push(block.as_debug_info_scope());
        true
    }

    fn pop_lexical_block(&mut self) {
        if let Some(d) = self.debug.as_mut() {
            // Never below the subprogram itself, which is the scope every location in
            // the function ultimately hangs off.
            if d.scopes.len() > 1 {
                d.scopes.pop();
            }
        }
    }

    /// Is the builder's current block still missing a terminator?
    fn current_block_open(&self) -> bool {
        self.builder
            .get_insert_block()
            .is_some_and(|b| b.get_terminator().is_none())
    }

    /// One value, then a newline unless this is `print_exact`.
    ///
    /// **The newline is the whole difference, and it is gated here rather than by a second
    /// formatter.** `gen_print_value` decides how every type displays; a separate no-newline
    /// writer would have been a second copy of that decision, and the first type either one
    /// learned about would print differently from the other.
    fn gen_print(&mut self, e: &TypedExpr, newline: bool) -> Result<(), String> {
        self.gen_print_value(e)?;
        if newline {
            return self.gen_newline();
        }
        Ok(())
    }

    /// Print a single trailing newline.
    fn gen_newline(&mut self) -> Result<(), String> {
        let fmt = self.global_str("\n", "fmt_nl");
        self.emit_print_call(&[fmt.into()], "printf_nl")?;
        Ok(())
    }

    /// Emit one formatted write, to whichever stream the current print statement names.
    ///
    /// `printf(fmt, ...)` for stdout and `fprintf(stderr, fmt, ...)` for stderr — the SAME format
    /// strings and the same arguments either way, which is the whole reason `print` and `print_error`
    /// are one statement rather than two. A second statement would have grown a second formatter, and
    /// the first time one of them learned about a new type the other would print something else.
    fn emit_print_call(
        &mut self,
        arguments: &[BasicMetadataValueEnum<'ctx>],
        name: &str,
    ) -> Result<(), String> {
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        if !self.print_to_stderr {
            let printf = self.printf.ok_or("codegen bug: printf not declared")?;
            self.builder.build_call(printf, arguments, name).map_err(err)?;
            return Ok(());
        }
        let ptr = self.ctx.ptr_type(AddressSpace::default());
        let i32t = self.ctx.i32_type();
        let fprintf = self.libc("fprintf", i32t.fn_type(&[ptr.into(), ptr.into()], true));
        let (stderr_g, _, _) = self.panic_deps();
        let stream = self.load_stderr(stderr_g)?;
        let mut all: Vec<BasicMetadataValueEnum<'ctx>> = Vec::with_capacity(arguments.len() + 1);
        all.push(stream.into());
        all.extend_from_slice(arguments);
        self.builder.build_call(fprintf, &all, name).map_err(err)?;
        Ok(())
    }

    /// Print one value with NO trailing newline. `print` adds the newline
    /// itself, and interpolation must not put one between pieces.
    fn gen_print_value(&mut self, e: &TypedExpr) -> Result<(), String> {
        let val = self.gen_expr(e)?;
        match &e.ty {
            // Substituted before codegen, so this is unreachable by construction.
            Type::Param(name) => {
                return Err(format!("codegen bug: type parameter `{}` survived", name))
            }
            Type::Generic { name, .. } => {
                return Err(format!("codegen bug: `{}<...>` was never instantiated", name))
            }
            Type::DynGeneric { name, .. } => {
                return Err(format!(
                    "codegen bug: `dynamic {}<...>` was never instantiated",
                    name
                ))
            }
            // `expand` turns a tuple into a class before anything is typed, so what reaches
            // print is `Named("(Int, String)")` — and a class has no print form either, which
            // is the refusal the CHECKER gives. This arm is the same unreachable-by-
            // construction case the two above are.
            Type::Tuple(_) => {
                return Err("codegen bug: a tuple was never made into its class".to_string())
            }
            // A handle prints as its number. Unlike an address this is REPRODUCIBLE — an index
            // and a generation are decided by how many values have been held, not by where the
            // allocator put them — so printing one keeps a program's output identical across
            // runs and machines. That is why this is allowed where `CPointer` below is not.
            // The checker refuses this, and for a reason worth restating here: an address differs
            // between runs, so printing one would make a program's output non-reproducible. Reaching
            // this arm means the refusal was lost.
            Type::CPointer => {
                return Err("codegen bug: a CPointer reached print".to_string())
            }
            // Unreachable for the same reason `CInt` never appears here: a width exists only in an
            // extern signature, and by the time a returned value is a Burxt expression it has been
            // extended to `Int`. So there is nothing to format, and a wrong guess would print a
            // truncated number — a silently wrong answer rather than a loud one.
            Type::Width { bits, signed } => {
                return Err(format!(
                    "codegen bug: `{}{}` reached print — a width exists only at the C boundary",
                    if *signed { "i" } else { "u" },
                    bits
                ))
            }
            // A handle does NOT print, and the checker says so before this runs. The number is
            // reproducible — an index and a generation, decided by how many values have been
            // held rather than by where the allocator put them — so unlike `CPointer` below it
            // could safely be displayed. It is refused because displaying it INVITES the bug the
            // generation exists to catch: a number written down and passed back later is exactly
            // a stale handle. Print a field of `held(h)` instead.
            Type::Handle(_) => {
                return Err("codegen bug: a Handle reached print".to_string())
            }
            Type::Int => {
                let fmt = self.global_str("%lld", "fmt_int");
                self.emit_print_call(&[fmt.into(), val.into()], "printf_int")?;
            }
            Type::String => {
                // User bytes are always an ARGUMENT, never the format string.
                let fmt = self.global_str("%s", "fmt_str");
                self.emit_print_call(&[fmt.into(), val.into()], "printf_str")?;
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
                let arguments: Vec<BasicMetadataValueEnum> = vec![fmt.into(), s.into()];
                self.emit_print_call(&arguments, "printf_bool")?;
            }
            Type::Named(_) | Type::CInt | Type::CDouble | Type::Array { .. }
            | Type::Dyn(_) | Type::Slice(_) => {
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

                // **Rendered in Burxt, not by the host's `snprintf`.** See `build_decimal_text`
                // — the arithmetic was always exact and the last step, turning it into the
                // characters a reader sees, used to be whatever libc the target had. `%s` and
                // `sign` above are now unused by this path and stay only for the `Int` branch.
                let _ = (is_neg, abs, sign, i128t, wide);
                let text = self.build_decimal_text(val, *scale)?;
                let fmt = self.global_str("%s", "fmt_dec_text");
                let arguments: Vec<BasicMetadataValueEnum> = vec![fmt.into(), text.into()];
                self.emit_print_call(&arguments, "printf_dec")?;
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
            TypedExprKind::Hash(inner) => {
                // FNV-1a over the bytes for a String; a multiplicative mix for the scalars.
                // Both are emitted as calls to one runtime helper rather than inline, so the two
                // compilers can be checked against each other by comparing numbers.
                //
                // Deterministic and unseeded: see spec/1.0/M11-MAPS.md Decision 4 for the trade and
                // the trigger that would add a seeded constructor.
                let value = self.gen_expr(inner)?;
                let helper = self.hash_fn(matches!(inner.ty, Type::String))?;
                let call = self
                    .builder
                    .build_call(helper, &[value.into()], "hashed")
                    .map_err(|e| e.to_string())?;
                match call.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(v) => Ok(v),
                    _ => Err("hash helper returned void".to_string()),
                }
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
            TypedExprKind::ArgCount => self.build_arg_count().map(Into::into),
            TypedExprKind::Arg(index) => {
                let i = self.gen_expr(index)?.into_int_value();
                self.build_arg(i).map(Into::into)
            }
            TypedExprKind::WriteFile { path, contents } => {
                let p = self.gen_expr(path)?.into_pointer_value();
                let c = self.gen_expr(contents)?.into_pointer_value();
                self.build_write_file(p, c).map(Into::into)
            }
            TypedExprKind::WriteBytes { path, buffer } => {
                let p = self.gen_expr(path)?.into_pointer_value();
                // The buffer's HEADER, not a copy of it: the value is a `[Int]` and this
                // needs its length and data pointer, which is what the struct holds.
                let header = self.gen_expr(buffer)?.into_struct_value();
                self.build_write_bytes(p, header).map(Into::into)
            }
            TypedExprKind::Substring { source, at, len } => {
                let bytes = self.gen_expr(source)?.into_pointer_value();
                let at = self.gen_expr(at)?.into_int_value();
                let count = self.gen_expr(len)?.into_int_value();
                self.build_substring(bytes, at, count).map(Into::into)
            }
            TypedExprKind::IntDiv { kind, lhs, rhs } => {
                let a = self.gen_expr(lhs)?.into_int_value();
                let b = self.gen_expr(rhs)?.into_int_value();
                self.build_int_div(*kind, a, b).map(Into::into)
            }
            TypedExprKind::Bit { kind, lhs, rhs } => {
                let a = self.gen_expr(lhs)?.into_int_value();
                let b = match rhs {
                    Some(r) => Some(self.gen_expr(r)?.into_int_value()),
                    None => None,
                };
                self.build_bit(*kind, a, b).map(Into::into)
            }
            TypedExprKind::Old(index) => {
                let (slot, ty) = self
                    .old_slots
                    .get(*index)
                    .cloned()
                    .ok_or("codegen bug: `old` slot missing")?;
                self.builder
                    .build_load(self.llvm_type(&ty), slot, "old")
                    .map_err(|e| e.to_string())
            }
            TypedExprKind::ReadFile(path) => {
                let p = self.gen_expr(path)?.into_pointer_value();
                self.build_read_file(p).map(Into::into)
            }
            // `c_is_null(p)` — one pointer comparison, widened to Burxt's i64 Bool.
            TypedExprKind::CIsNull(p) => {
                let ptr = self.gen_expr(p)?.into_pointer_value();
                let is_null = self
                    .builder
                    .build_is_null(ptr, "c_is_null")
                    .map_err(|e| e.to_string())?;
                self.builder
                    .build_int_z_extend(is_null, i64t, "c_is_null_i64")
                    .map(Into::into)
                    .map_err(|e| e.to_string())
            }
            // `c_bytes_at(p, n)` — n bytes from C into a Burxt `[Int]`, one byte per element.
            //
            // Zero-extended, not sign-extended: a byte is 0..=255, and a `[Int]` holding -1 for 0xFF
            // would be a different number than the one C had. `write_bytes` truncates on the way out,
            // so the two agree.
            // M17. Both are one call: the table and its three refusals live in
            // `@burxt.hold` and `@burxt.held`, emitted once per program, so a call site here
            // costs an argument and a call rather than an inlined table walk.
            TypedExprKind::Hold { value, of } => {
                // A class is a struct VALUE here, so the value has to be COPIED into the region
                // before there is anything to file. That is not overhead the handle adds — it is
                // the requirement the handle exists to meet: what a host holds between calls must
                // outlive the call, and a stack slot does not. The copy is one value's worth,
                // once, at the boundary.
                let v = self.gen_expr(value)?;
                let size = self.layout_of(&value.ty).size;
                let bytes = self.ctx.i64_type().const_int(size, false);
                let home = self.build_alloc_bytes(bytes)?;
                self.builder.build_store(home, v).map_err(|e| e.to_string())?;
                let v = home;
                let tag = self.handle_tag(of);
                let f = self.hold_fn()?;
                let tag_v = self.ctx.i64_type().const_int(tag, false);
                let call = self
                    .builder
                    .build_call(f, &[v.into(), tag_v.into()], "hold")
                    .map_err(|e| e.to_string())?;
                match call.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(v) => Ok(v),
                    _ => Err("codegen bug: burxt.hold answered nothing".to_string()),
                }
            }
            TypedExprKind::Held { handle, of } => {
                let h = self.gen_expr(handle)?.into_int_value();
                let tag = self.handle_tag(of);
                let f = self.held_fn()?;
                let tag_v = self.ctx.i64_type().const_int(tag, false);
                let call = self
                    .builder
                    .build_call(f, &[h.into(), tag_v.into()], "held")
                    .map_err(|e| e.to_string())?;
                let home = match call.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(v) => v.into_pointer_value(),
                    _ => return Err("codegen bug: burxt.held answered nothing".to_string()),
                };
                // The table answers WHERE the value is; the expression's type is the value, so
                // it is loaded back out. Any refusal has already exited by now.
                let ty = self.llvm_type(&e.ty);
                self.builder
                    .build_load(ty, home, "held_value")
                    .map_err(|e| e.to_string())
            }
            TypedExprKind::CBytesAt { pointer, count } => {
                let ptr_val = self.gen_expr(pointer)?.into_pointer_value();
                let n = self.gen_expr(count)?.into_int_value();
                self.build_c_bytes_at(&e.ty, ptr_val, n)
            }
            TypedExprKind::CBytesTo { pointer, bytes } => {
                let ptr_val = self.gen_expr(pointer)?.into_pointer_value();
                let arr = self.gen_expr(bytes)?.into_struct_value();
                self.build_c_bytes_to(ptr_val, arr).map(Into::into)
            }
            TypedExprKind::CStringAt(p) => {
                let ptr = self.gen_expr(p)?.into_pointer_value();
                self.build_c_string_at(ptr).map(Into::into)
            }
            TypedExprKind::ByteAsString(n) => {
                let byte = self.gen_expr(n)?.into_int_value();
                self.build_byte_as_string(byte).map(Into::into)
            }
            TypedExprKind::ToString(v) => {
                let val = self.gen_expr(v)?;
                self.build_to_string(&v.ty, val).map(Into::into)
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
                // String + String concatenates into the region: measure both,
                // allocate the sum plus a NUL, copy each half in.
                if lhs.ty == Type::String && rhs.ty == Type::String {
                    let a = self.gen_expr(lhs)?.into_pointer_value();
                    let b = self.gen_expr(rhs)?.into_pointer_value();
                    return self.build_str_concat(a, b).map(Into::into);
                }
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
                            let scale = decimal_scale(&e.ty)?;
                            let ls = decimal_scale(&lhs.ty)?;
                            let rs = decimal_scale(&rhs.ty)?;
                            let l128 = self.widen(l)?;
                            let r128 = self.widen(r)?;
                            let raw = self
                                .builder
                                .build_int_mul(l128, r128, "mul_raw")
                                .map_err(|e| e.to_string())?;
                            let shift = ls as i64 + rs as i64 - scale as i64;
                            if shift == 0 {
                                // The result is exactly as wide as the product, so the
                                // i128 product IS the answer and there is no rounding to
                                // do — which is why typeck asks for no contract here.
                                // Narrowing still checks: an exact value can overflow.
                                self.build_narrow_to_i64(raw)
                            } else if shift > 0 {
                                // Digits are being dropped, so a contract says how; typeck
                                // guarantees one is present on this path.
                                let (_, mode) = decimal_with_rounding(&e.ty)?;
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
                    // Equality keeps its own path: `build_str_eq` stops at the first differing byte
                    // and needs no sign, so it stays cheaper than a full compare.
                    if matches!(op, CmpOp::Eq | CmpOp::Ne) {
                        let eq = self.build_str_eq(a, b)?;
                        return match op {
                            CmpOp::Eq => Ok(eq.into()),
                            _ => self
                                .builder
                                .build_int_sub(i64t.const_int(1, false), eq, "str_ne")
                                .map(Into::into)
                                .map_err(|e| e.to_string()),
                        };
                    }
                    // Ordering is `strcmp`'s sign (v0.0.202). BYTE order, which is the only ordering
                    // that needs no decision: locale collation would mean picking a language and one
                    // of its several orders, silently. `strcmp` is already a declared runtime symbol
                    // and every Burxt String is NUL-terminated, so there is nothing to build.
                    let ptr = self.ctx.ptr_type(AddressSpace::default());
                    let i32t = self.ctx.i32_type();
                    let strcmp = self.libc("strcmp", i32t.fn_type(&[ptr.into(), ptr.into()], false));
                    let call = self
                        .builder
                        .build_call(strcmp, &[a.into(), b.into()], "strcmp")
                        .map_err(|e| e.to_string())?;
                    let sign = match call.try_as_basic_value() {
                        inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
                        _ => return Err("strcmp returned void".to_string()),
                    };
                    // Widened before comparing: strcmp answers a 32-bit int and a negative one read
                    // as 64 bits would be enormous. Sign-extended, because the sign IS the answer.
                    let widened = self
                        .builder
                        .build_int_s_extend(sign, i64t, "strcmp_wide")
                        .map_err(|e| e.to_string())?;
                    let predicate = match op {
                        CmpOp::Lt => inkwell::IntPredicate::SLT,
                        CmpOp::Le => inkwell::IntPredicate::SLE,
                        CmpOp::Gt => inkwell::IntPredicate::SGT,
                        _ => inkwell::IntPredicate::SGE,
                    };
                    let bit = self
                        .builder
                        .build_int_compare(predicate, widened, i64t.const_zero(), "str_cmp")
                        .map_err(|e| e.to_string())?;
                    return self
                        .builder
                        .build_int_z_extend(bit, i64t, "str_cmp_bool")
                        .map(Into::into)
                        .map_err(|e| e.to_string());
                }
                // A CLASS compares field by field. NOT `memcmp`, and that is the whole of the
                // work: a class holding a String holds a POINTER, and two equal strings need not
                // live at the same address, so comparing the struct's bytes would answer `false`
                // for two accounts with the same owner built separately. A wrong answer that looks
                // like a working program is the one outcome this language is built against.
                //
                // Typeck has already proved every field is comparable, so every arm below is
                // reachable and none of them can be a slice, an array, a `dynamic` or an enum.
                if let Type::Named(name) = &lhs.ty {
                    let a = self.gen_expr(lhs)?;
                    let b = self.gen_expr(rhs)?;
                    let eq = self.build_class_eq(name, a, b)?;
                    return match op {
                        CmpOp::Eq => Ok(eq.into()),
                        CmpOp::Ne => self
                            .builder
                            .build_int_sub(i64t.const_int(1, false), eq, "class_ne")
                            .map(Into::into)
                            .map_err(|e| e.to_string()),
                        other => Err(format!(
                            "codegen bug: `{}` on a class should have been refused",
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
            TypedExprKind::Call { name, arguments } => {
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

                // Kept alongside `vals` so a recursive call can re-evaluate the
                // termination measure with these arguments. `vals` holds ABI-shaped
                // values (truncated CInts, doubles); the measure needs the Burxt
                // ones, and an sret slot at index 0 would misalign the parameters.
                let mut plain: Vec<BasicValueEnum> = Vec::new();
                for (i, a) in arguments.iter().enumerate() {
                    // Aggregate arguments pass as an address; LLVM's byval
                    // makes the callee's copy.
                    if is_aggregate(&a.ty) {
                        let addr = self.gen_aggregate_addr(a)?;
                        vals.push(addr.into());
                        plain.push(addr.into());
                        continue;
                    }
                    // Generated ONCE: an argument can have side effects, and
                    // evaluating it again for the measure check would run them twice.
                    let raw = self.gen_expr(a)?;
                    plain.push(raw);
                    let mut v = raw;
                    // A CInt parameter is 32-bit on the C side: range-check
                    // and truncate — a value that doesn't fit is a loud
                    // runtime error, never a silent wrap.
                    if let Some((ptys, _)) = &extern_sig {
                        match ptys.get(i) {
                            Some(Type::CInt) => {
                                v = self.build_to_cint(v.into_int_value())?.into();
                            }
                            // Exactly what `CInt` does, per width. The check is the point: a
                            // `u8` parameter handed 256 must be a loud runtime error, because a
                            // silent truncation to 0 is a different number than the one written,
                            // and C would act on it.
                            Some(Type::Width { bits, signed }) => {
                                let (bits, signed) = (*bits, *signed);
                                v = self.build_to_width(v.into_int_value(), bits, signed)?.into();
                            }
                            // A double holds every integer up to 2^53 exactly and
                            // starts skipping them after that, so the crossing is
                            // range-checked. Handing C a different integer than
                            // the one written is the same class of defect as a
                            // silent rounding.
                            Some(Type::CDouble) => {
                                v = self.build_to_cdouble(v.into_int_value())?.into();
                            }
                            // `Decimal<S> as scaled` needs NO conversion: the
                            // value already IS the exact unscaled integer, which
                            // is the whole reason this encoding was chosen.
                            _ => {}
                        }
                    }
                    vals.push(v.into());
                }
                self.gen_measure_check(name, &plain)?;
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
                    let writable = self.fn_writable.get(name).cloned().unwrap_or_default();
                    for (i, p) in ptys.iter().enumerate() {
                        // The `mutable` check has to be HERE as well as on the declaration: LLVM
                        // wants `byval` on both, and mirroring it from types alone is how the first
                        // version of this feature silently kept copying — the declaration said
                        // pointer, the call said `byval`, and the caller saw nothing change.
                        if is_aggregate(p) && !writable.get(i).copied().unwrap_or(false) {
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
                // A width return widens to Burxt's i64, and WHICH extension is the whole content of
                // `signed`: an `i32` sign-extends so -1 stays -1, a `u8` zero-extends so 0xFF is 255
                // rather than -1. Getting this backwards is a silently wrong number, which is why
                // `tests/pass/widths.bx` checks both directions on the same bit pattern.
                //
                // `u64` is the one that cannot be made honest: it is already 64 bits, so there is no
                // extension to choose and a value above `Int`'s maximum arrives NEGATIVE. Named in
                // the documentation and in A7d rather than papered over — Burxt has no unsigned
                // 64-bit type for it to land in, and inventing one is not this item.
                if let Some((_, Type::Width { bits, signed })) = &extern_sig {
                    let (bits, signed) = (*bits, *signed);
                    if bits >= 64 {
                        return Ok(result);
                    }
                    let widened = if signed {
                        self.builder.build_int_s_extend(result.into_int_value(), i64t, "width_ret")
                    } else {
                        self.builder.build_int_z_extend(result.into_int_value(), i64t, "width_ret")
                    };
                    return widened.map(Into::into).map_err(|e| e.to_string());
                }
                Ok(result)
            }
            // `e?` — read the tag; on the failure variant, rebuild that failure as the
            // enclosing function's return value and leave immediately; otherwise carry on
            // with the success payload. The checker proved the two failures have the same
            // payload types (spec/1.0/M8-ERRORS.md §1a Decision A), so the copy is a copy and
            // never a conversion.
            TypedExprKind::Try { value, fail_tag, ok_tag, ret_enum, ret_fail_tag } => {
                let err = |x: inkwell::builder::BuilderError| x.to_string();
                let Type::Named(source) = &value.ty else {
                    return Err("codegen bug: `?` on a non-enum".to_string());
                };
                let (src_st, src_variants) = self.enum_types[source.as_str()].clone();
                let src_slots =
                    src_variants.iter().map(|p| p.len()).max().unwrap_or(0) as u32;
                let fail_payload = src_variants[*fail_tag as usize].clone();

                let slot = self.gen_aggregate_addr(value)?;
                let tag_ptr =
                    self.builder.build_struct_gep(src_st, slot, 0, "try_tag_ptr").map_err(err)?;
                let tag =
                    self.builder.build_load(i64t, tag_ptr, "try_tag").map_err(err)?.into_int_value();
                let failed = self
                    .builder
                    .build_int_compare(
                        inkwell::IntPredicate::EQ,
                        tag,
                        i64t.const_int(*fail_tag as u64, false),
                        "try_failed",
                    )
                    .map_err(err)?;

                let function = self
                    .builder
                    .get_insert_block()
                    .and_then(|b| b.get_parent())
                    .ok_or("codegen bug: `?` outside a function")?;
                let fail_bb = self.ctx.append_basic_block(function, "try.fail");
                let ok_bb = self.ctx.append_basic_block(function, "try.ok");
                self.builder.build_conditional_branch(failed, fail_bb, ok_bb).map_err(err)?;

                // ---- the failure path: rebuild and return ----
                self.builder.position_at_end(fail_bb);
                let (ret_st, ret_variants) = self.enum_types[ret_enum.as_str()].clone();
                let ret_slots =
                    ret_variants.iter().map(|p| p.len()).max().unwrap_or(0) as u32;
                let out = self.create_entry_alloca("try_err", &Type::Named(ret_enum.clone()))?;
                let out_tag =
                    self.builder.build_struct_gep(ret_st, out, 0, "out_tag").map_err(err)?;
                self.builder
                    .build_store(out_tag, i64t.const_int(*ret_fail_tag as u64, false))
                    .map_err(err)?;
                if !fail_payload.is_empty() {
                    let from = self
                        .builder
                        .build_struct_gep(src_st, slot, 1, "from_payload")
                        .map_err(err)?;
                    let into = self
                        .builder
                        .build_struct_gep(ret_st, out, 1, "into_payload")
                        .map_err(err)?;
                    let from_arr = i64t.array_type(src_slots);
                    let into_arr = i64t.array_type(ret_slots);
                    for (i, ty) in fail_payload.iter().enumerate() {
                        let idx = i64t.const_int(i as u64, false);
                        let p = unsafe {
                            self.builder.build_in_bounds_gep(
                                from_arr,
                                from,
                                &[i64t.const_zero(), idx],
                                "from_slot",
                            )
                        }
                        .map_err(err)?;
                        let v = self.builder.build_load(self.llvm_type(ty), p, "carried").map_err(err)?;
                        let q = unsafe {
                            self.builder.build_in_bounds_gep(
                                into_arr,
                                into,
                                &[i64t.const_zero(), idx],
                                "into_slot",
                            )
                        }
                        .map_err(err)?;
                        self.builder.build_store(q, v).map_err(err)?;
                    }
                }
                // An enum is an aggregate, so it leaves through the caller's storage, the
                // same way an ordinary `return` of one does.
                if let Some(sret) = self.current_sret {
                    let loaded = self
                        .builder
                        .build_load(self.llvm_type(&Type::Named(ret_enum.clone())), out, "err_value")
                        .map_err(err)?;
                    self.builder.build_store(sret, loaded).map_err(err)?;
                    self.close_open_region()?;
                    self.builder.build_return(None).map_err(err)?;
                } else {
                    let loaded = self
                        .builder
                        .build_load(self.llvm_type(&Type::Named(ret_enum.clone())), out, "err_value")
                        .map_err(err)?;
                    self.close_open_region()?;
                    self.builder.build_return(Some(&loaded)).map_err(err)?;
                }

                // ---- the success path: the payload ----
                self.builder.position_at_end(ok_bb);
                let ok_payload = &src_variants[*ok_tag as usize];
                let from = self
                    .builder
                    .build_struct_gep(src_st, slot, 1, "ok_payload")
                    .map_err(err)?;
                let arr_ty = i64t.array_type(src_slots);
                let p = unsafe {
                    self.builder.build_in_bounds_gep(
                        arr_ty,
                        from,
                        &[i64t.const_zero(), i64t.const_zero()],
                        "ok_slot",
                    )
                }
                .map_err(err)?;
                self.builder
                    .build_load(self.llvm_type(&ok_payload[0]), p, "unwrapped")
                    .map_err(err)
            }
            TypedExprKind::VariantLit { enum_name, tag, arguments } => {
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
                if !arguments.is_empty() {
                    let payload_ptr = self
                        .builder
                        .build_struct_gep(st, slot, 1, "payload_ptr")
                        .map_err(|e| e.to_string())?;
                    let variants = self.enum_types[enum_name.as_str()].1.clone();
                    let arr_ty = i64t.array_type(self.payload_area(&variants));
                    // Cell offsets, so a payload wider than one cell does not overlap the next.
                    // `store` places a whole LLVM aggregate as happily as an i64, so a class
                    // payload needs no memcpy here — only the right address.
                    let (offsets, _) = self.payload_offsets(&variants[*tag as usize]);
                    for (i, a) in arguments.iter().enumerate() {
                        let v = self.gen_expr(a)?;
                        let idx = i64t.const_int(offsets[i] as u64, false);
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
            TypedExprKind::MethodCall { receiver, method, receiver_mut, base, arguments } => {
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

                for a in arguments {
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
                for a in arguments {
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
            TypedExprKind::DynCoerce { interface_name, concrete, var } => {
                // Pair the binding's storage with the static vtable. No copy:
                // the data half IS the borrowed value (and the layout it points
                // at is unchanged by being viewed as an interface object).
                let (slot, _) = *self
                    .vars
                    .get(var)
                    .ok_or_else(|| format!("codegen: unknown variable {}", var))?;
                let vtable = self
                    .vtables
                    .get(&(interface_name.clone(), concrete.clone()))
                    .ok_or_else(|| {
                        format!("codegen bug: no vtable for {}/{}", concrete, interface_name)
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
            TypedExprKind::DynCall { interface_name, method, slot, base, arguments } => {
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

                // Rebuild the callee's type: (receiver ptr, parameters...) -> ret.
                //
                // The receiver is a plain pointer, mutating or not — see `declare_method`,
                // which made that choice so a direct call and a vtable call cannot disagree
                // about the ABI. **That is why A11 needed nothing here.** `data` is the source
                // binding's own storage (see `DynCoerce` below, which copies nothing), so a
                // `mutable self` slot reached through this call writes the caller's value,
                // exactly as the direct call does. The typechecker decides whether it MAY; the
                // emitter was already able.
                let (param_tys, ret_ty) = self
                    .interface_slots
                    .get(interface_name)
                    .and_then(|s| s.get(*slot as usize))
                    .cloned()
                    .ok_or_else(|| {
                        format!("codegen bug: no signature for {}.{}", interface_name, method)
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
                for a in arguments {
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

                // Mirror the declared ABI attributes onto the call site — the same
                // sweep the direct call and the direct method call already do.
                //
                // This was MISSING, and it was a wrong answer in money rather than a
                // crash. `byval` is not decoration: on x86-64 it means the aggregate
                // travels in the stack argument area, while a bare pointer travels in
                // a register. A vtable target declares `byval(%bx.Item)` and this call
                // passed a plain pointer, so the callee read its record from wherever
                // the stack happened to be — and `if !item.taxable` then answered from
                // garbage. It returned a 0.0000 tax rate on a taxable item, silently.
                //
                // Two properties of the bug are worth recording, because they are what
                // made it survive so long. It is stack-layout dependent, so adding a
                // `print` moved the frame and the same program started answering
                // correctly — which is why six reductions all "passed". And the
                // receiver was already handled (see `declare_method`), so the code
                // above looks like someone thought about the ABI here.
                use inkwell::attributes::{Attribute, AttributeLoc};
                let mut idx = 0u32;
                if ret_is_agg {
                    let attr = self.ctx.create_type_attribute(
                        Attribute::get_named_enum_kind_id("sret"),
                        self.llvm_type(&ret_ty).as_any_type_enum(),
                    );
                    call.add_attribute(AttributeLoc::Param(0), attr);
                    idx = 1;
                }
                idx += 1; // the receiver: a plain pointer in both forms, never byval
                for p in &param_tys {
                    if is_aggregate(p) {
                        let attr = self.ctx.create_type_attribute(
                            Attribute::get_named_enum_kind_id("byval"),
                            self.llvm_type(p).as_any_type_enum(),
                        );
                        call.add_attribute(AttributeLoc::Param(idx), attr);
                    }
                    idx += 1;
                }

                if let Some(s) = sret_slot {
                    return self
                        .builder
                        .build_load(self.llvm_type(&ret_ty), s, "dret_val")
                        .map_err(|e| e.to_string());
                }
                match call.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(v) => Ok(v),
                    _ => Err(format!(
                        "codegen bug: dynamic call to {}.{} returned void",
                        interface_name, method
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
            TypedExprKind::Truncate { place, length } => {
                let slot = self.gen_place_addr(place)?;
                let n = self.gen_expr(length)?.into_int_value();
                self.build_truncate(&place.ty, slot, n).map(Into::into)
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
                        return Err(format!("codegen bug: field of non-class {}", other))
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

        // A growable array: the bound is the header's length, read now, and the element
        // lives in the region rather than in this slot.
        if let Type::Slice(elem) = &ty {
            let err = |e: inkwell::builder::BuilderError| e.to_string();
            let st = self.llvm_type(&ty).into_struct_type();
            let ptr_ty = self.ctx.ptr_type(AddressSpace::default());
            let data_p = self.builder.build_struct_gep(st, slot, 0, "data_p").map_err(err)?;
            let len_p = self.builder.build_struct_gep(st, slot, 1, "len_p").map_err(err)?;
            let data = self.builder.build_load(ptr_ty, data_p, "data").map_err(err)?;
            let live = self.builder.build_load(i64t, len_p, "live").map_err(err)?.into_int_value();
            let checked = self.build_checked_index(idx_val, live)?;
            let ll_elem = self.llvm_type(elem);
            return unsafe {
                self.builder.build_gep(ll_elem, data.into_pointer_value(), &[checked], "elem_ptr")
            }
            .map_err(err);
        }

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

    /// A libc function, declared exactly once. Declaring one twice makes LLVM
    /// rename the second, which surfaces as an undefined symbol at link time.
    /// A libc function the compiler needs, declared once.
    ///
    /// **B50.** A name already in the module is reused — which is right when it is the compiler's
    /// own earlier declaration, and wrong when it is a user's `external function` with a DIFFERENT
    /// signature. That produced an LLVM verifier error, *"Call parameter type does not match
    /// function signature"*, from a compiler whose one non-negotiable guarantee is that every
    /// failure is named. A backend's diagnostic reaching a user is the same defect as no
    /// diagnostic at all: it names something they did not write.
    ///
    /// The NAME is not the problem, which is why this is not a reserved-word list. `lib/os.bx`,
    /// `lib/files.bx` and `lib/secure.bx` all declare `malloc` and always have; they agree with the
    /// compiler about what `malloc` is, so nothing conflicts. Only a DISAGREEMENT is refused, and
    /// the check therefore cannot fall behind a list — a symbol added to codegen tomorrow is
    /// covered the day it is added.
    fn libc(&mut self, name: &str, ty: inkwell::types::FunctionType<'ctx>) -> FunctionValue<'ctx> {
        match self.module.get_function(name) {
            Some(f) => {
                if f.get_type() != ty {
                    self.libc_conflicts.push(name.to_string());
                }
                f
            }
            None => self.module.add_function(name, ty, None),
        }
    }

    /// Read a whole file into the current region and return it as a String.
    /// NUL-terminated, so it is an ordinary Burxt String afterwards.
    fn build_read_file(&mut self, path: PointerValue<'ctx>) -> Result<PointerValue<'ctx>, String> {
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let i64t = self.ctx.i64_type();
        let i32t = self.ctx.i32_type();
        let i8t = self.ctx.i8_type();
        let ptr = self.ctx.ptr_type(AddressSpace::default());

        let fopen = self.libc("fopen", ptr.fn_type(&[ptr.into(), ptr.into()], false));
        let fseek = self.libc(
            "fseek",
            i32t.fn_type(&[ptr.into(), i64t.into(), i32t.into()], false),
        );
        let ftell = self.libc("ftell", i64t.fn_type(&[ptr.into()], false));
        let fread = self.libc(
            "fread",
            i64t.fn_type(&[ptr.into(), i64t.into(), i64t.into(), ptr.into()], false),
        );
        let fclose = self.libc("fclose", i32t.fn_type(&[ptr.into()], false));

        let mode = self.global_str("rb", "mode_rb");
        let handle = self
            .builder
            .build_call(fopen, &[path.into(), mode.into()], "fh")
            .map_err(err)?;
        let fh = match handle.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => v.into_pointer_value(),
            _ => return Err("fopen returned void".to_string()),
        };

        let function = self
            .builder
            .get_insert_block()
            .and_then(|b| b.get_parent())
            .ok_or("codegen bug: read_file outside a function")?;
        let missing_bb = self.ctx.append_basic_block(function, "no_file");
        let open_bb = self.ctx.append_basic_block(function, "file_open");
        let is_null = self.builder.build_is_null(fh, "no_handle").map_err(err)?;
        self.builder.build_conditional_branch(is_null, missing_bb, open_bb).map_err(err)?;

        // An unreadable file is a named error, not a silent empty string.
        self.builder.position_at_end(missing_bb);
        self.build_panic("burxt runtime error: cannot open file for reading\n")?;

        self.builder.position_at_end(open_bb);
        // SEEK_END is 2; measure, then rewind.
        self.builder
            .build_call(
                fseek,
                &[fh.into(), i64t.const_zero().into(), i32t.const_int(2, false).into()],
                "to_end",
            )
            .map_err(err)?;
        let size_call = self.builder.build_call(ftell, &[fh.into()], "size").map_err(err)?;
        let size = match size_call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
            _ => return Err("ftell returned void".to_string()),
        };
        self.builder
            .build_call(
                fseek,
                &[fh.into(), i64t.const_zero().into(), i32t.const_zero().into()],
                "rewind",
            )
            .map_err(err)?;

        // B49. A DIRECTORY opens, and `fseek` to its end answers 9223372036854775807 — so the
        // allocation below asked for eight exabytes and the program died saying "region memory
        // exhausted", which names the arena and blames the wrong thing entirely. The reader had
        // handed a directory to a file reader; nothing about their memory was the problem.
        //
        // The bound is the region's own size, which makes the message true either way: a size this
        // build cannot hold is unreadable whether it came from a directory or from a genuinely
        // enormous file, and both are named in one sentence rather than guessed between.
        //
        // Checked BEFORE the allocation, because after it the honest answer has already been
        // replaced by the arena's.
        let sane_bb = self.ctx.append_basic_block(function, "size_is_sane");
        let unreadable_bb = self.ctx.append_basic_block(function, "not_a_readable_file");
        let too_big = self
            .builder
            .build_int_compare(
                inkwell::IntPredicate::UGT,
                size,
                i64t.const_int(4 * 1024 * 1024 * 1024, false),
                "size_absurd",
            )
            .map_err(err)?;
        let negative = self
            .builder
            .build_int_compare(inkwell::IntPredicate::SLT, size, i64t.const_zero(), "size_neg")
            .map_err(err)?;
        let unreadable = self.builder.build_or(too_big, negative, "unreadable").map_err(err)?;
        self.builder.build_conditional_branch(unreadable, unreadable_bb, sane_bb).map_err(err)?;

        self.builder.position_at_end(unreadable_bb);
        self.build_panic(
            "burxt runtime error: cannot read this as a file — it is a directory, or it is larger \
             than this build can hold. `file_is_directory` asks the first question; \
             `file_read_bytes` reads what is not text.\n",
        )?;

        self.builder.position_at_end(sane_bb);
        let buf = self.build_alloc_string(size)?;
        self.builder
            .build_call(
                fread,
                &[buf.into(), i64t.const_int(1, false).into(), size.into(), fh.into()],
                "read",
            )
            .map_err(err)?;
        self.builder.build_call(fclose, &[fh.into()], "close").map_err(err)?;
        let end = unsafe { self.builder.build_gep(i8t, buf, &[size], "end") }.map_err(err)?;
        self.builder.build_store(end, i8t.const_zero()).map_err(err)?;
        // B5. The file's bytes are now a String, and a Burxt String is UTF-8. Checked HERE, at the
        // boundary, rather than left for whatever reads it later: an invalid byte that gets in is
        // a wrong answer somewhere else entirely, and the point of a boundary check is that the
        // error names the door it came through. `file_read_bytes` is the way in for data that is
        // not text.
        self.build_require_utf8(buf, "read_file")?;
        Ok(buf)
    }

    /// One bit operation, with the shift distance checked.
    ///
    /// The check is not politeness. A shift by 64 or more is UNDEFINED in LLVM — it does not mean
    /// "every bit falls off the end", it means the optimiser may assume it never happens and emit
    /// whatever follows from that. On x86 the hardware masks the distance to 6 bits, so `x << 64`
    /// silently becomes `x << 0` and the value comes back unchanged: a wrong answer that looks like
    /// a working program, which is the one outcome this language exists to prevent. A literal is
    /// refused by the checker; anything else dies here, naming the range.
    fn build_bit(
        &mut self,
        op: BitOp,
        a: IntValue<'ctx>,
        b: Option<IntValue<'ctx>>,
    ) -> Result<IntValue<'ctx>, String> {
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let i64t = self.ctx.i64_type();
        match op {
            BitOp::Not => self
                .builder
                .build_not(a, "bit_not")
                .map_err(err),
            BitOp::And => self
                .builder
                .build_and(a, b.ok_or("codegen bug: bit_and with one argument")?, "bit_and")
                .map_err(err),
            BitOp::Or => self
                .builder
                .build_or(a, b.ok_or("codegen bug: bit_or with one argument")?, "bit_or")
                .map_err(err),
            BitOp::Xor => self
                .builder
                .build_xor(a, b.ok_or("codegen bug: bit_xor with one argument")?, "bit_xor")
                .map_err(err),
            BitOp::Left | BitOp::RightZeros | BitOp::RightSign => {
                let n = b.ok_or("codegen bug: a shift with one argument")?;
                // 0 <= n <= 63, as one unsigned comparison: a negative distance wraps to something
                // enormous, so `n as u64 > 63` catches both ends at once.
                let too_far = self
                    .builder
                    .build_int_compare(
                        inkwell::IntPredicate::UGT,
                        n,
                        i64t.const_int(63, false),
                        "shift_too_far",
                    )
                    .map_err(err)?;
                let function = self
                    .builder
                    .get_insert_block()
                    .and_then(|bb| bb.get_parent())
                    .ok_or("codegen bug: a shift outside a function")?;
                let bad = self.ctx.append_basic_block(function, "shift_undefined");
                let ok = self.ctx.append_basic_block(function, "shift_ok");
                self.builder.build_conditional_branch(too_far, bad, ok).map_err(err)?;
                self.builder.position_at_end(bad);
                self.build_panic(
                    "burxt runtime error: a shift distance must be 0 to 63 — an Int is 64 bits\n",
                )?;
                self.builder.position_at_end(ok);
                match op {
                    BitOp::Left => self.builder.build_left_shift(a, n, "shift_left").map_err(err),
                    // `false` is a logical shift (zeros), `true` an arithmetic one (sign).
                    BitOp::RightZeros => self
                        .builder
                        .build_right_shift(a, n, false, "shift_right_zeros")
                        .map_err(err),
                    _ => self
                        .builder
                        .build_right_shift(a, n, true, "shift_right_sign")
                        .map_err(err),
                }
            }
        }
    }

    /// Copy the NUL-terminated bytes at a C pointer into a region-allocated Burxt String.
    ///
    /// **The copy is the pointer wall.** After this returns, Burxt holds bytes it owns and the C
    /// pointer is not kept anywhere — so "who frees that memory" and "is it still valid" stop being
    /// questions the compiler has to answer, because nothing will look again. If C wants the memory
    /// freed, the program calls an `external function free` itself, in the open.
    ///
    /// A null pointer dies here rather than answering "". An unset value and an empty one are
    /// different facts, and one String for both is exactly the silent wrong answer this language
    /// exists to refuse — `c_is_null(p)` is how you ask.
    /// Copy `n` bytes from a C pointer into a region-allocated Burxt `[Int]`.
    ///
    /// The counterpart to `build_c_string_at`, and the same wall: after this returns Burxt holds bytes
    /// it owns and the pointer is not kept, so "who frees it" and "is it still valid" stop being
    /// questions the compiler has to answer.
    ///
    /// **What differs is where the length comes from**, and it is the pointer wall's one soft edge.
    /// `c_string_at` reads to a NUL, which is a fact in the data. Here `n` is the caller's CLAIM, and
    /// nothing in the type can check it — a length longer than the buffer reads past the end. That is
    /// declared rather than inferred, the same bargain `as scaled` makes at the same boundary.
    ///
    /// The half that CAN be checked is: a null pointer, and a negative count. A negative count is not
    /// a smaller read — as an unsigned size it is an enormous one — so it dies here by name rather
    /// than allocating whatever `-1` becomes.
    fn build_c_bytes_at(
        &mut self,
        slice_ty: &Type,
        p: PointerValue<'ctx>,
        n: IntValue<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let i64t = self.ctx.i64_type();
        let i8t = self.ctx.i8_type();
        let ptr = self.ctx.ptr_type(AddressSpace::default());

        let function = self
            .builder
            .get_insert_block()
            .and_then(|b| b.get_parent())
            .ok_or("codegen bug: c_bytes_at outside a function")?;

        // A null pointer, first — the same refusal `c_string_at` makes, for the same reason: zero
        // bytes from nowhere is indistinguishable from zero bytes that were really there.
        let null_bb = self.ctx.append_basic_block(function, "c_bytes_null");
        let checked_bb = self.ctx.append_basic_block(function, "c_bytes_checked");
        let is_null = self.builder.build_is_null(p, "c_bytes_is_null").map_err(err)?;
        self.builder.build_conditional_branch(is_null, null_bb, checked_bb).map_err(err)?;
        self.builder.position_at_end(null_bb);
        self.build_panic(
            "burxt runtime error: c_bytes_at was given a null pointer; ask c_is_null(p) first\n",
        )?;

        // Then the count. A literal is refused by the checker; anything computed dies here.
        self.builder.position_at_end(checked_bb);
        let bad_bb = self.ctx.append_basic_block(function, "c_bytes_negative");
        let ok_bb = self.ctx.append_basic_block(function, "c_bytes_ok");
        let negative = self
            .builder
            .build_int_compare(inkwell::IntPredicate::SLT, n, i64t.const_zero(), "c_bytes_neg")
            .map_err(err)?;
        self.builder.build_conditional_branch(negative, bad_bb, ok_bb).map_err(err)?;
        self.builder.position_at_end(bad_bb);
        self.build_panic(
            "burxt runtime error: c_bytes_at was asked for a negative number of bytes\n",
        )?;

        self.builder.position_at_end(ok_bb);
        let elem_ty = match slice_ty {
            Type::Slice(t) => t.as_ref().clone(),
            other => return Err(format!("codegen bug: c_bytes_at answering {}", other)),
        };
        // At least one cell, so an empty read still has a home — the same rule the slice literal
        // follows for `[]`.
        let one = i64t.const_int(1, false);
        let cap = self
            .builder
            .build_select(
                self.builder
                    .build_int_compare(inkwell::IntPredicate::SGT, n, i64t.const_zero(), "any")
                    .map_err(err)?,
                n,
                one,
                "c_bytes_cap",
            )
            .map_err(err)?
            .into_int_value();
        let data = self.build_alloc_array(&elem_ty, cap)?;

        // One byte per element, ZERO-extended: a byte is 0..=255, and sign-extending 0xFF to -1 would
        // hand back a different number than C had. `write_bytes` truncates on the way out, so the two
        // are inverses.
        let loop_bb = self.ctx.append_basic_block(function, "c_bytes_loop");
        let body_bb = self.ctx.append_basic_block(function, "c_bytes_body");
        let done_bb = self.ctx.append_basic_block(function, "c_bytes_done");
        let index = self.create_entry_alloca("c_bytes_i", &Type::Int)?;
        self.builder.build_store(index, i64t.const_zero()).map_err(err)?;
        self.builder.build_unconditional_branch(loop_bb).map_err(err)?;

        self.builder.position_at_end(loop_bb);
        let i = self.builder.build_load(i64t, index, "i").map_err(err)?.into_int_value();
        let more = self
            .builder
            .build_int_compare(inkwell::IntPredicate::SLT, i, n, "more")
            .map_err(err)?;
        self.builder.build_conditional_branch(more, body_bb, done_bb).map_err(err)?;

        self.builder.position_at_end(body_bb);
        let at = unsafe { self.builder.build_gep(i8t, p, &[i], "c_byte_at") }.map_err(err)?;
        let byte = self.builder.build_load(i8t, at, "c_byte").map_err(err)?.into_int_value();
        let widened = self.builder.build_int_z_extend(byte, i64t, "c_byte_wide").map_err(err)?;
        let slot = unsafe { self.builder.build_gep(i64t, data, &[i], "c_byte_slot") }.map_err(err)?;
        self.builder.build_store(slot, widened).map_err(err)?;
        let next = self.builder.build_int_add(i, i64t.const_int(1, false), "next").map_err(err)?;
        self.builder.build_store(index, next).map_err(err)?;
        self.builder.build_unconditional_branch(loop_bb).map_err(err)?;

        self.builder.position_at_end(done_bb);
        let _ = ptr;
        self.build_slice_value(slice_ty, data, n, cap)
    }

    /// Burxt's bytes into C's memory. The exact mirror of `build_c_bytes_at`, and the loop is the
    /// same loop with the load and the store swapped.
    ///
    /// Three refusals, and the middle one is the only interesting decision:
    ///
    /// - **A null destination panics**, the same as reading one. Writing 16 bytes to address zero
    ///   is a segfault a moment later and a mystery to whoever reads the core dump.
    /// - **An element outside 0..=255 panics, naming the index.** Truncating would be cheaper by
    ///   one branch and would write a byte the caller did not write down — `256` silently becoming
    ///   `0` is a corrupt port number, a corrupt length prefix, a corrupt checksum. This is the
    ///   language whose whole argument is that the quiet wrong answer is the expensive one.
    /// - **Nothing checks that the destination is big enough**, because nothing can: the capacity
    ///   belongs to C. `c_bytes_at` documents the same soft edge on the way in.
    ///
    /// Answers the count written, so `let n: Int = c_bytes_to(p, sockaddr);` reads like the `read`
    /// and `write` it exists to feed.
    fn build_c_bytes_to(
        &mut self,
        p: PointerValue<'ctx>,
        arr: inkwell::values::StructValue<'ctx>,
    ) -> Result<IntValue<'ctx>, String> {
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let i64t = self.ctx.i64_type();
        let i8t = self.ctx.i8_type();

        let function = self
            .builder
            .get_insert_block()
            .and_then(|b| b.get_parent())
            .ok_or("codegen bug: c_bytes_to outside a function")?;

        let null_bb = self.ctx.append_basic_block(function, "c_to_null");
        let checked_bb = self.ctx.append_basic_block(function, "c_to_checked");
        let is_null = self.builder.build_is_null(p, "c_to_is_null").map_err(err)?;
        self.builder.build_conditional_branch(is_null, null_bb, checked_bb).map_err(err)?;
        self.builder.position_at_end(null_bb);
        self.build_panic(
            "burxt runtime error: c_bytes_to was given a null pointer; ask c_is_null(p) first\n",
        )?;

        self.builder.position_at_end(checked_bb);
        let data = self
            .builder
            .build_extract_value(arr, 0, "c_to_data")
            .map_err(err)?
            .into_pointer_value();
        let n = self.builder.build_extract_value(arr, 1, "c_to_len").map_err(err)?.into_int_value();

        let loop_bb = self.ctx.append_basic_block(function, "c_to_loop");
        let body_bb = self.ctx.append_basic_block(function, "c_to_body");
        let range_bb = self.ctx.append_basic_block(function, "c_to_range");
        let store_bb = self.ctx.append_basic_block(function, "c_to_store");
        let done_bb = self.ctx.append_basic_block(function, "c_to_done");
        let index = self.create_entry_alloca("c_to_i", &Type::Int)?;
        self.builder.build_store(index, i64t.const_zero()).map_err(err)?;
        self.builder.build_unconditional_branch(loop_bb).map_err(err)?;

        self.builder.position_at_end(loop_bb);
        let i = self.builder.build_load(i64t, index, "i").map_err(err)?.into_int_value();
        let more =
            self.builder.build_int_compare(inkwell::IntPredicate::SLT, i, n, "more").map_err(err)?;
        self.builder.build_conditional_branch(more, body_bb, done_bb).map_err(err)?;

        // A byte is 0..=255. Both ends, because -1 is as wrong as 256 and reads differently.
        self.builder.position_at_end(body_bb);
        let slot = unsafe { self.builder.build_gep(i64t, data, &[i], "c_to_slot") }.map_err(err)?;
        let value = self.builder.build_load(i64t, slot, "c_to_value").map_err(err)?.into_int_value();
        let low = self
            .builder
            .build_int_compare(inkwell::IntPredicate::SLT, value, i64t.const_zero(), "c_to_low")
            .map_err(err)?;
        let high = self
            .builder
            .build_int_compare(
                inkwell::IntPredicate::SGT,
                value,
                i64t.const_int(255, false),
                "c_to_high",
            )
            .map_err(err)?;
        let outside = self.builder.build_or(low, high, "c_to_outside").map_err(err)?;
        self.builder.build_conditional_branch(outside, range_bb, store_bb).map_err(err)?;

        self.builder.position_at_end(range_bb);
        self.build_panic(
            "burxt runtime error: c_bytes_to was given a number that is not a byte (0..=255)\n",
        )?;

        self.builder.position_at_end(store_bb);
        let byte = self.builder.build_int_truncate(value, i8t, "c_to_byte").map_err(err)?;
        let at = unsafe { self.builder.build_gep(i8t, p, &[i], "c_to_at") }.map_err(err)?;
        self.builder.build_store(at, byte).map_err(err)?;
        let next = self.builder.build_int_add(i, i64t.const_int(1, false), "next").map_err(err)?;
        self.builder.build_store(index, next).map_err(err)?;
        self.builder.build_unconditional_branch(loop_bb).map_err(err)?;

        self.builder.position_at_end(done_bb);
        Ok(n)
    }

    fn build_c_string_at(
        &mut self,
        p: PointerValue<'ctx>,
    ) -> Result<PointerValue<'ctx>, String> {
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let i64t = self.ctx.i64_type();
        let i8t = self.ctx.i8_type();
        let ptr = self.ctx.ptr_type(AddressSpace::default());

        let function = self
            .builder
            .get_insert_block()
            .and_then(|b| b.get_parent())
            .ok_or("codegen bug: c_string_at outside a function")?;
        let null_bb = self.ctx.append_basic_block(function, "c_str_null");
        let ok_bb = self.ctx.append_basic_block(function, "c_str_ok");
        let is_null = self.builder.build_is_null(p, "c_str_is_null").map_err(err)?;
        self.builder.build_conditional_branch(is_null, null_bb, ok_bb).map_err(err)?;

        self.builder.position_at_end(null_bb);
        self.build_panic(
            "burxt runtime error: c_string_at was given a null pointer; ask c_is_null(p) first\n",
        )?;

        self.builder.position_at_end(ok_bb);
        let strlen = self.libc("strlen", i64t.fn_type(&[ptr.into()], false));
        let len_call = self.builder.build_call(strlen, &[p.into()], "c_len").map_err(err)?;
        let len = match len_call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
            _ => return Err("strlen returned void".to_string()),
        };
        let buf = self.build_alloc_string(len)?;
        let memcpy = self.libc(
            "memcpy",
            ptr.fn_type(&[ptr.into(), ptr.into(), i64t.into()], false),
        );
        self.builder
            .build_call(memcpy, &[buf.into(), p.into(), len.into()], "c_copy")
            .map_err(err)?;
        let end = unsafe { self.builder.build_gep(i8t, buf, &[len], "c_end") }.map_err(err)?;
        self.builder.build_store(end, i8t.const_zero()).map_err(err)?;
        // B5, and this is the widest door of the four: every `char*` a C library hands back becomes
        // a Burxt String here. It also covers `os_env`, which is not a builtin at all — `lib/os.bx`
        // reaches `getenv` and copies through this, so checking here checks that too. One place,
        // not two rules that could drift.
        self.build_require_utf8(buf, "c_string_at")?;
        Ok(buf)
    }

    /// `byte_as_string(n)` — a one-byte String holding `n`, the exact inverse of `byte_at`.
    ///
    /// Emitted INLINE rather than as a lazily-defined `burxt.*` helper, which is what `substring`
    /// and `c_string_at` do and for the same reason: the failure NAMES the offending number, and a
    /// shared helper would have to take it as a parameter to print it anyway. Nine instructions.
    ///
    /// ONE unsigned comparison covers both ends. A negative `n` wraps to something enormous, so
    /// `n u<= 255` is `n >= 0 && n <= 255` signed — the same trick the shift-distance check uses,
    /// and the reason there is no second branch for the negative case.
    fn build_byte_as_string(&mut self, n: IntValue<'ctx>) -> Result<PointerValue<'ctx>, String> {
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let i64t = self.ctx.i64_type();
        let i8t = self.ctx.i8_type();

        let fits = self
            .builder
            .build_int_compare(
                inkwell::IntPredicate::ULE,
                n,
                i64t.const_int(255, false),
                "byte_fits",
            )
            .map_err(err)?;
        let function = self
            .builder
            .get_insert_block()
            .and_then(|b| b.get_parent())
            .ok_or("codegen bug: byte_as_string outside a function")?;
        let broken = self.ctx.append_basic_block(function, "byte_out_of_range");
        let ok = self.ctx.append_basic_block(function, "byte_ok");
        self.builder.build_conditional_branch(fits, ok, broken).map_err(err)?;

        self.builder.position_at_end(broken);
        let fprintf = self.fprintf_fn();
        let (stderr_g, _, exit) = self.panic_deps();
        let fmt = self.global_str(
            "burxt runtime error: byte_as_string(%lld) has no answer — a byte is 0 to 255\n",
            "fmt_byte_range",
        );
        let stream = self.load_stderr(stderr_g)?;
        let arguments: Vec<BasicMetadataValueEnum> = vec![stream.into(), fmt.into(), n.into()];
        self.builder.build_call(fprintf, &arguments, "fprintf").map_err(err)?;
        self.build_exit70(exit)?;

        self.builder.position_at_end(ok);
        // `build_alloc_string` writes the length header AND the trailing NUL, so the only byte
        // left to store is the one asked for. That is also why a NUL byte needs no special case:
        // the length is in the header, not in the bytes, so `byte_as_string(0)` is a String of
        // length 1 that happens to hold a zero.
        let out = self.build_alloc_string(i64t.const_int(1, false))?;
        let byte = self.builder.build_int_truncate(n, i8t, "byte").map_err(err)?;
        self.builder.build_store(out, byte).map_err(err)?;
        Ok(out)
    }

    /// Render a value to a region-allocated String, using the SAME format the
    /// printer uses so the two can never disagree.
    fn build_to_string(
        &mut self,
        ty: &Type,
        val: BasicValueEnum<'ctx>,
    ) -> Result<PointerValue<'ctx>, String> {
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let i64t = self.ctx.i64_type();
        let ptr = self.ctx.ptr_type(AddressSpace::default());

        // Bool renders to one of two literals — no allocation needed at all.
        if *ty == Type::Bool {
            let is_true = self
                .builder
                .build_int_compare(
                    inkwell::IntPredicate::NE,
                    val.into_int_value(),
                    i64t.const_zero(),
                    "is_true",
                )
                .map_err(err)?;
            let t = self.global_str("true", "s_true");
            let f = self.global_str("false", "s_false");
            return Ok(self
                .builder
                .build_select(is_true, t, f, "bool_str")
                .map_err(err)?
                .into_pointer_value());
        }

        let snprintf = self.libc(
            "snprintf",
            self.ctx
                .i32_type()
                .fn_type(&[ptr.into(), i64t.into(), ptr.into()], true),
        );

        // Build the same argument list the printer would, then size it with a
        // dry run before allocating.
        let (fmt, arguments): (PointerValue<'ctx>, Vec<BasicMetadataValueEnum>) = match ty {
            Type::Int => (self.global_str("%lld", "f_int"), vec![val.into()]),
            // **A Decimal returns here rather than falling through to `snprintf`.** This is the
            // path a value takes when it becomes text INSIDE a program — into a view, a log line,
            // a JSON field — and it is the one that matters most for a host that supplies its own
            // `printf`. `build_decimal_text` does the whole job, so there is no format string to
            // hand anybody and nothing for a varargs walker to misread.
            Type::Decimal { scale, .. } => {
                return self.build_decimal_text(val.into_int_value(), *scale);
            }
            other => return Err(format!("codegen bug: to_string of {}", other)),
        };

        let mut dry: Vec<BasicMetadataValueEnum> =
            vec![ptr.const_null().into(), i64t.const_zero().into(), fmt.into()];
        dry.extend(arguments.iter().cloned());
        let need = self.builder.build_call(snprintf, &dry, "need").map_err(err)?;
        let n32 = match need.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
            _ => return Err("snprintf returned void".to_string()),
        };
        let n = self.builder.build_int_s_extend(n32, i64t, "need64").map_err(err)?;
        // `n` is what snprintf said it needs, NOT counting the NUL. `build_alloc_string` writes
        // that as the header and reserves the NUL, and snprintf is then handed the capacity
        // including it — the two counts differ by one and mixing them up truncates the last byte.
        let cap = self
            .builder
            .build_int_add(n, i64t.const_int(1, false), "cap")
            .map_err(err)?;
        let buf = self.build_alloc_string(n)?;
        let mut real: Vec<BasicMetadataValueEnum> = vec![buf.into(), cap.into(), fmt.into()];
        real.extend(arguments);
        self.builder.build_call(snprintf, &real, "render").map_err(err)?;
        Ok(buf)
    }

    /// Get (or declare once) libc `fprintf`. Declaring it twice makes LLVM
    /// rename the second, which surfaces as an undefined symbol at link time.
    fn fprintf_fn(&mut self) -> FunctionValue<'ctx> {
        if let Some(f) = self.module.get_function("fprintf") {
            return f;
        }
        let ptr = self.ctx.ptr_type(AddressSpace::default());
        let i32t = self.ctx.i32_type();
        self.module
            .add_function("fprintf", i32t.fn_type(&[ptr.into(), ptr.into()], true), None)
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
                // Named `stderr` here and RENAMED for Apple targets in `stamp_target`, which is
                // the first moment the triple is known — see the note there. Choosing the name
                // at this point cannot work: `compile()` runs before any target is set.
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
    /// The one address every stack check compares against: how far down the stack a call may go
    /// before the program is out of room. Set once, in `main`. B7.
    fn stack_floor_global(&mut self) -> inkwell::values::GlobalValue<'ctx> {
        if let Some(g) = self.module.get_global("burxt.stack_floor") {
            return g;
        }
        let i64t = self.ctx.i64_type();
        let g = self.module.add_global(i64t, None, "burxt.stack_floor");
        // Zero means "not set yet", and the guard treats zero as no limit. `main` sets it before
        // anything else runs, so the only code that can see zero is code that ran before `main`,
        // which in a Burxt program is nothing.
        g.set_initializer(&i64t.const_zero());
        g
    }

    /// Work out where the stack runs out, and record it. Emitted at the top of `main`. B7.
    ///
    /// `DESIGN.md` says nothing in the language should fail anonymously, and a stack overflow was
    /// the ONE failure that did: a recursion with no base case died of a raw SIGSEGV, exit 139, no
    /// message. Verified still true at v0.0.284, at `-O2` and at `-O0` both — worth checking both,
    /// because at `-O2` LLVM turns some recursions into loops and the fault disappears, which makes
    /// this exactly the kind of bug that looks fixed depending on how you built it.
    ///
    /// **A probe alloca rather than `llvm.frameaddress`.** An `alloca` in the entry block IS a
    /// stack address, it needs no intrinsic, and it is one instruction that the register allocator
    /// folds away. It also spells identically in stage-1, which emits IR as text — and a guarantee
    /// the two compilers implement differently is a guarantee that will diverge.
    ///
    /// **`getrlimit` rather than a constant.** The stack is whatever the OS gave this process, and
    /// a hardcoded 8 MB would be wrong on a machine with `ulimit -s unlimited` and wrong the other
    /// way inside a container. `RLIMIT_STACK` is 3 on both Linux and macOS.
    fn build_stack_floor(&mut self) -> Result<(), String> {
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let i64t = self.ctx.i64_type();
        let i32t = self.ctx.i32_type();
        let i8t = self.ctx.i8_type();
        let ptr = self.ctx.ptr_type(AddressSpace::default());

        let probe = self.builder.build_alloca(i8t, "stack_top").map_err(err)?;
        let base = self.builder.build_ptr_to_int(probe, i64t, "stack_base").map_err(err)?;

        // struct rlimit { rlim_t rlim_cur; rlim_t rlim_max; } — two 64-bit values on every
        // platform this targets, so a two-element array is the whole of the layout.
        let rlimit_ty = i64t.array_type(2);
        let rlim = self.builder.build_alloca(rlimit_ty, "rlimit").map_err(err)?;
        self.builder.build_store(rlim, rlimit_ty.const_zero()).map_err(err)?;
        let getrlimit = self.libc("getrlimit", i32t.fn_type(&[i32t.into(), ptr.into()], false));
        self.builder
            .build_call(getrlimit, &[i32t.const_int(3, false).into(), rlim.into()], "rl")
            .map_err(err)?;
        let cur = self.builder.build_load(i64t, rlim, "stack_size").map_err(err)?.into_int_value();

        // RLIM_INFINITY, or anything absurd, falls back to 8 MB — the common default, and the
        // point is to have SOME floor rather than the exactly right one. A guard that gives up
        // when the limit is unusual is a guard that is absent precisely where recursion is deepest.
        //
        // **The lower bound is not decoration.** `getrlimit` can FAIL, and this code ignores its
        // return value, so `rlim_cur` stays at the zero it was initialised to. Zero passed the
        // `< 2^40` test, gave `size = 0`, and then `0 - 128 KB` wrapped to a colossal unsigned
        // number — making `floor` larger than any real stack pointer, so EVERY call reported
        // "this call went too deep" and the program exited 70 before running a line. That is a
        // defect on any platform where `getrlimit` fails, not a wasm one; wasm is merely where it
        // is guaranteed. So the sanity test now has both ends.
        let eight_mb = i64t.const_int(8 * 1024 * 1024, false);
        let margin = i64t.const_int(128 * 1024, false);
        let big_enough = self
            .builder
            .build_int_compare(inkwell::IntPredicate::UGT, cur, margin, "big_enough")
            .map_err(err)?;
        let not_absurd = self
            .builder
            .build_int_compare(inkwell::IntPredicate::ULT, cur, i64t.const_int(1 << 40, false), "not_absurd")
            .map_err(err)?;
        let sane = self.builder.build_and(big_enough, not_absurd, "sane").map_err(err)?;
        let size = self.builder.build_select(sane, cur, eight_mb, "stack_room").map_err(err)?.into_int_value();

        // Leave a margin so the guard itself, and `fprintf`, have room to run after it fires.
        // Reporting a full stack by overflowing the stack would be a poor joke.
        let usable = self.builder.build_int_sub(size, margin, "usable").map_err(err)?;

        // And the subtraction that produces the floor SATURATES at zero. On a 64-bit OS the stack
        // sits at a high address and `base - usable` can never wrap; on wasm32 the linear-memory
        // stack sits near address zero and it always does. Zero is the right saturation point
        // because the guard below already treats `floor == 0` as "not set yet" — so a machine
        // whose stack starts below `usable` gets no guard rather than a guard that fires on every
        // call, and wasm traps on its own call-stack exhaustion regardless.
        let would_wrap = self
            .builder
            .build_int_compare(inkwell::IntPredicate::ULT, base, usable, "would_wrap")
            .map_err(err)?;
        let raw_floor = self.builder.build_int_sub(base, usable, "raw_floor").map_err(err)?;
        let floor = self
            .builder
            .build_select(would_wrap, i64t.const_zero(), raw_floor, "floor")
            .map_err(err)?
            .into_int_value();
        let g = self.stack_floor_global();
        self.builder.build_store(g.as_pointer_value(), floor).map_err(err)?;
        Ok(())
    }

    /// One stack check, at the top of a function. B7.
    ///
    /// Cheap on purpose: an `alloca`, a load of a global that is hot in cache, a compare and a
    /// branch that predicts perfectly. It is emitted in EVERY function rather than only in ones the
    /// call graph shows to be recursive, and that is a deliberate trade — a static call graph
    /// cannot see recursion through a `dynamic` call, and a guard with a hole in it is worse than
    /// none, because the hole is exactly where someone writes the interesting recursion.
    fn build_stack_guard(&mut self) -> Result<(), String> {
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let i64t = self.ctx.i64_type();
        let i8t = self.ctx.i8_type();
        let function = self
            .builder
            .get_insert_block()
            .and_then(|b| b.get_parent())
            .ok_or("codegen bug: stack guard outside a function")?;

        let probe = self.builder.build_alloca(i8t, "here").map_err(err)?;
        let here = self.builder.build_ptr_to_int(probe, i64t, "sp").map_err(err)?;
        let g = self.stack_floor_global();
        let floor = self
            .builder
            .build_load(i64t, g.as_pointer_value(), "floor")
            .map_err(err)?
            .into_int_value();
        // Unsigned, and `floor != 0` guards the moment before `main` has set it.
        let set = self
            .builder
            .build_int_compare(inkwell::IntPredicate::NE, floor, i64t.const_zero(), "floor_set")
            .map_err(err)?;
        let low = self
            .builder
            .build_int_compare(inkwell::IntPredicate::ULT, here, floor, "too_deep")
            .map_err(err)?;
        let overflowed = self.builder.build_and(set, low, "stack_gone").map_err(err)?;
        let full_bb = self.ctx.append_basic_block(function, "stack_full");
        let room_bb = self.ctx.append_basic_block(function, "has_room");
        self.builder.build_conditional_branch(overflowed, full_bb, room_bb).map_err(err)?;

        self.builder.position_at_end(full_bb);
        self.build_panic(
            "burxt runtime error: this call went too deep and the stack is full — a recursion with \
             no base case, or one deeper than this machine's stack allows. `return f(...)` in tail \
             position reuses the frame and does not grow the stack.\n",
        )?;

        self.builder.position_at_end(room_bb);
        Ok(())
    }

    /// `@burxt.require.utf8(bytes, len, where)` — B5. Ends the program with a named error if the
    /// bytes are not valid UTF-8, naming WHERE they came in and WHICH byte is wrong.
    ///
    /// `spec/A4.4` says "a String is UTF-8. Decide this now and hold it", and `docs/limitations.md`
    /// tells a reader the invariant "is checked at every entry point". It was not. `read_file` of
    /// a file holding `0xff 0xfe` answered a 22-byte String and exit 0 — so the guarantee was
    /// published and not enforced, which is worse than not claiming it, because the whole point of
    /// the claim is that a reader stops checking.
    ///
    /// **One loop with a state machine, not one branch per length.** The obvious shape — decode the
    /// leading byte, then check two or three continuations — needs a block per width and repeats
    /// the continuation test three times, which is three places to get the surrogate and overlong
    /// edges wrong. Instead the leading byte sets how many continuations are still expected and the
    /// EXACT range the next one may take, and one test covers every case:
    ///
    ///   * `0xE0` demands `A0..BF` next, which is what rejects an overlong three-byte form.
    ///   * `0xED` demands `80..9F`, which is what rejects a surrogate — the encoding a UTF-16
    ///     escape pair produces if a decoder handles its halves separately (see `lib/json.bx`).
    ///   * `0xF0` demands `90..BF` and `0xF4` demands `80..8F`, which reject the overlong
    ///     four-byte form and everything above U+10FFFF.
    ///   * `0xC0`/`0xC1` never appear, so an overlong two-byte form cannot start.
    ///
    /// The leftover count is checked after the loop, which is what catches a sequence truncated by
    /// the end of the buffer — the case a per-width shape usually forgets, because it tests
    /// `i + width <= n` and then never looks again.
    fn utf8_fn(&mut self) -> Result<FunctionValue<'ctx>, String> {
        if let Some(f) = self.utf8_check_fn {
            return Ok(f);
        }
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let saved_block = self.builder.get_insert_block();

        let i64t = self.ctx.i64_type();
        let i8t = self.ctx.i8_type();
        let ptr = self.ctx.ptr_type(AddressSpace::default());
        let fprintf = self.fprintf_fn();
        let (stderr_g, _, exit) = self.panic_deps();

        let fn_ty = self.ctx.void_type().fn_type(&[ptr.into(), i64t.into(), ptr.into()], false);
        let f = self.module.add_function("burxt.require.utf8", fn_ty, None);
        let entry = self.ctx.append_basic_block(f, "entry");
        let head = self.ctx.append_basic_block(f, "loop");
        let body = self.ctx.append_basic_block(f, "byte");
        let lead = self.ctx.append_basic_block(f, "lead");
        let cont = self.ctx.append_basic_block(f, "continuation");
        let step = self.ctx.append_basic_block(f, "step");
        let done = self.ctx.append_basic_block(f, "done");
        let bad = self.ctx.append_basic_block(f, "not_utf8");
        let ok = self.ctx.append_basic_block(f, "ok");

        self.builder.position_at_end(entry);
        let bytes = f.get_nth_param(0).unwrap().into_pointer_value();
        let n = f.get_nth_param(1).unwrap().into_int_value();
        let source = f.get_nth_param(2).unwrap().into_pointer_value();
        // Allocas rather than phis: three carried values across five blocks is where a
        // hand-written phi web stops being readable, and mem2reg turns these back into registers.
        let i_slot = self.builder.build_alloca(i64t, "i").map_err(err)?;
        let need_slot = self.builder.build_alloca(i64t, "need").map_err(err)?;
        let lo_slot = self.builder.build_alloca(i64t, "lo").map_err(err)?;
        let hi_slot = self.builder.build_alloca(i64t, "hi").map_err(err)?;
        self.builder.build_store(i_slot, i64t.const_zero()).map_err(err)?;
        self.builder.build_store(need_slot, i64t.const_zero()).map_err(err)?;
        self.builder.build_store(lo_slot, i64t.const_int(0x80, false)).map_err(err)?;
        self.builder.build_store(hi_slot, i64t.const_int(0xBF, false)).map_err(err)?;
        self.builder.build_unconditional_branch(head).map_err(err)?;

        use inkwell::IntPredicate::*;
        self.builder.position_at_end(head);
        let i = self.builder.build_load(i64t, i_slot, "i.now").map_err(err)?.into_int_value();
        let more = self.builder.build_int_compare(SLT, i, n, "more").map_err(err)?;
        self.builder.build_conditional_branch(more, body, done).map_err(err)?;

        self.builder.position_at_end(body);
        let i = self.builder.build_load(i64t, i_slot, "i.b").map_err(err)?.into_int_value();
        let at = unsafe { self.builder.build_gep(i8t, bytes, &[i], "at") }.map_err(err)?;
        let raw = self.builder.build_load(i8t, at, "raw").map_err(err)?.into_int_value();
        let b = self.builder.build_int_z_extend(raw, i64t, "b").map_err(err)?;
        let need = self.builder.build_load(i64t, need_slot, "need.now").map_err(err)?.into_int_value();
        let mid_sequence =
            self.builder.build_int_compare(SGT, need, i64t.const_zero(), "mid").map_err(err)?;
        self.builder.build_conditional_branch(mid_sequence, cont, lead).map_err(err)?;

        // ---- a leading byte ----
        self.builder.position_at_end(lead);
        let c = |v: u64| i64t.const_int(v, false);
        let ascii = self.builder.build_int_compare(ULT, b, c(0x80), "ascii").map_err(err)?;
        // 0x80..0xC1 can never lead: 0x80..0xBF is a continuation with nothing to continue, and
        // 0xC0/0xC1 are the overlong two-byte forms.
        let stray = self.builder.build_int_compare(ULT, b, c(0xC2), "stray").map_err(err)?;
        let bad_lead = self.builder.build_and(
            self.builder.build_not(ascii, "not.ascii").map_err(err)?,
            stray,
            "bad.lead",
        ).map_err(err)?;
        let too_high = self.builder.build_int_compare(UGT, b, c(0xF4), "too.high").map_err(err)?;
        let reject = self.builder.build_or(bad_lead, too_high, "reject").map_err(err)?;
        let two = self.builder.build_int_compare(ULT, b, c(0xE0), "two").map_err(err)?;
        let three = self.builder.build_int_compare(ULT, b, c(0xF0), "three").map_err(err)?;
        // 0 for ASCII, else 1, 2 or 3 continuations.
        let n_three = self.builder.build_select(three, c(2), c(3), "n3").map_err(err)?.into_int_value();
        let n_multi = self.builder.build_select(two, c(1), n_three, "nm").map_err(err)?.into_int_value();
        let new_need =
            self.builder.build_select(ascii, i64t.const_zero(), n_multi, "need.next").map_err(err)?;
        // The range the NEXT byte may take. Only four leading bytes narrow it, and each one is
        // exactly one of UTF-8's four traps.
        let is_e0 = self.builder.build_int_compare(EQ, b, c(0xE0), "is.e0").map_err(err)?;
        let is_ed = self.builder.build_int_compare(EQ, b, c(0xED), "is.ed").map_err(err)?;
        let is_f0 = self.builder.build_int_compare(EQ, b, c(0xF0), "is.f0").map_err(err)?;
        let is_f4 = self.builder.build_int_compare(EQ, b, c(0xF4), "is.f4").map_err(err)?;
        let lo1 = self.builder.build_select(is_e0, c(0xA0), c(0x80), "lo1").map_err(err)?.into_int_value();
        let lo2 = self.builder.build_select(is_f0, c(0x90), lo1, "lo2").map_err(err)?;
        let hi1 = self.builder.build_select(is_ed, c(0x9F), c(0xBF), "hi1").map_err(err)?.into_int_value();
        let hi2 = self.builder.build_select(is_f4, c(0x8F), hi1, "hi2").map_err(err)?;
        self.builder.build_store(need_slot, new_need).map_err(err)?;
        self.builder.build_store(lo_slot, lo2).map_err(err)?;
        self.builder.build_store(hi_slot, hi2).map_err(err)?;
        self.builder.build_conditional_branch(reject, bad, step).map_err(err)?;

        // ---- a continuation byte ----
        self.builder.position_at_end(cont);
        let lo = self.builder.build_load(i64t, lo_slot, "lo.now").map_err(err)?.into_int_value();
        let hi = self.builder.build_load(i64t, hi_slot, "hi.now").map_err(err)?.into_int_value();
        let under = self.builder.build_int_compare(ULT, b, lo, "under").map_err(err)?;
        let over = self.builder.build_int_compare(UGT, b, hi, "over").map_err(err)?;
        let out_of_range = self.builder.build_or(under, over, "out").map_err(err)?;
        let need_now =
            self.builder.build_load(i64t, need_slot, "need.c").map_err(err)?.into_int_value();
        let left = self.builder.build_int_sub(need_now, c(1), "left").map_err(err)?;
        self.builder.build_store(need_slot, left).map_err(err)?;
        // Every continuation after the first is an ordinary one; only the byte right after the
        // leading one carries a narrowed range.
        self.builder.build_store(lo_slot, c(0x80)).map_err(err)?;
        self.builder.build_store(hi_slot, c(0xBF)).map_err(err)?;
        self.builder.build_conditional_branch(out_of_range, bad, step).map_err(err)?;

        self.builder.position_at_end(step);
        let i = self.builder.build_load(i64t, i_slot, "i.s").map_err(err)?.into_int_value();
        let next = self.builder.build_int_add(i, c(1), "i.next").map_err(err)?;
        self.builder.build_store(i_slot, next).map_err(err)?;
        self.builder.build_unconditional_branch(head).map_err(err)?;

        // Ran out of bytes mid-sequence. This is the case a per-width implementation forgets.
        self.builder.position_at_end(done);
        let leftover =
            self.builder.build_load(i64t, need_slot, "need.end").map_err(err)?.into_int_value();
        let truncated =
            self.builder.build_int_compare(SGT, leftover, i64t.const_zero(), "truncated").map_err(err)?;
        self.builder.build_conditional_branch(truncated, bad, ok).map_err(err)?;

        self.builder.position_at_end(bad);
        let fmt = self.global_str(
            "burxt runtime error: %s handed back bytes that are not valid UTF-8, at byte %lld — \
             a Burxt String is UTF-8, so this is refused where it enters rather than becoming a \
             wrong answer later. For data that is not text, read it as bytes.\n",
            "utf8_msg",
        );
        let stream = self.load_stderr(stderr_g)?;
        let where_at = self.builder.build_load(i64t, i_slot, "i.bad").map_err(err)?;
        self.builder
            .build_call(
                fprintf,
                &[stream.into(), fmt.into(), source.into(), where_at.into()],
                "fprintf",
            )
            .map_err(err)?;
        self.build_exit70(exit)?;

        self.builder.position_at_end(ok);
        self.builder.build_return(None).map_err(err)?;

        if let Some(b) = saved_block {
            self.builder.position_at_end(b);
        }
        self.utf8_check_fn = Some(f);
        Ok(f)
    }

    /// Check a String that has just come in from outside. B5.
    fn build_require_utf8(&mut self, s: PointerValue<'ctx>, source: &str) -> Result<(), String> {
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let f = self.utf8_fn()?;
        let len = self.build_str_len(s)?;
        let name = self.global_str(source, "utf8_where");
        self.builder
            .build_call(f, &[s.into(), len.into(), name.into()], "require_utf8")
            .map_err(err)?;
        Ok(())
    }

    fn index_fn(&mut self) -> Result<FunctionValue<'ctx>, String> {
        if let Some(f) = self.index_check_fn {
            return Ok(f);
        }
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let saved_block = self.builder.get_insert_block();

        let i64t = self.ctx.i64_type();
        let fprintf = self.fprintf_fn();
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
        let arguments: Vec<BasicMetadataValueEnum> =
            vec![stream.into(), fmt.into(), i.into(), n.into(), n_minus_1.into()];
        self.builder.build_call(fprintf, &arguments, "fprintf").map_err(err)?;
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

    /// Get (or lazily define) the two hash helpers.
    ///
    /// **FNV-1a** for a String: offset basis 0xcbf29ce484222325, prime 0x100000001b3, one xor and
    /// one multiply per byte. Chosen over anything cleverer because it is four lines, has no
    /// tables, and — the reason that matters here — is easy to write a second time and get the
    /// same numbers, which is exactly what the differential test demands of it.
    ///
    /// A multiplicative mix for the scalars, so that small consecutive Ints do not land in
    /// consecutive slots. An identity hash would make `Map<Int, V>` degenerate into the linear
    /// probe chain this milestone exists to remove.
    ///
    /// Wrapping arithmetic throughout, and that is deliberate in a language where `+` panics on
    /// overflow: a hash is not a quantity, it is a bit pattern, and there is nothing to conserve.
    /// The helper is the ONLY place in a Burxt program where that is true, which is a good reason
    /// for it to be a helper rather than something a program could write with `*`.
    fn hash_fn(&mut self, of_string: bool) -> Result<FunctionValue<'ctx>, String> {
        let name = if of_string { "burxt.hash_str" } else { "burxt.hash_int" };
        if let Some(f) = self.module.get_function(name) {
            if f.count_basic_blocks() > 0 {
                return Ok(f);
            }
        }
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let saved_block = self.builder.get_insert_block();
        let i64t = self.ctx.i64_type();
        let i8t = self.ctx.i8_type();
        let ptr = self.ctx.ptr_type(AddressSpace::default());
        let basis = i64t.const_int(0xcbf2_9ce4_8422_2325, false);
        let prime = i64t.const_int(0x0000_0100_0000_01b3, false);
        // The sign bit cleared, so a hash is never negative. Every caller turns a hash into an
        // index with `remainder(h, capacity)`, and in Burxt `remainder` keeps the sign of its left
        // operand — so a negative hash would produce a negative index and a bounds failure at run
        // time. One instruction here removes that from every caller forever, which is the trade
        // this language is supposed to make. Documented as part of what `hash` promises.
        let positive = i64t.const_int(0x7fff_ffff_ffff_ffff, false);

        let fn_ty = if of_string {
            i64t.fn_type(&[ptr.into()], false)
        } else {
            i64t.fn_type(&[i64t.into()], false)
        };
        let f = match self.module.get_function(name) {
            Some(existing) => existing,
            None => self.module.add_function(name, fn_ty, None),
        };
        let entry = self.ctx.append_basic_block(f, "entry");
        self.builder.position_at_end(entry);

        if !of_string {
            // Two rounds of xor-shift and multiply. Enough to spread the low bits of a small
            // integer across the word, which is all a slot index reads.
            let x = f.get_nth_param(0).unwrap().into_int_value();
            let mixed = self.builder.build_int_mul(x, prime, "mix1").map_err(err)?;
            let shifted = self
                .builder
                .build_right_shift(mixed, i64t.const_int(29, false), false, "shift1")
                .map_err(err)?;
            let folded = self.builder.build_xor(mixed, shifted, "fold1").map_err(err)?;
            let again = self.builder.build_int_mul(folded, prime, "mix2").map_err(err)?;
            let shifted2 = self
                .builder
                .build_right_shift(again, i64t.const_int(32, false), false, "shift2")
                .map_err(err)?;
            let folded2 = self.builder.build_xor(again, shifted2, "fold2").map_err(err)?;
            let out = self.builder.build_and(folded2, positive, "positive").map_err(err)?;
            self.builder.build_return(Some(&out)).map_err(err)?;
        } else {
            let sp = f.get_nth_param(0).unwrap().into_pointer_value();
            let acc = self.builder.build_alloca(i64t, "acc").map_err(err)?;
            let idx = self.builder.build_alloca(i64t, "i").map_err(err)?;
            self.builder.build_store(acc, basis).map_err(err)?;
            self.builder.build_store(idx, i64t.const_zero()).map_err(err)?;
            let head = self.ctx.append_basic_block(f, "head");
            let body = self.ctx.append_basic_block(f, "body");
            let done = self.ctx.append_basic_block(f, "done");
            self.builder.build_unconditional_branch(head).map_err(err)?;

            self.builder.position_at_end(head);
            let i = self.builder.build_load(i64t, idx, "i_now").map_err(err)?.into_int_value();
            let at = unsafe { self.builder.build_gep(i8t, sp, &[i], "at") }.map_err(err)?;
            let byte = self.builder.build_load(i8t, at, "byte").map_err(err)?.into_int_value();
            let more = self
                .builder
                .build_int_compare(inkwell::IntPredicate::NE, byte, i8t.const_zero(), "more")
                .map_err(err)?;
            self.builder.build_conditional_branch(more, body, done).map_err(err)?;

            self.builder.position_at_end(body);
            let wide = self.builder.build_int_z_extend(byte, i64t, "wide").map_err(err)?;
            let current = self.builder.build_load(i64t, acc, "h").map_err(err)?.into_int_value();
            let xored = self.builder.build_xor(current, wide, "h_xor").map_err(err)?;
            let scaled = self.builder.build_int_mul(xored, prime, "h_mul").map_err(err)?;
            self.builder.build_store(acc, scaled).map_err(err)?;
            let next = self
                .builder
                .build_int_add(i, i64t.const_int(1, false), "i_next")
                .map_err(err)?;
            self.builder.build_store(idx, next).map_err(err)?;
            self.builder.build_unconditional_branch(head).map_err(err)?;

            self.builder.position_at_end(done);
            let raw = self.builder.build_load(i64t, acc, "h_out").map_err(err)?.into_int_value();
            let out = self.builder.build_and(raw, positive, "positive").map_err(err)?;
            self.builder.build_return(Some(&out)).map_err(err)?;
        }

        if let Some(b) = saved_block {
            self.builder.position_at_end(b);
        }
        Ok(f)
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
    /// The region cursor: ONE logical offset across every chunk.
    ///
    /// Keeping it logical rather than a pointer into a particular chunk is what makes a growable
    /// region cheap — a region MARK is this integer, so `build_region_open` and
    /// `build_region_close` never learn that chunks exist. A chunk index is this value shifted, an
    /// offset inside that chunk is this value masked, and both live in `alloc_fn` alone.
    fn heap_cursor(&mut self) -> inkwell::values::GlobalValue<'ctx> {
        if let Some(g) = self.heap {
            return g;
        }
        let i64t = self.ctx.i64_type();
        let next = self.module.add_global(i64t, None, "burxt.heap.next");
        next.set_initializer(&i64t.const_zero());
        *self.heap.insert(next)
    }

    /// The chunk table: `slots` pointers, all null until the cursor reaches them.
    ///
    /// A table rather than a linked list because the lookup is on the allocation path: an index
    /// derived by shifting has to reach its chunk in one load, and walking a list would make the
    /// cost of an allocation depend on how much the program has already allocated.
    fn heap_table(&mut self, slots: u32) -> inkwell::values::GlobalValue<'ctx> {
        if let Some(g) = self.module.get_global("burxt.heap.table") {
            return g;
        }
        let ptr = self.ctx.ptr_type(AddressSpace::default());
        let arr = ptr.array_type(slots);
        let g = self.module.add_global(arr, None, "burxt.heap.table");
        g.set_initializer(&arr.const_zero());
        g
    }

    /// log2 of the chunk size, decided at run time by the ladder in `alloc_fn`; zero until then.
    ///
    /// A shift rather than the size itself because it is used to divide, and the size is used to
    /// mask — so this is the form that needs no division. Zero is a safe "not yet" because a real
    /// chunk is never one byte.
    fn heap_shift(&mut self) -> inkwell::values::GlobalValue<'ctx> {
        if let Some(g) = self.module.get_global("burxt.heap.shift") {
            return g;
        }
        let i64t = self.ctx.i64_type();
        let g = self.module.add_global(i64t, None, "burxt.heap.shift");
        g.set_initializer(&i64t.const_zero());
        g
    }

    /// Where `main` stashed its arguments, so `argument_count()` and `argument(n)` can read
    /// them from anywhere in the program.
    fn args_globals(
        &mut self,
    ) -> (
        inkwell::values::GlobalValue<'ctx>,
        inkwell::values::GlobalValue<'ctx>,
    ) {
        if let Some(g) = self.arguments {
            return g;
        }
        let i64t = self.ctx.i64_type();
        let ptr = self.ctx.ptr_type(AddressSpace::default());
        let argc = self.module.add_global(i64t, None, "burxt.argc");
        argc.set_initializer(&i64t.const_zero());
        let argv = self.module.add_global(ptr, None, "burxt.argv");
        argv.set_initializer(&ptr.const_null());
        *self.arguments.insert((argc, argv))
    }

    /// `argument(n)` — the n-th command-line argument, bounds-checked.
    ///
    /// No allocation: the C runtime's strings outlive the program, so this hands back
    /// a borrowed pointer that is already NUL-terminated. That is why it needs no
    /// region, unlike everything else that produces a String.
    fn build_arg(&mut self, index: IntValue<'ctx>) -> Result<PointerValue<'ctx>, String> {
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let i64t = self.ctx.i64_type();
        let ptr = self.ctx.ptr_type(AddressSpace::default());
        let (argc_g, argv_g) = self.args_globals();
        let argc = self
            .builder
            .build_load(i64t, argc_g.as_pointer_value(), "argc")
            .map_err(err)?
            .into_int_value();
        let checked = self.build_checked_arg_index(index, argc)?;
        let argv = self
            .builder
            .build_load(ptr, argv_g.as_pointer_value(), "argv")
            .map_err(err)?
            .into_pointer_value();
        let slot = unsafe { self.builder.build_gep(ptr, argv, &[checked], "argslot") }.map_err(err)?;
        let borrowed = self
            .builder
            .build_load(ptr, slot, "argument")
            .map_err(err)?
            .into_pointer_value();
        // COPIED into the region, with a header. `argv` holds C's strings, which have no header, so
        // handing one back directly would make `len` of it read whatever the loader happened to place
        // before it — a silent wrong length, which is worse than a crash.
        //
        // This is the one place a foreign string enters a Burxt program, and it is why `argument`
        // needs a region now when it did not before. The strlen does not disappear: it happens ONCE
        // here, at the boundary, instead of once per byte read afterwards. See
        // spec/1.0/M12-STRINGS.md §3 — the accounting it describes for a future `char*` return is
        // exactly this, arrived at early because `argument` was already that case.
        let strlen = self.libc(
            "strlen",
            self.ctx.i64_type().fn_type(&[ptr.into()], false),
        );
        let measured = self
            .builder
            .build_call(strlen, &[borrowed.into()], "arglen")
            .map_err(err)?;
        let n = match measured.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
            _ => return Err("strlen returned void".to_string()),
        };
        let owned = self.build_alloc_string(n)?;
        self.builder.build_memcpy(owned, 1, borrowed, 1, n).map_err(err)?;
        // B5. A command line is bytes the shell handed over, and nothing checked they were text.
        self.build_require_utf8(owned, "argument")?;
        Ok(owned)
    }

    fn build_arg_count(&mut self) -> Result<IntValue<'ctx>, String> {
        let (argc_g, _) = self.args_globals();
        self.builder
            .build_load(self.ctx.i64_type(), argc_g.as_pointer_value(), "argc")
            .map(|v| v.into_int_value())
            .map_err(|e| e.to_string())
    }

    /// The same shape as every other bounds check, with a message about arguments.
    fn build_checked_arg_index(
        &mut self,
        i: IntValue<'ctx>,
        n: IntValue<'ctx>,
    ) -> Result<IntValue<'ctx>, String> {
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        use inkwell::IntPredicate::*;
        let i64t = self.ctx.i64_type();
        let neg = self.builder.build_int_compare(SLT, i, i64t.const_zero(), "neg").map_err(err)?;
        let big = self.builder.build_int_compare(SGE, i, n, "too_big").map_err(err)?;
        let bad = self.builder.build_or(neg, big, "oob").map_err(err)?;
        let function = self
            .builder
            .get_insert_block()
            .and_then(|b| b.get_parent())
            .ok_or("codegen bug: argument() outside a function")?;
        let broken = self.ctx.append_basic_block(function, "arg_oob");
        let ok = self.ctx.append_basic_block(function, "arg_ok");
        self.builder.build_conditional_branch(bad, broken, ok).map_err(err)?;

        self.builder.position_at_end(broken);
        let fprintf = self.fprintf_fn();
        let (stderr_g, _, exit) = self.panic_deps();
        let fmt = self.global_str(
            "burxt runtime error: argument(%lld) does not exist — this program was given \
             %lld arguments (0 is its own name)\n",
            "fmt_arg_oob",
        );
        let stream = self.load_stderr(stderr_g)?;
        let arguments: Vec<BasicMetadataValueEnum> = vec![stream.into(), fmt.into(), i.into(), n.into()];
        self.builder.build_call(fprintf, &arguments, "fprintf").map_err(err)?;
        self.build_exit70(exit)?;

        self.builder.position_at_end(ok);
        Ok(i)
    }

    /// `write_file(path, contents)` — the whole String, replacing whatever was there.
    /// Returns the number of bytes written, so a caller can check.
    fn build_write_file(
        &mut self,
        path: PointerValue<'ctx>,
        contents: PointerValue<'ctx>,
    ) -> Result<IntValue<'ctx>, String> {
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let i64t = self.ctx.i64_type();
        let i32t = self.ctx.i32_type();
        let ptr = self.ctx.ptr_type(AddressSpace::default());
        let fopen = self.libc("fopen", ptr.fn_type(&[ptr.into(), ptr.into()], false));
        let fwrite = self.libc(
            "fwrite",
            i64t.fn_type(&[ptr.into(), i64t.into(), i64t.into(), ptr.into()], false),
        );
        let fclose = self.libc("fclose", i32t.fn_type(&[ptr.into()], false));

        let mode = self.global_str("wb", "mode_wb");
        let handle = self
            .builder
            .build_call(fopen, &[path.into(), mode.into()], "out")
            .map_err(err)?;
        let file = match handle.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => v.into_pointer_value(),
            _ => return Err("fopen returned void".to_string()),
        };
        let function = self
            .builder
            .get_insert_block()
            .and_then(|b| b.get_parent())
            .ok_or("codegen bug: write_file outside a function")?;
        let missing = self.ctx.append_basic_block(function, "cannot_write");
        let opened = self.ctx.append_basic_block(function, "opened");
        let is_null = self.builder.build_is_null(file, "no_handle").map_err(err)?;
        self.builder.build_conditional_branch(is_null, missing, opened).map_err(err)?;

        self.builder.position_at_end(missing);
        self.build_panic("burxt runtime error: cannot open file for writing\n")?;

        self.builder.position_at_end(opened);
        let count = self.build_str_len(contents)?;
        let written = self
            .builder
            .build_call(
                fwrite,
                &[contents.into(), i64t.const_int(1, false).into(), count.into(), file.into()],
                "written",
            )
            .map_err(err)?;
        self.builder.build_call(fclose, &[file.into()], "close").map_err(err)?;
        match written.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => Ok(v.into_int_value()),
            _ => Err("fwrite returned void".to_string()),
        }
    }

    /// `write_bytes(path, buffer)` — the low byte of every element, written out.
    ///
    /// Elements are i64 because a growable array's element type is one of Burxt's, and
    /// there is no byte type yet. The narrowing is deliberate and documented: an element
    /// outside 0..255 keeps its low eight bits, which is what a byte buffer means. The 8x
    /// memory cost of holding bytes in i64s is the price of not adding a type, and it is
    /// paid in a buffer that lives for one region.
    fn build_write_bytes(
        &mut self,
        path: PointerValue<'ctx>,
        header: inkwell::values::StructValue<'ctx>,
    ) -> Result<IntValue<'ctx>, String> {
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let i64t = self.ctx.i64_type();
        let i32t = self.ctx.i32_type();
        let i8t = self.ctx.i8_type();
        let ptr = self.ctx.ptr_type(AddressSpace::default());

        let data = self
            .builder
            .build_extract_value(header, 0, "bytes_data")
            .map_err(err)?
            .into_pointer_value();
        let count = self
            .builder
            .build_extract_value(header, 1, "bytes_len")
            .map_err(err)?
            .into_int_value();

        // One narrow pass into region memory, then a single fwrite. Writing byte by byte
        // through the C library would be a million calls for a megabyte.
        let flat = self.build_alloc_bytes(count)?;
        let function = self
            .builder
            .get_insert_block()
            .and_then(|b| b.get_parent())
            .ok_or("codegen bug: write_bytes outside a function")?;
        let loop_head = self.ctx.append_basic_block(function, "narrow_head");
        let loop_body = self.ctx.append_basic_block(function, "narrow_body");
        let loop_done = self.ctx.append_basic_block(function, "narrow_done");
        let index = self.builder.build_alloca(i64t, "narrow_i").map_err(err)?;
        self.builder.build_store(index, i64t.const_zero()).map_err(err)?;
        self.builder.build_unconditional_branch(loop_head).map_err(err)?;

        self.builder.position_at_end(loop_head);
        let i = self.builder.build_load(i64t, index, "i").map_err(err)?.into_int_value();
        let more = self
            .builder
            .build_int_compare(inkwell::IntPredicate::SLT, i, count, "more")
            .map_err(err)?;
        self.builder.build_conditional_branch(more, loop_body, loop_done).map_err(err)?;

        self.builder.position_at_end(loop_body);
        let source = unsafe { self.builder.build_gep(i64t, data, &[i], "elem_p") }.map_err(err)?;
        let wide = self.builder.build_load(i64t, source, "elem").map_err(err)?.into_int_value();
        let narrow = self.builder.build_int_truncate(wide, i8t, "byte").map_err(err)?;
        let target = unsafe { self.builder.build_gep(i8t, flat, &[i], "out_p") }.map_err(err)?;
        self.builder.build_store(target, narrow).map_err(err)?;
        let next = self
            .builder
            .build_int_add(i, i64t.const_int(1, false), "i_next")
            .map_err(err)?;
        self.builder.build_store(index, next).map_err(err)?;
        self.builder.build_unconditional_branch(loop_head).map_err(err)?;

        self.builder.position_at_end(loop_done);
        let fopen = self.libc("fopen", ptr.fn_type(&[ptr.into(), ptr.into()], false));
        let fwrite = self.libc(
            "fwrite",
            i64t.fn_type(&[ptr.into(), i64t.into(), i64t.into(), ptr.into()], false),
        );
        let fclose = self.libc("fclose", i32t.fn_type(&[ptr.into()], false));
        let mode = self.global_str("wb", "mode_wb_bytes");
        let handle = self
            .builder
            .build_call(fopen, &[path.into(), mode.into()], "out")
            .map_err(err)?;
        let file = match handle.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => v.into_pointer_value(),
            _ => return Err("fopen returned void".to_string()),
        };
        let missing = self.ctx.append_basic_block(function, "cannot_write_bytes");
        let opened = self.ctx.append_basic_block(function, "opened_bytes");
        let is_null = self.builder.build_is_null(file, "no_handle").map_err(err)?;
        self.builder.build_conditional_branch(is_null, missing, opened).map_err(err)?;

        self.builder.position_at_end(missing);
        self.build_panic("burxt runtime error: cannot open file for writing\n")?;

        self.builder.position_at_end(opened);
        let written = self
            .builder
            .build_call(
                fwrite,
                &[flat.into(), i64t.const_int(1, false).into(), count.into(), file.into()],
                "written",
            )
            .map_err(err)?;
        self.builder.build_call(fclose, &[file.into()], "close").map_err(err)?;
        match written.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => Ok(v.into_int_value()),
            _ => Err("fwrite returned void".to_string()),
        }
    }

    /// Concatenate two strings into the current region. The result is
    /// NUL-terminated so it remains a plain `const char*` at the FFI boundary,
    /// exactly like a literal.
    fn build_str_concat(
        &mut self,
        a: PointerValue<'ctx>,
        b: PointerValue<'ctx>,
    ) -> Result<PointerValue<'ctx>, String> {
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let i64t = self.ctx.i64_type();
        let i8t = self.ctx.i8_type();
        let _ = i64t;
        let la = self.build_str_len(a)?;
        let lb = self.build_str_len(b)?;
        let total = self.builder.build_int_add(la, lb, "join_len").map_err(err)?;
        let dest = self.build_alloc_string(total)?;
        self.builder.build_memcpy(dest, 1, a, 1, la).map_err(|e| e.to_string())?;
        let second = unsafe { self.builder.build_gep(i8t, dest, &[la], "second") }.map_err(err)?;
        self.builder.build_memcpy(second, 1, b, 1, lb).map_err(|e| e.to_string())?;
        let end = unsafe { self.builder.build_gep(i8t, dest, &[total], "end") }.map_err(err)?;
        self.builder.build_store(end, i8t.const_zero()).map_err(err)?;
        Ok(dest)
    }

    /// Allocate raw bytes in the current region.
    /// Allocate a String of `len` bytes in the current region and write its LENGTH HEADER.
    ///
    /// The layout is `[ i64 length ][ len bytes ][ NUL ]`, and the pointer answered points at the
    /// first BYTE, not at the header. So a Burxt String is still one pointer and still a valid
    /// `char*` for C — the header is additional information sitting behind it, which C never looks
    /// at. See spec/1.0/M12-STRINGS.md §1.
    ///
    /// Every place that makes a String goes through here, which is the point: a length written in
    /// one place and read in another is exactly the kind of thing that works for the case you
    /// tested.
    /// A `Decimal` to text, in Burxt, with no libc in the path.
    ///
    /// **This is the last inch of the money thesis.** The arithmetic is exact — scaled integers,
    /// no float, from literal through every operation — and until this existed the final step, the
    /// one that produces the characters a human reads, was `snprintf("%s%llu.%0<scale>llu")` and
    /// therefore whatever libc the target happened to have. `N1-BOUNDARY-EXACTNESS.md` §7 has the
    /// argument; the short form is that no conforming libc renders it differently, and a host that
    /// supplies its own `printf` is a surface that did not exist before wasm. One did, and it
    /// rendered `$1299.05` as `1299.5` by discarding the width in `%02llu`.
    ///
    /// **Digits are written backwards into a stack buffer and copied forward once.** Writing
    /// backwards is what makes the length fall out rather than needing to be computed: the number
    /// of digits in the integer part is not known until the division stops, and computing it up
    /// front would mean a second loop that has to agree with the first.
    ///
    /// **u64, not i128 — and that is a portability fix as much as a simplification.** The
    /// magnitude of any `i64` fits a `u64` exactly: `abs(i64::MIN)` is 2^63, and two's-complement
    /// negation already produces those bits, so `udiv`/`urem` reading them unsigned give the right
    /// digits with no widening at all.
    ///
    /// The first version used i128, inherited from the `snprintf` path it replaces — which widened
    /// for the same overflow reason and had no better option because it was handing the parts to a
    /// varargs call. **i128 arithmetic makes LLVM emit `__multi3` and `__udivti3`**, compiler-rt
    /// builtins that x86-64 and aarch64 supply invisibly and `wasm32-unknown-unknown` does not.
    /// The fixture, the fixpoint and the two-backend agreement were all green; a wasm host refused
    /// to instantiate with `function import requires a callable`. Found by a consumer on a target
    /// this machine cannot run — which is the only kind of check that could have found it.
    ///
    /// It is also the better trade on its own merits: a host author who has to supply a correct
    /// 128-bit long division is worse off than one who had to supply a correct width specifier.
    ///
    /// 48 bytes is more than twice what is reachable: 20 digits for the magnitude, one point, one
    /// sign, and a scale capped at 18.
    fn build_decimal_text(
        &mut self,
        unscaled: IntValue<'ctx>,
        scale: u32,
    ) -> Result<PointerValue<'ctx>, String> {
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let i64t = self.ctx.i64_type();
        let i8t = self.ctx.i8_type();
        let function = self
            .builder
            .get_insert_block()
            .and_then(|b| b.get_parent())
            .ok_or("codegen bug: decimal text outside a function")?;

        let is_neg = self
            .builder
            .build_int_compare(inkwell::IntPredicate::SLT, unscaled, i64t.const_zero(), "dt_neg")
            .map_err(err)?;
        // Two's-complement negation, read unsigned. For `i64::MIN` this yields 2^63, which is
        // exactly its magnitude — no widening, and therefore no compiler-rt.
        let flipped = self
            .builder
            .build_int_sub(i64t.const_zero(), unscaled, "dt_flip")
            .map_err(err)?;
        let magnitude = self
            .builder
            .build_select(is_neg, flipped, unscaled, "dt_mag")
            .map_err(err)?
            .into_int_value();

        let scratch = self
            .builder
            .build_array_alloca(i8t, i64t.const_int(48, false), "dt_scratch")
            .map_err(err)?;
        let end = unsafe {
            self.builder.build_gep(i8t, scratch, &[i64t.const_int(48, false)], "dt_end")
        }
        .map_err(err)?;

        let cursor = self.builder.build_alloca(self.ctx.ptr_type(AddressSpace::default()), "dt_p")
            .map_err(err)?;
        self.builder.build_store(cursor, end).map_err(err)?;
        let value = self.builder.build_alloca(i64t, "dt_v").map_err(err)?;
        self.builder.build_store(value, magnitude).map_err(err)?;

        let ten = i64t.const_int(10, false);
        let zero_ch = i8t.const_int(48, false); // '0'

        // One digit off the end of `value`, written one byte back from `cursor`.
        let emit_digit = |me: &mut Self| -> Result<(), String> {
            let v = me.builder.build_load(i64t, value, "dt_val").map_err(err)?.into_int_value();
            let rem = me.builder.build_int_unsigned_rem(v, ten, "dt_rem").map_err(err)?;
            let next = me.builder.build_int_unsigned_div(v, ten, "dt_div").map_err(err)?;
            me.builder.build_store(value, next).map_err(err)?;
            let small = me.builder.build_int_truncate(rem, i8t, "dt_small").map_err(err)?;
            let ch = me.builder.build_int_add(small, zero_ch, "dt_ch").map_err(err)?;
            let p = me
                .builder
                .build_load(me.ctx.ptr_type(AddressSpace::default()), cursor, "dt_cur")
                .map_err(err)?
                .into_pointer_value();
            let back = unsafe {
                me.builder.build_gep(i8t, p, &[i64t.const_int(u64::MAX, true)], "dt_back")
            }
            .map_err(err)?;
            me.builder.build_store(back, ch).map_err(err)?;
            me.builder.build_store(cursor, back).map_err(err)?;
            Ok(())
        };

        // The fractional digits: exactly `scale` of them, zero-padded by construction rather than
        // by a width the host has to honour. This is the half a non-conforming `printf` got wrong.
        for _ in 0..scale {
            emit_digit(self)?;
        }
        if scale > 0 {
            let p = self
                .builder
                .build_load(self.ctx.ptr_type(AddressSpace::default()), cursor, "dt_cur_pt")
                .map_err(err)?
                .into_pointer_value();
            let back = unsafe {
                self.builder.build_gep(i8t, p, &[i64t.const_int(u64::MAX, true)], "dt_pt")
            }
            .map_err(err)?;
            self.builder.build_store(back, i8t.const_int(46, false)).map_err(err)?; // '.'
            self.builder.build_store(cursor, back).map_err(err)?;
        }

        // The integer digits: at least one, so zero renders as "0" rather than as nothing.
        let body = self.ctx.append_basic_block(function, "dt_int_body");
        let test = self.ctx.append_basic_block(function, "dt_int_test");
        let done = self.ctx.append_basic_block(function, "dt_int_done");
        self.builder.build_unconditional_branch(body).map_err(err)?;
        self.builder.position_at_end(body);
        emit_digit(self)?;
        self.builder.build_unconditional_branch(test).map_err(err)?;
        self.builder.position_at_end(test);
        let left = self.builder.build_load(i64t, value, "dt_left").map_err(err)?.into_int_value();
        let more = self
            .builder
            .build_int_compare(inkwell::IntPredicate::NE, left, i64t.const_zero(), "dt_more")
            .map_err(err)?;
        self.builder.build_conditional_branch(more, body, done).map_err(err)?;
        self.builder.position_at_end(done);

        // The sign. `is_neg` is false for a zero magnitude because the sign lives in the scaled
        // integer and zero has no sign — `-$0.00` is `0.00`, which is Andre's ruling and what the
        // `snprintf` path already did. No special case is needed to preserve it.
        let signed = self.ctx.append_basic_block(function, "dt_sign");
        let after = self.ctx.append_basic_block(function, "dt_after");
        self.builder.build_conditional_branch(is_neg, signed, after).map_err(err)?;
        self.builder.position_at_end(signed);
        let p = self
            .builder
            .build_load(self.ctx.ptr_type(AddressSpace::default()), cursor, "dt_cur_sg")
            .map_err(err)?
            .into_pointer_value();
        let back = unsafe {
            self.builder.build_gep(i8t, p, &[i64t.const_int(u64::MAX, true)], "dt_sg")
        }
        .map_err(err)?;
        self.builder.build_store(back, i8t.const_int(45, false)).map_err(err)?; // '-'
        self.builder.build_store(cursor, back).map_err(err)?;
        self.builder.build_unconditional_branch(after).map_err(err)?;
        self.builder.position_at_end(after);

        let start = self
            .builder
            .build_load(self.ctx.ptr_type(AddressSpace::default()), cursor, "dt_start")
            .map_err(err)?
            .into_pointer_value();
        let start_i = self.builder.build_ptr_to_int(start, i64t, "dt_si").map_err(err)?;
        let end_i = self.builder.build_ptr_to_int(end, i64t, "dt_ei").map_err(err)?;
        let len = self.builder.build_int_sub(end_i, start_i, "dt_len").map_err(err)?;

        let buf = self.build_alloc_string(len)?;
        self.builder
            .build_memcpy(buf, 1, start, 1, len)
            .map_err(|e| e.to_string())?;
        Ok(buf)
    }

    fn build_alloc_string(&mut self, len: IntValue<'ctx>) -> Result<PointerValue<'ctx>, String> {
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let i64t = self.ctx.i64_type();
        let i8t = self.ctx.i8_type();
        let total = self
            .builder
            .build_int_add(len, i64t.const_int(9, false), "with_header_and_nul")
            .map_err(err)?;
        let base = self.build_alloc_bytes(total)?;
        self.builder.build_store(base, len).map_err(err)?;
        let bytes = unsafe {
            self.builder.build_gep(i8t, base, &[i64t.const_int(8, false)], "bytes")
        }
        .map_err(err)?;
        // NUL written here rather than left to each caller, so no maker can forget it.
        let end = unsafe { self.builder.build_gep(i8t, bytes, &[len], "end") }.map_err(err)?;
        self.builder.build_store(end, i8t.const_zero()).map_err(err)?;
        Ok(bytes)
    }

    fn build_alloc_bytes(&mut self, bytes: IntValue<'ctx>) -> Result<PointerValue<'ctx>, String> {
        let f = self.alloc_fn()?;
        let call = self
            .builder
            .build_call(f, &[bytes.into()], "region_bytes")
            .map_err(|e| e.to_string())?;
        match call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => Ok(v.into_pointer_value()),
            _ => Err("allocator returned void".to_string()),
        }
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
    /// Shorten a growable array to `n`. The capacity and the buffer are untouched:
    /// the elements past `n` are simply no longer part of it, so a scope that pushes
    /// and truncates repeatedly reuses the same memory rather than growing forever.
    ///
    /// Bounds-checked in both directions — a length above the current one would
    /// expose elements that were never written, which is the kind of "silently wrong"
    /// this language exists to refuse.
    fn build_truncate(
        &mut self,
        slice_ty: &Type,
        slot: PointerValue<'ctx>,
        new_len: IntValue<'ctx>,
    ) -> Result<IntValue<'ctx>, String> {
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let i64t = self.ctx.i64_type();
        let st = self.llvm_type(slice_ty).into_struct_type();
        let len_p = self.builder.build_struct_gep(st, slot, 1, "len_p").map_err(err)?;
        let len = self.builder.build_load(i64t, len_p, "len").map_err(err)?.into_int_value();

        use inkwell::IntPredicate::*;
        let negative = self.builder.build_int_compare(SLT, new_len, i64t.const_zero(), "neg").map_err(err)?;
        let longer = self.builder.build_int_compare(SGT, new_len, len, "longer").map_err(err)?;
        let bad = self.builder.build_or(negative, longer, "bad_length").map_err(err)?;
        let function = self
            .builder
            .get_insert_block()
            .and_then(|b| b.get_parent())
            .ok_or("codegen bug: truncate outside a function")?;
        let broken = self.ctx.append_basic_block(function, "truncate_bad");
        let ok = self.ctx.append_basic_block(function, "truncate_ok");
        self.builder.build_conditional_branch(bad, broken, ok).map_err(err)?;

        self.builder.position_at_end(broken);
        let fprintf = self.fprintf_fn();
        let (stderr_g, _, exit) = self.panic_deps();
        let fmt = self.global_str(
            "burxt runtime error: truncate(xs, %lld) — this array has %lld elements, \
             and truncate only ever makes one shorter\n",
            "fmt_truncate",
        );
        let stream = self.load_stderr(stderr_g)?;
        let arguments: Vec<BasicMetadataValueEnum> =
            vec![stream.into(), fmt.into(), new_len.into(), len.into()];
        self.builder.build_call(fprintf, &arguments, "fprintf").map_err(err)?;
        self.build_exit70(exit)?;

        self.builder.position_at_end(ok);
        self.builder.build_store(len_p, new_len).map_err(err)?;
        Ok(new_len)
    }

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

    /// A block that releases what it built: take the mark, run the body, put the
    /// cursor back if control reaches the closing brace.
    ///
    /// It may not — a `return`, a `break`, a `continue` or a `?` leaves early, and each
    /// of those puts the cursor back itself, reading this mark off `region_marks`. That
    /// is the only reason the stack exists: an early exit has to know what to undo.
    fn gen_released_block(&mut self, body: &[TypedStmt]) -> Result<(), String> {
        let mark = self.build_region_open()?;
        let saved = self.vars.clone();
        self.region_marks.push(mark);
        let r = body.iter().try_for_each(|s| self.gen_stmt(s));
        self.vars = saved;
        self.region_marks.pop();
        r?;
        if self.current_block_open() {
            self.build_region_close(mark)?;
        }
        Ok(())
    }

    /// `open` is just "remember where the cursor is".
    fn build_region_open(&mut self) -> Result<IntValue<'ctx>, String> {
        // Opening a region brings its allocator into the module: a region and
        // the bump allocator are one mechanism, not two.
        self.alloc_fn()?;
        let next = self.heap_cursor();
        self.builder
            .build_load(self.ctx.i64_type(), next.as_pointer_value(), "region_mark")
            .map(|v| v.into_int_value())
            .map_err(|e| e.to_string())
    }

    /// Set up a function's contracts: capture every `old(...)` value, check the
    /// preconditions, and hand the postconditions to `return`.
    ///
    /// Order matters. `old` is captured FIRST, because a precondition that fails
    /// should fail on the state as it arrived; and the captures must happen before
    /// any of the body runs, or they would not be "old" at all.
    fn gen_contract_prologue(
        &mut self,
        requires: &[crate::typeck::TypedContract],
        ensures: &[crate::typeck::TypedContract],
        olds: &[crate::typeck::TypedExpr],
        name: &str,
        at: crate::diag::Span,
    ) -> Result<(), String> {
        self.old_slots.clear();
        // Every `old(...)` is hoisted out of an `ensures`, so the clause it came from is
        // the honest position for the work of capturing it. The first `ensures` is close
        // enough and is a real line; `at` covers a body with none.
        let olds_at = ensures.first().map(|c| c.span).unwrap_or(at);
        self.set_debug_location(olds_at);
        for (i, expr) in olds.iter().enumerate() {
            let value = self.gen_expr(expr)?;
            let slot = self.create_entry_alloca(&format!("old{}", i), &expr.ty)?;
            self.builder.build_store(slot, value).map_err(|e| e.to_string())?;
            self.old_slots.push((slot, expr.ty.clone()));
        }
        for clause in requires {
            // The CLAUSE's own line, not the function's. A `requires` that fails should
            // report the sentence the reader has to satisfy, and a debugger stopped in
            // the check should show it — see the `pure`-function probe that made this a
            // hard verifier failure rather than a cosmetic one.
            self.set_debug_location(clause.span);
            self.gen_contract_check(clause, name, "requires")?;
        }

        self.current_ensures =
            ensures.iter().map(|c| (c.clone(), name.to_string())).collect();
        Ok(())
    }

    /// Capture this invocation's termination measure, and refuse a negative one.
    ///
    /// A measure that can fall below zero is not a ladder to the floor, it is a
    /// hole: "strictly smaller" alone would let it descend forever.
    fn gen_measure_prologue(&mut self, f: &crate::typeck::TypedFn) -> Result<(), String> {
        self.current_measure = None;
        let Some(clause) = &f.decreases else { return Ok(()) };
        self.set_debug_location(clause.span);
        let value = self.gen_expr(&clause.cond)?.into_int_value();
        let slot = self.create_entry_alloca("measure", &Type::Int)?;
        self.builder.build_store(slot, value).map_err(|e| e.to_string())?;
        self.check_or_die(
            inkwell::IntPredicate::SGE,
            value,
            self.ctx.i64_type().const_zero(),
            &format!(
                "burxt runtime error: `decreases {}` is negative in `{}`\n",
                clause.text, f.name
            ),
        )?;
        self.current_measure = Some(MeasureState {
            slot,
            measure: clause.cond.clone(),
            parameters: f.parameters.clone(),
            text: clause.text.clone(),
            function: f.name.clone(),
        });
        Ok(())
    }

    /// At a recursive call: does the measure actually get smaller?
    ///
    /// The callee's measure is this function's measure evaluated with the arguments,
    /// which is obtained by binding the parameter names to the argument values and
    /// generating the same expression again. No substitution pass, no rewritten AST.
    fn gen_measure_check(
        &mut self,
        callee: &str,
        arguments: &[BasicValueEnum<'ctx>],
    ) -> Result<(), String> {
        let Some(state) = self.current_measure.clone() else { return Ok(()) };
        if state.function != callee || arguments.len() != state.parameters.len() {
            return Ok(());
        }
        let saved = self.vars.clone();
        for ((name, ty), value) in state.parameters.iter().zip(arguments) {
            if is_aggregate(ty) {
                // An aggregate argument is already a pointer to our own copy.
                self.vars.insert(name.clone(), (value.into_pointer_value(), ty.clone()));
            } else {
                let slot = self.create_entry_alloca(&format!("arg_{}", name), ty)?;
                self.builder.build_store(slot, *value).map_err(|e| e.to_string())?;
                self.vars.insert(name.clone(), (slot, ty.clone()));
            }
        }
        let next = self.gen_expr(&state.measure);
        self.vars = saved;
        let next = next?.into_int_value();

        let mine = self
            .builder
            .build_load(self.ctx.i64_type(), state.slot, "measure_now")
            .map_err(|e| e.to_string())?
            .into_int_value();
        self.check_or_die(
            inkwell::IntPredicate::SLT,
            next,
            mine,
            &format!(
                "burxt runtime error: `decreases {}` did not decrease on a recursive \
                 call to `{}`\n",
                state.text, state.function
            ),
        )?;
        Ok(())
    }

    /// `left <pred> right`, or die with `message`. The shape every runtime check in
    /// the compiler has: a comparison, a branch, and a named failure.
    fn check_or_die(
        &mut self,
        pred: inkwell::IntPredicate,
        left: IntValue<'ctx>,
        right: IntValue<'ctx>,
        message: &str,
    ) -> Result<(), String> {
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let holds = self.builder.build_int_compare(pred, left, right, "holds").map_err(err)?;
        let function = self
            .builder
            .get_insert_block()
            .and_then(|b| b.get_parent())
            .ok_or("codegen bug: check outside a function")?;
        let broken = self.ctx.append_basic_block(function, "check_broken");
        let ok = self.ctx.append_basic_block(function, "check_ok");
        self.builder.build_conditional_branch(holds, ok, broken).map_err(err)?;
        self.builder.position_at_end(broken);
        self.build_panic(message)?;
        self.builder.position_at_end(ok);
        Ok(())
    }

    /// Emit one contract check: evaluate the condition, and if it is false, die
    /// with the clause quoted exactly as it was written.
    ///
    /// Always emitted — there is no build mode that removes contracts. A flag that
    /// decided whether a program enforces its own stated invariants would make its
    /// behaviour depend on how it was built.
    fn gen_contract_check(
        &mut self,
        clause: &crate::typeck::TypedContract,
        function: &str,
        kind: &str,
    ) -> Result<(), String> {
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let value = self.gen_expr(&clause.cond)?.into_int_value();
        let holds = self
            .builder
            .build_int_compare(
                inkwell::IntPredicate::NE,
                value,
                self.ctx.i64_type().const_zero(),
                "contract_holds",
            )
            .map_err(err)?;
        let function_value = self
            .builder
            .get_insert_block()
            .and_then(|b| b.get_parent())
            .ok_or("codegen bug: contract check outside a function")?;
        let broken = self.ctx.append_basic_block(function_value, "contract_broken");
        let ok = self.ctx.append_basic_block(function_value, "contract_ok");
        self.builder.build_conditional_branch(holds, ok, broken).map_err(err)?;

        self.builder.position_at_end(broken);
        self.build_panic(&format!(
            "burxt runtime error: `{} {}` failed in `{}`\n",
            kind, clause.text, function
        ))?;

        self.builder.position_at_end(ok);
        Ok(())
    }

    /// Leaving by `return` releases every open region exactly as reaching their
    /// closing braces would. Without this the bump cursor kept climbing for the life
    /// of the process, so a function that returned from inside a region leaked it on
    /// every call.
    ///
    /// The cursor goes back to the OUTERMOST mark this body took — one store, whatever
    /// the nesting depth, because the allocator is LIFO and that mark is below all the
    /// others.
    fn close_open_region(&mut self) -> Result<(), String> {
        self.close_regions_below(0)
    }

    /// Put the cursor back to where it stood when the region at `depth` opened, if any
    /// region at or beyond that depth is open. What `break` and `continue` need: they
    /// leave the blocks opened INSIDE the loop and stay inside the ones that enclose it.
    fn close_regions_below(&mut self, depth: usize) -> Result<(), String> {
        if let Some(mark) = self.region_marks.get(depth).copied() {
            self.build_region_close(mark)?;
        }
        Ok(())
    }

    /// `close` is "put the cursor back" — that is the entire deallocation.
    fn build_region_close(&mut self, mark: IntValue<'ctx>) -> Result<(), String> {
        let next = self.heap_cursor();
        self.builder
            .build_store(next.as_pointer_value(), mark)
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Get (or lazily define) `ptr @burxt.alloc(i64 bytes)`: bump the cursor, 8-byte aligned.
    /// Exhaustion is a named runtime error, never a silent overrun — the same standard every
    /// other check in Burxt meets.
    ///
    /// # The region GROWS, and that is why no number is chosen here
    ///
    /// **History, kept because each step was a wall that turned out to be a number.** This
    /// reserved 512 MB, then 1 GB, then 4 GB, and each raise was reported as a design constraint
    /// until somebody measured it. On a 64-bit OS with overcommit the reservation is *virtual*, so
    /// a program that touches a kilobyte pays for a kilobyte and raising the figure costs nothing
    /// resident. Then v0.0.261: **that paragraph is true of a 64-bit OS and FALSE of wasm32**,
    /// which has a 4 GiB address space in total and whose `memory.grow` COMMITS — lazy page
    /// mapping is what the argument rested on, and wasm has none. So it became a run-time ladder
    /// rather than a constant, which also keeps `the_ir_is_the_same_for_every_target` passing:
    /// pointer width never reaches the IR, and a `CHUNK` that varied by triple would.
    ///
    /// **What was still wrong, and it is the thing a bigger number could never fix: it could only
    /// ask ONCE.** One chunk was taken on first use and running out was fatal. That made whatever
    /// rung the machine granted a hard ceiling — on wasm with `memory.grow` sitting unused, and on
    /// a 64-bit OS with nothing bounding what became resident. It was not hypothetical:
    /// `emit.bx`'s own comment records **stage-1 built by itself dying with `region memory
    /// exhausted` while compiling `main.bx`**, at a margin of 0.53%, breaking the fixpoint. Its
    /// conclusion is the one that matters — *what is RESIDENT is the real limit and no constant
    /// moves it.*
    ///
    /// So there is no total to choose. The arena is a **table of equal, power-of-two chunks, added
    /// on demand**, and the ladder now picks the CHUNK size rather than the whole reservation —
    /// small enough that a memory-capped container or a small device gets one, with the count
    /// growing to whatever the program actually touches. A program that needs a megabyte holds a
    /// megabyte on every target; a program that needs a gigabyte asks sixty-four times.
    ///
    /// **Why this is contained rather than a rewrite of the memory model:** the cursor stays a
    /// single logical offset across all chunks, so a region MARK stays a single integer and
    /// `build_region_open`, `build_region_close` and every `region_marks` site are untouched. A
    /// chunk index is a shift of the cursor and an offset within it is a mask.
    ///
    /// **Verified rather than assumed: nothing depends on the arena being contiguous.** Slice
    /// growth allocates a fresh buffer and copies the live elements (`build_slice_push`), and a
    /// String is allocated whole. Chunks are never moved or reallocated, so every pointer handed
    /// out stays valid for the life of the region.
    ///
    /// Two refusals remain, both named, because the alternative to a named refusal here is a
    /// silent overrun: a single value larger than one chunk, and running out of table slots.
    fn alloc_fn(&mut self) -> Result<FunctionValue<'ctx>, String> {
        if let Some(f) = self.alloc_fn {
            return Ok(f);
        }
        // The chunk-size ladder. Descending by a large factor on purpose — a ladder with close
        // rungs spends syscalls to discover a number nobody will notice. 16 MiB suits a hosted
        // program, 1 MiB a memory-capped container or a browser tab, 64 KiB a small device (and
        // it is one wasm page times sixteen). Each is a power of two so the index is a shift.
        const SHIFTS: [u32; 3] = [24, 20, 16];
        // 4096 slots. With the first rung that is 64 GiB of reachable region, and the table costs
        // 32 KB of zeroed .bss. It is a bound rather than a target: crossing it is a named error,
        // and a freestanding build will want its own much smaller table with no `malloc` behind it.
        const SLOTS: u32 = 4096;
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let saved = self.builder.get_insert_block();
        let next = self.heap_cursor();
        let table = self.heap_table(SLOTS);
        let shift_g = self.heap_shift();

        let i64t = self.ctx.i64_type();
        let i32t = self.ctx.i32_type();
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
        let init_bb = self.ctx.append_basic_block(f, "pick_chunk_size");
        let rung_bb: Vec<_> = (1..SHIFTS.len())
            .map(|i| self.ctx.append_basic_block(f, &format!("smaller_chunk{}", i)))
            .collect();
        let no_chunk_bb = self.ctx.append_basic_block(f, "no_chunk");
        let have_bb = self.ctx.append_basic_block(f, "have_size");
        let too_big_bb = self.ctx.append_basic_block(f, "value_over_chunk");
        let big_slots_bb = self.ctx.append_basic_block(f, "big_slots_ok");
        let big_got_bb = self.ctx.append_basic_block(f, "big_chunk");
        let fits_bb = self.ctx.append_basic_block(f, "fits_a_chunk");
        let no_slots_bb = self.ctx.append_basic_block(f, "no_slots");
        let in_range_bb = self.ctx.append_basic_block(f, "slot_in_range");
        let grow_bb = self.ctx.append_basic_block(f, "add_chunk");
        let store_bb = self.ctx.append_basic_block(f, "keep_chunk");
        let no_room_bb = self.ctx.append_basic_block(f, "exhausted");
        let ok_bb = self.ctx.append_basic_block(f, "ok");

        self.builder.position_at_end(entry);
        let want = f.get_nth_param(0).unwrap().into_int_value();
        // round the request up to 8 bytes so every value stays aligned
        let seven = i64t.const_int(7, false);
        let bumped = self.builder.build_int_add(want, seven, "bumped").map_err(err)?;
        let mask8 = i64t.const_int(!7u64, false);
        let size = self.builder.build_and(bumped, mask8, "aligned").map_err(err)?;
        let cur_shift = self
            .builder
            .build_load(i64t, shift_g.as_pointer_value(), "shift")
            .map_err(err)?
            .into_int_value();
        // Zero means "no chunk size decided yet". A real shift is never zero.
        let undecided = self
            .builder
            .build_int_compare(inkwell::IntPredicate::EQ, cur_shift, i64t.const_zero(), "undecided")
            .map_err(err)?;
        self.builder.build_conditional_branch(undecided, init_bb, have_bb).map_err(err)?;

        // Pick the chunk size ONCE: ask for one chunk at each rung and keep the first that is
        // granted. The chunk itself is kept — it becomes slot 0 — so the probe is not wasted.
        for (i, sh) in SHIFTS.iter().enumerate() {
            let this_bb = if i == 0 { init_bb } else { rung_bb[i - 1] };
            let next_bb = rung_bb.get(i).copied().unwrap_or(no_chunk_bb);
            self.builder.position_at_end(this_bb);
            let bytes = i64t.const_int(1u64 << sh, false);
            let chunk = self
                .builder
                .build_call(malloc, &[bytes.into()], "chunk")
                .map_err(err)?;
            let chunk_ptr = match chunk.try_as_basic_value() {
                inkwell::values::ValueKind::Basic(v) => v.into_pointer_value(),
                _ => return Err("malloc returned void".to_string()),
            };
            let failed = self.builder.build_is_null(chunk_ptr, "no_room").map_err(err)?;
            let got_bb = self.ctx.append_basic_block(f, &format!("got_chunk{}", i));
            self.builder.build_conditional_branch(failed, next_bb, got_bb).map_err(err)?;

            self.builder.position_at_end(got_bb);
            let slot0 = unsafe {
                self.builder.build_gep(
                    ptr,
                    table.as_pointer_value(),
                    &[i32t.const_zero()],
                    "slot0",
                )
            }
            .map_err(err)?;
            self.builder.build_store(slot0, chunk_ptr).map_err(err)?;
            self.builder
                .build_store(shift_g.as_pointer_value(), i64t.const_int(*sh as u64, false))
                .map_err(err)?;
            self.builder.build_unconditional_branch(have_bb).map_err(err)?;
        }

        // Every rung refused. Naming it is the whole point: storing a null and letting the next
        // write find out is the silent overrun this function promises not to be.
        self.builder.position_at_end(no_chunk_bb);
        self.build_panic(
            "burxt runtime error: could not reserve any region memory — the machine refused \
             16 MB, 1 MB and 64 KB\n",
        )?;

        self.builder.position_at_end(have_bb);
        let sh = self
            .builder
            .build_load(i64t, shift_g.as_pointer_value(), "shift2")
            .map_err(err)?
            .into_int_value();
        let chunk_size = self
            .builder
            .build_left_shift(i64t.const_int(1, false), sh, "chunk_size")
            .map_err(err)?;
        // A value wider than one chunk can never be placed, whatever the cursor is. Checked
        // BEFORE the straddle skip below, which would otherwise advance for ever looking for
        // room that does not exist at any offset.
        let cursor = self
            .builder
            .build_load(i64t, next.as_pointer_value(), "cursor")
            .map_err(err)?
            .into_int_value();
        let within = self
            .builder
            .build_int_sub(chunk_size, i64t.const_int(1, false), "chunk_mask")
            .map_err(err)?;
        let over_chunk = self
            .builder
            .build_int_compare(inkwell::IntPredicate::UGT, size, chunk_size, "over_chunk")
            .map_err(err)?;
        self.builder.build_conditional_branch(over_chunk, too_big_bb, fits_bb).map_err(err)?;

        // A value WIDER THAN A CHUNK gets a chunk of its own, sized to it.
        //
        // Without this the growable region would be a regression rather than an improvement:
        // `read_file` of a 20 MB file fits a 4 GiB reservation and does not fit a 16 MiB chunk, so
        // refusing here would take away something that worked. Measured before it was written —
        // the refusal this replaces really did stop a 20 MB read.
        //
        // It begins at a chunk BOUNDARY, which is what keeps the index arithmetic honest: the slot
        // it lands in is one nothing has allocated, and the cursor then jumps past however many
        // whole chunks the value spans, so the slots it covers are never handed to anything else.
        // Those slots stay null and cost nothing — a slot number is not memory.
        self.builder.position_at_end(too_big_bb);
        let round = self.builder.build_int_add(cursor, within, "round_up").map_err(err)?;
        let big_idx = self
            .builder
            .build_right_shift(round, sh, false, "big_index")
            .map_err(err)?;
        let big_span_bytes = self.builder.build_int_add(size, within, "span_bytes").map_err(err)?;
        let big_span = self
            .builder
            .build_right_shift(big_span_bytes, sh, false, "span_chunks")
            .map_err(err)?;
        let big_end = self.builder.build_int_add(big_idx, big_span, "big_end").map_err(err)?;
        let big_past = self
            .builder
            .build_int_compare(
                inkwell::IntPredicate::UGT,
                big_end,
                i64t.const_int(SLOTS as u64, false),
                "big_past_table",
            )
            .map_err(err)?;
        self.builder.build_conditional_branch(big_past, no_slots_bb, big_slots_bb).map_err(err)?;

        self.builder.position_at_end(big_slots_bb);
        let big = self
            .builder
            .build_call(malloc, &[size.into()], "big_chunk")
            .map_err(err)?;
        let big_ptr = match big.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => v.into_pointer_value(),
            _ => return Err("malloc returned void".to_string()),
        };
        let big_refused = self.builder.build_is_null(big_ptr, "big_refused").map_err(err)?;
        self.builder.build_conditional_branch(big_refused, no_room_bb, big_got_bb).map_err(err)?;

        self.builder.position_at_end(big_got_bb);
        let big_slot = unsafe {
            self.builder
                .build_gep(ptr, table.as_pointer_value(), &[big_idx], "big_slot")
        }
        .map_err(err)?;
        self.builder.build_store(big_slot, big_ptr).map_err(err)?;
        let big_after = self
            .builder
            .build_left_shift(big_end, sh, "after_big")
            .map_err(err)?;
        self.builder.build_store(next.as_pointer_value(), big_after).map_err(err)?;
        self.builder.build_return(Some(&big_ptr)).map_err(err)?;

        self.builder.position_at_end(fits_bb);
        let off = self.builder.build_and(cursor, within, "off").map_err(err)?;
        let end = self.builder.build_int_add(off, size, "end").map_err(err)?;
        // An allocation never straddles two chunks: they are separate mappings. If this one
        // would, skip the tail of the current chunk and start the next. The waste is bounded by
        // the size of one value, and only ever on a boundary.
        let straddles = self
            .builder
            .build_int_compare(inkwell::IntPredicate::UGT, end, chunk_size, "straddles")
            .map_err(err)?;
        let base_of_chunk = self.builder.build_int_sub(cursor, off, "chunk_start").map_err(err)?;
        let skipped = self
            .builder
            .build_int_add(base_of_chunk, chunk_size, "next_chunk_start")
            .map_err(err)?;
        let at = self
            .builder
            .build_select(straddles, skipped, cursor, "at")
            .map_err(err)?
            .into_int_value();
        let at_off = self
            .builder
            .build_select(straddles, i64t.const_zero(), off, "at_off")
            .map_err(err)?
            .into_int_value();
        let idx = self
            .builder
            .build_right_shift(at, sh, false, "chunk_index")
            .map_err(err)?;
        let past_table = self
            .builder
            .build_int_compare(
                inkwell::IntPredicate::UGE,
                idx,
                i64t.const_int(SLOTS as u64, false),
                "past_table",
            )
            .map_err(err)?;
        self.builder.build_conditional_branch(past_table, no_slots_bb, in_range_bb).map_err(err)?;

        self.builder.position_at_end(no_slots_bb);
        self.build_panic(
            "burxt runtime error: region memory exhausted — every chunk slot is in use\n",
        )?;

        self.builder.position_at_end(in_range_bb);
        let slot = unsafe {
            self.builder
                .build_gep(ptr, table.as_pointer_value(), &[idx], "slot")
        }
        .map_err(err)?;
        let held = self
            .builder
            .build_load(ptr, slot, "chunk_base")
            .map_err(err)?
            .into_pointer_value();
        let absent = self.builder.build_is_null(held, "absent").map_err(err)?;
        self.builder.build_conditional_branch(absent, grow_bb, ok_bb).map_err(err)?;

        // GROW: the cursor has reached a chunk that does not exist yet, so add it. This is the
        // whole of the change — where this used to be the exhaustion panic, it now asks again.
        self.builder.position_at_end(grow_bb);
        let fresh = self
            .builder
            .build_call(malloc, &[chunk_size.into()], "fresh_chunk")
            .map_err(err)?;
        let fresh_ptr = match fresh.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => v.into_pointer_value(),
            _ => return Err("malloc returned void".to_string()),
        };
        let refused = self.builder.build_is_null(fresh_ptr, "refused").map_err(err)?;
        self.builder.build_conditional_branch(refused, no_room_bb, store_bb).map_err(err)?;
        self.builder.position_at_end(store_bb);
        self.builder.build_store(slot, fresh_ptr).map_err(err)?;
        self.builder.build_unconditional_branch(ok_bb).map_err(err)?;

        self.builder.position_at_end(no_room_bb);
        self.build_panic(
            "burxt runtime error: region memory exhausted — the machine would not give this \
             process another region chunk\n",
        )?;

        self.builder.position_at_end(ok_bb);
        let base_phi = self.builder.build_phi(ptr, "base").map_err(err)?;
        base_phi.add_incoming(&[(&held, in_range_bb), (&fresh_ptr, store_bb)]);
        let real_base = base_phi.as_basic_value().into_pointer_value();
        let out = unsafe { self.builder.build_gep(i8t, real_base, &[at_off], "cell") }
            .map_err(err)?;
        let after = self.builder.build_int_add(at, size, "after").map_err(err)?;
        self.builder.build_store(next.as_pointer_value(), after).map_err(err)?;
        self.builder.build_return(Some(&out)).map_err(err)?;

        if let Some(bb) = saved {
            self.builder.position_at_end(bb);
        }
        self.alloc_fn = Some(f);
        Ok(f)
    }


    /// The handle table's globals: where a held value is, which generation issued it, and what
    /// type it was. M17.
    ///
    /// Three parallel arrays rather than an array of structs, because every one of them is
    /// indexed by the same slot number and LLVM addresses a flat array in one `getelementptr`.
    fn handle_globals(
        &mut self,
    ) -> (
        inkwell::values::GlobalValue<'ctx>,
        inkwell::values::GlobalValue<'ctx>,
        inkwell::values::GlobalValue<'ctx>,
        inkwell::values::GlobalValue<'ctx>,
    ) {
        let ptr = self.ctx.ptr_type(AddressSpace::default());
        let i64t = self.ctx.i64_type();
        let make = |m: &inkwell::module::Module<'ctx>, name: &str, ty: inkwell::types::BasicTypeEnum<'ctx>| {
            if let Some(g) = m.get_global(name) {
                return g;
            }
            let g = m.add_global(ty, None, name);
            match ty {
                inkwell::types::BasicTypeEnum::ArrayType(a) => g.set_initializer(&a.const_zero()),
                _ => g.set_initializer(&i64t.const_zero()),
            }
            g
        };
        let slots = HANDLE_SLOTS as u32;
        (
            make(&self.module, "burxt.handle.where", ptr.array_type(slots).into()),
            make(&self.module, "burxt.handle.generation", i64t.array_type(slots).into()),
            make(&self.module, "burxt.handle.tag", i64t.array_type(slots).into()),
            make(&self.module, "burxt.handle.next", i64t.into()),
        )
    }

    /// `ptr @burxt.hold(ptr value, i64 tag) -> i64` — file a value, answer a packed handle.
    ///
    /// **The generation is what an index alone cannot do.** An index catches out-of-range and
    /// misses the case that actually happens: a host that kept a handle after a later `update`
    /// replaced the value. Slot 0 stays live across that, so an index check passes and the host
    /// reads the wrong model with no diagnostic — the silent use-after-free this milestone
    /// exists to refuse. So each slot counts how many times it has been issued, and the count
    /// travels in the handle.
    ///
    /// Slots are reused round-robin on purpose. A UI builds a model per keystroke, so handles
    /// are made in the thousands and a table that only grew would be the leak wearing a hat.
    /// Reuse is safe precisely because the generation moved.
    fn hold_fn(&mut self) -> Result<FunctionValue<'ctx>, String> {
        if let Some(f) = self.hold_fn {
            return Ok(f);
        }
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let saved = self.builder.get_insert_block();
        let (wheres, gens, tags, next) = self.handle_globals();
        let i64t = self.ctx.i64_type();
        let ptr = self.ctx.ptr_type(AddressSpace::default());

        let f = self.module.add_function(
            "burxt.hold",
            i64t.fn_type(&[ptr.into(), i64t.into()], false),
            None,
        );
        let entry = self.ctx.append_basic_block(f, "entry");
        self.builder.position_at_end(entry);
        let value = f.get_nth_param(0).unwrap().into_pointer_value();
        let tag = f.get_nth_param(1).unwrap().into_int_value();

        let n = self
            .builder
            .build_load(i64t, next.as_pointer_value(), "next")
            .map_err(err)?
            .into_int_value();
        let mask = i64t.const_int(HANDLE_SLOTS - 1, false);
        let slot = self.builder.build_and(n, mask, "slot").map_err(err)?;
        let bumped = self
            .builder
            .build_int_add(n, i64t.const_int(1, false), "next_after")
            .map_err(err)?;
        self.builder.build_store(next.as_pointer_value(), bumped).map_err(err)?;

        let gen_slot = unsafe {
            self.builder.build_gep(i64t, gens.as_pointer_value(), &[slot], "gen_slot")
        }
        .map_err(err)?;
        let was = self
            .builder
            .build_load(i64t, gen_slot, "was")
            .map_err(err)?
            .into_int_value();
        let now = self
            .builder
            .build_int_add(was, i64t.const_int(1, false), "generation")
            .map_err(err)?;
        self.builder.build_store(gen_slot, now).map_err(err)?;

        let where_slot = unsafe {
            self.builder.build_gep(ptr, wheres.as_pointer_value(), &[slot], "where_slot")
        }
        .map_err(err)?;
        self.builder.build_store(where_slot, value).map_err(err)?;
        let tag_slot = unsafe {
            self.builder.build_gep(i64t, tags.as_pointer_value(), &[slot], "tag_slot")
        }
        .map_err(err)?;
        self.builder.build_store(tag_slot, tag).map_err(err)?;

        // (generation << 32) | slot. A handle of 0 is therefore never one this issued, because
        // a generation is incremented BEFORE it travels — so the integer a host has not been
        // given yet is refused rather than read as slot zero.
        let high = self
            .builder
            .build_left_shift(now, i64t.const_int(32, false), "high")
            .map_err(err)?;
        let packed = self.builder.build_or(high, slot, "handle").map_err(err)?;
        self.builder.build_return(Some(&packed)).map_err(err)?;

        if let Some(bb) = saved {
            self.builder.position_at_end(bb);
        }
        self.hold_fn = Some(f);
        Ok(f)
    }

    /// `ptr @burxt.held(i64 handle, i64 tag) -> ptr` — the value back, or a named refusal.
    ///
    /// **Three causes, three messages, and that is the requirement rather than a nicety.** A
    /// check that cannot tell two failures apart sends the reader to the wrong one — the same
    /// rule the `std/` diagnostics were rewritten to obey. "Never issued" is a host bug in the
    /// integer it kept; "superseded" is a host bug in WHEN it used one, and the fix is to keep
    /// the handle the last call answered with; "another type" is a wiring mistake, or a handle
    /// from a different module, and no amount of retrying will help.
    fn held_fn(&mut self) -> Result<FunctionValue<'ctx>, String> {
        if let Some(f) = self.held_fn {
            return Ok(f);
        }
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let saved = self.builder.get_insert_block();
        let (wheres, gens, tags, _next) = self.handle_globals();
        let i64t = self.ctx.i64_type();
        let ptr = self.ctx.ptr_type(AddressSpace::default());

        let f = self.module.add_function(
            "burxt.held",
            ptr.fn_type(&[i64t.into(), i64t.into()], false),
            None,
        );
        let entry = self.ctx.append_basic_block(f, "entry");
        let unknown_bb = self.ctx.append_basic_block(f, "never_issued");
        let in_range_bb = self.ctx.append_basic_block(f, "slot_in_range");
        let live_bb = self.ctx.append_basic_block(f, "slot_live");
        let ahead_bb = self.ctx.append_basic_block(f, "generation_differs");
        let stale_bb = self.ctx.append_basic_block(f, "superseded");
        let same_gen_bb = self.ctx.append_basic_block(f, "generation_matches");
        let wrong_type_bb = self.ctx.append_basic_block(f, "another_type");
        let ok_bb = self.ctx.append_basic_block(f, "ok");

        self.builder.position_at_end(entry);
        let handle = f.get_nth_param(0).unwrap().into_int_value();
        let want = f.get_nth_param(1).unwrap().into_int_value();
        let low = i64t.const_int(0xFFFF_FFFF, false);
        let slot = self.builder.build_and(handle, low, "slot").map_err(err)?;
        let gen = self
            .builder
            .build_right_shift(handle, i64t.const_int(32, false), false, "generation")
            .map_err(err)?;
        let past = self
            .builder
            .build_int_compare(
                inkwell::IntPredicate::UGE,
                slot,
                i64t.const_int(HANDLE_SLOTS, false),
                "past_table",
            )
            .map_err(err)?;
        // **A handle carrying generation ZERO was never issued, and this arm exists because the
        // message was wrong without it.** `hold` increments before it packs, so a real handle
        // always carries 1 or more — and the integer a host is most likely to pass by mistake is
        // exactly `0`: an uninitialised variable, a missing return, a literal where a call should
        // have been. Without this it landed in the generation comparison below and was told the
        // value had been "replaced by a later call", sending a reader to look for a call that was
        // never made. Reported from a JS host by star-burxt, which is the only place it is
        // reachable from.
        let unissued = self
            .builder
            .build_int_compare(inkwell::IntPredicate::EQ, gen, i64t.const_zero(), "generation_zero")
            .map_err(err)?;
        let bad_shape = self.builder.build_or(past, unissued, "never_from_here").map_err(err)?;
        self.builder.build_conditional_branch(bad_shape, unknown_bb, in_range_bb).map_err(err)?;

        self.builder.position_at_end(in_range_bb);
        let gen_slot = unsafe {
            self.builder.build_gep(i64t, gens.as_pointer_value(), &[slot], "gen_slot")
        }
        .map_err(err)?;
        let live = self
            .builder
            .build_load(i64t, gen_slot, "live")
            .map_err(err)?
            .into_int_value();
        // A slot at generation zero has never been issued, so this is the same failure as an
        // index past the end rather than a stale handle — and saying "superseded" about a value
        // that never existed would send the reader looking for a call that never happened.
        let never = self
            .builder
            .build_int_compare(inkwell::IntPredicate::EQ, live, i64t.const_zero(), "never")
            .map_err(err)?;
        self.builder.build_conditional_branch(never, unknown_bb, live_bb).map_err(err)?;

        self.builder.position_at_end(live_bb);
        let same = self
            .builder
            .build_int_compare(inkwell::IntPredicate::EQ, gen, live, "same_generation")
            .map_err(err)?;
        self.builder.build_conditional_branch(same, same_gen_bb, ahead_bb).map_err(err)?;

        // **Superseded means BEHIND. A generation ahead of the live one was never issued**, and
        // calling it "replaced by a later call" reads backwards — 9 is not behind 1. The two are
        // different mistakes: one handle is too old, the other never existed. Also star-burxt's,
        // from the same host run.
        self.builder.position_at_end(ahead_bb);
        let behind = self
            .builder
            .build_int_compare(inkwell::IntPredicate::ULT, gen, live, "behind_the_live_one")
            .map_err(err)?;
        self.builder.build_conditional_branch(behind, stale_bb, unknown_bb).map_err(err)?;

        self.builder.position_at_end(same_gen_bb);
        let tag_slot = unsafe {
            self.builder.build_gep(i64t, tags.as_pointer_value(), &[slot], "tag_slot")
        }
        .map_err(err)?;
        let held_tag = self
            .builder
            .build_load(i64t, tag_slot, "held_tag")
            .map_err(err)?
            .into_int_value();
        let matches = self
            .builder
            .build_int_compare(inkwell::IntPredicate::EQ, held_tag, want, "tag_matches")
            .map_err(err)?;
        self.builder.build_conditional_branch(matches, ok_bb, wrong_type_bb).map_err(err)?;

        self.builder.position_at_end(ok_bb);
        let where_slot = unsafe {
            self.builder.build_gep(ptr, wheres.as_pointer_value(), &[slot], "where_slot")
        }
        .map_err(err)?;
        let value = self.builder.build_load(ptr, where_slot, "value").map_err(err)?;
        self.builder.build_return(Some(&value)).map_err(err)?;

        self.builder.position_at_end(unknown_bb);
        // Read with the cause deliberately forgotten, which is star-burxt's rule: *a diagnostic
        // written while you know the answer is a diagnostic written for someone who does.* The
        // old wording described the situation accurately and told a host author nothing to DO.
        self.build_panic(
            "burxt runtime error: this handle was never issued by this module. Pass back exactly \
             the integer a call into this module answered with — a 0, a remembered constant, or \
             a number from somewhere else names nothing here.\n",
        )?;

        // The one message that must carry NUMBERS, because the two generations are the whole
        // content of the advice: keep the handle the last call answered with.
        self.builder.position_at_end(stale_bb);
        let fprintf = self.fprintf_fn();
        let (stderr_g, _fputs, exit) = self.panic_deps();
        let fmt = self.global_str(
            "burxt runtime error: this handle refers to a value that was replaced by a later \
             call. It was issued at generation %lld and the live one is generation %lld — use \
             the handle the last call answered with.\n",
            "stale_handle_msg",
        );
        let stream = self.load_stderr(stderr_g)?;
        self.builder
            .build_call(fprintf, &[stream.into(), fmt.into(), gen.into(), live.into()], "fprintf")
            .map_err(err)?;
        self.build_exit70(exit)?;

        self.builder.position_at_end(wrong_type_bb);
        // **"or came from a different module" was advice for a cause this branch cannot
        // reach**, and it went out claiming a capability the check does not have. This compares
        // the slot's tag — written by THIS module's own `handle_of` — against what this module
        // expects, so a handle issued elsewhere finds this module's tag sitting there and
        // matches. star-burxt measured it: two fresh instances issue identical bit patterns and
        // neither detects the other. Cross-module detection needs the fingerprint in the HANDLE
        // rather than the table, and is recorded open. Until it exists, saying so here sends a
        // host author hunting a module mix-up that is not what happened.
        self.build_panic(
            "burxt runtime error: this handle names a value of a different type than the one \
             being asked for. A handle from `handle_of(Model)` reads back only as a Model — \
             check that the call you passed it to is the one that issued it.\n",
        )?;

        if let Some(bb) = saved {
            self.builder.position_at_end(bb);
        }
        self.held_fn = Some(f);
        Ok(f)
    }

    /// The runtime tag for a held type: which class, and which module issued it.
    ///
    /// **The module half is what makes "a handle from another module is refused" true**, and it
    /// has to be derived rather than random: Burxt compiles reproducibly, so a per-build random
    /// id would make the same source produce different bytes. FNV-1a over the program's declared
    /// type names is stable for the same program and different for a different one.
    fn handle_tag(&mut self, class: &str) -> u64 {
        let fingerprint = match self.module_fingerprint {
            Some(f) => f,
            None => {
                let mut names: Vec<&String> = self.struct_fields.keys().collect();
                names.sort();
                let mut h: u64 = 0xcbf2_9ce4_8422_2325;
                for n in names {
                    for b in n.as_bytes() {
                        h ^= *b as u64;
                        h = h.wrapping_mul(0x100_0000_01b3);
                    }
                    h ^= 0xff;
                    h = h.wrapping_mul(0x100_0000_01b3);
                }
                *self.module_fingerprint.insert(h)
            }
        };
        let mut h = fingerprint;
        for b in class.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100_0000_01b3);
        }
        // Zero is the "empty slot" tag, so a real one is never zero.
        if h == 0 { 1 } else { h }
    }

    /// Emit a call to `@burxt.strlen(s)` — the byte length of a String.
    /// A String's length: ONE LOAD, from the eight bytes before its first byte.
    ///
    /// This was a `strlen` until v0.0.120, which is why reading n bytes cost n² — a bounds check
    /// is a length, and a length was a scan. M9 §3 measured that and named this as the only fix
    /// that changes the shape rather than how often the scan happens.
    fn build_str_len(&mut self, s: PointerValue<'ctx>) -> Result<IntValue<'ctx>, String> {
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let i64t = self.ctx.i64_type();
        let i8t = self.ctx.i8_type();
        let header = unsafe {
            self.builder.build_gep(i8t, s, &[i64t.const_int(-8i64 as u64, true)], "header")
        }
        .map_err(err)?;
        Ok(self
            .builder
            .build_load(i64t, header, "len")
            .map_err(err)?
            .into_int_value())
    }

    /// Emit a call to `@burxt.streq(a, b)` — 1 if the bytes match, else 0.
    /// Field-by-field equality for a class, answered as an i64 that is 0 or 1.
    ///
    /// **Not `memcmp`.** A class holding a String holds a pointer, and two equal strings need not
    /// live at the same address — so comparing the struct's bytes would answer `false` for two
    /// accounts with the same owner built separately. That is a wrong answer that looks like a
    /// working program, which is the failure this language exists to prevent.
    ///
    /// It is also why padding does not matter here: nothing reads the bytes, only the fields.
    ///
    /// Both sides arrive as STRUCT VALUES — `gen_expr` on an aggregate answers one — so each field
    /// comes out with `extract_value` rather than a load from an address. That also makes the
    /// operands unrestricted: a call result and a temporary have no address, and comparing
    /// `open("a", $1.00) == open("a", $1.00)` has to work.
    ///
    /// Ands the field results rather than branching, so there is no short circuit and no basic-block
    /// bookkeeping. A class has a handful of fields and each comparison is a few instructions; a
    /// branch per field would cost more to emit than it could ever save, and LLVM will find whatever
    /// early exit is worth having.
    fn build_class_eq(
        &mut self,
        name: &str,
        a: BasicValueEnum<'ctx>,
        b: BasicValueEnum<'ctx>,
    ) -> Result<IntValue<'ctx>, String> {
        let i64t = self.ctx.i64_type();
        let fields = self
            .struct_fields
            .get(name)
            .cloned()
            .ok_or_else(|| format!("codegen bug: no layout for class `{}`", name))?;
        let sa = a.into_struct_value();
        let sb = b.into_struct_value();
        let mut all = i64t.const_int(1, false);
        for (i, fty) in fields.iter().enumerate() {
            let va = self
                .builder
                .build_extract_value(sa, i as u32, "eq_a")
                .map_err(|e| e.to_string())?;
            let vb = self
                .builder
                .build_extract_value(sb, i as u32, "eq_b")
                .map_err(|e| e.to_string())?;
            let one = match fty {
                // A nested class: recurse. `extract_value` answers its struct value.
                Type::Named(inner) if self.struct_fields.contains_key(inner) => {
                    self.build_class_eq(inner, va, vb)?
                }
                Type::String => {
                    self.build_str_eq(va.into_pointer_value(), vb.into_pointer_value())?
                }
                // Int, Bool and Decimal are all one i64 cell, and a scaled decimal of equal scale
                // compares exactly as a plain integer — no rescaling, no rounding, no float.
                _ => {
                    let bit = self
                        .builder
                        .build_int_compare(
                            inkwell::IntPredicate::EQ,
                            va.into_int_value(),
                            vb.into_int_value(),
                            "eq_f",
                        )
                        .map_err(|e| e.to_string())?;
                    self.builder
                        .build_int_z_extend(bit, i64t, "eq_f64")
                        .map_err(|e| e.to_string())?
                }
            };
            all = self
                .builder
                .build_int_mul(all, one, "eq_and")
                .map_err(|e| e.to_string())?;
        }
        Ok(all)
    }

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
        let fprintf = self.fprintf_fn();
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
        let arguments: Vec<BasicMetadataValueEnum> =
            vec![stream.into(), fmt.into(), i.into(), n.into(), last.into()];
        self.builder.build_call(fprintf, &arguments, "fprintf").map_err(err)?;
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

    /// Range-check and narrow an Int to a sized C integer, roadmap A7.
    ///
    /// `build_to_cint` generalised: the helper is named `burxt.checked.<bits>.<s|u>` so each width
    /// gets exactly one, defined the first time it is needed and shared after that.
    fn build_to_width(
        &mut self,
        v: IntValue<'ctx>,
        bits: u32,
        signed: bool,
    ) -> Result<IntValue<'ctx>, String> {
        let f = self.to_width_fn(bits, signed)?;
        let call = self
            .builder
            .build_call(f, &[v.into()], "checked_width")
            .map_err(|e| e.to_string())?;
        match call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(x) => Ok(x.into_int_value()),
            _ => Err("width helper returned void".to_string()),
        }
    }

    /// Get (or lazily define) `iN @burxt.checked.<bits>.<s|u>(i64)`.
    ///
    /// **The bounds are computed from the two numbers rather than tabulated**, which is what one
    /// `Type::Width` variant buys: `i32` is `-2^31 ..= 2^31-1`, `u8` is `0 ..= 2^8-1`, and a table
    /// of four would be four chances to write one bound wrong.
    ///
    /// **`u64`'s upper bound is `i64::MAX`, not `u64::MAX`, and the message says so.** A Burxt `Int`
    /// is a signed i64, so there is no Int above `i64::MAX` to check — the honest statement is that
    /// the language cannot express the top half of a `u64`, and the refusal names that rather than
    /// claiming a range it cannot hold. `u64` therefore only rejects NEGATIVES, which is a real
    /// check and not a no-op: `-1` as a `size_t` is an enormous number and every C API that has
    /// been handed one has the bug to show for it.
    fn to_width_fn(&mut self, bits: u32, signed: bool) -> Result<FunctionValue<'ctx>, String> {
        let spelled = format!("{}{}", if signed { "i" } else { "u" }, bits);
        let symbol = format!("burxt.checked.{}.{}", bits, if signed { "s" } else { "u" });
        if let Some(f) = self.module.get_function(&symbol) {
            return Ok(f);
        }
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let saved_block = self.builder.get_insert_block();

        let i64t = self.ctx.i64_type();
        let target = std::num::NonZeroU32::new(bits)
            .and_then(|b| self.ctx.custom_width_int_type(b).ok())
            .ok_or_else(|| format!("codegen bug: {} is not a width LLVM accepts", bits))?;
        let fn_ty = target.fn_type(&[i64t.into()], false);
        let f = self.module.add_function(&symbol, fn_ty, None);
        let entry = self.ctx.append_basic_block(f, "entry");
        let panic_bb = self.ctx.append_basic_block(f, "doesnt_fit");
        let ok_bb = self.ctx.append_basic_block(f, "ok");

        self.builder.position_at_end(entry);
        let v = f.get_nth_param(0).unwrap().into_int_value();
        use inkwell::IntPredicate::*;
        // Bounds as i64, which is what the value being checked is. `1 << (bits - 1)` for a signed
        // width and `1 << bits` for an unsigned one — and `bits == 64` unsigned would shift by 64,
        // which is undefined, so that case takes `i64::MAX` directly. That is the same limit the
        // doc comment names, arriving as an arithmetic edge as well as a language one.
        let (low, high): (i64, i64) = if signed && bits >= 64 {
            // Not reachable from the four widths the lexer admits, and written anyway because the
            // signed branch below would OVERFLOW here: `1i64 << 63` is `i64::MIN`, and negating it
            // has no i64 answer. A compiler that panicked while compiling would be the worst
            // possible version of this feature, and the day someone adds `i64` this arm is already
            // the right answer — every i64 fits an Int, so the check is vacuous.
            (i64::MIN, i64::MAX)
        } else if signed {
            let span = 1i64 << (bits - 1);
            (-span, span - 1)
        } else if bits >= 64 {
            (0, i64::MAX)
        } else {
            (0, (1i64 << bits) - 1)
        };
        let max = i64t.const_int(high as u64, true);
        let min = i64t.const_int(low as u64, true);
        let too_big = self.builder.build_int_compare(SGT, v, max, "too_big").map_err(err)?;
        let too_small = self.builder.build_int_compare(SLT, v, min, "too_small").map_err(err)?;
        let out = self.builder.build_or(too_big, too_small, "out_of_range").map_err(err)?;
        self.builder.build_conditional_branch(out, panic_bb, ok_bb).map_err(err)?;

        self.builder.position_at_end(panic_bb);
        // The bounds are IN the message. "does not fit in a u8" sends a reader to a header file;
        // "a u8 holds 0 to 255" is the whole answer, and the numbers are the ones actually checked
        // rather than a second copy that could disagree with them.
        // "an i32", "a u8" — the article follows the SPELLING's sound, not the type's signedness
        // as such: `i` reads "eye" and takes "an", `u` reads "you" and takes "a". Stage-1 builds the
        // same string the same way, because these two messages are compared byte for byte.
        let article = if signed { "an" } else { "a" };
        let mut said = format!(
            "burxt runtime error: this value does not fit in {} {} — the external parameter holds \
             {} to {}\n",
            article, spelled, low, high
        );
        if !signed && bits >= 64 {
            said = format!(
                "burxt runtime error: this value does not fit in a u64 as Burxt can hold it — an \
                 Int is a SIGNED 64-bit integer, so the checked range is 0 to {}, and the top half \
                 of a u64 has no Int to land in\n",
                i64::MAX
            );
        }
        self.build_panic(&said)?;

        self.builder.position_at_end(ok_bb);
        let narrowed = self.builder.build_int_truncate(v, target, "width").map_err(err)?;
        self.builder.build_return(Some(&narrowed)).map_err(err)?;

        if let Some(bb) = saved_block {
            self.builder.position_at_end(bb);
        }
        Ok(f)
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
            "burxt runtime error: this value does not fit in a C int — the external \
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

    /// Copy `count` bytes from `at` into the current region, NUL-terminated.
    ///
    /// The result is an ordinary Burxt String: indistinguishable from a literal, so
    /// it can be compared, printed, joined, or handed to C. Bounds are checked
    /// against the source's length, and the failure names the numbers rather than
    /// saying "out of range".
    fn build_substring(
        &mut self,
        bytes: PointerValue<'ctx>,
        at: IntValue<'ctx>,
        count: IntValue<'ctx>,
    ) -> Result<PointerValue<'ctx>, String> {
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let i64t = self.ctx.i64_type();
        let i8t = self.ctx.i8_type();
        let source_len = self.build_str_len(bytes)?;
        let end = self.builder.build_int_add(at, count, "end").map_err(err)?;

        use inkwell::IntPredicate::*;
        let neg_at = self.builder.build_int_compare(SLT, at, i64t.const_zero(), "neg_at").map_err(err)?;
        let neg_len = self.builder.build_int_compare(SLT, count, i64t.const_zero(), "neg_len").map_err(err)?;
        let past = self.builder.build_int_compare(SGT, end, source_len, "past_end").map_err(err)?;
        let bad = self.builder.build_or(neg_at, neg_len, "bad_offsets").map_err(err)?;
        let bad = self.builder.build_or(bad, past, "out_of_range").map_err(err)?;

        let function = self
            .builder
            .get_insert_block()
            .and_then(|b| b.get_parent())
            .ok_or("codegen bug: substring outside a function")?;
        let broken = self.ctx.append_basic_block(function, "substring_oob");
        let ok = self.ctx.append_basic_block(function, "substring_ok");
        self.builder.build_conditional_branch(bad, broken, ok).map_err(err)?;

        self.builder.position_at_end(broken);
        let fprintf = self.fprintf_fn();
        let (stderr_g, _, exit) = self.panic_deps();
        let fmt = self.global_str(
            "burxt runtime error: substring(s, %lld, %lld) does not fit — this string \
             has %lld bytes\n",
            "fmt_substring_oob",
        );
        let stream = self.load_stderr(stderr_g)?;
        let arguments: Vec<BasicMetadataValueEnum> =
            vec![stream.into(), fmt.into(), at.into(), count.into(), source_len.into()];
        self.builder.build_call(fprintf, &arguments, "fprintf").map_err(err)?;
        self.build_exit70(exit)?;

        self.builder.position_at_end(ok);
        let out = self.build_alloc_string(count)?;
        let from = unsafe { self.builder.build_gep(i8t, bytes, &[at], "from") }.map_err(err)?;
        self.builder
            .build_memcpy(out, 1, from, 1, count)
            .map_err(|e| e.to_string())?;
        let tail = unsafe { self.builder.build_gep(i8t, out, &[count], "tail") }.map_err(err)?;
        self.builder.build_store(tail, i8t.const_zero()).map_err(err)?;
        Ok(out)
    }

    /// Integer division and remainder, in three named forms.
    ///
    /// `/` on two Ints stays refused, because a single operator cannot say which way
    /// it rounds and the answer differs for negatives: -7 divided by 2 is -3 rounding
    /// toward zero and -4 rounding down. Naming the operation says it out loud, which
    /// is the same reasoning that made `byte_at` say "byte".
    ///
    /// Every form checks what C leaves undefined: division by zero, and
    /// `i64::MIN / -1`, whose quotient does not exist in an i64.
    fn build_int_div(
        &mut self,
        kind: IntDiv,
        a: IntValue<'ctx>,
        b: IntValue<'ctx>,
    ) -> Result<IntValue<'ctx>, String> {
        let f = self.int_div_fn(kind)?;
        let call = self
            .builder
            .build_call(f, &[a.into(), b.into()], "intdiv")
            .map_err(|e| e.to_string())?;
        match call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => Ok(v.into_int_value()),
            _ => Err("integer division helper returned void".to_string()),
        }
    }

    fn int_div_fn(&mut self, kind: IntDiv) -> Result<FunctionValue<'ctx>, String> {
        let (symbol, name) = match kind {
            IntDiv::Floor => ("burxt.div.floor", "divide_floor"),
            IntDiv::Trunc => ("burxt.div.trunc", "divide_toward_zero"),
            IntDiv::Rem => ("burxt.remainder", "remainder"),
        };
        if let Some(f) = self.module.get_function(symbol) {
            return Ok(f);
        }
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let saved_block = self.builder.get_insert_block();
        let i64t = self.ctx.i64_type();
        let f = self.module.add_function(symbol, i64t.fn_type(&[i64t.into(), i64t.into()], false), None);
        let entry = self.ctx.append_basic_block(f, "entry");
        let by_zero = self.ctx.append_basic_block(f, "by_zero");
        let not_zero = self.ctx.append_basic_block(f, "not_zero");
        let overflows = self.ctx.append_basic_block(f, "overflows");
        let compute = self.ctx.append_basic_block(f, "compute");

        self.builder.position_at_end(entry);
        let a = f.get_nth_param(0).unwrap().into_int_value();
        let b = f.get_nth_param(1).unwrap().into_int_value();
        use inkwell::IntPredicate::*;
        let zero = i64t.const_zero();
        let is_zero = self.builder.build_int_compare(EQ, b, zero, "is_zero").map_err(err)?;
        self.builder.build_conditional_branch(is_zero, by_zero, not_zero).map_err(err)?;

        self.builder.position_at_end(by_zero);
        self.build_panic(&format!(
            "burxt runtime error: {}(...) by zero\n",
            name
        ))?;

        // i64::MIN / -1 has no i64 answer, and LLVM's sdiv leaves it undefined.
        self.builder.position_at_end(not_zero);
        let min = i64t.const_int(i64::MIN as u64, true);
        let neg_one = i64t.const_int(-1i64 as u64, true);
        let a_is_min = self.builder.build_int_compare(EQ, a, min, "a_min").map_err(err)?;
        let b_is_neg1 = self.builder.build_int_compare(EQ, b, neg_one, "b_neg1").map_err(err)?;
        let bad = self.builder.build_and(a_is_min, b_is_neg1, "min_over_neg1").map_err(err)?;
        self.builder.build_conditional_branch(bad, overflows, compute).map_err(err)?;

        self.builder.position_at_end(overflows);
        self.build_panic(&format!(
            "burxt runtime error: {}(...) overflowed — the most negative Int divided \
             by -1 has no Int answer\n",
            name
        ))?;

        self.builder.position_at_end(compute);
        let result = match kind {
            IntDiv::Rem => self.builder.build_int_signed_rem(a, b, "remainder").map_err(err)?,
            IntDiv::Trunc => self.builder.build_int_signed_div(a, b, "quot").map_err(err)?,
            IntDiv::Floor => {
                // Truncating division rounds toward zero; flooring rounds down. They
                // differ by one exactly when there is a remainder and the operands
                // have opposite signs.
                let q = self.builder.build_int_signed_div(a, b, "quot").map_err(err)?;
                let r = self.builder.build_int_signed_rem(a, b, "remainder").map_err(err)?;
                let has_rem = self.builder.build_int_compare(NE, r, zero, "has_rem").map_err(err)?;
                let r_neg = self.builder.build_int_compare(SLT, r, zero, "r_neg").map_err(err)?;
                let b_neg = self.builder.build_int_compare(SLT, b, zero, "b_neg").map_err(err)?;
                let signs_differ = self
                    .builder
                    .build_xor(r_neg, b_neg, "signs_differ")
                    .map_err(err)?;
                let adjust = self.builder.build_and(has_rem, signs_differ, "adjust").map_err(err)?;
                let lower = self
                    .builder
                    .build_int_sub(q, i64t.const_int(1, false), "lower")
                    .map_err(err)?;
                self.builder
                    .build_select(adjust, lower, q, "floor")
                    .map_err(err)?
                    .into_int_value()
            }
        };
        self.builder.build_return(Some(&result)).map_err(err)?;

        if let Some(bb) = saved_block {
            self.builder.position_at_end(bb);
        }
        Ok(f)
    }

    /// Emit a call to the range-checked i64 -> double conversion used for a
    /// `CDouble` extern parameter.
    fn build_to_cdouble(&mut self, v: IntValue<'ctx>) -> Result<FloatValue<'ctx>, String> {
        let f = self.to_cdouble_fn()?;
        let call = self
            .builder
            .build_call(f, &[v.into()], "to_cdouble")
            .map_err(|e| e.to_string())?;
        match call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => Ok(v.into_float_value()),
            _ => Err("CDouble helper returned void".to_string()),
        }
    }

    /// Get (or lazily define) `double @burxt.checked.cdouble(i64)`: the value as
    /// C's double, or a named panic if the conversion would not be exact.
    ///
    /// 2^53 is where doubles stop being able to represent every integer. Below
    /// it every Int converts exactly; above it some do and some silently become
    /// their neighbour, and "silently becomes a different number" is precisely
    /// what Burxt refuses everywhere else. A Decimal never reaches here at all —
    /// typeck refuses that crossing outright.
    fn to_cdouble_fn(&mut self) -> Result<FunctionValue<'ctx>, String> {
        if let Some(f) = self.cdouble_fn {
            return Ok(f);
        }
        let err = |e: inkwell::builder::BuilderError| e.to_string();
        let saved_block = self.builder.get_insert_block();

        let i64t = self.ctx.i64_type();
        let f64t = self.ctx.f64_type();
        let fn_ty = f64t.fn_type(&[i64t.into()], false);
        let f = self.module.add_function("burxt.checked.cdouble", fn_ty, None);
        let entry = self.ctx.append_basic_block(f, "entry");
        let panic_bb = self.ctx.append_basic_block(f, "not_exact");
        let ok_bb = self.ctx.append_basic_block(f, "ok");

        self.builder.position_at_end(entry);
        let v = f.get_nth_param(0).unwrap().into_int_value();
        use inkwell::IntPredicate::*;
        const EXACT: u64 = 1 << 53;
        let max = i64t.const_int(EXACT, false);
        let min = i64t.const_int((-(EXACT as i64)) as u64, true);
        let too_big = self.builder.build_int_compare(SGT, v, max, "too_big").map_err(err)?;
        let too_small = self.builder.build_int_compare(SLT, v, min, "too_small").map_err(err)?;
        let out = self.builder.build_or(too_big, too_small, "not_exact").map_err(err)?;
        self.builder.build_conditional_branch(out, panic_bb, ok_bb).map_err(err)?;

        self.builder.position_at_end(panic_bb);
        self.build_panic(
            "burxt runtime error: this Int cannot cross as a C double exactly — \
             a double represents every integer only up to 2^53\n",
        )?;

        self.builder.position_at_end(ok_bb);
        let converted = self
            .builder
            .build_signed_int_to_float(v, f64t, "cdouble")
            .map_err(err)?;
        self.builder.build_return(Some(&converted)).map_err(err)?;

        if let Some(bb) = saved_block {
            self.builder.position_at_end(bb);
        }
        self.cdouble_fn = Some(f);
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
    /// A string literal, as `{ i64 length, [n+1 x i8] bytes }` in the module's globals, answering a
    /// pointer to the BYTES.
    ///
    /// A literal needs the same header every region-built String has, or `len` of one would read
    /// whatever the linker happened to place before it. Constant rather than allocated: a literal
    /// outlives every region, which is why it never needed one.
    fn global_str(&self, s: &str, name: &str) -> PointerValue<'ctx> {
        let i64t = self.ctx.i64_type();
        let i8t = self.ctx.i8_type();
        let mut bytes: Vec<inkwell::values::IntValue> =
            s.bytes().map(|b| i8t.const_int(b as u64, false)).collect();
        bytes.push(i8t.const_zero());
        let body = i8t.const_array(&bytes);
        let whole = self.ctx.const_struct(&[i64t.const_int(s.len() as u64, false).into(), body.into()], false);
        let gv = self.module.add_global(whole.get_type(), None, name);
        gv.set_initializer(&whole);
        gv.set_constant(true);
        gv.set_linkage(inkwell::module::Linkage::Private);
        unsafe {
            self.builder
                .build_gep(
                    self.ctx.i8_type(),
                    gv.as_pointer_value(),
                    &[i64t.const_int(8, false)],
                    "literal_bytes",
                )
                .expect("literal bytes")
        }
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

    /// The MODULE has to say which target it is for, or LLVM lays out its types with whatever
    /// default it was built with and the object disagrees with its own datalayout. Set from the
    /// machine rather than from the string, so the two can never differ.
    fn stamp_target(
        &self,
        triple: &inkwell::targets::TargetTriple,
        tm: &inkwell::targets::TargetMachine,
    ) {
        self.module.set_triple(triple);
        self.module.set_data_layout(&tm.get_target_data().get_data_layout());

        // Darwin's libc exports NO symbol called `stderr`. <stdio.h> defines `stderr` as a
        // macro for `__stderrp`, so a reference to `stderr` links against glibc and fails on
        // every Apple target with:
        //
        //     Undefined symbols for architecture arm64: "_stderr"
        //
        // That is every program able to report a runtime error — so, every program. Measured
        // on a macos-14 runner, and true for `--target *-apple-darwin` ever since
        // cross-targeting shipped in v0.0.197.
        //
        // **Why it is done HERE, by renaming, and not where the global is created.** The whole
        // module is built before any target is chosen: `main.rs` calls `cg.compile()` and only
        // then `retarget()`. During construction `get_triple()` is empty, so the name cannot be
        // decided at `add_global`. `stamp_target` is the one point both the `emit-ir --target`
        // path and the object-emission path pass through with a triple in hand.
        //
        // **Why no test caught it.** `the_ir_is_the_same_for_every_target` compares targets
        // against each other, and the wrong symbol was equally wrong in all of them — a test
        // for sameness cannot see an error that is the same everywhere. What it needs is a LINK
        // test; emitting an object proves nothing about whether the object links.
        //
        // **This is the documented exception to byte-identical IR.** The guarantee is about the
        // ARITHMETIC — every decimal operation, rounding helper and overflow check is identical
        // on every target, which is what makes the answers identical. A libc interface symbol is
        // not arithmetic, and one platform simply spells this one differently. ROADMAP-2.0 §D2.
        if triple.as_str().to_string_lossy().contains("apple") {
            if let Some(g) = self.module.get_global("stderr") {
                g.set_name("__stderrp");
            }
        }
    }

    /// Stamp the module for `triple` without emitting anything — what `emit-ir --target` needs, so
    /// the IR a cross build would compile can be READ rather than inferred.
    pub fn retarget(&self, triple: &str) -> Result<(), String> {
        use inkwell::targets::{
            CodeModel, InitializationConfig, RelocMode, Target, TargetTriple,
        };
        use inkwell::OptimizationLevel;
        Target::initialize_all(&InitializationConfig::default());
        let triple = TargetTriple::create(triple);
        let target = Target::from_triple(&triple).map_err(|e| {
            format!(
                "no backend for target `{}`: {}. `llc --version` lists the \
                 architectures this LLVM can emit.",
                triple.as_str().to_string_lossy(),
                e
            )
        })?;
        let tm = target
            .create_target_machine(
                &triple,
                "generic",
                "",
                OptimizationLevel::Default,
                RelocMode::PIC,
                CodeModel::Default,
            )
            .ok_or("failed to create target machine")?;
        self.stamp_target(&triple, &tm);
        Ok(())
    }

    /// Emit a native object file using the host target machine.
    pub fn write_object(&self, path: &str, optimise: bool) -> Result<(), String> {
        self.write_object_for(path, None, optimise)
    }

    /// Emit an object file for `triple`, or for the host when it is None.
    ///
    /// **The interesting property is what does NOT change with the triple.** Burxt has no float, so
    /// no arithmetic here depends on a CPU's rounding; every scalar is an i64 and every layout
    /// decision is made by TYPE rather than by size. So the IR this compiler produces is identical
    /// for two 64-bit targets apart from the `target triple` and `target datalayout` lines — which
    /// is the claim "the same money math on every target", made checkable instead of asserted.
    /// `the_ir_is_the_same_for_every_64_bit_target` in tests/runner.rs is the check.
    ///
    /// A generic CPU and no features for a cross target, deliberately: `get_host_cpu_name` would
    /// name THIS machine's CPU, which for a foreign triple is either meaningless or wrong, and
    /// "wrong but it compiled" is the failure mode this whole language is arranged against.
    /// `optimise` is false for `-O0`. Both halves of it matter and they are different
    /// mechanisms: the TargetMachine's level governs instruction selection and
    /// scheduling, and `run_passes` is the mid-level IR pipeline. Turning off only one
    /// leaves a build a debugger still cannot follow, which is why `-O0` sets both.
    pub fn write_object_for(&self, path: &str, triple: Option<&str>, optimise: bool) -> Result<(), String> {
        use inkwell::passes::PassBuilderOptions;
        use inkwell::targets::{
            CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine,
        };
        use inkwell::OptimizationLevel;

        // Every target LLVM was built with, not just this machine's. `initialize_native` is what
        // made `--target` impossible before v0.0.197: the backend for the requested triple was
        // simply not registered, and `Target::from_triple` failed with "no available targets are
        // compatible" — a message about the compiler's own initialisation, which is the least
        // helpful thing it could have said about the user's triple.
        inkwell::targets::Target::initialize_all(&InitializationConfig::default());

        let triple = match triple {
            Some(t) => inkwell::targets::TargetTriple::create(t),
            None => TargetMachine::get_default_triple(),
        };
        let target = Target::from_triple(&triple).map_err(|e| {
            format!(
                "no backend for target `{}`: {}. `llc --version` lists the \
                 architectures this LLVM can emit.",
                triple.as_str().to_string_lossy(),
                e
            )
        })?;
        let host = TargetMachine::get_default_triple();
        let is_host = triple.as_str() == host.as_str();
        let cpu = if is_host {
            TargetMachine::get_host_cpu_name().to_string_lossy().into_owned()
        } else {
            "generic".to_string()
        };
        let features = if is_host {
            TargetMachine::get_host_cpu_features().to_string_lossy().into_owned()
        } else {
            String::new()
        };
        let tm = target
            .create_target_machine(
                &triple,
                &cpu,
                &features,
                if optimise { OptimizationLevel::Default } else { OptimizationLevel::None },
                RelocMode::PIC,
                CodeModel::Default,
            )
            .ok_or("failed to create target machine")?;

        self.stamp_target(&triple, &tm);

        // The optimisation level on a TargetMachine governs instruction selection and
        // scheduling — it does NOT run the mid-level IR pipeline, and `write_to_file`
        // alone therefore ships whatever this file built, unsimplified. That gap is what
        // M9 turned out to be about.
        //
        // `byte_at(s, i)` bounds-checks against the string's length, and a Burxt String
        // is NUL-terminated, so the length is a `strlen`. One per byte read is O(n) per
        // byte and O(n²) per pass over a file, which is exactly what a compiler does all
        // day: stage-1 took three minutes on its own source, and 133 KB of comments alone
        // took thirty seconds. Nothing about the Burxt was wrong. `strlen` is `readonly`
        // and the loop writes nothing it reads, so LICM hoists it out — once there is a
        // pipeline to run LICM.
        //
        // Correctness first: the check stays, and every program still refuses to read a
        // byte it does not own. It is simply hoisted rather than repeated.
        //
        // `-O0` skips it entirely rather than running `default<O0>`: that pipeline still
        // runs `mem2reg` in some configurations, and a promoted alloca is a local a
        // debugger can no longer read — the one thing an `-O0` build exists to preserve.
        // The cost is the `strlen` above staying in the loop, which is the trade a
        // person asking for `-O0` has already accepted.
        if optimise {
            self.module
                .run_passes("default<O2>", &tm, PassBuilderOptions::create())
                .map_err(|e| e.to_string())?;
        }

        tm.write_to_file(&self.module, FileType::Object, std::path::Path::new(path))
            .map_err(|e| e.to_string())
    }
}

/// A path split into the (filename, directory) pair DWARF wants.
///
/// The directory is made ABSOLUTE, because a debugger resolves a relative one against
/// its own working directory rather than the compiler's, and `burxt build sub/p.bx` from
/// a parent then finds no source at all. This is also the reason debug info is opt-in:
/// an absolute path is the compiler's own machine baked into the object, which is the
/// one thing this project's reproducibility claim does not tolerate in a default build.
fn split_path(path: &str) -> (String, String) {
    let p = std::path::Path::new(path);
    let name = p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string());
    let dir = p
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .map(|d| d.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
    (name, dir.to_string_lossy().into_owned())
}

/// Is this type an aggregate (multi-field or multi-element)?
/// The boundary is decided by the TYPE, never by size, so it is identical on
/// every target.
/// What a `decreases` measure needs at a recursive call site.
#[derive(Clone)]
struct MeasureState<'ctx> {
    slot: PointerValue<'ctx>,
    measure: crate::typeck::TypedExpr,
    parameters: Vec<(String, Type)>,
    text: String,
    function: String,
}

/// Which bit operation a call asked for.
///
/// **Seven names rather than five operators**, and that is the same decision `IntDiv` records one
/// paragraph down rather than a new one. Two reasons, and the second is the one that settles it:
///
/// 1. `a & b == c` means `a & (b == c)` in C, and has been a bug in every C program that forgot.
///    Burxt's claim is that a reviewer can SEE a program is right; a precedence table they have to
///    remember is the opposite of that. `bit_and(a, b) == c` cannot be misread.
/// 2. **The right shift is genuinely two operations.** On a negative value, filling with zeros and
///    copying the sign bit give different answers, and one operator cannot say which — exactly the
///    situation `/` on two Ints is in. Once the shift needs two names, giving `&` an operator and
///    `>>` a name would be the inconsistency.
///
/// So `&` and `|` keep their helpful lexer error, now pointing at `bit_and`/`bit_or`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BitOp {
    And,
    Or,
    Xor,
    /// Every bit flipped. `bit_not(0)` is -1, because an Int is signed and there is nowhere else
    /// for the top bit to go.
    Not,
    /// Bits shifted past the top are DISCARDED, and this is the one place in the language where
    /// losing information is not an error — because it is what a shift is for. `shift_left(x, n)`
    /// is therefore **not** `x * 2^n`: multiplication traps on overflow and this does not. If you
    /// mean arithmetic, write arithmetic and get the trap.
    Left,
    /// Fills with zeros — a logical shift. `shift_right_zeros(-1, 63)` is 1.
    RightZeros,
    /// Copies the sign bit — an arithmetic shift. `shift_right_sign(-1, 63)` is -1, and
    /// `shift_right_sign(x, n)` is `divide_floor(x, 2^n)`.
    RightSign,
}

/// Which integer division a call asked for. Three names rather than one operator,
/// because they disagree on negatives and the difference must be visible.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IntDiv {
    /// Rounds down, toward negative infinity: `divide_floor(-7, 2) == -4`.
    Floor,
    /// Rounds toward zero, as C does: `divide_toward_zero(-7, 2) == -3`.
    Trunc,
    /// The remainder that pairs with `divide_toward_zero`: its sign follows the dividend.
    Rem,
}

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
/// is a pure function of the declared field types and order, so adding an interface
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
