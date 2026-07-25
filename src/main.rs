//! The `burxt` compiler driver.
//!
//! Usage:
//!   burxt build <file.bx>      compile to a native executable (./<file>)
//!   burxt run   <file.bx>      compile, then run it
//!   burxt emit-ir <file.bx>    print the LLVM IR (for the curious)

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
        eprintln!("  burxt build   <file.bx>   compile to a native executable");
        eprintln!("  burxt run     <file.bx>   compile then run");
        eprintln!("  burxt emit-ir <file.bx>   print LLVM IR");
        eprintln!("  burxt layout  <file.bx>   print struct layouts (size/align/offsets)");
        std::process::exit(2);
    }
    let cmd = &args[1];
    let path = &args[2];

    if let Err(e) = run(cmd, path) {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}

fn run(cmd: &str, path: &str) -> Result<(), String> {
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

            // link with the system C compiler (for printf + crt startup)
            let exe = format!("./{}", stem);
            let status = Command::new("cc")
                .args([&obj, "-o", stem])
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
