//! The `burxt` compiler driver.
//!
//! Usage:
//!   burxt lsp                              language server over stdio
//!   burxt check <file.bx>                  parse and typecheck only, no codegen
//!   burxt check -                          ... reading the program from stdin
//!   burxt build <file.bx> [link arguments...]   compile to a native executable
//!   burxt run   <file.bx> [link arguments...]   compile, then run it
//!   burxt emit-ir <file.bx>                print the LLVM IR (for the curious)
//!
//! Anything after the source file is handed to the system linker unchanged
//! (`cside.o`, `-lm`, `-L/opt/lib -lfoo`). An `extern fn` declaration is only
//! half of an FFI: the other half is a real object to link against, and Burxt
//! delegates linking to system tools rather than owning it.

mod effects;
mod ast;
mod manifest;
mod diag;
mod json;
mod lsp;
mod lexer;
mod parser;
mod typeck;
mod codegen;
mod review;
mod schema;

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
    let arguments: Vec<String> = std::env::args().collect();

    // `lsp` takes no file: the editor sends the buffers. Handled before the
    // usage check for that reason.
    // C2. `burxt fetch` — the ONLY place this compiler touches the network, and only when asked.
    // Handled here, beside `lsp`, because it takes no source file: it works on the package the
    // current directory is in.
    if arguments.len() == 2 && arguments[1] == "fetch" {
        match manifest::Manifest::discover(Path::new(".")) {
            Err(e) => {
                eprintln!("burxt fetch: {}", e);
                std::process::exit(1);
            }
            Ok(None) => {
                eprintln!(
                    "burxt fetch: no `{}` here or in any directory above. A package declares what \
                     it depends on; without one there is nothing to fetch.",
                    manifest::MANIFEST_NAME
                );
                std::process::exit(1);
            }
            Ok(Some(package)) => match manifest::fetch(&package) {
                Err(e) => {
                    eprintln!("burxt fetch: {}", e);
                    std::process::exit(1);
                }
                Ok(report) => {
                    if report.is_empty() {
                        println!("nothing to fetch — every dependency is a local directory");
                    } else {
                        print!("{}", report);
                        println!("wrote {}", manifest::LOCK_NAME);
                    }
                    return;
                }
            },
        }
    }

    if arguments.len() == 2 && arguments[1] == "lsp" {
        if let Err(e) = lsp::serve() {
            eprintln!("burxt lsp: {}", e);
            std::process::exit(1);
        }
        return;
    }

    if arguments.len() < 3 {
        eprintln!("burxt {} — the Burxt compiler", env!("CARGO_PKG_VERSION"));
        eprintln!("usage:");
        eprintln!("  burxt check   <file.bx>                  parse and typecheck only");
        eprintln!("                <file.bx> --json         ... as JSON, for editors and CI");
        eprintln!("                -                        ... reading the program from stdin");
        eprintln!("  burxt lsp                                language server over stdio");
        eprintln!("  burxt fetch                              get the dependencies, write burxt.lock");
        eprintln!("  burxt build   <file.bx> [link args...]   compile to a native executable");
        eprintln!("                <file.bx> --target <triple> ... an object for another machine");
        eprintln!("  burxt run     <file.bx> [link args...]   compile then run");
        eprintln!("  burxt emit-ir <file.bx> [--target ...]   print LLVM IR");
        eprintln!("  burxt layout  <file.bx>                  print class layouts");
        eprintln!("  burxt explain memory <file.bx>           what each function builds");
        // One `eprintln!` per row. These two were a single call holding a raw newline, which
        // read the same on a terminal and broke the scrape: `scripts/site-reference.py` matches
        // `eprintln!("(  burxt [^"]*)")`, so `[^"]*` ran through the newline and produced one
        // reference row whose "what it does" column was the whole `mcp-schema` line. The
        // published page said so for months. A generated document is only as honest as the
        // shape it is generated from.
        eprintln!("  burxt review  <old.bx> <new.bx>          what changed about what it PROMISES");
        eprintln!("  burxt mcp-schema <file.bx>               the MCP tool manifest, from the preconditions");
        eprintln!();
        eprintln!("  -o <path>     where to write the executable (default ./<name>)");
        eprintln!("  -g            emit DWARF debug info: a line table, and every parameter");
        eprintln!("                and `let` with its name, type and stack slot. A debugger");
        eprintln!("                can then stop on a line and read a local — which is the");
        eprintln!("                alternative to inserting a `print`, and a `print` MOVES THE");
        eprintln!("                STACK and can change the answer. Off by default: debug info");
        eprintln!("                carries absolute paths and a producer string, so it would");
        eprintln!("                make the emitted IR differ between machines.");
        eprintln!("  -O0           do not optimise. Independent of -g on purpose: -O2 -g is");
        eprintln!("                for a crash report from the field, -O0 -g is for stepping.");
        eprintln!("                Use both to follow a program statement by statement.");
        eprintln!("  --target <triple>  build for another machine, e.g. aarch64-apple-darwin.");
        eprintln!("                Emits an OBJECT and stops: linking needs that target\'s libc");
        eprintln!("                and linker, so it is left to that target\'s toolchain. The");
        eprintln!("                emitted IR is identical for every target, which is what makes");
        eprintln!("                the decimal answers identical too.");
        eprintln!();
        eprintln!("Arguments after the source file go to the linker unchanged,");
        eprintln!("e.g. `burxt run pay.bx cside.o -lm` to link the C you call.");
        std::process::exit(2);
    }
    let cmd = &arguments[1];
    // `burxt explain memory <file>` — the subject is written out, per M14 §7's spelling, because
    // memory is not the only thing a program could be asked to explain and a bare `explain` would
    // have to be guessed at later. Refused rather than defaulted: guessing which subject was meant
    // is the shape of thing this language does not do.
    let mut arguments = arguments.clone();
    if cmd == "explain" {
        if arguments.len() < 4 || arguments[2] != "memory" {
            eprintln!("usage: burxt explain memory <file.bx>");
            eprintln!();
            eprintln!("Answers what each function builds, from the same inference `allocates` is");
            eprintln!("derived from. `memory` is written out because it is not the only thing a");
            eprintln!("program could be asked to explain.");
            std::process::exit(2);
        }
        arguments.remove(2);
    }

    // Flags may come BEFORE the source file: `burxt build -O0 -g prog.bx -o prog` is how
    // anyone who has used a C compiler will write it, and until C1 it produced
    // "cannot read -O0", which blames the user for the compiler's parser. The file is
    // moved to slot 2 and the flags left in their order after it, so everything below —
    // which has always assumed `arguments[2]` is the path — is unchanged.
    //
    // `-o` and `--target` take an operand, so their next argument is skipped rather than
    // mistaken for the file. A bare `-` is the stdin spelling, not a flag.
    {
        let mut i = 2;
        while i < arguments.len() {
            let a = arguments[i].clone();
            if a == "-o" || a == "--target" {
                i += 2;
            } else if a.starts_with('-') && a != "-" {
                i += 1;
            } else {
                break;
            }
        }
        if i > 2 && i < arguments.len() {
            let file = arguments.remove(i);
            arguments.insert(2, file);
        }
    }

    let cmd = &arguments[1];
    let path = &arguments[2];
    // `review` is the odd one out: two paths, no output file, no linking. It answers what changed
    // about what the program PROMISES — signatures, contracts, privacy — rather than what changed
    // in the text. Handled here, before the flags every other command shares.
    if cmd == "review" {
        // C2. `--semver` answers a DIFFERENT question from the default, which is why it is a mode
        // rather than a replacement: the default asks "did this promise less" (a reviewer of an
        // agent's diff), `--semver` asks "can a consumer upgrade without editing their code".
        // Those disagree — a stricter `requires` promises more and breaks callers.
        let semver = arguments.iter().any(|a| a == "--semver");
        let required = arguments
            .iter()
            .position(|a| a == "--require")
            .and_then(|i| arguments.get(i + 1))
            .cloned();
        let files: Vec<String> = arguments[2..]
            .iter()
            .filter(|a| !a.starts_with("--"))
            .cloned()
            .collect();
        // `--require` takes an operand, and that operand is not a file.
        let files: Vec<String> = match &required {
            Some(word) => files.into_iter().filter(|f| f != word).collect(),
            None => files,
        };
        if files.len() < 2 {
            eprintln!("usage: burxt review <old.bx> <new.bx>");
            eprintln!("       burxt review --semver <old.bx> <new.bx> [--require patch|minor|major]");
            eprintln!();
            eprintln!("Compares what two versions of a program GUARANTEE. Exits 1 if any promise");
            eprintln!("was weakened, so it works as a gate without parsing the output.");
            eprintln!();
            eprintln!("--semver  answers the smallest version bump this change may ship under.");
            eprintln!("          A stricter `requires` is a MAJOR — it promises more and breaks");
            eprintln!("          callers. A public function that gains an effect is a major too,");
            eprintln!("          because effects propagate and every caller must declare it.");
            eprintln!("          It reads the interface, not the behaviour: it can prove a change");
            eprintln!("          is AT LEAST a major, never that an upgrade is safe.");
            eprintln!("--require exits 1 when the bump you claim is smaller than the one demanded.");
            std::process::exit(2);
        }
        if semver {
            match review::semver(&files[0], &files[1], required.as_deref()) {
                Ok(code) => std::process::exit(code),
                Err(message) => {
                    eprintln!("error: {}", message);
                    std::process::exit(2);
                }
            }
        }
        match review::review(&files[0], &files[1]) {
            Ok(code) => std::process::exit(code),
            Err(message) => {
                eprintln!("error: {}", message);
                std::process::exit(2);
            }
        }
    }
    // §Q1. What can this program reach, and where did each reach enter? See src/rust-compiler/effects.rs —
    // it reads declarations the checker already REFUSED to let be incomplete, which is why the
    // answer is what the program CAN do rather than what one run happened to do.
    if cmd == "effects" {
        let json = arguments.iter().any(|a| a == "--json");
        let allow = arguments
            .iter()
            .position(|a| a == "--allow")
            .and_then(|i| arguments.get(i + 1))
            .cloned();
        match effects::report(path, allow.as_deref(), json) {
            Ok(code) => std::process::exit(code),
            Err(message) => {
                eprintln!("error: {}", message);
                std::process::exit(2);
            }
        }
    }
    if cmd == "mcp-schema" {
        // The manifest for an MCP server, derived from the preconditions the tools already carry.
        // See src/rust-compiler/schema.rs — the schema and the check are one sentence, so they cannot drift.
        match schema::emit(path) {
            Ok(code) => std::process::exit(code),
            Err(message) => {
                eprintln!("error: {}", message);
                std::process::exit(2);
            }
        }
    }
    let rest = &arguments[3..];
    // `--json` makes diagnostics machine-readable: one JSON object per line, for
    // editors and CI. It is not passed on to the linker.
    let json = rest.iter().any(|a| a == "--json");
    // `-o <path>` says where the executable goes. Without it the compiler writes into
    // the working directory, which is convenient for one program and a litter of
    // extensionless binaries for fifty — the repository root learned this the hard way.
    let mut out: Option<String> = None;
    // `--target <triple>` says which machine the code is FOR. Absent means the host, which is what
    // every build did before v0.0.197.
    //
    // Linking is deliberately not attempted for a foreign target: it needs that target's libc,
    // sysroot and linker, and owning that is how a compiler grows a second job it is bad at. The
    // roadmap's decision (M3) is to delegate linking and own only the triple and the object — so a
    // cross build emits a `.o` and says so, and the caller links it with the toolchain that already
    // knows about its platform.
    let mut target: Option<String> = None;
    // C1. Two flags, kept INDEPENDENT, and the spelling is `-g` and `-O0` because those
    // are the two every C toolchain has spelled the same way for forty years — a person
    // debugging is already typing them from muscle memory.
    //
    // Why they are not one flag. `-g` says "describe this program to a debugger" and
    // `-O0` says "do not rearrange it". They are genuinely different requests: `-O2 -g`
    // is what you want for a profiler or a crash report from the field, and `-O0` alone
    // is what you want when a miscompilation is suspected. Folding either into the other
    // would be the compiler guessing which was meant, which is the shape of thing this
    // language refuses (see `explain memory` above, refused rather than defaulted).
    //
    // What IS done is to say so once: `-g` without `-O0` warns, because a line table over
    // optimised code is honest about instructions and misleading about statements, and a
    // reader who did not know that would blame the debugger. A warning is neither
    // guessing nor silence.
    let mut debug_info = false;
    let mut optimise = true;
    let mut link_args: Vec<String> = Vec::new();
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--json" => {}
            "-g" => debug_info = true,
            "-O0" => optimise = false,
            // Named explicitly so `-O2` is a way to say "the default, and I mean it"
            // next to a `-g`, rather than an unknown flag handed to the linker.
            "-O2" => optimise = true,
            "--target" => {
                if i + 1 >= rest.len() {
                    eprintln!("error: --target needs a triple after it, e.g. aarch64-unknown-linux-gnu");
                    std::process::exit(2);
                }
                target = Some(rest[i + 1].clone());
                i += 1;
            }
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

    if debug_info && optimise && cmd != "check" {
        eprintln!(
            "warning: -g without -O0. The line table will be correct about instructions and \
             misleading about statements, because optimisation moves, merges and deletes them. \
             Add -O0 for a build a debugger can follow."
        );
    }

    if let Err(e) = run(
        cmd,
        path,
        &link_args,
        json,
        out.as_deref(),
        target.as_deref(),
        debug_info,
        optimise,
    ) {
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
pub struct SourceFile {
    pub path: String,
    pub start: usize,
    pub len: usize,
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
/// know nothing about modules at all. See spec/1.0/M6-MODULES.md §1.5.
///
/// Imports come first in a file, before any other item. That is what makes the pre-pass
/// safe: it stops at the first line that is not blank, a comment, or a `use`, so a `use`
/// appearing later inside a string or a comment is never mistaken for one.
pub fn load_program(path: &str) -> Result<(String, Vec<SourceFile>), String> {
    let mut buffer = String::new();
    let mut files: Vec<SourceFile> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    // C2. The manifest is found ONCE, from the root source file, and every package import in the
    // whole program is answered from it. Not once per file: a dependency of a dependency is
    // resolved by ITS own manifest, and mixing the two would let a nested package quietly rebind a
    // name its parent had already bound.
    let found = manifest::Manifest::discover(std::path::Path::new(path))?;
    load_into(path, &mut buffer, &mut files, &mut seen, true, found.as_ref())?;
    Ok((buffer, files))
}


/// Where the standard library is, for `use "std/…"`. C2b.
///
/// **A package cannot reach the standard library by a relative path, and that is what makes a
/// framework a separate technology rather than a folder in this repository.** `use` resolves
/// relative to the importing file, so a dependency asking for `lib/html.bx` looks for `lib/` under
/// *itself*. A release installs the library to `$PREFIX/lib/burxt/` and the only way to name it was
/// an absolute path — one machine's layout, baked into something other people install.
///
/// Laravel works because PHP has an include path; React works because Node resolves modules. This
/// is that, and it is the smallest version of it: one prefix, three roots, checked in order.
///
///   1. `BURXT_LIB`, so a test or an unusual install can say where without editing anything
///   2. `$PREFIX/lib/burxt/` — where `scripts/install.sh` and `scripts/release.sh` put it
///   3. `lib/` beside the compiler's own source, so the repository builds without installing
///
/// **The roots are reported when the import misses**, because a path that depends on the
/// environment has to say which environment answered — that is the same objection this design
/// raises against a silent fallback, applied to itself.
fn stdlib_roots() -> Vec<std::path::PathBuf> {
    let mut roots = Vec::new();
    if let Ok(from_env) = std::env::var("BURXT_LIB") {
        if !from_env.is_empty() {
            roots.push(std::path::PathBuf::from(from_env));
        }
    }
    // **A compiler prefers its OWN library, and the order is the whole of that.**
    //
    // `CARGO_MANIFEST_DIR` is compile-time, so this is the tree the binary was built from. On a
    // released binary that directory does not exist on the user's machine and the search falls
    // through to the install prefix below — so a release behaves exactly as if this entry were
    // absent. On a source build it is the tree you are working in.
    //
    // It was the other way round first, and that ordering carries a defect invisible on a machine
    // with nothing installed: with Burxt ALSO installed, a compiler built from this repo and run
    // in this repo resolved `use "std/html.bx"` to the INSTALLED library — a different file. A
    // test asserting a `pure` view compiles would fail for a reason nowhere in the diff, and a
    // weaker one would pass against the wrong library. This repository has that lesson already:
    // a test can certify the wrong artifact.
    //
    // It is also what the decision NOT to version-pin the standard library rests on — "the
    // compiler and the library ship in one tarball, so `burxt --version` already pins it exactly".
    // That is only true if a compiler uses its own library.
    roots.push(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("lib"));
    roots.push(std::path::PathBuf::from("/usr/local/lib/burxt"));
    roots
}

fn load_into(
    path: &str,
    buffer: &mut String,
    files: &mut Vec<SourceFile>,
    seen: &mut Vec<String>,
    is_root: bool,
    package: Option<&manifest::Manifest>,
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
        let relative = match &here {
            Some(dir) => dir.join(import),
            None => std::path::PathBuf::from(import),
        };
        // C2b. `use "std/…"` is the STANDARD LIBRARY, wherever it is installed. An explicit
        // prefix rather than a fallback that tries the library when a relative path misses:
        // a fallback would make resolution depend on whether a file happens to exist, so the
        // same program would resolve differently on two machines — which is the objection the
        // ambiguity refusal below already makes about dependencies, one layer out.
        if let Some(rest) = import.strip_prefix("std/") {
            // A directory named `std/` beside the importing file means the author wrote one and
            // meant it. Refuse rather than pick, exactly as for a dependency.
            if relative.exists() {
                return Err(format!(
                    "`use \"{}\"` in {} could mean two things: the standard library, or the \
                     file at {}. Rename one of them — `std/` is reserved for the library that \
                     ships with the compiler.",
                    import, path, relative.display()
                ));
            }
            let roots = stdlib_roots();
            match roots.iter().map(|r| r.join(rest)).find(|c| c.exists()) {
                Some(found) => {
                    load_into(&found.to_string_lossy(), buffer, files, seen, false, package)
                        .map_err(|e| format!("{}\n  ...used by {}", e, path))?;
                    continue;
                }
                // **Two different failures, and saying the wrong one sends a reader to the
                // wrong problem.** A library that is not installed and a module that does not
                // exist look identical from here — one is fixed by installing, the other by
                // correcting a name — so the message says which by asking whether any root is
                // a directory at all.
                None => {
                    let present: Vec<&std::path::PathBuf> =
                        roots.iter().filter(|r| r.is_dir()).collect();
                    return Err(if present.is_empty() {
                        format!(
                            "`use \"{}\"` — no standard library found. Looked in:\n{}\n\
                             Set BURXT_LIB to the directory holding the library's .bx files.",
                            import,
                            roots.iter()
                                .filter(|r| r.parent().map_or(true, |p| p.exists()))
                                .map(|r| format!("    {}", r.display()))
                                .collect::<Vec<_>>()
                                .join("\n")
                        )
                    } else {
                        format!(
                            "`use \"{}\"` — the standard library has no `{}`. Looked in:\n{}",
                            import,
                            rest,
                            present.iter()
                                .map(|r| format!("    {}", r.join(rest).display()))
                                .collect::<Vec<_>>()
                                .join("\n")
                        )
                    })
                }
            }
        }

        // C2. An import whose first segment names a declared dependency is a PACKAGE import.
        // Everything else is what it has always been: a path relative to the importing file.
        let from_package = package.and_then(|m| m.resolve_package_import(import));
        let resolved = match from_package {
            // Both readings exist, and picking one silently would make resolution depend on the
            // shape of a directory tree — so the failure would appear on somebody else's machine,
            // with a program that compiled here. Refused where it is written instead.
            Some(_via) if relative.exists() => {
                return Err(format!(
                    "`use \"{}\"` in {} could mean two things: the dependency `{}` declared in \
                     {}, or the file at {}. Rename one of them — a dependency's name is the first \
                     segment of every import that reaches it.",
                    import,
                    path,
                    import.split('/').next().unwrap_or(import),
                    manifest::MANIFEST_NAME,
                    relative.display()
                ))
            }
            // A git dependency that has not been fetched. "cannot read
            // .burxt/packages/…/tax.bx" is true and sends the reader looking for a file they were
            // never meant to create; this names the command instead. Deliberately NOT fetched
            // automatically — a build that reaches the network does different things on different
            // days, which is the opposite of every other guarantee here.
            Some(via) if !via.exists() => {
                let first = import.split('/').next().unwrap_or(import);
                return Err(format!(
                    "`use \"{}\"` needs the dependency `{}`, and it has not been fetched. \
                     Run `burxt fetch`.",
                    import, first
                ));
            }
            Some(via) => via.to_string_lossy().into_owned(),
            None => relative.to_string_lossy().into_owned(),
        };
        load_into(&resolved, buffer, files, seen, false, package).map_err(|e| {
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
pub fn strip_imports(text: &str) -> (String, Vec<String>) {
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
    target: Option<&str>,
    debug_info: bool,
    optimise: bool,
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

/// Hide what a dependency did not declare `public`. C2.
///
/// The boundary is the PACKAGE and not the file, which M6 Decision 5 forces rather than suggests:
/// `use` concatenates every source into one buffer, so no file boundary survives to be private
/// across. What does survive is which directory a file was read from, and a dependency's files all
/// sit under its root.
///
/// **Removed from the program rather than marked and refused later.** A private declaration that is
/// still in the tree is one every later pass has to remember to ignore — the checker, `review`, the
/// language server, `mcp-schema` — and the first one that forgets makes the privacy a suggestion.
/// Dropping it means a use of it fails as an unknown name, through the machinery that already
/// exists, everywhere at once.
///
/// The names are kept so the message can be the right one. "unknown function: `helper`" is true and
/// unhelpful when `helper` is sitting in the dependency the reader is looking at.
fn hide_private_dependencies(
    program: &ast::Program,
    files: &[SourceFile],
    package: Option<&manifest::Manifest>,
) -> std::collections::BTreeMap<String, String> {
    let mut hidden = std::collections::BTreeMap::new();
    let Some(package) = package else {
        return hidden;
    };
    // Which package each file belongs to: the root package, or the dependency whose directory it
    // sits under. Longest match wins, so a dependency vendored inside another one is attributed to
    // the inner package rather than the outer.
    let owner = |offset: usize| -> Option<String> {
        let (file, _) = locate_file(files, offset)?;
        let path = std::fs::canonicalize(&file.path).ok()?;
        let mut best: Option<(usize, String)> = None;
        for (name, dependency) in &package.dependencies {
            let root = match &dependency.source {
                manifest::Source::Path(dir) => package.root.join(dir),
                manifest::Source::Git { url, tag } => package
                    .root
                    .join(".burxt")
                    .join("packages")
                    .join(manifest::cache_key(url, tag)),
            };
            let Ok(root) = std::fs::canonicalize(&root) else { continue };
            if path.starts_with(&root) {
                let depth = root.components().count();
                if best.as_ref().map(|(d, _)| depth > *d).unwrap_or(true) {
                    best = Some((depth, name.clone()));
                }
            }
        }
        best.map(|(_, name)| name)
    };

    let mut note = |name: &str, span: diag::Span, public: bool| {
        if let Some(from) = owner(span.start as usize) {
            if !public {
                hidden.insert(name.to_string(), from);
            }
        }
    };
    for f in &program.fns {
        note(&f.name, f.span, f.public);
    }
    for d in &program.structs {
        note(&d.name, d.span, d.public);
    }
    for d in &program.enums {
        note(&d.name, d.span, d.public);
    }
    for d in &program.interfaces {
        note(&d.name, d.span, d.public);
    }
    hidden
}

/// Which package each byte range of the buffer belongs to. C2.
///
/// Only foreign ranges are listed: an offset that matches nothing is in the root package, which is
/// the common case and the one worth making free.
fn package_ranges(
    files: &[SourceFile],
    package: Option<&manifest::Manifest>,
) -> Vec<(usize, usize, String)> {
    let mut ranges = Vec::new();
    let Some(package) = package else {
        return ranges;
    };
    for file in files {
        let Ok(path) = std::fs::canonicalize(&file.path) else { continue };
        let mut best: Option<(usize, String)> = None;
        for (name, dependency) in &package.dependencies {
            let root = match &dependency.source {
                manifest::Source::Path(dir) => package.root.join(dir),
                manifest::Source::Git { url, tag } => package
                    .root
                    .join(".burxt")
                    .join("packages")
                    .join(manifest::cache_key(url, tag)),
            };
            let Ok(root) = std::fs::canonicalize(&root) else { continue };
            if path.starts_with(&root) {
                let depth = root.components().count();
                if best.as_ref().map(|(d, _)| depth > *d).unwrap_or(true) {
                    best = Some((depth, name.clone()));
                }
            }
        }
        if let Some((_, name)) = best {
            ranges.push((file.start, file.start + file.len, name));
        }
    }
    ranges
}

    // ---- front end (backend-independent) ----
    // Every front-end failure carries a span, so it can be rendered with the
    // offending line and a caret under it.
    // The lexer and parser stop at the first problem (recovering a token stream
    // is its own design question); the typechecker reports everything it finds.
    let one = |d: diag::Diagnostic| Failure::At(vec![d], src.clone(), files.clone());
    let all = |ds: Vec<diag::Diagnostic>| Failure::At(ds, src.clone(), files.clone());
    let tokens = lexer::Lexer::new(&src).tokenize().map_err(one)?;
    let program = parser::Parser::with_source(tokens, &src).parse().map_err(one)?;
    // C2. What a dependency did not declare `public`, and which bytes belong to which package.
    // Both are needed because privacy is a RELATION between the use and the declaration: a helper
    // a package keeps to itself is still perfectly visible to the rest of that package, and an
    // earlier attempt that simply removed such declarations broke the dependency's own code.
    let package = manifest::Manifest::discover(Path::new(path)).map_err(Failure::Plain)?;
    let hidden = hide_private_dependencies(&program, &files, package.as_ref());
    let owners = package_ranges(&files, package.as_ref());

    // A module holds DECLARATIONS, not statements: a file that runs when it is used is the
    // import side-effect problem, and every language that allows it grows a convention
    // against it. The file being compiled is exempt — statements are what make it the
    // program. See spec/1.0/M6-MODULES.md §1.3.
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
    // Kept alive past `check`, because `explain memory` asks it what it inferred — the same
    // question every allocation rule asks, rather than a second pass with its own answer.
    let mut checker = typeck::TypeChecker::new();
    checker.with_packages(hidden, owners);
    let typed = checker.check(&program).map_err(all)?;

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
    if debug_info {
        // Where each function was DECLARED, taken from the UNTYPED tree — the only one
        // that still knows. `TypedFn` carries a body and no position of its own, and the
        // typed tree is not structurally 1:1 with this one anyway (the checker's
        // `place_releases` inserts blocks), so this is a lookup by NAME rather than a
        // parallel walk. A name is unique; a traversal order under a rewriting pass is
        // not, and a line table built on the second kind would be wrong silently.
        //
        // A monomorphised generic has a mangled name that appears here under none of its
        // spellings; codegen falls back to the first statement of its body.
        let mut decls: std::collections::HashMap<String, diag::Span> =
            std::collections::HashMap::new();
        for f in &program.fns {
            decls.insert(f.name.clone(), f.span);
        }
        for m in &program.methods {
            decls.insert(format!("{}.{}", m.receiver, m.name), m.span);
        }
        cg.enable_debug_info(&files, &src, decls, optimise);
    }
    cg.compile(&typed)?;

    match cmd {
        // `burxt explain memory <file>` — M14 §7. A query rather than an annotation: the fact is
        // wanted occasionally, and putting it on every signature forever taught nothing (the word
        // landed on three functions out of three in `examples/pos/receipt.bx`).
        "explain" => {
            print!("{}", checker.memory_report(&program, &typed, &src));
            Ok(())
        }
        "layout" => {
            print!("{}", cg.layout_report(&typed));
            Ok(())
        }
        "emit-ir" => {
            // `--target` here makes the cross-target claim inspectable: the IR a foreign build
            // would compile can be READ, and compared against another target's.
            if let Some(triple) = target {
                cg.retarget(triple)?;
            }
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
            // A CROSS build stops at the object, and that is a decision rather than a shortfall:
            // linking needs the target's libc, sysroot and linker, and owning that is how a compiler
            // grows a second job it is bad at. spec/FAR-HORIZON-ROADMAP.md M3 says delegate linking
            // and own only the triple and the object emission. So the object is named, kept, and the
            // caller links it with the toolchain that already knows its platform.
            if let Some(triple) = target {
                if cmd == "run" {
                    return Err("`run` builds for THIS machine, so it cannot take --target. \
                                Use `build --target` and run the result where it belongs."
                        .to_string()
                        .into());
                }
                let obj = match out {
                    Some(p) => p.to_string(),
                    None => format!("./{}.o", stem),
                };
                cg.write_object_for(&obj, Some(triple), optimise)?;
                eprintln!("compiled {} -> {} ({})", path, obj, triple);
                eprintln!(
                    "not linked: link it with that target's toolchain, e.g. \
                     `{}-gcc {} -o {}`",
                    triple, obj, stem
                );
                return Ok(());
            }
            // The object is an intermediate, and it goes where intermediates belong: NOT
            // into the working directory, where two builds running at once collide on the
            // same name — which is exactly what happened when two tests built the
            // self-hosted compiler in parallel. Unique per process, and removed after.
            let obj = std::env::temp_dir()
                .join(format!("burxt-{}-{}.o", std::process::id(), stem))
                .to_string_lossy()
                .into_owned();
            cg.write_object(&obj, optimise)?;

            // link with the system C compiler (for printf + crt startup), plus
            // whatever the caller needs for the C it declared.
            // **`run`'s executable is an intermediate too, and until v0.0.256 it was not treated
            // as one.** The paragraph above explains why the `.o` goes to a temp dir, is
            // pid-unique and is removed — and every word of it applies to the binary when the
            // command is `run`, where the binary is a means rather than the product. It was
            // written to `./<stem>` and left there.
            //
            // The cost was not theoretical. `burxt run` from a project root leaves a stray
            // extensionless executable, and `the_repository_root_holds_only_what_belongs_there`
            // caught seven of mine in one day plus one from a teammate. `.gitignore` hides them
            // from `git status` (the `/*` then `!/*.*` whitelist ignores extensionless root
            // files, by design), so the only thing that sees them is a test that walks the
            // filesystem — which means a USER gets no warning at all.
            //
            // No mainstream `run` behaves this way: `go run` uses a temp dir, `cargo run` writes
            // under `target/`. A command whose name says "and then run it" should not leave an
            // artifact behind, and `build` — which exists precisely to leave one — is unchanged.
            let exe = match out {
                Some(p) => p.to_string(),
                None if cmd == "run" => std::env::temp_dir()
                    .join(format!("burxt-{}-{}", std::process::id(), stem))
                    .to_string_lossy()
                    .into_owned(),
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
            // Not announced when the path is ours and about to be deleted: telling a reader
            // "compiled X -> /tmp/burxt-8973-X" invites them to go and look for a file that will
            // not be there. `build` announces, because `build`'s whole answer is where the file is.
            if !(cmd == "run" && out.is_none()) {
                eprintln!("compiled {} -> {}", path, exe);
            }

            if cmd == "run" {
                let status = Command::new(&exe)
                    .status()
                    .map_err(|e| format!("failed to run {}: {}", exe, e))?;
                // Removed BEFORE exiting, because `process::exit` runs no destructors — and only
                // when we chose the path ourselves. A caller who passed `-o` asked for a file and
                // gets to keep it.
                if out.is_none() {
                    let _ = std::fs::remove_file(&exe);
                }
                std::process::exit(status.code().unwrap_or(1));
            }
            Ok(())
        }
        other => Err(format!("unknown command: {}", other).into()),
    }
}
