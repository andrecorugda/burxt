//! The `burxt` compiler driver.
//!
//! Usage:
//!   burxt lsp                              language server over stdio
//!   burxt check <file.bx>                  parse and typecheck only, no codegen
//!   burxt check -                          ... reading the program from stdin
//!   burxt build <file.bx> [link args...]   compile to a native executable
//!   burxt run   <file.bx> [link args...]   compile, then run it
//!   burxt emit-ir <file.bx>                print the LLVM IR (for the curious)
//!
//! Anything after the source file is handed to the system linker unchanged
//! (`cside.o`, `-lm`, `-L/opt/lib -lfoo`). An `extern fn` declaration is only
//! half of an FFI: the other half is a real object to link against, and Burxt
//! delegates linking to system tools rather than owning it.

mod ast;
mod diag;
mod json;
mod lsp;
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

    // `lsp` takes no file: the editor sends the buffers. Handled before the
    // usage check for that reason.
    if args.len() == 2 && args[1] == "lsp" {
        if let Err(e) = lsp::serve() {
            eprintln!("burxt lsp: {}", e);
            std::process::exit(1);
        }
        return;
    }

    if args.len() < 3 {
        eprintln!("burxt {} — the Burxt compiler", env!("CARGO_PKG_VERSION"));
        eprintln!("usage:");
        eprintln!("  burxt check   <file.bx>                  parse and typecheck only");
        eprintln!("                <file.bx> --json         ... as JSON, for editors and CI");
        eprintln!("                -                        ... reading the program from stdin");
        eprintln!("  burxt lsp                                language server over stdio");
        eprintln!("  burxt build   <file.bx> [link args...]   compile to a native executable");
        eprintln!("  burxt run     <file.bx> [link args...]   compile then run");
        eprintln!("  burxt emit-ir <file.bx>                  print LLVM IR");
        eprintln!("  burxt layout  <file.bx>                  print struct layouts");
        eprintln!();
        eprintln!("  -o <path>     where to write the executable (default ./<name>)");
        eprintln!();
        eprintln!("Arguments after the source file go to the linker unchanged,");
        eprintln!("e.g. `burxt run pay.bx cside.o -lm` to link the C you call.");
        std::process::exit(2);
    }
    let cmd = &args[1];
    let path = &args[2];
    let rest = &args[3..];
    // `--json` makes diagnostics machine-readable: one JSON object per line, for
    // editors and CI. It is not passed on to the linker.
    let json = rest.iter().any(|a| a == "--json");
    // `-o <path>` says where the executable goes. Without it the compiler writes into
    // the working directory, which is convenient for one program and a litter of
    // extensionless binaries for fifty — the repository root learned this the hard way.
    let mut out: Option<String> = None;
    let mut link_args: Vec<String> = Vec::new();
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--json" => {}
            "-o" => {
                if i + 1 >= rest.len() {
                    eprintln!("error: -o needs a path after it");
                    std::process::exit(2);
                }
                out = Some(rest[i + 1].clone());
                i += 1;
            }
            other => link_args.push(other.to_string()),
        }
        i += 1;
    }

    if let Err(e) = run(cmd, path, &link_args, json, out.as_deref()) {
        match e {
            // Diagnostics know where they are, so they can be shown properly —
            // all of them, in the order a reader meets them.
            Failure::At(ds, src) => {
                let total = ds.len();
                for (i, d) in ds.iter().enumerate() {
                    if json {
                        println!("{}", diag::to_json(path, &src, d));
                    } else {
                        if i > 0 {
                            eprintln!();
                        }
                        eprint!("{}", diag::render(path, &src, d));
                    }
                }
                if !json && total > 1 {
                    eprintln!("\n{} errors", total);
                }
            }
            // Something with no position: a missing file, a failed link.
            Failure::Plain(message) => {
                if json {
                    println!(
                        "{{\"file\":{},\"severity\":\"error\",\"message\":{}}}",
                        diag::json_string(path),
                        diag::json_string(&message)
                    );
                } else {
                    eprintln!("error: {}", message);
                }
            }
        }
        std::process::exit(1);
    }
}

