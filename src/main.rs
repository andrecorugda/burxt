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
        eprintln!("  burxt layout  <file.bx>                  print record layouts");
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
            Failure::At(ds, src, files) => {
                let total = ds.len();
                for (i, d) in ds.iter().enumerate() {
                    // An error in a used module names THAT module, and the line number is
                    // the one in it — not an offset into a buffer nobody wrote. The span
                    // is a plain byte range; the map is what turns it back into a place.
                    let mut shown_path: &str = path;
                    let mut shown_src = src.as_str();
                    let mut shown = d.clone();
                    // Always through the map, even for one file: the buffer carries a
                    // separator the file does not, so rendering the buffer would count a
                    // line the programmer never wrote.
                    {
                        if let Some((file, local)) = locate_file(&files, d.span.start as usize) {
                            shown_path = file.path.as_str();
                            shown_src = &src[file.start..file.start + file.len];
                            shown = diag::Diagnostic::new(
                                d.message.clone(),
                                diag::Span::new(
                                    local,
                                    local + (d.span.end - d.span.start) as usize,
                                ),
                            );
                        }
                    }
                    if json {
                        println!("{}", diag::to_json(shown_path, shown_src, &shown));
                    } else {
                        if i > 0 {
                            eprintln!();
                        }
                        eprint!("{}", diag::render(shown_path, shown_src, &shown));
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
    At(Vec<diag::Diagnostic>, String, Vec<SourceFile>),
    Plain(String),
}

impl From<String> for Failure {
    fn from(message: String) -> Self {
        Failure::Plain(message)
    }
}

/// One file inside the concatenated buffer: where it starts, how long it is, and what to
/// call it in a diagnostic.
#[derive(Clone)]
struct SourceFile {
    path: String,
    start: usize,
    len: usize,
}

/// Which file an offset fell in, and how far into it — so an error in a used module names
/// that module rather than an offset into a buffer the programmer never saw.
fn locate_file<'a>(files: &'a [SourceFile], offset: usize) -> Option<(&'a SourceFile, usize)> {
    // Inclusive at the end: a diagnostic about the END of a file — "expected `;`, found the
    // end of the file" — points one past its last byte, and that position belongs to the
    // file it ended rather than to the separator after it.
    files
        .iter()
        .find(|f| offset >= f.start && offset <= f.start + f.len)
        .map(|f| (f, offset - f.start))
}

/// Read a program and everything it `use`s, into ONE buffer with a map back to the files.
///
/// The imports are resolved as a pre-pass over the text rather than as a parser feature,
/// and the `use` lines are BLANKED OUT — replaced by spaces of the same length — so every
/// byte offset in what follows is unchanged and the lexer, parser and typechecker need to
/// know nothing about modules at all. See spec/M6-MODULES.md §1.5.
///
/// Imports come first in a file, before any other item. That is what makes the pre-pass
/// safe: it stops at the first line that is not blank, a comment, or a `use`, so a `use`
/// appearing later inside a string or a comment is never mistaken for one.
fn load_program(path: &str) -> Result<(String, Vec<SourceFile>), String> {
    let mut buffer = String::new();
    let mut files: Vec<SourceFile> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    load_into(path, &mut buffer, &mut files, &mut seen, true)?;
    Ok((buffer, files))
}

fn load_into(
    path: &str,
    buffer: &mut String,
    files: &mut Vec<SourceFile>,
    seen: &mut Vec<String>,
    is_root: bool,
) -> Result<(), String> {
    let canonical = std::fs::canonicalize(path)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.to_string());
    if seen.contains(&canonical) {
        // Used twice — directly and through another module — is compiled once. Cycles
        // work for the same reason, since declarations are collected before any body is
        // checked and nothing needs a forward declaration.
        return Ok(());
    }
    seen.push(canonical);

    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {}", path, e))?;
    let (blanked, imports) = strip_imports(&text);

    // Dependencies first, so their declarations precede the file that asked for them.
    // Nothing requires this — the checker collects declarations in a pass of its own — but
    // a buffer that reads in dependency order is a buffer a person can debug.
    let here = std::path::Path::new(path).parent().map(|p| p.to_path_buf());
    for import in &imports {
        let resolved = match &here {
            Some(dir) => dir.join(import).to_string_lossy().into_owned(),
            None => import.clone(),
        };
        load_into(&resolved, buffer, files, seen, false).map_err(|e| {
            format!("{}\n  ...used by {}", e, path)
        })?;
    }

    // A newline BETWEEN files, never after the last one: it exists so the final token of
    // one file cannot run into the first of the next, and after the last file there is
    // nothing to run into. Appending it unconditionally moved the end-of-file position one
    // byte past the program, and "expected `;`, found the end of the file" started
    // reporting a line the programmer had not written.
    if !buffer.is_empty() {
        buffer.push('\n');
    }
    let start = buffer.len();
    buffer.push_str(&blanked);
    files.push(SourceFile { path: path.to_string(), start, len: blanked.len() });
    let _ = is_root;
    Ok(())
}

