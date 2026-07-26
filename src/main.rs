//! The `burxt` compiler driver.
//!
//! Usage:
//!   burxt build <file.bx> [link args...]   compile to a native executable
//!   burxt run   <file.bx> [link args...]   compile, then run it
//!   burxt emit-ir <file.bx>                print the LLVM IR (for the curious)
//!
//! Anything after the source file is handed to the system linker unchanged
//! (`cside.o`, `-lm`, `-L/opt/lib -lfoo`). An `extern fn` declaration is only
//! half of an FFI: the other half is a real object to link against, and Burxt
//! delegates linking to system tools rather than owning it.

mod ast;
mod lexer;
mod parser;
mod typeck;
mod codegen;

use inkwell::context::Context;
use std::path::Path;
use std::process::Command;

/// The parser, typechecker and codegen all recurse over expression trees, so
/// deeply nested source needs stack. Machine-generated Burxt (and eventually
/// the self-hosted compiler's own output) can nest thousands deep, so run the
/// whole compilation on a thread with a large stack rather than aborting.
const COMPILER_STACK_BYTES: usize = 512 * 1024 * 1024;

fn main() {
    let child = std::thread::Builder::new()
        .stack_size(COMPILER_STACK_BYTES)
        .spawn(compile_main)
        .expect("failed to start the compiler thread");
    match child.join() {
        Ok(()) => {}
        // The thread already reported the failure; don't double-print.
        Err(_) => std::process::exit(101),
    }
}

fn compile_main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("burxt {} — the Burxt compiler", env!("CARGO_PKG_VERSION"));
        eprintln!("usage:");
        eprintln!("  burxt build   <file.bx> [link args...]   compile to a native executable");
        eprintln!("  burxt run     <file.bx> [link args...]   compile then run");
        eprintln!("  burxt emit-ir <file.bx>                  print LLVM IR");
        eprintln!("  burxt layout  <file.bx>                  print struct layouts");
        eprintln!();
        eprintln!("Arguments after the source file go to the linker unchanged,");
        eprintln!("e.g. `burxt run pay.bx cside.o -lm` to link the C you call.");
        std::process::exit(2);
    }
    let cmd = &args[1];
    let path = &args[2];
    let link_args = &args[3..];

    if let Err(e) = run(cmd, path, link_args) {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}

fn run(cmd: &str, path: &str, link_args: &[String]) -> Result<(), String> {
    let src = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {}", path, e))?;

    // ---- front end (backend-independent) ----
    let tokens = lexer::Lexer::new(&src).tokenize()?;
    let program = parser::Parser::new(tokens).parse_program()?;
    let typed = typeck::TypeChecker::new().check_program(&program)?;

    // ---- back end (LLVM) ----
    let ctx = Context::create();
    let stem = Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("module");
    let mut cg = codegen::CodeGen::new(&ctx, stem);
    cg.compile(&typed)?;

    match cmd {
        "layout" => {
            print!("{}", cg.layout_report(&typed));
            Ok(())
        }
        "emit-ir" => {
            let ir_path = format!("{}.ll", stem);
            cg.write_ir(&ir_path)?;
            let ir = std::fs::read_to_string(&ir_path).map_err(|e| e.to_string())?;
            println!("{}", ir);
            Ok(())
        }
        "build" | "run" => {
            let obj = format!("{}.o", stem);
            cg.write_object(&obj)?;

            // link with the system C compiler (for printf + crt startup), plus
            // whatever the caller needs for the C it declared.
            let exe = format!("./{}", stem);
            let status = Command::new("cc")
                .args([obj.as_str(), "-o", stem])
                .args(link_args)
                .status()
                .map_err(|e| format!("failed to invoke cc: {}", e))?;
            if !status.success() {
                return Err("linking failed".to_string());
            }
            let _ = std::fs::remove_file(&obj);
            eprintln!("compiled {} -> {}", path, exe);

            if cmd == "run" {
                let status = Command::new(&exe)
                    .status()
                    .map_err(|e| format!("failed to run {}: {}", exe, e))?;
                std::process::exit(status.code().unwrap_or(1));
            }
            Ok(())
        }
        other => Err(format!("unknown command: {}", other)),
    }
}