/// A failure that either knows where it happened or does not. Keeping the two
/// apart means the position is never invented — a link error has no line.
enum Failure {
    At(Vec<diag::Diagnostic>, String),
    Plain(String),
}

impl From<String> for Failure {
    fn from(message: String) -> Self {
        Failure::Plain(message)
    }
}

fn run(
    cmd: &str,
    path: &str,
    link_args: &[String],
    json: bool,
    out: Option<&str>,
) -> Result<(), Failure> {
    // `-` means "the program is on stdin": what an editor has in its buffer is
    // not what is on disk, and checking the file would report yesterday's errors.
    // Only `check` accepts it — there is no sensible name for the executable
    // otherwise.
    let src = if path == "-" {
        if cmd != "check" {
            return Err(format!(
                "`{}` needs a file: reading the program from stdin only makes sense \
                 for `check`, since there would be no name for the output.",
                cmd
            )
            .into());
        }
        use std::io::Read;
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .map_err(|e| format!("cannot read stdin: {}", e))?;
        text
    } else {
        std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {}", path, e))?
    };

    // ---- front end (backend-independent) ----
    // Every front-end failure carries a span, so it can be rendered with the
    // offending line and a caret under it.
    // The lexer and parser stop at the first problem (recovering a token stream
    // is its own design question); the typechecker reports everything it finds.
    let one = |d: diag::Diagnostic| Failure::At(vec![d], src.clone());
    let all = |ds: Vec<diag::Diagnostic>| Failure::At(ds, src.clone());
    let tokens = lexer::Lexer::new(&src).tokenize().map_err(one)?;
    let program = parser::Parser::with_source(tokens, &src).parse().map_err(one)?;
    let typed = typeck::TypeChecker::new().check(&program).map_err(all)?;

    // `check` is the front end and nothing more: no LLVM context, no object
    // file, no linker. This is what an editor or a CI gate calls, so it must
    // stay the cheapest way to ask "is this program legal?".
    if cmd == "check" {
        // Silence on success in JSON mode: no diagnostics IS the result.
        if !json {
            eprintln!("{}: no errors", path);
        }
        return Ok(());
    }

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
            // The text goes to stdout; the file is an intermediate, so it is written
            // where intermediates belong unless a path was asked for. Writing it beside
            // the source is how `t.ll` and `emitted.ll` ended up in this repository.
            let ir_path = match out {
                Some(p) => p.to_string(),
                None => std::env::temp_dir()
                    .join(format!("burxt-{}-{}.ll", std::process::id(), stem))
                    .to_string_lossy()
                    .into_owned(),
            };
            cg.write_ir(&ir_path)?;
            let ir = std::fs::read_to_string(&ir_path).map_err(|e| e.to_string())?;
            if out.is_none() {
                let _ = std::fs::remove_file(&ir_path);
            }
            println!("{}", ir);
            Ok(())
        }
        "build" | "run" => {
            // The object is an intermediate, and it goes where intermediates belong: NOT
            // into the working directory, where two builds running at once collide on the
            // same name — which is exactly what happened when two tests built the
            // self-hosted compiler in parallel. Unique per process, and removed after.
            let obj = std::env::temp_dir()
                .join(format!("burxt-{}-{}.o", std::process::id(), stem))
                .to_string_lossy()
                .into_owned();
            cg.write_object(&obj)?;

            // link with the system C compiler (for printf + crt startup), plus
            // whatever the caller needs for the C it declared.
            let exe = match out {
                Some(p) => p.to_string(),
                None => format!("./{}", stem),
            };
            let status = Command::new("cc")
                .args([obj.as_str(), "-o", exe.as_str()])
                .args(link_args)
                .status()
                .map_err(|e| format!("failed to invoke cc: {}", e))?;
            if !status.success() {
                return Err("linking failed".to_string().into());
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
        other => Err(format!("unknown command: {}", other).into()),
    }
}