/// Find the leading `use "path";` lines, and answer the text with them blanked out.
fn strip_imports(text: &str) -> (String, Vec<String>) {
    let mut imports = Vec::new();
    let mut out = String::with_capacity(text.len());
    let mut in_header = true;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        if in_header {
            if trimmed.is_empty() || trimmed.starts_with("//") {
                out.push_str(line);
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("use ") {
                let quoted = rest.trim().trim_end_matches(';').trim();
                if quoted.len() >= 2 && quoted.starts_with('"') && quoted.ends_with('"') {
                    imports.push(quoted[1..quoted.len() - 1].to_string());
                    // Blanked, not removed: every offset after this line stays exactly
                    // where it was, which is why no span anywhere needs adjusting.
                    for ch in line.chars() {
                        out.push(if ch == '\n' { '\n' } else { ' ' });
                    }
                    continue;
                }
            }
            in_header = false;
        }
        out.push_str(line);
    }
    (out, imports)
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
    // Which file each byte came from, for diagnostics. A single-file program has one
    // entry, which costs nothing and keeps one path through the renderer.
    let mut files: Vec<SourceFile> = Vec::new();
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
        files.push(SourceFile { path: path.to_string(), start: 0, len: text.len() });
        text
    } else {
        // The program AND everything it uses, in one buffer with a map back to the files.
        let (buffer, loaded) = load_program(path)?;
        files = loaded;
        buffer
    };

    // ---- front end (backend-independent) ----
    // Every front-end failure carries a span, so it can be rendered with the
    // offending line and a caret under it.
    // The lexer and parser stop at the first problem (recovering a token stream
    // is its own design question); the typechecker reports everything it finds.
    let one = |d: diag::Diagnostic| Failure::At(vec![d], src.clone(), files.clone());
    let all = |ds: Vec<diag::Diagnostic>| Failure::At(ds, src.clone(), files.clone());
    let tokens = lexer::Lexer::new(&src).tokenize().map_err(one)?;
    let program = parser::Parser::with_source(tokens, &src).parse().map_err(one)?;

    // A module holds DECLARATIONS, not statements: a file that runs when it is used is the
    // import side-effect problem, and every language that allows it grows a convention
    // against it. The file being compiled is exempt — statements are what make it the
    // program. See spec/M6-MODULES.md §1.3.
    if files.len() > 1 {
        let root = files.last().expect("the program is the last file loaded");
        for stmt in &program.stmts {
            let at = stmt.span.start as usize;
            if at >= root.start {
                continue;
            }
            if let Some((file, _)) = locate_file(&files, at) {
                return Err(Failure::At(
                    vec![diag::Diagnostic::new(
                        format!(
                            "a module holds declarations, not statements: this would run \
                             when `{}` was used, and a `use` is not a call",
                            file.path
                        ),
                        stmt.span,
                    )],
                    src.clone(),
                    files.clone(),
                ));
            }
        }
    }
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
